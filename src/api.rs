//! Blocking HTTP client for the bundle's loopback management API.
//!
//! The bundle guards every route with `sameOrigin()` (`src/app/http.ts`): the `Host` header
//! must be `127.0.0.1:<port>` or `localhost:<port>`, the request must carry
//! `x-orchestrator-request: 1`, and an `Origin` header, if present, must match. A local
//! process satisfies all three, so this console needs no token and no change to the bundle.

use crate::model::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

const HEADER: &str = "x-orchestrator-request";

#[derive(Debug)]
pub enum ApiError {
    Transport(String),
    Status(u16, String),
    Protocol(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Transport(detail) => write!(f, "cannot reach dsh-superagent ({detail})"),
            ApiError::Status(403, _) => write!(
                f,
                "the service refused a same-origin request; check that --url points at the \
                 bundle's loopback port (127.0.0.1 or localhost)"
            ),
            ApiError::Status(code, message) if message.is_empty() => {
                write!(f, "the service answered HTTP {code}")
            }
            ApiError::Status(code, message) => write!(f, "HTTP {code}: {message}"),
            ApiError::Protocol(detail) => write!(f, "unexpected response ({detail})"),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<ApiError> for String {
    fn from(value: ApiError) -> Self {
        value.to_string()
    }
}

#[derive(Clone)]
pub struct Api {
    base: String,
    agent: ureq::Agent,
}

impl Api {
    pub fn new(base: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout(Duration::from_secs(20))
            .build();
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}/orchestrator/api{}", self.base, path)
    }

    pub(crate) fn get(&self, path: &str) -> Result<Value, ApiError> {
        let response = self
            .agent
            .get(&self.url(path))
            .set(HEADER, "1")
            .set("accept", "application/json")
            .call();
        finish(response)
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, ApiError> {
        let response = self
            .agent
            .post(&self.url(path))
            .set(HEADER, "1")
            .set("accept", "application/json")
            .send_json(body);
        finish(response)
    }

    fn get_as<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let value = self.get(path)?;
        serde_json::from_value(value).map_err(|error| ApiError::Protocol(error.to_string()))
    }

    pub(crate) fn put(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        let response = self
            .agent
            .put(&self.url(path))
            .set(HEADER, "1")
            .set("accept", "application/json")
            .send_json(body.clone());
        finish(response)
    }

    /// Saves one settings block; the bundle answers `saved`, the new `revision` and whether it restarts.
    pub fn save(&self, block: &str, value: &Value, revision: u64) -> Result<Value, ApiError> {
        self.put(
            &format!("/settings/{}", enc(block)),
            &json!({ "value": value, "revision": revision }),
        )
    }

    /// The service's model catalog: the default model and the providers it can route to.
    pub fn models(&self) -> Result<Value, ApiError> {
        self.get("/models")
    }

    pub fn settings(&self) -> Result<Value, ApiError> {
        self.get("/settings")
    }

    /// One settings block's value and revision, read fresh so a save cannot beat a stale copy.
    pub fn block(&self, name: &str) -> Result<(Value, u64), ApiError> {
        let settings = self.settings()?;
        let entry = settings
            .get("blocks")
            .and_then(|blocks| blocks.get(name))
            .ok_or_else(|| ApiError::Protocol(format!("no settings block `{name}`")))?;
        Ok((
            entry.get("value").cloned().unwrap_or_else(|| json!({})),
            entry.get("revision").and_then(Value::as_u64).unwrap_or(0),
        ))
    }

    /// Saves one settings block after rereading its revision.
    pub fn set(&self, name: &str, change: &dyn Fn(&mut Value)) -> Result<Value, ApiError> {
        let (mut value, revision) = self.block(name)?;
        change(&mut value);
        self.save(name, &value, revision)
    }

    pub fn state(&self) -> Result<StateView, ApiError> {
        self.get_as("/state")
    }

    pub fn portal(&self, offset: u32) -> Result<Value, ApiError> {
        self.get(&format!("/portal?offset={offset}&limit=20"))
    }

    pub fn overview(&self) -> Result<Overview, ApiError> {
        self.get_as("/overview")
    }

    pub fn tickets(&self, state: &str) -> Result<Page<TicketRow>, ApiError> {
        self.get_as(&format!("/tickets?state={}&limit=50", enc(state)))
    }

    pub fn ticket(&self, id: &str) -> Result<TicketDetail, ApiError> {
        self.get_as(&format!("/tickets/{}", enc(id)))
    }

    /// Every Thread the service keeps for `filter`, not just the first page.
    ///
    /// `/threads` pages (`{ items, total, next_offset }`, at most 1000 a page), and
    /// `format::thread_list` prints no cursor: `booth threads` showed the first 50 of `total`
    /// and silently hid the rest. Follow the served cursor instead, so the list is complete.
    pub fn threads(&self, filter: &str) -> Result<Page<ThreadRow>, ApiError> {
        let mut items = Vec::new();
        let mut offset = 0u32;
        let total;
        loop {
            let page: Page<ThreadRow> = self.get_as(&format!(
                "/threads?filter={}&limit=50&offset={offset}",
                enc(filter)
            ))?;
            items.extend(page.items);
            match page.next_offset {
                Some(next) if next > offset => offset = next,
                _ => {
                    total = page.total;
                    break;
                }
            }
        }
        Ok(Page {
            items,
            total,
            next_offset: None,
        })
    }

    /// One Thread with its whole timeline, not just the newest page.
    ///
    /// `/threads/{key}` pages its `timeline` (50 by default) and serves the cursor for the next
    /// older page in `earlier`; `format::thread_detail` prints that cursor, but no command can
    /// pass it back, so `booth thread` showed the newest 50 messages of a long thread and left
    /// the rest unreachable. Follow the cursor and prepend each older page, as the web panel's
    /// "load more" does, so the view the console prints is the whole thread.
    pub fn thread(&self, key: &str) -> Result<ThreadDetail, ApiError> {
        let path = format!("/threads/{}", enc(key));
        let mut view: ThreadDetail = self.get_as(&path)?;
        let mut older = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut earlier = view.earlier.clone();
        while let Some(before) = earlier.clone() {
            // A cursor that does not move would repeat the same page; keep the served cursor
            // and stop instead of looping on it.
            if !seen.insert(before.clone()) {
                break;
            }
            let page: ThreadDetail = self.get_as(&format!("{}?before={}", path, enc(&before)))?;
            if page.timeline.is_empty() {
                earlier = page.earlier.clone();
                break;
            }
            older.push(page.timeline);
            earlier = page.earlier.clone();
        }
        // Each page holds the newest items older than the previous page's oldest, oldest-first,
        // so the pages are prepended in reverse fetch order.
        let mut timeline = Vec::new();
        for page in older.into_iter().rev() {
            timeline.extend(page);
        }
        timeline.append(&mut view.timeline);
        view.timeline = timeline;
        view.earlier = earlier;
        Ok(view)
    }

    pub fn decisions(&self) -> Result<DecisionList, ApiError> {
        self.get_as("/decisions?limit=50")
    }

    pub fn sources(&self) -> Result<Vec<SourceStatus>, ApiError> {
        let mut value = self.get("/sources")?;
        // A Source kind may report its title as a plain string, as channel labels did before the
        // localized pair (see `Link::label`), and one such Source must not hide every Source.
        if let Some(items) = value.as_array_mut() {
            for item in items {
                if let Some(title) = item.get_mut("title") {
                    if let Some(text) = title.as_str() {
                        *title = json!({ "zh": text, "en": text });
                    }
                }
            }
        }
        serde_json::from_value(value).map_err(|error| ApiError::Protocol(error.to_string()))
    }

    pub fn search(&self, query: &str) -> Result<SearchView, ApiError> {
        self.get_as(&format!("/search?q={}", enc(query)))
    }

    pub fn stop(&self, id: &str) -> Result<Value, ApiError> {
        self.post(&format!("/tickets/{}/stop", enc(id)), json!({}))
    }

    pub fn retry(&self, id: &str) -> Result<Value, ApiError> {
        self.post(&format!("/tickets/{}/retry", enc(id)), json!({}))
    }

    pub fn requeue(&self, trigger: &str) -> Result<Value, ApiError> {
        self.post(&format!("/triggers/{}/requeue", enc(trigger)), json!({}))
    }

    pub fn dismiss(&self, decision: &str) -> Result<Value, ApiError> {
        self.post(&format!("/decisions/{}/dismiss", enc(decision)), json!({}))
    }

    pub fn pause(&self, paused: bool) -> Result<Value, ApiError> {
        self.post("/executor/pause", json!({ "paused": paused }))
    }

    pub fn unsubscribe(&self, thread: &str) -> Result<Value, ApiError> {
        self.post(&format!("/threads/{}/unsubscribe", enc(thread)), json!({}))
    }
}

fn finish(response: Result<ureq::Response, ureq::Error>) -> Result<Value, ApiError> {
    match response {
        Ok(response) => {
            // Read the body before parsing it: `into_json` folds a body that stops arriving into
            // a JSON error, so a response cut off by a restart (or by the 20-second deadline)
            // printed "unexpected response (Failed to read JSON: …)" instead of the transport
            // class. The read is the transport part and the parse the protocol part, and
            // `intake::between_instances` retries the former but not the latter.
            let mut body = Vec::new();
            response
                .into_reader()
                .read_to_end(&mut body)
                .map_err(|error| ApiError::Transport(error.to_string()))?;
            serde_json::from_slice(&body).map_err(|error| ApiError::Protocol(error.to_string()))
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            let message = serde_json::from_str::<Value>(&body).ok().and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            match (code, message) {
                // Every error dsh-superagent itself writes carries `{"error": …}`, including
                // an unknown operation and a missing ticket. Without one, these two statuses
                // are the DSH web server's fallback for a route whose owner has not registered
                // yet (it starts listening first): the static frontend answers an unregistered
                // GET with an empty 404 and any other method with an empty 405, so a POST or
                // PUT during startup or restart arrived before the management API did.
                (404 | 405, None) => Err(ApiError::Transport(
                    "the management API is not mounted yet; the service is starting or \
                     restarting, or --url points at another server"
                        .into(),
                )),
                (_, Some(message)) => Err(ApiError::Status(code, message)),
                (_, None) => Err(ApiError::Status(code, body)),
            }
        }
        Err(ureq::Error::Transport(error)) => Err(ApiError::Transport(error.to_string())),
    }
}

/// Percent-encodes one path segment. Thread keys contain `:`, `#` and `/`, which must not
/// reach the route as separators.
pub fn enc(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::{enc, Api, ApiError};
    use crate::testkit::FakeApi;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Answers one request with exactly `status` and `body`, so a test can pin a response the
    /// typed fixture does not model, such as the DSH web server's fallback.
    fn one_shot(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });
        base
    }

    /// Answers one request with headers that promise 100 bytes and then closes after 10, the way
    /// a service killed in the middle of its response looks to the client.
    fn a_cut_off_body() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 100\r\nconnection: close\r\n\r\n{\"paused\":",
            );
            let _ = stream.flush();
        });
        base
    }

    #[test]
    fn a_body_cut_off_mid_response_is_a_transport_failure() {
        // The service going away while it writes its answer is "cannot reach", not a malformed
        // answer: a restart in the middle of a poll used to print
        // "unexpected response (Failed to read JSON: response body closed …)" and also skipped
        // the restart retry that `intake::between_instances` gives the transport class.
        let api = Api::new(&a_cut_off_body());
        let error = api.state().expect_err("a cut-off body is not an answer");
        assert!(
            matches!(&error, ApiError::Transport(_)),
            "a cut-off body is the transport class, got {error:?}"
        );
        assert!(
            error.to_string().contains("cannot reach dsh-superagent"),
            "{error}"
        );
    }

    #[test]
    fn a_complete_body_that_is_not_json_is_still_a_protocol_error() {
        // Reading the body first must not turn a malformed answer into an unreachable service.
        let api = Api::new(&one_shot(200, "not json"));
        let error = api.state().expect_err("a non-JSON body is not an answer");
        assert!(
            matches!(&error, ApiError::Protocol(_)),
            "a complete non-JSON body is a protocol error, got {error:?}"
        );
    }

    #[test]
    fn an_empty_405_from_the_web_server_is_also_a_starting_service() {
        // The DSH web server's static frontend serves GET/HEAD only and answers any other
        // method with an empty 405, so `pause` (a POST) before dsh-superagent registers its
        // routes used to print "the service answered HTTP 405" instead of the startup hint.
        let api = Api::new(&one_shot(405, ""));
        let error = api
            .pause(true)
            .expect_err("an empty 405 is not an answer from a mounted route");
        match error {
            ApiError::Transport(detail) => {
                assert!(detail.contains("not mounted yet"), "{detail}");
            }
            other => panic!("a starting service is the transport class, got {other:?}"),
        }
    }

    #[test]
    fn a_405_with_the_bundles_error_is_still_a_status() {
        // A mounted route's own answer keeps its message; only the empty fallback means the
        // management API is absent.
        let api = Api::new(&one_shot(405, r#"{"error":"Method not allowed"}"#));
        match api.pause(true).expect_err("a 405 is an error") {
            ApiError::Status(405, message) => assert_eq!(message, "Method not allowed"),
            other => panic!("a JSON error is the service's answer, got {other:?}"),
        }
    }

    #[test]
    fn encodes_path_separators_but_not_ids() {
        assert_eq!(enc("t-abc_1.2~x"), "t-abc_1.2~x");
        assert_eq!(enc("github:o/r#12"), "github%3Ao%2Fr%2312");
    }

    #[test]
    fn a_thread_list_follows_the_served_cursor_instead_of_stopping_at_one_page() {
        // `/threads` pages at 50 and `format::thread_list` prints no cursor, so `booth threads`
        // showed the first 50 of `total` and hid the rest. The list must read the pages.
        let service = FakeApi::start();
        service.set_route(
            "/threads?filter=all&limit=50&offset=0",
            json!({
                "items": [
                    { "key": "github:1", "source": "github", "title": "one", "stage": "subscribed" }
                ],
                "total": 2,
                "next_offset": 1
            }),
        );
        service.set_route(
            "/threads?filter=all&limit=50&offset=1",
            json!({
                "items": [
                    { "key": "github:2", "source": "github", "title": "two", "stage": "subscribed" }
                ],
                "total": 2,
                "next_offset": null
            }),
        );
        let page = Api::new(&service.base)
            .threads("all")
            .expect("both served pages decode");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, 2);
        assert_eq!(page.next_offset, None);
    }

    #[test]
    fn a_thread_detail_reads_the_earlier_timeline_pages() {
        // `/threads/{key}` serves the newest 50 timeline items and the cursor for the older page
        // in `earlier`. `format::thread_detail` prints that cursor, but no command accepts it, so
        // a long thread's older messages were unreachable from the console.
        let service = FakeApi::start();
        let cursor = "2026-11-10T00:00:00.000Z|t1";
        let key = enc("github:1");
        service.set_route(
            &format!("/threads/{key}"),
            json!({
                "key": "github:1",
                "source": "github",
                "title": "one",
                "stage": "subscribed",
                "timeline": [
                    { "id": "t2", "source": "github", "human": true,
                      "at": "2026-11-10T00:01:00.000Z", "preview": "newer" }
                ],
                "earlier": cursor
            }),
        );
        service.set_route(
            &format!("/threads/{key}?before={}", enc(cursor)),
            json!({
                "key": "github:1",
                "source": "github",
                "title": "one",
                "stage": "subscribed",
                "timeline": [
                    { "id": "t1", "source": "github", "human": true,
                      "at": "2026-11-10T00:00:00.000Z", "preview": "older" }
                ],
                "earlier": null
            }),
        );
        let view = Api::new(&service.base)
            .thread("github:1")
            .expect("both served pages decode");
        let ids: Vec<&str> = view.timeline.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["t1", "t2"], "older messages come before newer ones");
        assert_eq!(view.earlier, None, "the whole timeline was read");
    }

    #[test]
    fn a_textual_source_title_is_a_label_not_a_protocol_error() {
        let service = FakeApi::start();
        service.routes.lock().unwrap().push((
            "/sources".to_owned(),
            json!([
                // The fixture and older Source kinds name a title as one string.
                { "id": "github", "kind": "github", "title": "GitHub",
                  "enabled": true, "installed": true, "health": [] },
                { "id": "feishu", "kind": "feishu", "title": { "zh": "飞书", "en": "Feishu" },
                  "enabled": false, "installed": true, "health": [] }
            ]),
        ));
        let sources = Api::new(&service.base)
            .sources()
            .expect("one textual title must not hide every source");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].title.text(), "GitHub");
        assert_eq!(sources[1].title.text(), "飞书");
    }
}

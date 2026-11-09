//! Blocking HTTP client for the bundle's loopback management API.
//!
//! The bundle guards every route with `sameOrigin()` (`src/app/http.ts`): the `Host` header
//! must be `127.0.0.1:<port>` or `localhost:<port>`, the request must carry
//! `x-orchestrator-request: 1`, and an `Origin` header, if present, must match. A local
//! process satisfies all three, so this console needs no token and no change to the bundle.

use crate::model::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
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

    fn get(&self, path: &str) -> Result<Value, ApiError> {
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

    fn put(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
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

    pub fn overview(&self) -> Result<Overview, ApiError> {
        self.get_as("/overview")
    }

    pub fn tickets(&self, state: &str) -> Result<Page<TicketRow>, ApiError> {
        self.get_as(&format!("/tickets?state={}&limit=50", enc(state)))
    }

    pub fn ticket(&self, id: &str) -> Result<TicketDetail, ApiError> {
        self.get_as(&format!("/tickets/{}", enc(id)))
    }

    pub fn threads(&self, filter: &str) -> Result<Page<ThreadRow>, ApiError> {
        self.get_as(&format!("/threads?filter={}&limit=50", enc(filter)))
    }

    pub fn thread(&self, key: &str) -> Result<ThreadDetail, ApiError> {
        self.get_as(&format!("/threads/{}", enc(key)))
    }

    pub fn decisions(&self) -> Result<DecisionList, ApiError> {
        self.get_as("/decisions?limit=50")
    }

    pub fn sources(&self) -> Result<Vec<SourceStatus>, ApiError> {
        self.get_as("/sources")
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
        Ok(response) => response
            .into_json::<Value>()
            .map_err(|error| ApiError::Protocol(error.to_string())),
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            let message = serde_json::from_str::<Value>(&body).ok().and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            match (code, message) {
                // Every 404 dsh-superagent itself writes carries `{"error": …}`, including an
                // unknown operation. A 404 without one is the DSH web server's fallback for a
                // route whose owner has not registered yet (it starts listening first), so the
                // management API is absent rather than answering "not found".
                (404, None) => Err(ApiError::Transport(
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
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn encodes_path_separators_but_not_ids() {
        assert_eq!(enc("t-abc_1.2~x"), "t-abc_1.2~x");
        assert_eq!(enc("github:o/r#12"), "github%3Ao%2Fr%2312");
    }

    /// Answers one request the way the DSH web server answers a route whose owner has not
    /// registered yet: a 404 with no body. This is what `/orchestrator/api/state` looks like
    /// while dsh-superagent is still starting, before its plugin registers the route.
    fn web_server_fallback() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut head = [0u8; 1024];
                let _ = stream.read(&mut head);
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                );
                let _ = stream.flush();
            }
        });
        base
    }

    #[test]
    fn an_unmounted_management_route_is_not_a_missing_record() {
        let error = Api::new(&web_server_fallback())
            .state()
            .expect_err("the web server's bare 404 is not a state view");
        match error {
            ApiError::Transport(detail) => {
                assert!(detail.contains("not mounted"), "{detail}");
                assert!(detail.contains("starting"), "{detail}");
            }
            other => panic!("a bare 404 means the service is not answering, not {other:?}"),
        }
    }

    #[test]
    fn a_route_404_still_names_the_refusal() {
        let service = FakeApi::start();
        // The bundle's own 404s carry `{"error": …}`; this one must stay a status, not become
        // "the service is not mounted".
        let error = Api::new(&service.base)
            .ticket("no-such-ticket")
            .unwrap_err();
        match error {
            ApiError::Status(404, message) => assert_eq!(message, "Unknown operation"),
            other => panic!("a JSON 404 is the route answering, not {other:?}"),
        }
    }
}

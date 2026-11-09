//! A canned management API for tests: real HTTP on a loopback port, no service needed.
//!
//! The console's commands are thin wrappers over this API, so rendering and the request shape
//! are worth testing against the wire rather than against a mock object: a renamed route or a
//! wrong header then fails here.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A process-wide lock for tests that set environment variables or change the working directory.
///
/// cargo runs tests in parallel threads of one process, so `std::env::set_var` in one test is
/// visible to another. Every test that reads or writes such state takes this first.
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

/// A route body keyed by the path suffix it answers.
type Routes = Arc<Mutex<Vec<(String, Value)>>>;
/// Every PUT the service received, by block name, with its body.
type Puts = Arc<Mutex<Vec<(String, Value)>>>;

/// One canned service. `settings` is the mutable `/settings` body; every PUT is recorded.
pub struct FakeApi {
    pub base: String,
    pub settings: Arc<Mutex<Value>>,
    /// Statuses to return for the next PUTs, one per call, before answering normally.
    pub failures: Arc<Mutex<Vec<(u16, String)>>>,
    /// How many `/state` answers to refuse before the service "comes back".
    pub state_failures: Remaining,
    /// How many `/settings` answers to refuse, the way a restart does.
    pub settings_failures: Remaining,
    pub puts: Puts,
    #[allow(dead_code)]
    /// Bodies for the other GET routes, by path; the tests that need one extend this.
    pub routes: Routes,
}

impl FakeApi {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().unwrap().port();
        let settings = Arc::new(Mutex::new(default_settings()));
        let puts = Arc::new(Mutex::new(Vec::new()));
        let failures = Arc::new(Mutex::new(Vec::new()));
        let state_failures = Arc::new(Mutex::new(0usize));
        let settings_failures = Arc::new(Mutex::new(0usize));
        let routes = Arc::new(Mutex::new(vec![
            (
                "/state".to_owned(),
                json!({
                    "paused": false,
                    "masterId": "orchestrator-test",
                    "health": { "runtime": { "state": "ready", "detail": "" } },
                    "load": { "running": 0, "queued": 0, "maxWorkers": 3 },
                    "setup": { "problem": null, "accounts": { "repository": "o/r", "agent": "tester" } }
                }),
            ),
            (
                "/models".to_owned(),
                json!({
                    "default": { "provider": "deepseek-official", "model": "deepseek-flash" },
                    "routableProviders": ["deepseek-official", "deepseek-account"],
                    "groups": [{
                        "id": "deepseek-official",
                        "name": "DeepSeek",
                        "models": [{ "id": "deepseek-flash", "name": "DeepSeek-V4.1-Flash" }]
                    }]
                }),
            ),
            (
                "/overview".to_owned(),
                json!({
                    "problems": [],
                    "person": [],
                    "executor": { "running": 0, "queued": 0, "maxWorkers": 3, "idle": 0 },
                    "flow": { "received": 0, "filtered": 0, "judged": 0, "accepted": 0, "evaluating": 0, "dropped": 0 },
                    "policyVersion": "test"
                }),
            ),
        ]));
        let shared = (
            settings.clone(),
            puts.clone(),
            routes.clone(),
            failures.clone(),
            state_failures.clone(),
            settings_failures.clone(),
        );
        std::thread::spawn(move || serve(listener, shared));
        Self {
            base: format!("http://127.0.0.1:{port}"),
            settings,
            puts,
            failures,
            state_failures,
            settings_failures,
            routes,
        }
    }

    /// Makes `/settings` refuse this many times first, as a restarting service does.
    pub fn refuse_settings(&self, times: usize) {
        *self.settings_failures.lock().unwrap() = times;
    }

    /// Makes `/state` refuse this many times first, as a restarting service does.
    pub fn refuse_state(&self, times: usize) {
        *self.state_failures.lock().unwrap() = times;
    }

    /// Replaces the Source list, so a test can model a registry that is not ready.
    pub fn set_sources(&self, sources: Value) {
        if let Some(list) = self.settings.lock().unwrap().get_mut("sources") {
            *list = sources;
        }
    }

    /// Makes the next `times` PUTs fail with `status` and `message`.
    pub fn fail_next_puts(&self, times: usize, status: u16, message: &str) {
        let mut failures = self.failures.lock().unwrap();
        for _ in 0..times {
            failures.push((status, message.to_owned()));
        }
    }

    pub fn put_count(&self) -> usize {
        self.puts.lock().unwrap().len()
    }

    pub fn last_put(&self) -> Option<(String, Value)> {
        self.puts.lock().unwrap().last().cloned()
    }
}

pub fn default_settings() -> Value {
    json!({
        "policyVersion": "test",
        "blocks": {
            "policy": { "value": { "rules": [], "sources": {} }, "revision": 0 },
            "eval": { "value": { "mode": "observe", "timeoutSeconds": 120, "model": null }, "revision": 3 },
            "executor": { "value": { "maxWorkers": 3, "directory": "", "prepare": "", "blockedCommands": [] }, "revision": 1 }
        },
        "sources": [
            { "id": "github", "kind": "github", "installed": true, "revision": 0,
              "value": { "enabled": true, "config": { "repository": "o/r", "cli": "gh" } },
              "fields": [], "facts": {} },
            { "id": "local", "kind": "local", "installed": true, "revision": 0,
              "value": { "enabled": true, "config": {} }, "fields": [], "facts": {} }
        ]
    })
}

type Failures = Arc<Mutex<Vec<(u16, String)>>>;
/// A counter of answers still to refuse, such as a restarting service.
type Remaining = Arc<Mutex<usize>>;

fn serve(
    listener: TcpListener,
    shared: (
        Arc<Mutex<Value>>,
        Puts,
        Routes,
        Failures,
        Remaining,
        Remaining,
    ),
) {
    let (settings, puts, routes, failures, state_failures, settings_failures) = shared;
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let (settings, puts, routes, failures) = (
            settings.clone(),
            puts.clone(),
            routes.clone(),
            failures.clone(),
        );
        let state_failures = state_failures.clone();
        let settings_failures = settings_failures.clone();
        std::thread::spawn(move || {
            let mut raw = Vec::new();
            let mut buffer = [0u8; 8192];
            // Read the head, then the body according to content-length.
            loop {
                let read = match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => read,
                };
                raw.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&raw);
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let length = text[..head_end]
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())?
                        })
                        .unwrap_or(0);
                    if raw.len() >= head_end + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&raw).into_owned();
            let mut head = text.split("\r\n");
            let request = head.next().unwrap_or_default().to_owned();
            let mut parts = request.split_whitespace();
            let method = parts.next().unwrap_or_default().to_owned();
            let path = parts.next().unwrap_or_default().to_owned();
            let body = text.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned();
            // The same-origin guard is a header. Scanning the whole request would accept a
            // headerless save whose body happens to contain the header text, which is exactly
            // the mistake this fake exists to catch.
            let head_block = text.split("\r\n\r\n").next().unwrap_or_default();
            let guarded = head_block.lines().any(|line| {
                line.split_once(':').is_some_and(|(name, value)| {
                    name.trim().eq_ignore_ascii_case("x-orchestrator-request")
                        && value.trim() == "1"
                })
            });

            // A queued failure answers this PUT instead of the normal handler. Only a PUT pops
            // one: a GET must not swallow a failure meant for the save that follows it.
            let forced: Option<(u16, String)> = if method == "PUT" {
                failures.lock().ok().and_then(|mut queue| queue.pop())
            } else {
                None
            };
            let (status, payload) = if !guarded {
                (
                    403,
                    json!({ "error": "Same-origin management request required" }),
                )
            } else if method == "PUT" {
                match forced {
                    Some((status, message)) => (status, json!({ "error": message })),
                    None => {
                        let name = path.rsplit('/').next().unwrap_or_default().to_owned();
                        let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                        let mut settings = settings.lock().unwrap();
                        if let Some(entry) = block_mut(&mut settings, &name) {
                            if let Some(value) = parsed.get("value") {
                                entry["value"] = value.clone();
                            }
                            let revision = entry["revision"].as_u64().unwrap_or(0) + 1;
                            entry["revision"] = json!(revision);
                            puts.lock().unwrap().push((name, parsed));
                            (200, json!({ "saved": true, "revision": revision }))
                        } else {
                            (404, json!({ "error": "Source not found" }))
                        }
                    }
                }
            } else if path.ends_with("/state") {
                let mut left = state_failures.lock().unwrap();
                if *left > 0 {
                    *left -= 1;
                    (503, json!({ "error": "restarting" }))
                } else {
                    drop(left);
                    let body = routes
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|(known, _)| known == "/state")
                        .map(|(_, body)| body.clone())
                        .unwrap_or(Value::Null);
                    (200, body)
                }
            } else if path.ends_with("/settings") {
                let mut left = settings_failures.lock().unwrap();
                if *left > 0 {
                    *left -= 1;
                    (503, json!({ "error": "restarting" }))
                } else {
                    drop(left);
                    (200, settings.lock().unwrap().clone())
                }
            } else {
                let known = routes
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|(known, _)| path.ends_with(known.as_str()))
                    .map(|(_, body)| body.clone());
                match known {
                    Some(body) => (200, body),
                    None => (404, json!({ "error": "Unknown operation" })),
                }
            };

            let payload = serde_json::to_string(&payload).unwrap_or_default();
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });
    }
}

/// `PUT /settings/<block>` addresses a block by name; a Source is `source:<id>`.
fn block_mut<'a>(settings: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    if let Some(id) = name
        .strip_prefix("source%3A")
        .or_else(|| name.strip_prefix("source:"))
    {
        return settings
            .get_mut("sources")?
            .as_array_mut()?
            .iter_mut()
            .find(|source| source.get("id").and_then(Value::as_str) == Some(id));
    }
    settings.get_mut("blocks")?.get_mut(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    /// Sends one raw request, so a test can leave out a header the real client always sets.
    fn raw(base: &str, request: &str) -> String {
        let addr = base.strip_prefix("http://").expect("a loopback base");
        let mut stream = TcpStream::connect(addr).expect("connect to the fake");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn status(response: &str) -> u16 {
        response
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .expect("a status line")
    }

    fn save_policy(header: Option<&str>, body: &str) -> String {
        let header = header.map(|value| format!("x-orchestrator-request: {value}\r\n"));
        format!(
            "PUT /orchestrator/api/settings/policy HTTP/1.1\r\n\
             host: 127.0.0.1\r\n\
             content-type: application/json\r\n\
             content-length: {}\r\n\
             {}\
             connection: close\r\n\
             \r\n\
             {body}",
            body.len(),
            header.unwrap_or_default()
        )
    }

    #[test]
    fn the_same_origin_header_must_be_a_header_not_body_text() {
        let api = FakeApi::start();
        // A settings value can hold arbitrary text; it must not stand in for the header.
        let body = r#"{"value":{"note":"x-orchestrator-request: 1"},"revision":0}"#;
        let response = raw(&api.base, &save_policy(None, body));
        assert_eq!(status(&response), 403, "{response}");
        assert_eq!(api.put_count(), 0, "a refused request must not save");
    }

    #[test]
    fn a_request_with_the_same_origin_header_is_accepted() {
        let api = FakeApi::start();
        let body = r#"{"value":{"note":"a save"},"revision":0}"#;
        let response = raw(&api.base, &save_policy(Some("1"), body));
        assert_eq!(status(&response), 200, "{response}");
        assert_eq!(api.put_count(), 1);
    }
}

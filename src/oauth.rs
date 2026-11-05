//! DeepSeek account login, the same protocol DSH's web panel uses: PKCE against
//! `platform.deepseek.com`, a loopback redirect, and the grant stored in `.credentials.yaml`.
//!
//! Two ways to finish: the listener catches `?code=` when the browser can reach 127.0.0.1,
//! or the user pastes the callback URL into the console. The exchange and the credential
//! write are identical either way.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const ORIGIN: &str = "https://platform.deepseek.com";
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126 Safari/537.36";
const GRANT_KEY: &str = "deepseek-account-platform/default";
const DEVICE_KEY: &str = "deepseek-account-platform/device";

#[derive(Clone)]
pub struct Attempt {
    pub url: String,
    pub verifier: String,
    pub state: String,
    pub redirect_uri: String,
}

/// Bind the callback port and ask the platform for an authorize URL.
pub fn begin(port: u16) -> Result<(Attempt, TcpListener), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot listen on 127.0.0.1:{port}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot configure the listener: {error}"))?;

    let port = listener
        .local_addr()
        .map_err(|error| format!("cannot read the callback port: {error}"))?
        .port();
    let verifier = random_text(32)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_text(16)?;
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/callback");
    let body = json!({
        "code_challenge": challenge,
        "code_challenge_method": "S256",
        "state": state,
        "redirect_uri": redirect_uri,
        "locale": "en",
        "login_source": "web",
    });
    let data = post("auth_init", &body)?;
    let url = data
        .get("authorize_url")
        .and_then(Value::as_str)
        .ok_or("auth_init returned no authorize_url")?
        .to_owned();
    Ok((
        Attempt {
            url,
            verifier,
            state,
            redirect_uri,
        },
        listener,
    ))
}

/// Accept one callback on the loopback port. Polls so cancellation can end the wait.
pub fn wait_code(
    listener: &TcpListener,
    state: &str,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let request = read_request(&mut stream);
                match parse_callback(&request, state) {
                    Ok(Some(code)) => {
                        let _ = respond(&mut stream, 200, "Signed in. You can close this tab.");
                        return Ok(code);
                    }
                    Ok(None) => {
                        let _ = respond(&mut stream, 404, "Not a sign-in callback.");
                        continue;
                    }
                    Err(_) => {
                        // A stale tab or a copied URL from an earlier attempt: answer it and
                        // keep this attempt alive instead of failing the login.
                        let _ = respond(&mut stream, 400, "This callback is not valid. Try again.");
                        continue;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err("timed out waiting for the browser".into());
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(error) => return Err(format!("callback listener failed: {error}")),
        }
    }
}

/// Parse a pasted `http://127.0.0.1:.../oauth/callback?code=..&state=..` URL.
pub fn code_from_url(url: &str, state: &str) -> Result<String, String> {
    // Terminals and chat clients add quotes, angle brackets, a fragment or extra spaces around
    // a copied URL; strip those before looking for the parameters.
    let url = url
        .trim()
        .trim_matches(|ch| matches!(ch, '<' | '>' | '"' | '\''));
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .or_else(|| url.contains("code=").then_some(url))
        .ok_or("that URL has no `?code=` part")?;
    let query = query.split('#').next().unwrap_or(query);
    let mut code = None;
    let mut received = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "code" => code = Some(percent_decode(value)),
            "state" => received = Some(percent_decode(value)),
            _ => {}
        }
    }
    let received = received.ok_or("that URL has no `state`")?;
    if received != state {
        return Err("that callback belongs to another login attempt".into());
    }
    code.filter(|value| !value.is_empty())
        .ok_or_else(|| "that URL has no `code`".into())
}

/// Exchange the code and write the grant DSH's account provider reads.
pub fn exchange(attempt: &Attempt, code: &str) -> Result<(), String> {
    let device = device_id()?;
    let body = json!({
        "code": code,
        "code_verifier": attempt.verifier,
        "redirect_uri": attempt.redirect_uri,
        "device_id": device,
        "device_model": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        "os_version": os_version(),
    });
    let data = post("auth_exchange", &body)?;
    let token = data
        .get("token")
        .and_then(Value::as_str)
        .ok_or("auth_exchange returned no token")?;
    store_grant(token)
}

fn post(method: &str, body: &Value) -> Result<Value, String> {
    let url = format!("{ORIGIN}/auth-api/v0/dsh/{method}");
    let response = ureq::post(&url)
        .set("content-type", "application/json")
        .set("user-agent", USER_AGENT)
        .set("origin", ORIGIN)
        .set("referer", &format!("{ORIGIN}/"))
        // The platform rejects requests without the web client identity headers.
        .set("x-client-bundle-id", "")
        .set("x-client-platform", "web")
        .set("x-client-version", env!("CARGO_PKG_VERSION"))
        .set("x-client-locale", "en_US")
        .set("x-client-timezone-offset", &timezone_offset().to_string())
        .timeout(Duration::from_secs(30))
        .send_json(body);
    let value: Value = match response {
        Ok(response) => response
            .into_json()
            .map_err(|error| format!("platform sent invalid JSON: {error}"))?,
        Err(ureq::Error::Status(status, _)) => {
            return Err(format!("platform answered HTTP {status}"));
        }
        Err(error) => return Err(format!("cannot reach platform.deepseek.com: {error}")),
    };
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        return Err(format!("platform refused the request (code {code})"));
    }
    let data = value.get("data").ok_or("platform response has no data")?;
    let biz = data.get("biz_code").and_then(Value::as_i64).unwrap_or(-1);
    if biz != 0 {
        let message = data
            .get("biz_msg")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .unwrap_or("unknown");
        return Err(format!("platform rejected the request: {message}"));
    }
    data.get("biz_data")
        .cloned()
        .ok_or_else(|| "platform response has no biz_data".into())
}

/// Whether the account grant DSH's `deepseek-official` provider consumes is on disk.
pub fn account_signed_in() -> bool {
    let Ok(path) = credentials_path() else {
        return false;
    };
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(root) = serde_norway::from_str::<Value>(&text) else {
        return false;
    };
    root.get("records")
        .and_then(|records| records.get(GRANT_KEY))
        .and_then(|record| record.get("payload"))
        .and_then(|payload| payload.get("token"))
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty())
}

fn store_grant(token: &str) -> Result<(), String> {
    let path = credentials_path()?;
    let mut root: Value = match fs::read_to_string(&path) {
        Ok(text) => serde_norway::from_str(&text)
            .map_err(|error| format!("cannot parse {}: {error}", path.display()))?,
        Err(_) => json!({ "version": 1, "refs": {}, "records": {} }),
    };
    if !root.is_object() {
        root = json!({ "version": 1, "records": {} });
    }
    let records = root
        .as_object_mut()
        .expect("checked above")
        .entry("records")
        .or_insert_with(|| json!({}));
    let records = records
        .as_object_mut()
        .ok_or("credentials `records` is not a map")?;

    if !records.contains_key(DEVICE_KEY) {
        records.insert(
            DEVICE_KEY.to_owned(),
            json!({ "kind": "grant", "payload": { "id": uuid_v4() } }),
        );
    }
    records.insert(
        GRANT_KEY.to_owned(),
        json!({
            "kind": "grant",
            "payload": { "version": 1, "token": token, "issuer": ORIGIN },
        }),
    );

    let text = serde_norway::to_string(&root)
        .map_err(|error| format!("cannot encode credentials: {error}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    // An existing file keeps its own mode through `mode()`, so tighten it explicitly.
    fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot restrict {}: {error}", path.display()))
}

fn device_id() -> Result<String, String> {
    let path = credentials_path()?;
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(root) = serde_norway::from_str::<Value>(&text) {
            if let Some(id) = root
                .get("records")
                .and_then(|records| records.get(DEVICE_KEY))
                .and_then(|record| record.get("payload"))
                .and_then(|payload| payload.get("id"))
                .and_then(Value::as_str)
            {
                return Ok(id.to_owned());
            }
        }
    }
    Ok(uuid_v4())
}

fn credentials_path() -> Result<PathBuf, String> {
    match std::env::var_os("DSH_HOME") {
        Some(value) => Ok(PathBuf::from(value).join(".credentials.yaml")),
        None => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".dsh/.credentials.yaml"))
            .ok_or_else(|| "HOME is not set".to_owned()),
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 4096];
    let mut text = String::new();
    if let Ok(size) = stream.read(&mut buffer) {
        text.push_str(&String::from_utf8_lossy(&buffer[..size]));
    }
    text.lines().next().unwrap_or_default().to_owned()
}

fn respond(stream: &mut TcpStream, status: u16, message: &str) -> std::io::Result<()> {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>booth</title><body style=\"font:16px system-ui;padding:3rem\"><p>{message}</p>"
    );
    let head = format!(
        "HTTP/1.1 {status} {}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        if status == 200 { "OK" } else { "Bad Request" },
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn parse_callback(request: &str, state: &str) -> Result<Option<String>, String> {
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" || !target.starts_with("/oauth/callback") {
        return Ok(None);
    }
    match code_from_url(&format!("http://127.0.0.1{target}"), state) {
        Ok(code) => Ok(Some(code)),
        Err(error) => Err(error),
    }
}

fn random_text(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut buffer))
        .map_err(|error| format!("cannot read randomness: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

pub(crate) fn uuid_v4() -> String {
    let mut bytes: [u8; 16] = [0; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn timezone_offset() -> i64 {
    let output = std::process::Command::new("date").arg("+%z").output();
    let Ok(output) = output else { return 0 };
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if text.len() != 5 {
        return 0;
    }
    let sign = if text.starts_with('-') { -1 } else { 1 };
    let hours: i64 = text[1..3].parse().unwrap_or(0);
    let minutes: i64 = text[3..5].parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

fn os_version() -> String {
    std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_code_from_a_pasted_callback() {
        let code = code_from_url(
            "http://127.0.0.1:37029/oauth/callback?code=abc_DEF-123&state=xyz",
            "xyz",
        )
        .unwrap();
        assert_eq!(code, "abc_DEF-123");
    }

    #[test]
    fn rejects_another_attempts_state() {
        let error =
            code_from_url("http://127.0.0.1:1/oauth/callback?code=a&state=b", "c").unwrap_err();
        assert!(error.contains("another login attempt"), "{error}");
    }

    #[test]
    fn listener_returns_the_code_and_answers_the_browser() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let browser = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream
                .write_all(b"GET /oauth/callback?code=abc&state=st HTTP/1.1\r\nHost: x\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });
        let cancel = AtomicBool::new(false);
        let code = wait_code(&listener, "st", &cancel, Duration::from_secs(5)).unwrap();
        assert_eq!(code, "abc");
        assert!(browser.join().unwrap().starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn stores_the_grant_dsh_reads() {
        let home = std::env::temp_dir().join(format!("booth-oauth-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(".credentials.yaml"),
            "version: 1\nrecords:\n  keep/me:\n    kind: grant\n    payload: {}\n",
        )
        .unwrap();
        std::env::set_var("DSH_HOME", &home);
        store_grant("tok-123").unwrap();
        let path = home.join(".credentials.yaml");
        let root: Value = serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["records"][GRANT_KEY]["payload"]["token"], "tok-123");
        assert_eq!(root["records"][GRANT_KEY]["payload"]["issuer"], ORIGIN);
        assert!(root["records"][DEVICE_KEY]["payload"]["id"].is_string());
        assert!(root["records"]["keep/me"].is_object());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn rejects_a_url_without_a_code() {
        assert!(code_from_url("http://127.0.0.1:1/oauth/callback?state=b", "b").is_err());
        assert!(code_from_url("not a url", "b").is_err());
    }
}

#[cfg(test)]
mod more_tests {
    use super::code_from_url;

    #[test]
    fn reads_a_callback_with_noise_around_it() {
        let url = "<http://127.0.0.1:37029/oauth/callback?code=abc%2F123&state=st-1#done>";
        assert_eq!(code_from_url(url, "st-1").unwrap(), "abc/123");
        assert_eq!(code_from_url("code=abc&state=st-1", "st-1").unwrap(), "abc");
    }

    #[test]
    fn explains_a_cut_or_foreign_callback() {
        let error = code_from_url("http://127.0.0.1/x?code=abc", "st-1").unwrap_err();
        assert!(error.contains("state"), "{error}");
        let error = code_from_url("http://127.0.0.1/x?code=abc&state=other", "st-1").unwrap_err();
        assert!(error.contains("another login"), "{error}");
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;

    /// `device_id` is what the exchange sends as the device, so it must be stable and non-empty.
    #[test]
    fn device_id_is_created_once_and_then_reused() {
        // DSH_HOME is process-wide, so this test takes its turn with the others that set it.
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-device-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);

        let first = device_id().unwrap();
        assert!(!first.is_empty(), "a device id is never empty");
        assert!(first.len() >= 32, "a uuid, not a placeholder: {first}");

        // A stored record wins over a fresh id: the same install keeps one device identity.
        std::fs::write(
            home.join(".credentials.yaml"),
            format!(
                "version: 1\nrecords:\n  {DEVICE_KEY}:\n    kind: device\n    payload:\n      id: kept-device\n"
            ),
        )
        .unwrap();
        assert_eq!(device_id().unwrap(), "kept-device");

        match previous {
            Some(value) => std::env::set_var("DSH_HOME", value),
            None => std::env::remove_var("DSH_HOME"),
        }
        std::fs::remove_dir_all(&home).unwrap();
    }
}

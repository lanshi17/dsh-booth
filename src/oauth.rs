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
use std::path::{Path, PathBuf};
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
        "device_id": &device,
        "device_model": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        "os_version": os_version(),
    });
    let data = post("auth_exchange", &body)?;
    let token = data
        .get("token")
        .and_then(Value::as_str)
        .ok_or("auth_exchange returned no token")?;
    store_grant(token, &device)
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

fn store_grant(token: &str, device: &str) -> Result<(), String> {
    let path = credentials_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    // The rename below is atomic for readers, but this is a read-modify-write cycle: another
    // writer (DSH itself, or another booth command) that commits between the read and the
    // rename would have its record silently replaced by the bytes this cycle started from. DSH
    // serializes its own cycles with the store's `<file>.lock` sibling, so take the same lock
    // before reading and hold it until the replacement is committed.
    with_store_lock(&path, || {
        let mut root: Value = match fs::read_to_string(&path) {
            Ok(text) => serde_norway::from_str(&text)
                .map_err(|error| format!("cannot parse {}: {error}", path.display()))?,
            // Only an absent file is a fresh store. Any other read failure — an unreadable mode,
            // a truncated write, an I/O error — must not read as "empty": the write below would
            // then replace the shared file and take DSH's records, including the account grant,
            // with it.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                json!({ "version": 1, "refs": {}, "records": {} })
            }
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
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

        // Keep the device the exchange just named. A different id here would mean the platform
        // registered one device and the next sign-in presented another.
        records.insert(
            DEVICE_KEY.to_owned(),
            json!({ "kind": "grant", "payload": { "id": device } }),
        );
        records.insert(
            GRANT_KEY.to_owned(),
            json!({
                "kind": "grant",
                "payload": { "version": 1, "token": token, "issuer": ORIGIN },
            }),
        );

        let text = serde_norway::to_string(&root)
            .map_err(|error| format!("cannot encode credentials: {error}"))?;
        replace_store(&path, &text)
    })
}

/// Hold the store's cross-process writer lock around one read-modify-rename cycle. The lock is a
/// `<store>.lock` sibling that records the writer's pid, the same file and format DSH's own
/// credentials store uses, so the two writers serialize against each other. Readers stay
/// lock-free: the rename commit is atomic.
fn with_store_lock<T>(
    store: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let lock = acquire_store_lock(store)?;
    let result = operation();
    drop(lock);
    result
}

/// A held writer lock; dropping it releases the lock file, on the error path too.
struct StoreLock {
    path: PathBuf,
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// How long a contender waits for the writer lock before reporting it. DSH sizes the wait for its
/// own credentials document at 30 s because a record mutation can hold the lock across a network
/// round trip (an expired-token refresh); a shorter wait here would fail a sign-in that raced one.
const STORE_LOCK_WAIT: Duration = Duration::from_secs(30);

fn acquire_store_lock(store: &Path) -> Result<StoreLock, String> {
    let path = lock_path(store);
    let deadline = Instant::now() + STORE_LOCK_WAIT;
    let mut delay = Duration::from_millis(20);
    loop {
        match create_lock(&path) {
            Ok(()) => return Ok(StoreLock { path }),
            // A writer that died while holding the lock must not block sign-in forever: take
            // over a lock only when its recorded pid is proven gone, and wait for every other
            // holder, including one whose record cannot be read.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if take_over_exited_lock(&path) {
                    continue;
                }
            }
            Err(error) => return Err(format!("cannot write {}: {error}", store.display())),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "cannot write {}: timed out waiting for the writer lock at {}",
                store.display(),
                path.display()
            ));
        }
        std::thread::sleep(delay);
        delay = (delay * 2).min(Duration::from_millis(200));
    }
}

fn lock_path(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

fn create_lock(path: &Path) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    if let Err(error) = writeln!(file, "{}", std::process::id()) {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

/// Remove a lock whose recorded holder is gone. The claim file serializes contenders, and the
/// lock is re-read and re-probed under it, so a removal never deletes a lock acquired after the
/// dead holder's — including one whose holder reused the pid.
fn take_over_exited_lock(lock: &Path) -> bool {
    let Ok(record) = fs::read_to_string(lock) else {
        return false;
    };
    if !holder_exited(&record) {
        return false;
    }
    let mut claim_name = lock.as_os_str().to_os_string();
    claim_name.push(format!(".takeover-{}", short_hash(&record)));
    let claim = PathBuf::from(claim_name);
    let Ok(mut file) = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&claim)
    else {
        return false;
    };
    let _ = writeln!(file, "{}", std::process::id());
    let removed = fs::read_to_string(lock)
        .map(|current| current == record)
        .unwrap_or(false)
        && holder_exited(&record)
        && fs::remove_file(lock).is_ok();
    let _ = fs::remove_file(&claim);
    removed
}

/// Whether a `<pid>\n` lock record names a process that is proven gone. A malformed record, this
/// process, or a probe that cannot decide all count as "not gone": the lock is then waited for,
/// never stolen.
fn holder_exited(record: &str) -> bool {
    let Some(pid) = record
        .strip_suffix('\n')
        .and_then(|text| text.parse::<i32>().ok())
    else {
        return false;
    };
    if pid <= 0 || pid as u32 == std::process::id() {
        return false;
    }
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| !status.success())
        .unwrap_or(false)
}

fn short_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Replace the shared store in one step. DSH and the other booth commands read this file too, so
/// it must never be visible half-written: truncating it in place and then writing leaves a window
/// where a crash, a full disk or a concurrent reader sees a truncated store. Stage the whole text
/// as a sibling with exclusive create and the credential mode, then rename it over the store,
/// which commits atomically on one filesystem. The staged name is removed on either outcome; a
/// failed install leaves the old store exactly as it was.
fn replace_store(path: &Path, text: &str) -> Result<(), String> {
    let staged = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid_v4()
    ));
    let installed = write_staged(&staged, text).and_then(|()| fs::rename(&staged, path));
    // On success the rename consumed the staged name and this is the harmless ENOENT; otherwise
    // it clears the half-written sibling so no partial store outlives the failure.
    let _ = fs::remove_file(&staged);
    installed.map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Write the complete replacement to a fresh inode. An exclusive create refuses a file already at
/// the staged name, and the credential mode is set before the rename, so the replacement is never
/// wider than the store it replaces.
fn write_staged(staged: &Path, text: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(staged)?;
    file.write_all(text.as_bytes())?;
    fs::set_permissions(staged, fs::Permissions::from_mode(0o600))
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
    Ok(harness_home()?.join(".credentials.yaml"))
}

/// DSH's `$DSH_HOME` precedence and spelling: a blank override is unset, and a leading `~` names
/// the OS home. Resolving the raw value instead points the account grant at a different store
/// than the one DSH reads — for an empty override, `.credentials.yaml` in the working directory.
fn harness_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("DSH_HOME") {
        if let Some(text) = value.to_str() {
            if !text.trim().is_empty() {
                return expand_home(text);
            }
        } else {
            return Ok(PathBuf::from(value));
        }
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".dsh"))
        .ok_or_else(|| "HOME is not set".to_owned())
}

/// Expand the `~` prefixes DSH's home resolver accepts. A `~` that cannot be expanded is an
/// error, not a literal directory beside the working directory.
fn expand_home(text: &str) -> Result<PathBuf, String> {
    let rest = if text == "~" {
        ""
    } else if let Some(rest) = text.strip_prefix("~/") {
        rest
    } else if let Some(rest) = text.strip_prefix("~\\") {
        rest
    } else {
        return Ok(PathBuf::from(text));
    };
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "cannot expand `~` in DSH_HOME: HOME is not set".to_owned())?;
    Ok(home.join(rest))
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

/// Put an environment variable back the way a test found it: the tests below share one process,
/// so a value one of them overwrites is another one's starting state.
#[cfg(test)]
fn restore_env(name: &str, previous: Option<std::ffi::OsString>) {
    match previous {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

/// Put `DSH_HOME` back the way a test found it.
#[cfg(test)]
fn restore_dsh_home(previous: Option<std::ffi::OsString>) {
    restore_env("DSH_HOME", previous);
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
        // DSH_HOME is process-wide and the provider tests set it too, under this lock.
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-oauth-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(".credentials.yaml"),
            "version: 1\nrecords:\n  keep/me:\n    kind: grant\n    payload: {}\n",
        )
        .unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);
        store_grant("tok-123", "device-1").unwrap();
        let path = home.join(".credentials.yaml");
        let root: Value = serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["records"][GRANT_KEY]["payload"]["token"], "tok-123");
        assert_eq!(root["records"][GRANT_KEY]["payload"]["issuer"], ORIGIN);
        assert_eq!(root["records"][DEVICE_KEY]["payload"]["id"], "device-1");
        assert!(root["records"]["keep/me"].is_object());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        restore_dsh_home(previous);
        std::fs::remove_dir_all(&home).ok();
    }

    /// An absent file is the one read failure that means "nothing to keep" and may start fresh.
    #[test]
    fn a_missing_credentials_file_starts_a_fresh_store() {
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-oauth-missing-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);

        store_grant("tok-123", "device-1").unwrap();
        let path = home.join(".credentials.yaml");
        let root: Value = serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["version"], 1);
        assert_eq!(root["records"][GRANT_KEY]["payload"]["token"], "tok-123");
        assert_eq!(root["records"][DEVICE_KEY]["payload"]["id"], "device-1");

        restore_dsh_home(previous);
        std::fs::remove_dir_all(&home).ok();
    }

    /// The shared store holds DSH's own refs and records, including the account grant. A read
    /// failure is not an empty store: rewriting it would drop all of that.
    #[test]
    fn a_credentials_file_it_cannot_read_is_not_replaced() {
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-oauth-unreadable-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join(".credentials.yaml");
        std::fs::write(
            &path,
            format!(
                "version: 1\nrecords:\n  keep/me:\n    kind: grant\n    payload: {{}}\n  {GRANT_KEY}:\n    kind: grant\n    payload:\n      token: kept-token\n  {DEVICE_KEY}:\n    kind: grant\n    payload:\n      id: kept-device\n"
            ),
        )
        .unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);
        use std::os::unix::fs::PermissionsExt;
        // Readable by no one, writable by its owner: reading fails, rewriting would succeed.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
        let error = store_grant("tok-new", "device-2").unwrap_err();
        assert!(error.contains("cannot read"), "{error}");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let root: Value = serde_norway::from_str(&text).unwrap();
        assert_eq!(root["records"]["keep/me"]["kind"], "grant");
        assert_eq!(root["records"][GRANT_KEY]["payload"]["token"], "kept-token");
        assert_eq!(root["records"][DEVICE_KEY]["payload"]["id"], "kept-device");

        restore_dsh_home(previous);
        std::fs::remove_dir_all(&home).ok();
    }

    /// The store DSH and the other booth commands share must never be truncated in place. A
    /// directory it cannot stage a replacement in still lets its existing file be rewritten, so
    /// the refusal has to happen before the old bytes are touched.
    #[test]
    fn a_store_it_cannot_stage_is_left_intact() {
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-oauth-staging-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join(".credentials.yaml");
        std::fs::write(
            &path,
            format!(
                "version: 1\nrecords:\n  keep/me:\n    kind: grant\n    payload: {{}}\n  {GRANT_KEY}:\n    kind: grant\n    payload:\n      token: kept-token\n  {DEVICE_KEY}:\n    kind: grant\n    payload:\n      id: kept-device\n"
            ),
        )
        .unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);
        use std::os::unix::fs::PermissionsExt;
        // The file stays writable by its owner; only the directory cannot take a sibling.
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500)).unwrap();
        let error = store_grant("tok-new", "device-2").unwrap_err();
        assert!(error.contains("cannot write"), "{error}");

        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let root: Value = serde_norway::from_str(&text).unwrap();
        assert_eq!(root["records"]["keep/me"]["kind"], "grant");
        assert_eq!(root["records"][GRANT_KEY]["payload"]["token"], "kept-token");
        assert_eq!(root["records"][DEVICE_KEY]["payload"]["id"], "kept-device");
        let siblings: Vec<_> = std::fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(siblings, [".credentials.yaml"], "a staged sibling survived");

        restore_dsh_home(previous);
        std::fs::remove_dir_all(&home).ok();
    }

    /// The shared store is read-modify-written by DSH and the other booth commands. The atomic
    /// rename keeps readers safe, but two writers that both read the old bytes still race: the
    /// later rename drops the earlier writer's record. The store's writer lock is what serializes
    /// that cycle, so a sign-in must wait for a writer that already holds it.
    #[test]
    fn a_store_write_waits_for_the_shared_writer_lock() {
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-oauth-lock-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join(".credentials.yaml");
        let lock = home.join(".credentials.yaml.lock");
        let old = "version: 1\nrecords:\n  keep/me:\n    kind: grant\n    payload: {}\n";
        std::fs::write(&path, old).unwrap();
        // A live writer holds the lock. Its record names this process, so a contender that
        // honours the lock waits instead of taking it over.
        std::fs::write(&lock, format!("{}\n", std::process::id())).unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);

        let (tx, rx) = std::sync::mpsc::channel();
        let sign_in = std::thread::spawn(move || {
            let result = store_grant("tok-new", "device-new");
            tx.send(result).unwrap();
        });

        // The lock holder is still writing; the sign-in must not have touched the store.
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "the sign-in wrote the store while another writer held its lock"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);

        // The other writer finishes its read-modify-rename while still holding the lock.
        std::fs::write(
            &path,
            format!("{old}  other/writer:\n    kind: grant\n    payload:\n      token: kept\n"),
        )
        .unwrap();
        std::fs::remove_file(&lock).unwrap();

        rx.recv_timeout(Duration::from_secs(5))
            .expect("the sign-in never took the writer lock")
            .unwrap();
        sign_in.join().unwrap();

        let root: Value = serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["records"][GRANT_KEY]["payload"]["token"], "tok-new");
        assert_eq!(
            root["records"]["other/writer"]["payload"]["token"], "kept",
            "the sign-in overwrote the record the other writer committed"
        );
        assert!(!lock.exists(), "the sign-in left its lock behind");

        restore_dsh_home(previous);
        std::fs::remove_dir_all(&home).ok();
    }

    /// A writer killed while holding the lock leaves the lock file behind. Its record names a pid
    /// that no longer exists, so the next sign-in takes it over instead of waiting out the
    /// timeout, and leaves neither the stale lock nor a takeover claim behind.
    #[test]
    fn a_lock_from_a_dead_writer_is_taken_over() {
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-oauth-stale-lock-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let lock = home.join(".credentials.yaml.lock");
        // Above every host's pid_max, so no live process can carry it.
        std::fs::write(&lock, "2147483646\n").unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);

        store_grant("tok-new", "device-new").unwrap();

        let path = home.join(".credentials.yaml");
        let root: Value = serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["records"][GRANT_KEY]["payload"]["token"], "tok-new");
        let siblings: Vec<_> = std::fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            siblings,
            [".credentials.yaml"],
            "a lock or takeover claim survived"
        );

        restore_dsh_home(previous);
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
mod home_tests {
    use super::*;
    use std::ffi::OsString;

    /// Put both variables the home resolution reads back the way this test found them, on the
    /// panic path too: they are process-wide and the other modules set them under this lock.
    struct Env {
        dsh_home: Option<OsString>,
        home: Option<OsString>,
    }

    impl Env {
        fn capture() -> Self {
            Self {
                dsh_home: std::env::var_os("DSH_HOME"),
                home: std::env::var_os("HOME"),
            }
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            restore_dsh_home(self.dsh_home.take());
            restore_env("HOME", self.home.take());
        }
    }

    fn temp_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("booth-oauth-{name}-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    /// DSH documents an empty or whitespace-only `$DSH_HOME` as unset, so a blank override must
    /// not resolve the store to `.credentials.yaml` in whatever directory booth was started from.
    #[test]
    fn a_blank_dsh_home_is_the_default_store() {
        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        let home = temp_home("blank-home");
        std::env::set_var("HOME", &home);

        for value in ["", " ", "\t\n"] {
            std::env::set_var("DSH_HOME", value);
            assert_eq!(
                credentials_path().unwrap(),
                home.join(".dsh/.credentials.yaml"),
                "DSH_HOME={value:?}"
            );
        }

        drop(env);
        std::fs::remove_dir_all(&home).ok();
    }

    /// DSH expands `~`, `~/…` and `~\…` against the OS home before joining the store; a literal
    /// `~` directory beside the working directory is not the store DSH reads.
    #[test]
    fn a_tilde_dsh_home_is_expanded_like_dsh() {
        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        let home = temp_home("tilde-home");
        std::env::set_var("HOME", &home);

        for (value, expected) in [
            ("~", home.join(".credentials.yaml")),
            ("~/state", home.join("state/.credentials.yaml")),
            ("~\\state", home.join("state/.credentials.yaml")),
            ("/srv/booth", PathBuf::from("/srv/booth/.credentials.yaml")),
        ] {
            std::env::set_var("DSH_HOME", value);
            assert_eq!(credentials_path().unwrap(), expected, "DSH_HOME={value:?}");
        }

        drop(env);
        std::fs::remove_dir_all(&home).ok();
    }

    /// No home resolver can invent a home: with `$HOME` gone, a tilde or a blank override is an
    /// error, not a path under the working directory.
    #[test]
    fn a_missing_home_is_an_error() {
        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        std::env::remove_var("HOME");

        std::env::set_var("DSH_HOME", "~");
        let error = credentials_path().unwrap_err();
        assert!(error.contains("HOME"), "{error}");

        std::env::set_var("DSH_HOME", "");
        let error = credentials_path().unwrap_err();
        assert!(error.contains("HOME"), "{error}");

        drop(env);
    }

    /// A `$DSH_HOME` that is not UTF-8 is still one path; decoding it lossily would rename it.
    #[test]
    fn a_non_utf8_dsh_home_keeps_its_bytes() {
        use std::os::unix::ffi::OsStringExt;

        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        let raw = OsString::from_vec(b"/srv/booth-\xff".to_vec());
        std::env::set_var("DSH_HOME", &raw);
        assert_eq!(
            credentials_path().unwrap(),
            PathBuf::from(raw).join(".credentials.yaml")
        );

        drop(env);
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

        restore_dsh_home(previous);
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// `exchange` reads the device id before the network call and stores the grant after it.
    /// Both must be the same device: the platform registers the grant against the id it was
    /// shown, so storing another one means the next sign-in presents an unknown device.
    #[test]
    fn the_grant_is_stored_beside_the_device_that_signed_in() {
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-signin-device-{}", uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let previous = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", &home);

        let signed_in_with = device_id().unwrap();
        store_grant("tok", &signed_in_with).unwrap();
        assert_eq!(
            device_id().unwrap(),
            signed_in_with,
            "the grant is stored beside the device the exchange used"
        );

        // A second sign-in presents the same device instead of inventing another one.
        store_grant("tok-2", &signed_in_with).unwrap();
        assert_eq!(device_id().unwrap(), signed_in_with);

        restore_dsh_home(previous);
        std::fs::remove_dir_all(&home).unwrap();
    }
}

#[cfg(test)]
mod env_tests {
    use super::restore_dsh_home;
    use std::ffi::{OsStr, OsString};

    /// Both shapes of "put DSH_HOME back": a caller may have had it set, or not set at all.
    #[test]
    fn dsh_home_is_restored_either_way() {
        let _env = crate::testkit::env_lock();
        let previous = std::env::var_os("DSH_HOME");

        restore_dsh_home(Some(OsString::from("/tmp/booth-restored-home")));
        assert_eq!(
            std::env::var_os("DSH_HOME").as_deref(),
            Some(OsStr::new("/tmp/booth-restored-home"))
        );

        restore_dsh_home(None);
        assert!(std::env::var_os("DSH_HOME").is_none());

        restore_dsh_home(previous);
    }
}

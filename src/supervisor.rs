//! A small, loopback-only supervisor for independently running components.
//!
//! Components do not become booth plugins. They register an id, pid and management URL with
//! this process and renew a short lease. The registry is deliberately boring JSON so a crashed
//! supervisor can recover the last known component metadata without importing component state.

use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_PORT: u16 = 3081;
const LEASE_MS: u64 = 15_000;
const MAX_BODY: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct Config {
    pub state_dir: PathBuf,
    pub port: u16,
    pub token: String,
}

impl Config {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub fn pid_path(&self) -> PathBuf {
        self.state_dir.join("booth-supervisor.pid")
    }
    pub fn registry_path(&self) -> PathBuf {
        self.state_dir.join("booth-components.json")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Component {
    pub id: String,
    pub pid: u32,
    #[serde(rename = "apiUrl")]
    pub api_url: String,
    pub version: String,
    #[serde(rename = "registeredAt")]
    pub registered_at: u64,
    #[serde(rename = "lastSeen")]
    pub last_seen: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct Registry {
    components: BTreeMap<String, Component>,
}

#[derive(Clone)]
struct Shared {
    cfg: Config,
    registry: Arc<Mutex<Registry>>,
}

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn loopback_url(url: &str) -> bool {
    (url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:"))
        && url[url.find("://").unwrap_or(0) + 3..].contains(':')
}

fn load(path: &Path) -> Registry {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(shared: &Shared, registry: &Registry) -> Result<(), String> {
    let path = shared.cfg.registry_path();
    let staged = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(registry).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&staged)
        .map_err(|e| format!("cannot write {}: {e}", staged.display()))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("cannot write {}: {e}", staged.display()))?;
    fs::rename(&staged, path).map_err(|e| format!("cannot install component registry: {e}"))
}

fn prune(registry: &mut Registry) {
    let cutoff = now().saturating_sub(LEASE_MS * 2);
    registry
        .components
        .retain(|_, component| component.last_seen >= cutoff);
}

fn parse_request(stream: &mut TcpStream) -> Result<Request, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(4096);
    let header_end;
    loop {
        let mut chunk = [0u8; 4096];
        let count = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("connection closed before request headers".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_BODY + 16 * 1024 {
            return Err("request is too large".into());
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let header =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| "request headers are not UTF-8")?;
    let mut lines = header.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split_whitespace();
    let method = first.next().unwrap_or_default().to_owned();
    let path = first.next().unwrap_or_default().to_owned();
    if method.is_empty() || path.is_empty() {
        return Err("invalid HTTP request line".into());
    }
    let mut headers = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err("request body is too large".into());
    }
    while bytes.len() < header_end + length {
        let mut chunk = [0u8; 4096];
        let count = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("connection closed before request body".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(Request {
        method,
        path: path.split('?').next().unwrap_or(&path).to_owned(),
        headers,
        body: bytes[header_end..header_end + length].to_vec(),
    })
}

fn response(stream: &mut TcpStream, status: u16, body: serde_json::Value) {
    let bytes = serde_json::to_vec(&body)
        .unwrap_or_else(|_| b"{\"error\":\"serialization failure\"}".to_vec());
    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    let head = format!("HTTP/1.1 {status} {reason}\r\ncontent-type: application/json; charset=utf-8\r\ncache-control: no-store\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", bytes.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&bytes);
}

fn authorized(request: &Request, token: &str) -> bool {
    request
        .headers
        .get("x-booth-supervisor-token")
        .map(String::as_str)
        == Some(token)
}

fn json_body<T: for<'de> Deserialize<'de>>(request: &Request) -> Result<T, String> {
    serde_json::from_slice(&request.body).map_err(|e| format!("invalid JSON body: {e}"))
}

#[derive(Deserialize)]
struct RegisterRequest {
    id: String,
    pid: u32,
    #[serde(rename = "apiUrl")]
    api_url: String,
    #[serde(default)]
    version: String,
}

fn handle(mut stream: TcpStream, shared: Shared) {
    let request = match parse_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            response(&mut stream, 400, serde_json::json!({"error": error}));
            return;
        }
    };
    if !authorized(&request, &shared.cfg.token) {
        response(
            &mut stream,
            401,
            serde_json::json!({"error":"invalid supervisor token"}),
        );
        return;
    }
    let mut registry = shared.registry.lock().expect("registry lock poisoned");
    prune(&mut registry);
    let result: Result<(u16, serde_json::Value, bool), (u16, String)> = match (
        request.method.as_str(),
        request.path.as_str(),
    ) {
        ("GET", "/supervisor/v1/health") => Ok((
            200,
            serde_json::json!({"status":"ready", "leaseMs":LEASE_MS, "components":registry.components.len()}),
            false,
        )),
        ("GET", "/supervisor/v1/components") => Ok((
            200,
            serde_json::json!({"components": registry.components.values().cloned().collect::<Vec<_>>() }),
            false,
        )),
        ("POST", "/supervisor/v1/components/register") => {
            let body: RegisterRequest = match json_body(&request) {
                Ok(body) => body,
                Err(error) => {
                    response(&mut stream, 400, serde_json::json!({"error": error}));
                    return;
                }
            };
            if !valid_id(&body.id) || body.pid == 0 || !loopback_url(&body.api_url) {
                return response(
                    &mut stream,
                    400,
                    serde_json::json!({"error":"id, pid or apiUrl is invalid"}),
                );
            }
            let timestamp = now();
            registry.components.insert(
                body.id.clone(),
                Component {
                    id: body.id,
                    pid: body.pid,
                    api_url: body.api_url,
                    version: body.version,
                    registered_at: timestamp,
                    last_seen: timestamp,
                },
            );
            let saved = save(&shared, &registry);
            match saved {
                Ok(()) => Ok((
                    201,
                    serde_json::json!({"registered":true, "leaseMs":LEASE_MS}),
                    true,
                )),
                Err(error) => Err((500, error)),
            }
        }
        ("POST", path)
            if path.starts_with("/supervisor/v1/components/") && path.ends_with("/heartbeat") =>
        {
            let id = &path["/supervisor/v1/components/".len()..path.len() - "/heartbeat".len()];
            match registry.components.get_mut(id) {
                Some(component) => {
                    component.last_seen = now();
                    match save(&shared, &registry) {
                        Ok(()) => Ok((
                            200,
                            serde_json::json!({"alive":true, "leaseMs":LEASE_MS}),
                            true,
                        )),
                        Err(error) => Err((500, error)),
                    }
                }
                None => Err((404, "component is not registered".into())),
            }
        }
        ("POST", path)
            if path.starts_with("/supervisor/v1/components/") && path.ends_with("/unregister") =>
        {
            let id = &path["/supervisor/v1/components/".len()..path.len() - "/unregister".len()];
            let removed = registry.components.remove(id).is_some();
            if removed {
                if let Err(error) = save(&shared, &registry) {
                    return response(&mut stream, 500, serde_json::json!({"error":error}));
                }
            }
            Ok((200, serde_json::json!({"unregistered":removed}), removed))
        }
        _ => Err((404, "unknown supervisor operation".into())),
    };
    match result {
        Ok((status, body, _)) => response(&mut stream, status, body),
        Err((status, error)) => response(&mut stream, status, serde_json::json!({"error":error})),
    }
}

pub fn run(cfg: Config) -> Result<(), String> {
    fs::create_dir_all(&cfg.state_dir)
        .map_err(|e| format!("cannot create {}: {e}", cfg.state_dir.display()))?;
    fs::write(cfg.pid_path(), std::process::id().to_string())
        .map_err(|e| format!("cannot write supervisor pid: {e}"))?;
    let registry = Arc::new(Mutex::new(load(&cfg.registry_path())));
    {
        let mut guard = registry.lock().expect("registry lock poisoned");
        prune(&mut guard);
        let _ = save(
            &Shared {
                cfg: cfg.clone(),
                registry: registry.clone(),
            },
            &guard,
        );
    }
    let listener = TcpListener::bind(("127.0.0.1", cfg.port))
        .map_err(|e| format!("cannot bind supervisor on {}: {e}", cfg.url()))?;
    let shared = Shared {
        cfg: cfg.clone(),
        registry,
    };
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let shared = shared.clone();
                thread::spawn(move || handle(stream, shared));
            }
            Err(error) => eprintln!("booth supervisor accept failed: {error}"),
        }
    }
    let _ = fs::remove_file(cfg.pid_path());
    Ok(())
}

pub fn token_path(state_dir: &Path) -> PathBuf {
    state_dir.join("booth-supervisor.token")
}

pub fn read_token(state_dir: &Path) -> Result<String, String> {
    fs::read_to_string(token_path(state_dir))
        .map(|value| value.trim().to_owned())
        .map_err(|e| format!("cannot read supervisor token: {e}"))
}

pub fn ensure_token(state_dir: &Path) -> Result<String, String> {
    let path = token_path(state_dir);
    if let Ok(token) = fs::read_to_string(&path) {
        if token.trim().len() >= 24 {
            protect_token(&path)?;
            return Ok(token.trim().to_owned());
        }
    }
    let mut random = [0u8; 32];
    let digest = if OpenOptions::new()
        .read(true)
        .open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .is_ok()
    {
        random.to_vec()
    } else {
        let stamp = format!(
            "{}-{}-{}",
            std::process::id(),
            now(),
            std::thread::current().name().unwrap_or("booth")
        );
        sha2::Sha256::digest(stamp.as_bytes()).to_vec()
    };
    let token = format!("booth-{}", hex(&digest));
    let staged = path.with_extension("token.tmp");
    fs::write(&staged, format!("{token}\n"))
        .map_err(|e| format!("cannot write supervisor token: {e}"))?;
    protect_token(&staged)?;
    fs::rename(staged, path).map_err(|e| format!("cannot install supervisor token: {e}"))?;
    Ok(token)
}

fn protect_token(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot protect supervisor token: {e}"))?;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

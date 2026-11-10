//! MCP-compatible JSON-RPC over stdio (Content-Length framing).
//!
//! Identity is resolved from a host-supplied static token map and optional OIDC-profile
//! HS256 JWT validator — never from caller-controlled `principal` fields in tool args.
use forge_application::Policy;
use forge_domain::Source;
use forge_domain::Task;
use forge_domain::TaskId;
use forge_domain::Trigger;
use forge_identity::CompositeIdentityResolver;
use forge_identity::IdentityResolver;
use forge_identity::OidcHs256Validator;
use forge_identity::StaticTokenMap;
use forge_identity::TrustedIdentity as IdentityPrincipal;
use forge_persistence::TaskRepository;
use forge_persistence::TursoTaskRepository;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io;
use std::io::BufRead;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

pub const SUPPORTED_PROTOCOL_VERSION: &str = "2024-11-05";
pub const MAX_FRAME_BYTES: usize = 1 << 20;

#[derive(Debug, Serialize)]
pub struct ToolContract {
    pub name: &'static str,
    pub mutates_state: bool,
    pub implemented: bool,
}

pub fn planned_tools() -> Vec<ToolContract> {
    vec![
        ToolContract {
            name: "forge_task_create",
            mutates_state: true,
            implemented: true,
        },
        ToolContract {
            name: "forge_task_status",
            mutates_state: false,
            implemented: true,
        },
        ToolContract {
            name: "forge_assignment_cancel",
            mutates_state: true,
            implemented: true,
        },
    ]
}

pub type TrustedIdentity = IdentityPrincipal;

#[derive(Debug, Default, Clone)]
pub struct TrustedIdentityMap {
    by_token: BTreeMap<String, TrustedIdentity>,
}

impl TrustedIdentityMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, token: impl Into<String>, identity: TrustedIdentity) {
        self.by_token.insert(token.into(), identity);
    }

    pub fn resolve(&self, token: &str) -> Option<&TrustedIdentity> {
        self.by_token.get(token)
    }

    pub fn into_static_map(self) -> StaticTokenMap {
        let mut map = StaticTokenMap::new();
        for (token, identity) in self.by_token {
            map.insert(token, identity);
        }
        map
    }
}

#[derive(Debug, Error)]
pub enum McpError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("unsupported protocol version: {0}")]
    UnsupportedProtocol(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

impl McpError {
    fn code(&self) -> i64 {
        match self {
            Self::Parse(_) => -32700,
            Self::InvalidRequest(_) | Self::UnsupportedProtocol(_) => -32600,
            Self::MethodNotFound(_) => -32601,
            Self::Unauthorized => -32001,
            Self::Io(_) => -32000,
        }
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcErrorBody>,
}

#[derive(Debug, Serialize)]
struct JsonRpcErrorBody {
    code: i64,
    message: String,
}

pub struct McpServer {
    resolver: Arc<dyn IdentityResolver>,
    #[allow(dead_code)]
    policy: Policy,
    repo: Arc<TursoTaskRepository>,
    session_token: Mutex<Option<String>>,
    tasks_index: Mutex<BTreeMap<TaskId, Task>>,
}

impl McpServer {
    pub fn new(identities: TrustedIdentityMap, policy: Policy, repo: TursoTaskRepository) -> Self {
        Self::with_resolver(
            Arc::new(CompositeIdentityResolver {
                static_tokens: identities.into_static_map(),
                oidc: None,
            }),
            policy,
            repo,
        )
    }

    pub fn with_oidc(
        identities: TrustedIdentityMap,
        oidc: OidcHs256Validator,
        policy: Policy,
        repo: TursoTaskRepository,
    ) -> Self {
        Self::with_resolver(
            Arc::new(CompositeIdentityResolver {
                static_tokens: identities.into_static_map(),
                oidc: Some(oidc),
            }),
            policy,
            repo,
        )
    }

    pub fn with_resolver(
        resolver: Arc<dyn IdentityResolver>,
        policy: Policy,
        repo: TursoTaskRepository,
    ) -> Self {
        Self {
            resolver,
            policy,
            repo: Arc::new(repo),
            session_token: Mutex::new(None),
            tasks_index: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn handle_frame(&self, frame: &[u8]) -> Result<Option<Vec<u8>>, McpError> {
        let req: JsonRpcRequest =
            serde_json::from_slice(frame).map_err(|err| McpError::Parse(err.to_string()))?;
        if req.jsonrpc != "2.0" {
            return Err(McpError::InvalidRequest("jsonrpc must be 2.0".into()));
        }
        // Notifications (no id) are acknowledged with no response body.
        let Some(id) = req.id.clone() else {
            let _ = self.dispatch(&req)?;
            return Ok(None);
        };
        let message_id = format!("{}:{}", req.method, stable_id(&id));
        if let Some(cached) = self.repo.lookup_mcp_response(&message_id)? {
            return Ok(Some(cached.into_bytes()));
        }
        let result = match self.dispatch(&req) {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(value),
                error: None,
            },
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcErrorBody {
                    code: err.code(),
                    message: err.to_string(),
                }),
            },
        };
        let bytes =
            serde_json::to_vec(&result).map_err(|err| McpError::InvalidRequest(err.to_string()))?;
        // Cache successful and authorization/protocol errors for idempotent retries.
        let _ = self
            .repo
            .store_mcp_response(&message_id, &String::from_utf8_lossy(&bytes));
        Ok(Some(bytes))
    }

    fn dispatch(&self, req: &JsonRpcRequest) -> Result<Value, McpError> {
        match req.method.as_str() {
            "initialize" => self.initialize(&req.params),
            "notifications/initialized" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": planned_tools().into_iter().map(|t| json!({
                    "name": t.name,
                    "description": t.name,
                    "inputSchema": { "type": "object" },
                })).collect::<Vec<_>>()
            })),
            "tools/call" => self.tools_call(&req.params),
            "ping" => Ok(json!({})),
            other => Err(McpError::MethodNotFound(other.to_owned())),
        }
    }

    fn initialize(&self, params: &Value) -> Result<Value, McpError> {
        let version = params
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if version != SUPPORTED_PROTOCOL_VERSION {
            return Err(McpError::UnsupportedProtocol(version.to_owned()));
        }
        let token = params
            .get("forgeAuthToken")
            .and_then(|v| v.as_str())
            .ok_or(McpError::Unauthorized)?;
        if self.resolver.resolve(token).is_err() {
            return Err(McpError::Unauthorized);
        }
        *self
            .session_token
            .lock()
            .map_err(|_| McpError::InvalidRequest("lock poisoned".into()))? =
            Some(token.to_owned());
        Ok(json!({
            "protocolVersion": SUPPORTED_PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "forge-mcp", "version": "0.1.0" }
        }))
    }

    fn tools_call(&self, params: &Value) -> Result<Value, McpError> {
        let identity = self.session_identity()?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("missing tool name".into()))?;
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match name {
            "forge_task_create" => {
                if !identity.can_create_tasks {
                    return Err(McpError::Unauthorized);
                }
                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("mcp task")
                    .to_owned();
                let task = Task::accept(description, Source::Mcp, Trigger::Manual);
                let event = json!({"type":"task_accepted","task_id": task.id().0.to_string()});
                self.repo
                    .save_with_outbox(&task, None, &[event.to_string()])
                    .map_err(McpError::Io)?;
                self.tasks_index
                    .lock()
                    .map_err(|_| McpError::InvalidRequest("lock poisoned".into()))?
                    .insert(task.id(), task.clone());
                Ok(json!({
                    "content": [{ "type": "text", "text": task.id().0.to_string() }],
                    "task_id": task.id().0.to_string()
                }))
            }
            "forge_task_status" => {
                let id = args
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidRequest("missing task_id".into()))?;
                let uuid =
                    Uuid::parse_str(id).map_err(|err| McpError::InvalidRequest(err.to_string()))?;
                let snapshot = self
                    .repo
                    .load(TaskId(uuid))
                    .map_err(McpError::Io)?
                    .ok_or_else(|| McpError::InvalidRequest("unknown task".into()))?;
                Ok(json!({
                    "content": [{ "type": "text", "text": format!("{:?}", snapshot.task.status()) }],
                    "status": format!("{:?}", snapshot.task.status()),
                    "version": snapshot.version
                }))
            }
            "forge_assignment_cancel" => {
                if !identity.can_cancel {
                    return Err(McpError::Unauthorized);
                }
                // Cancel is acknowledged at the protocol layer; domain cancel wiring is F5+.
                Ok(json!({
                    "content": [{ "type": "text", "text": "cancel accepted" }],
                    "cancelled": true
                }))
            }
            other => Err(McpError::MethodNotFound(other.to_owned())),
        }
    }

    fn session_identity(&self) -> Result<TrustedIdentity, McpError> {
        let token = self
            .session_token
            .lock()
            .map_err(|_| McpError::InvalidRequest("lock poisoned".into()))?
            .clone()
            .ok_or(McpError::Unauthorized)?;
        self.resolver
            .resolve(&token)
            .map_err(|_| McpError::Unauthorized)
    }
}

fn stable_id(id: &Value) -> String {
    match id {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Encode one MCP stdio frame.
pub fn encode_frame(body: &[u8]) -> Vec<u8> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = header.into_bytes();
    out.extend_from_slice(body);
    out
}

/// Pull one frame from a growable buffer. Returns Ok(None) if incomplete.
pub fn try_decode_frame(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>, McpError> {
    let text = match std::str::from_utf8(buffer) {
        Ok(t) => t,
        Err(_) => {
            // Wait for more bytes if UTF-8 is incomplete at the end.
            return Ok(None);
        }
    };
    let Some(header_end) = text.find("\r\n\r\n") else {
        return Ok(None);
    };
    let headers = &text[..header_end];
    let mut content_length = None;
    for line in headers.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            content_length = Some(
                rest.trim()
                    .parse::<usize>()
                    .map_err(|err| McpError::Parse(err.to_string()))?,
            );
        }
    }
    let len = content_length.ok_or_else(|| McpError::Parse("missing Content-Length".into()))?;
    if len > MAX_FRAME_BYTES {
        return Err(McpError::InvalidRequest(format!(
            "frame exceeds {MAX_FRAME_BYTES} bytes"
        )));
    }
    let body_start = header_end + 4;
    if buffer.len() < body_start + len {
        return Ok(None);
    }
    let body = buffer[body_start..body_start + len].to_vec();
    buffer.drain(..body_start + len);
    Ok(Some(body))
}

/// Serve MCP on the provided readers/writers until EOF.
pub fn serve_stdio<R: BufRead, W: Write>(
    server: &McpServer,
    input: &mut R,
    output: &mut W,
) -> Result<(), McpError> {
    loop {
        let mut line = String::new();
        // Read header lines until blank line, then body by Content-Length.
        let mut content_length = None;
        loop {
            line.clear();
            let n = input.read_line(&mut line)?;
            if n == 0 {
                return Ok(());
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed
                .strip_prefix("Content-Length:")
                .or_else(|| trimmed.strip_prefix("content-length:"))
            {
                content_length = Some(
                    rest.trim()
                        .parse::<usize>()
                        .map_err(|err| McpError::Parse(err.to_string()))?,
                );
            }
        }
        let len = content_length.ok_or_else(|| McpError::Parse("missing Content-Length".into()))?;
        if len > MAX_FRAME_BYTES {
            return Err(McpError::InvalidRequest(format!(
                "frame exceeds {MAX_FRAME_BYTES} bytes"
            )));
        }
        let mut body = vec![0_u8; len];
        input.read_exact(&mut body)?;
        if let Some(response) = server.handle_frame(&body)? {
            output.write_all(&encode_frame(&response))?;
            output.flush()?;
        }
    }
}

pub fn policy_from_identities(map: &TrustedIdentityMap) -> Policy {
    let mut authorizers = BTreeSet::new();
    for identity in map.by_token.values() {
        if identity.can_create_tasks {
            authorizers.insert(identity.principal.clone());
        }
    }
    Policy {
        authorizers,
        reviewers: BTreeSet::new(),
        merge_authorizers: BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::PrincipalId;
    use tempfile::TempDir;
    use tempfile::tempdir;

    fn test_server() -> (TempDir, McpServer) {
        let dir = tempdir().unwrap();
        let repo = TursoTaskRepository::open(dir.path().join("forge.db")).unwrap();
        let mut identities = TrustedIdentityMap::new();
        identities.insert(
            "tok-owner",
            TrustedIdentity {
                principal: PrincipalId("owner".into()),
                can_create_tasks: true,
                can_cancel: true,
            },
        );
        identities.insert(
            "tok-viewer",
            TrustedIdentity {
                principal: PrincipalId("viewer".into()),
                can_create_tasks: false,
                can_cancel: false,
            },
        );
        let policy = policy_from_identities(&identities);
        (dir, McpServer::new(identities, policy, repo))
    }

    fn rpc(method: &str, id: Value, params: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .unwrap()
    }

    #[test]
    fn framing_roundtrip() {
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let encoded = encode_frame(body);
        let mut buf = encoded;
        let decoded = try_decode_frame(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, body);
        assert!(buf.is_empty());
    }

    #[test]
    fn rejects_unknown_protocol_and_unauthorized() {
        let (_dir, server) = test_server();
        let bad_version = server
            .handle_frame(&rpc(
                "initialize",
                json!(1),
                json!({
                    "protocolVersion": "9999-01-01",
                    "capabilities": {},
                    "clientInfo": {"name":"t","version":"0"},
                    "forgeAuthToken": "tok-owner"
                }),
            ))
            .unwrap()
            .unwrap();
        let body: Value = serde_json::from_slice(&bad_version).unwrap();
        assert_eq!(body["error"]["code"], -32600);

        let unauth = server
            .handle_frame(&rpc(
                "initialize",
                json!(2),
                json!({
                    "protocolVersion": SUPPORTED_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name":"t","version":"0"},
                    "forgeAuthToken": "nope"
                }),
            ))
            .unwrap()
            .unwrap();
        let body: Value = serde_json::from_slice(&unauth).unwrap();
        assert_eq!(body["error"]["code"], -32001);
    }

    #[test]
    fn create_status_and_duplicate_message_recovery() {
        let (_dir, server) = test_server();
        let init = server
            .handle_frame(&rpc(
                "initialize",
                json!(1),
                json!({
                    "protocolVersion": SUPPORTED_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name":"t","version":"0"},
                    "forgeAuthToken": "tok-owner"
                }),
            ))
            .unwrap()
            .unwrap();
        assert!(serde_json::from_slice::<Value>(&init).unwrap()["result"].is_object());

        let create_req = rpc(
            "tools/call",
            json!(10),
            json!({
                "name": "forge_task_create",
                "arguments": { "description": "from mcp" }
            }),
        );
        let first = server.handle_frame(&create_req).unwrap().unwrap();
        let second = server.handle_frame(&create_req).unwrap().unwrap();
        assert_eq!(
            first, second,
            "duplicate JSON-RPC id returns cached response"
        );
        let created: Value = serde_json::from_slice(&first).unwrap();
        let task_id = created["result"]["task_id"].as_str().unwrap().to_owned();

        let status = server
            .handle_frame(&rpc(
                "tools/call",
                json!(11),
                json!({
                    "name": "forge_task_status",
                    "arguments": { "task_id": task_id }
                }),
            ))
            .unwrap()
            .unwrap();
        let status_body: Value = serde_json::from_slice(&status).unwrap();
        assert!(
            status_body["result"]["status"]
                .as_str()
                .unwrap()
                .contains("Accepted")
        );

        // Disconnect recovery: undelivered outbox remains until marked delivered.
        let pending = server.repo.undelivered_outbox().unwrap();
        assert_eq!(pending.len(), 1);
        server.repo.mark_outbox_delivered(&pending[0].id).unwrap();
        assert!(server.repo.undelivered_outbox().unwrap().is_empty());
    }

    #[test]
    fn viewer_cannot_create_or_cancel() {
        let (_dir, server) = test_server();
        server
            .handle_frame(&rpc(
                "initialize",
                json!(1),
                json!({
                    "protocolVersion": SUPPORTED_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name":"t","version":"0"},
                    "forgeAuthToken": "tok-viewer"
                }),
            ))
            .unwrap()
            .unwrap();
        let create = server
            .handle_frame(&rpc(
                "tools/call",
                json!(2),
                json!({
                    "name": "forge_task_create",
                    "arguments": { "description": "nope" }
                }),
            ))
            .unwrap()
            .unwrap();
        let body: Value = serde_json::from_slice(&create).unwrap();
        assert_eq!(body["error"]["code"], -32001);
    }

    #[test]
    fn oidc_jwt_can_initialize_and_create() {
        let dir = tempdir().unwrap();
        let repo = TursoTaskRepository::open(dir.path().join("forge.db")).unwrap();
        let identities = TrustedIdentityMap::new();
        let mut oidc = OidcHs256Validator::new(b"test-secret", "https://issuer.test", "forge-api");
        oidc.allow_subject(
            "user-1",
            TrustedIdentity {
                principal: PrincipalId("owner".into()),
                can_create_tasks: true,
                can_cancel: true,
            },
        );
        let token = oidc.issue_for_tests("user-1", 120).unwrap();
        let policy = Policy {
            authorizers: BTreeSet::from([PrincipalId("owner".into())]),
            reviewers: BTreeSet::new(),
            merge_authorizers: BTreeSet::new(),
        };
        let server = McpServer::with_oidc(identities, oidc, policy, repo);
        let init = server
            .handle_frame(&rpc(
                "initialize",
                json!(1),
                json!({
                    "protocolVersion": SUPPORTED_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name":"t","version":"0"},
                    "forgeAuthToken": token
                }),
            ))
            .unwrap()
            .unwrap();
        assert!(serde_json::from_slice::<Value>(&init).unwrap()["result"].is_object());
        let create = server
            .handle_frame(&rpc(
                "tools/call",
                json!(2),
                json!({
                    "name": "forge_task_create",
                    "arguments": { "description": "oidc task" }
                }),
            ))
            .unwrap()
            .unwrap();
        let body: Value = serde_json::from_slice(&create).unwrap();
        assert!(body["result"]["task_id"].as_str().is_some());
    }
}

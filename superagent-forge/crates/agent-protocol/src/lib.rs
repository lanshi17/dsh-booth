use forge_domain::Assignment;
use forge_domain::TaskMandate;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;
pub const WORKER_PROTOCOL_VERSION: u32 = 1;

/// Hard caps enforced by adapters before report ingestion.
pub const MAX_STDOUT_BYTES: usize = 1 << 20; // 1 MiB
pub const MAX_STDERR_BYTES: usize = 64 << 10; // 64 KiB
pub const MAX_JSON_LINE_BYTES: usize = 256 << 10; // 256 KiB

/// Host-created request; a worker receives a mandate, never authority to expand it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub version: u32,
    pub assignment: Assignment,
    pub mandate: TaskMandate,
    pub prompt: String,
}

/// Canonical Forge worker event after adapter translation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerEvent {
    Hello {
        version: u32,
        backend: String,
        cli_version: String,
    },
    Result {
        exit_code: i32,
        summary: String,
        head_commit: String,
        changed_files: Vec<PathBuf>,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkerProtocolError {
    #[error("stdout exceeded {0} bytes")]
    StdoutLimit(usize),
    #[error("stderr exceeded {0} bytes")]
    StderrLimit(usize),
    #[error("worker wrote to stderr: {0}")]
    UnexpectedStderr(String),
    #[error("json line exceeded {0} bytes")]
    LineLimit(usize),
    #[error("invalid worker json: {0}")]
    BadJson(String),
    #[error("missing worker hello event")]
    MissingHello,
    #[error("missing worker result event")]
    MissingResult,
    #[error("worker protocol version mismatch")]
    VersionMismatch,
    #[error("worker backend kind mismatch")]
    BackendMismatch,
}

/// OMP dialect: Forge canonical JSONL on stdout.
pub fn parse_omp_jsonl(stdout: &[u8]) -> Result<(WorkerEvent, WorkerEvent), WorkerProtocolError> {
    parse_canonical_jsonl(stdout, "omp")
}

/// DSH dialect: alternate field names translated into canonical events.
pub fn parse_dsh_jsonl(stdout: &[u8]) -> Result<(WorkerEvent, WorkerEvent), WorkerProtocolError> {
    let mut hello = None;
    let mut result = None;
    for line in bounded_lines(stdout)? {
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|err| WorkerProtocolError::BadJson(err.to_string()))?;
        let protocol = value
            .get("dsh_protocol")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| WorkerProtocolError::BadJson("missing dsh_protocol".into()))?;
        if protocol != u64::from(WORKER_PROTOCOL_VERSION) {
            return Err(WorkerProtocolError::VersionMismatch);
        }
        let event = value
            .get("event")
            .and_then(|v| v.as_str())
            .ok_or_else(|| WorkerProtocolError::BadJson("missing event".into()))?;
        match event {
            "ready" => {
                let version = value
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_owned();
                hello = Some(WorkerEvent::Hello {
                    version: WORKER_PROTOCOL_VERSION,
                    backend: "dsh".to_owned(),
                    cli_version: version,
                });
            }
            "done" => {
                let exit_code = value
                    .get("code")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| WorkerProtocolError::BadJson("missing code".into()))?
                    as i32;
                let summary = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let head_commit = value
                    .get("commit")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let changed_files = value
                    .get("files")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(PathBuf::from))
                            .collect()
                    })
                    .unwrap_or_default();
                result = Some(WorkerEvent::Result {
                    exit_code,
                    summary,
                    head_commit,
                    changed_files,
                });
            }
            other => {
                return Err(WorkerProtocolError::BadJson(format!(
                    "unknown dsh event {other}"
                )));
            }
        }
    }
    Ok((
        hello.ok_or(WorkerProtocolError::MissingHello)?,
        result.ok_or(WorkerProtocolError::MissingResult)?,
    ))
}

fn parse_canonical_jsonl(
    stdout: &[u8],
    expected_backend: &str,
) -> Result<(WorkerEvent, WorkerEvent), WorkerProtocolError> {
    let mut hello = None;
    let mut result = None;
    for line in bounded_lines(stdout)? {
        let event: WorkerEvent = serde_json::from_str(line)
            .map_err(|err| WorkerProtocolError::BadJson(err.to_string()))?;
        match &event {
            WorkerEvent::Hello {
                version, backend, ..
            } => {
                if *version != WORKER_PROTOCOL_VERSION {
                    return Err(WorkerProtocolError::VersionMismatch);
                }
                if backend != expected_backend {
                    return Err(WorkerProtocolError::BackendMismatch);
                }
                hello = Some(event);
            }
            WorkerEvent::Result { .. } => result = Some(event),
        }
    }
    Ok((
        hello.ok_or(WorkerProtocolError::MissingHello)?,
        result.ok_or(WorkerProtocolError::MissingResult)?,
    ))
}

fn bounded_lines(stdout: &[u8]) -> Result<Vec<&str>, WorkerProtocolError> {
    let text =
        std::str::from_utf8(stdout).map_err(|err| WorkerProtocolError::BadJson(err.to_string()))?;
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_JSON_LINE_BYTES {
            return Err(WorkerProtocolError::LineLimit(MAX_JSON_LINE_BYTES));
        }
        lines.push(line);
    }
    Ok(lines)
}

pub fn reject_stderr(stderr: &[u8]) -> Result<(), WorkerProtocolError> {
    if stderr.is_empty() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let preview: String = trimmed.chars().take(200).collect();
    Err(WorkerProtocolError::UnexpectedStderr(preview))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_omp_and_rejects_bad_json() {
        let ok = concat!(
            r#"{"type":"hello","version":1,"backend":"omp","cli_version":"1.0.0"}"#,
            "\n",
            r#"{"type":"result","exit_code":0,"summary":"done","head_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","changed_files":[]}"#,
            "\n",
        );
        let (hello, result) = parse_omp_jsonl(ok.as_bytes()).unwrap();
        assert!(matches!(hello, WorkerEvent::Hello { .. }));
        assert!(matches!(result, WorkerEvent::Result { exit_code: 0, .. }));
        assert!(matches!(
            parse_omp_jsonl(b"{not-json}\n"),
            Err(WorkerProtocolError::BadJson(_))
        ));
    }

    #[test]
    fn translates_dsh_dialect() {
        let body = concat!(
            r#"{"dsh_protocol":1,"event":"ready","version":"1.0.0-forge"}"#,
            "\n",
            r#"{"dsh_protocol":1,"event":"done","code":0,"message":"ok","commit":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","files":["a.rs"]}"#,
            "\n",
        );
        let (hello, result) = parse_dsh_jsonl(body.as_bytes()).unwrap();
        match hello {
            WorkerEvent::Hello { backend, .. } => assert_eq!(backend, "dsh"),
            _ => panic!("expected hello"),
        }
        match result {
            WorkerEvent::Result {
                exit_code,
                changed_files,
                ..
            } => {
                assert_eq!(exit_code, 0);
                assert_eq!(changed_files, vec![PathBuf::from("a.rs")]);
            }
            _ => panic!("expected result"),
        }
    }
}

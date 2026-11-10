//! Booth is the desk. Forge is the control plane that runs DSH and OMP workers.
//!
//! `booth forge send` speaks Forge's MCP stdio protocol and creates one task. The worker
//! behind that task is Forge's choice: DSH or OMP, in its own worktree.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Create one Forge task and return the MCP result.
pub fn send(description: &str) -> Result<Value, String> {
    let description = description.trim();
    if description.is_empty() {
        return Err("a forge task needs text".into());
    }
    call("forge_task_create", json!({ "description": description }))
}

/// Read one Forge task.
pub fn status(task_id: &str) -> Result<Value, String> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err("a forge task id is required".into());
    }
    call("forge_task_status", json!({ "task_id": task_id }))
}

fn call(name: &str, arguments: Value) -> Result<Value, String> {
    let mut child = forge_command()?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start forge: {error}"))?;
    let mut stdin = child.stdin.take().ok_or("forge stdin is closed")?;
    let mut stdout = child.stdout.take().ok_or("forge stdout is closed")?;
    write_frame(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "booth", "version": env!("CARGO_PKG_VERSION") },
                "forgeAuthToken": token()
            }
        }),
    )?;
    let _init = read_frame(&mut stdout)?;
    write_frame(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }),
    )?;
    let response = read_frame(&mut stdout)?;
    drop(stdin);
    let _ = child.wait();
    if let Some(error) = response.get("error") {
        return Err(error.to_string());
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| format!("forge returned no result: {response}"))
}

fn token() -> String {
    std::env::var("FORGE_BOOTH_TOKEN").unwrap_or_else(|_| "booth".into())
}

fn forge_command() -> Result<Command, String> {
    let mut command = if let Ok(bin) = std::env::var("FORGE_BIN") {
        let mut command = Command::new(bin);
        command.arg("mcp-stdio");
        command
    } else {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("codex-forge/superagent-forge/Cargo.toml");
        if !manifest.is_file() {
            return Err(
                "codex-forge/superagent-forge is not in this checkout; set FORGE_BIN".into(),
            );
        }
        let mut command = Command::new("cargo");
        command
            .args([
                "run",
                "--quiet",
                "--locked",
                "-p",
                "forge-cli",
                "--manifest-path",
            ])
            .arg(manifest)
            .arg("--")
            .arg("mcp-stdio");
        command
    };
    if std::env::var_os("FORGE_MCP_DB").is_none() {
        let db = std::env::temp_dir().join("booth-forge.sqlite");
        command.env("FORGE_MCP_DB", db);
    }
    if std::env::var_os("FORGE_MCP_TOKENS").is_none() {
        command.env(
            "FORGE_MCP_TOKENS",
            format!("{}=operator:create,cancel", token()),
        );
    }
    Ok(command)
}

fn write_frame(writer: &mut impl Write, body: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", bytes.len()).as_bytes())
        .and_then(|_| writer.write_all(&bytes))
        .and_then(|_| writer.flush())
        .map_err(|error| error.to_string())
}

fn read_frame(reader: &mut impl Read) -> Result<Value, String> {
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        reader
            .read_exact(&mut byte)
            .map_err(|error| format!("forge closed before a frame: {error}"))?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() > 8192 {
            return Err("forge frame header is too long".into());
        }
    }
    let text = String::from_utf8_lossy(&header);
    let mut length = None;
    for line in text.split("\r\n") {
        let Some(value) = line
            .trim()
            .strip_prefix("Content-Length:")
            .or_else(|| line.trim().strip_prefix("Content-Length:"))
        else {
            continue;
        };
        length = value.trim().parse::<usize>().ok();
    }
    let length = length.ok_or("forge frame has no Content-Length")?;
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("forge frame body is short: {error}"))?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_round_trips() {
        let body = json!({"jsonrpc": "2.0", "id": 2, "result": {"task_id": "t-1"}});
        let bytes = {
            let encoded = serde_json::to_vec(&body).unwrap();
            let mut out = format!("Content-Length: {}\r\n\r\n", encoded.len()).into_bytes();
            out.extend(encoded);
            out
        };
        let value = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(value["result"]["task_id"], "t-1");
    }
}

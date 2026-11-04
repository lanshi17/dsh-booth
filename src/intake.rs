//! The console's front door: instructions typed here enter the agent through the booth Source.
//!
//! dsh-superagent accepts work only from a Source. The bundle in `local-source/` is one whose
//! stream is a file: `booth-inbox.jsonl` for what the operator types, `booth-outbox.jsonl` for what
//! the agent acknowledges and replies. Both live beside the service's state, so the console and the
//! Source read the same paths without a protocol.

use crate::api::Api;
use crate::format::L;
use crate::oauth;
use crate::provider::Store;
use crate::service;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

const GREEN: Color = Color::Rgb(0x5f, 0xd7, 0x87);
const RED: Color = Color::Rgb(0xff, 0x5f, 0x5f);
const DIM: Color = Color::Rgb(0xa8, 0xa8, 0xa8);
const CYAN: Color = Color::Rgb(0x00, 0xaf, 0xff);
/// A restart after a settings change, including the model catalog, takes a few seconds.
const RESTART_TIMEOUT: Duration = Duration::from_secs(90);

fn inbox(cfg: &service::Config) -> PathBuf {
    cfg.state_dir.join("booth-inbox.jsonl")
}

fn outbox(cfg: &service::Config) -> PathBuf {
    cfg.state_dir.join("booth-outbox.jsonl")
}

/// One agent message the console shows: an acknowledgement, a reply, or a notice.
pub struct Reply {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub ticket: Option<String>,
}

/// Appends one instruction for the Source and returns its id.
pub fn send(cfg: &service::Config, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("an instruction needs text".into());
    }
    std::fs::create_dir_all(&cfg.state_dir)
        .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
    let id = oauth::uuid_v4();
    let line = json!({ "id": id, "text": text, "sender": "operator" });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(inbox(cfg))
        .map_err(|error| format!("cannot write {}: {error}", inbox(cfg).display()))?;
    file.write_all(format!("{line}\n").as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", inbox(cfg).display()))?;
    Ok(id)
}

/// Reads every agent message the console has not shown yet; the caller decides which are new by id.
pub fn replies(cfg: &service::Config) -> Vec<Reply> {
    let text = match std::fs::read_to_string(outbox(cfg)) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|value| {
            Some(Reply {
                id: value.get("id")?.as_str()?.to_owned(),
                kind: value
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("reply")
                    .to_owned(),
                text: value.get("text")?.as_str()?.to_owned(),
                ticket: value
                    .get("ticket")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// The `/provider` view: which provider and model the agent runs with.
pub fn summary(store: &Store) -> Vec<L> {
    let mut lines = Vec::new();
    match store.current() {
        Some(current) => {
            lines.push(field(
                "provider",
                &format!("{} · {}", current.id, current.model),
            ));
            lines.push(field("base", &current.base));
            lines.push(field("api", &current.api));
            lines.push(field(
                "key",
                &format!(
                    "{} (stored in provider.json, 0600)",
                    crate::provider::key_env(&current.id)
                ),
            ));
        }
        None if oauth::account_signed_in() => {
            lines.push(field(
                "provider",
                "DeepSeek account · profile default model",
            ));
        }
        None => lines.push(muted("no provider yet — /login adds one")),
    }
    let others: Vec<String> = store
        .providers
        .iter()
        .filter(|item| {
            store
                .current()
                .map(|current| current.id != item.id)
                .unwrap_or(true)
        })
        .map(|item| format!("{}/{}", item.id, item.model))
        .collect();
    if !others.is_empty() {
        lines.push(field("also", &others.join(", ")));
    }
    lines.push(field("switch", "/model picks one, or adds a provider"));
    lines
}

/// `/setup`: names the repository, allows the people who may ask for work, and turns on the console's Source.
pub fn setup(api: &Api, cfg: &service::Config, argument: &str) -> Result<Vec<L>, String> {
    let mut words = argument.split_whitespace();
    let repository = match words.next() {
        Some(value) if value.contains('/') => value.to_owned(),
        Some(other) => {
            return Err(format!(
                "{other} is not owner/name; use /setup owner/repo [directory]"
            ))
        }
        None => infer_repository().ok_or(
            "name the repository: /setup owner/repo — run it inside a checkout to infer it",
        )?,
    };
    let directory = words.next().map(str::to_owned);

    let login = gh_login()?;
    let settings = api.settings()?;
    let source_block = |id: &str| -> Result<(Value, u64), String> {
        let source = settings
            .get("sources")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find(|item| item.get("id") == Some(&json!(id))))
            .ok_or(format!(
                "the Source `{id}` is not installed; run `booth init` to install the booth console Source"
            ))?;
        Ok((
            source.get("value").cloned().unwrap_or_else(|| json!({})),
            source.get("revision").and_then(Value::as_u64).unwrap_or(0),
        ))
    };
    let block = |name: &str| -> Result<(Value, u64), String> {
        let entry = settings
            .get("blocks")
            .and_then(|blocks| blocks.get(name))
            .ok_or(format!("the settings block `{name}` is missing"))?;
        Ok((
            entry.get("value").cloned().unwrap_or_else(|| json!({})),
            entry.get("revision").and_then(Value::as_u64).unwrap_or(0),
        ))
    };

    // The GitHub Source's reads use `gh api --slurp`, which older CLIs reject; the agent's own
    // pushes and pull requests work without it, so an old `gh` turns only that door off.
    let reads = gh_supports_slurp();
    let (mut github, revision) = source_block("github")?;
    github["enabled"] = json!(reads);
    github["config"]["repository"] = json!(repository);
    github["config"]["cli"] = json!("gh");
    github["config"]["observeIssues"] = json!(true);
    api.save("source:github", &github, revision)?;
    wait_ready(api)?;

    let (mut local, revision) = source_block("local")?;
    local["enabled"] = json!(true);
    api.save("source:local", &local, revision)?;
    wait_ready(api)?;

    let (mut policy, revision) = block("policy")?;
    policy["sources"]["github"]["allowed"] = json!([{ "id": login }]);
    policy["sources"]["local"]["allowed"] = json!([{ "id": "operator", "name": "booth" }]);
    api.save("policy", &policy, revision)?;
    wait_ready(api)?;

    if let Some(ref directory) = directory {
        let (mut executor, revision) = block("executor")?;
        executor["directory"] = json!(directory);
        api.save("executor", &executor, revision)?;
        wait_ready(api)?;
    }

    let state = api.state()?;
    let mut lines = vec![Line::from(vec![
        Span::styled("✓ ", Style::default().fg(GREEN)),
        Span::styled(
            format!("repository {repository} · agent @{login}"),
            Style::default().fg(GREEN),
        ),
    ])];
    if let Some(problem) = &state.setup.problem {
        lines.push(Line::from(Span::styled(
            format!("! setup still pending: {}", problem.detail),
            Style::default().fg(RED),
        )));
        return Ok(lines);
    }
    lines.push(Line::from(vec![
        Span::styled("✓ ", Style::default().fg(GREEN)),
        Span::styled(
            "console instructions allowed · type an instruction, without a slash",
            Style::default().fg(GREEN),
        ),
    ]));
    if !reads {
        let version = gh_version();
        lines.push(Line::from(Span::styled(
            format!(
                "! GitHub Issues are off: gh {version} lacks `gh api --slurp`; upgrade to gh 2.57+ and run /setup again"
            ),
            Style::default().fg(RED),
        )));
    }
    if directory.is_none() {
        lines.push(muted(
            "workers get no repository checkout; add one with /setup owner/repo <directory>",
        ));
    }
    lines.push(muted(&format!(
        "inbox {} · one instruction at a time",
        inbox(cfg).display()
    )));
    Ok(lines)
}

fn wait_ready(api: &Api) -> Result<(), String> {
    let deadline = Instant::now() + RESTART_TIMEOUT;
    loop {
        if api.state().is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!(
                "the service did not answer within {}s after the settings change",
                RESTART_TIMEOUT.as_secs()
            ));
        }
        sleep(Duration::from_millis(500));
    }
}

/// Reads `owner/name` from the checkout the console runs in.
fn infer_repository() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    repository_from_url(String::from_utf8_lossy(&output.stdout).trim())
}

/// `owner/name` from the GitHub forms git accepts as an `origin`; other hosts yield none.
fn repository_from_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
    let name = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = name.split('/');
    let owner = parts.next()?;
    let repository = parts.next()?;
    if owner.is_empty() || repository.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repository}"))
}

/// Whether the installed `gh` has the flag dsh-superagent's GitHub reads pass.
fn gh_supports_slurp() -> bool {
    Command::new("gh")
        .args(["api", "--help"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains("--slurp"))
        .unwrap_or(false)
}

fn gh_version() -> String {
    Command::new("gh")
        .args(["--version"])
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .nth(2)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "?".into())
}

fn gh_login() -> Result<String, String> {
    let output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .map_err(|error| format!("cannot run gh: {error}; install the GitHub CLI"))?;
    if !output.status.success() {
        return Err(format!(
            "gh is not signed in: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let login = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if login.is_empty() {
        return Err("gh did not report an account".into());
    }
    Ok(login)
}

fn field(name: &str, value: &str) -> L {
    Line::from(vec![
        Span::styled(format!("{name:<10}"), Style::default().fg(CYAN)),
        Span::styled(value.to_owned(), Style::default().fg(DIM)),
    ])
}

fn muted(text: &str) -> L {
    Line::from(Span::styled(text.to_owned(), Style::default().fg(DIM)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config(dir: PathBuf) -> service::Config {
        service::Config {
            profile: "test".into(),
            port: 0,
            dsh_home: dir.clone(),
            state_dir: dir.clone(),
            bundle: dir,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("booth-{name}-{}", oauth::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn one_instruction_per_line() {
        let dir = scratch("send");
        let cfg = config(dir.clone());
        send(&cfg, "first").unwrap();
        send(&cfg, "second").unwrap();
        let text = std::fs::read_to_string(inbox(&cfg)).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["text"], "first");
        assert_eq!(lines[1]["text"], "second");
        assert!(lines[0]["id"].as_str().unwrap().len() >= 16);
        assert!(send(&cfg, "  ").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_agent_messages_from_the_outbox() {
        let dir = scratch("replies");
        let cfg = config(dir.clone());
        std::fs::write(
            outbox(&cfg),
            concat!(
                "{\"id\":\"a\",\"kind\":\"acknowledgement\",\"text\":\"收到\",\"ticket\":\"t-1\"}\n",
                "not json\n",
                "{\"id\":\"b\",\"kind\":\"reply\",\"text\":\"done\"}\n",
            ),
        )
        .unwrap();
        let messages = replies(&cfg);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].ticket.as_deref(), Some("t-1"));
        assert_eq!(messages[1].kind, "reply");
        assert!(replies(&config(dir.join("missing"))).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_the_github_remote_forms() {
        assert_eq!(
            repository_from_url("git@github.com:lanshi17/dsh-booth.git").as_deref(),
            Some("lanshi17/dsh-booth")
        );
        assert_eq!(
            repository_from_url("https://github.com/o/r").as_deref(),
            Some("o/r")
        );
        assert_eq!(
            repository_from_url("ssh://git@github.com/o/r.git").as_deref(),
            Some("o/r")
        );
        assert_eq!(repository_from_url("git@gitlab.com:o/r.git"), None);
        assert_eq!(repository_from_url("https://github.com/owner"), None);
    }
}

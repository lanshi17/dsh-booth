//! The console's front door: instructions typed here enter the agent through the booth Source.
//!
//! dsh-superagent accepts work only from a Source. The bundle in `local-source/` is one whose
//! stream is a file: `booth-inbox.jsonl` for what the operator types, `booth-outbox.jsonl` for what
//! the agent acknowledges and replies. Both live beside the service's state, so the console and the
//! Source read the same paths without a protocol.

use crate::api::Api;
use crate::format::L;
use crate::oauth;
use crate::provider::Catalog;
use crate::service;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
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

/// The ticket the console was continuing, so a restart resumes it instead of starting over.
pub fn follow(cfg: &service::Config) -> Option<(String, String)> {
    let text = std::fs::read_to_string(cfg.state_dir.join("booth-follow.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let ticket = value.get("ticket")?.as_str()?.to_owned();
    let thread = value.get("thread")?.as_str()?.to_owned();
    Some((ticket, thread))
}

/// Records the ticket the console continues; `None` forgets it.
pub fn set_follow(cfg: &service::Config, target: Option<(&str, &str)>) {
    let path = cfg.state_dir.join("booth-follow.json");
    match target {
        Some((ticket, thread)) => {
            let _ = std::fs::write(
                &path,
                json!({ "ticket": ticket, "thread": thread }).to_string(),
            );
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// One agent message the console shows: an acknowledgement, a reply, or a notice.
pub struct Reply {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub ticket: Option<String>,
}

/// Appends one instruction for the Source and returns its id; `thread` continues a ticket's thread.
pub fn send(cfg: &service::Config, text: &str, thread: Option<&str>) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("an instruction needs text".into());
    }
    std::fs::create_dir_all(&cfg.state_dir)
        .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
    let id = oauth::uuid_v4();
    let mut line = json!({ "id": id, "text": text, "sender": "operator" });
    if let Some(thread) = thread {
        line["thread"] = json!(thread);
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(inbox(cfg))
        .map_err(|error| format!("cannot write {}: {error}", inbox(cfg).display()))?;
    file.write_all(format!("{line}\n").as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", inbox(cfg).display()))?;
    Ok(id)
}

/// How many bytes before the offset are kept to recognize a rewritten file.
const WITNESS: usize = 32;

/// Where the console's last outbox read stopped, and the bytes it left behind.
///
/// Length alone cannot tell an append from a rewrite: a replaced file can be longer than the
/// old one, so an offset kept from the old file would land in the middle of the new one. The
/// bytes just before the offset are the witness — an append leaves them in place, a rewrite
/// does not — so the next read knows whether to continue or start over.
#[derive(Default, Clone)]
pub struct Tail {
    offset: u64,
    witness: Vec<u8>,
}

/// How much of the outbox is read to find its last complete line when skipping the history.
const TAIL_WINDOW: u64 = 8 * 1024;

impl Tail {
    pub fn fresh() -> Self {
        Self::default()
    }

    /// Forgets the offset, so the next read starts at the beginning of the file.
    fn restart(&mut self) {
        self.offset = 0;
        self.witness.clear();
    }

    /// The last few bytes ending at `offset`, or `None` when they cannot be read.
    fn bytes_before(file: &mut std::fs::File, offset: u64) -> Option<Vec<u8>> {
        let length = (offset as usize).min(WITNESS);
        let mut buffer = vec![0u8; length];
        file.seek(SeekFrom::Start(offset - length as u64)).ok()?;
        file.read_exact(&mut buffer).ok()?;
        Some(buffer)
    }

    /// Whether the file still holds the bytes this tail stopped after.
    ///
    /// A tail positioned without reading (a fresh console that seeks to the end) has no witness
    /// yet; its offset is trusted, and the next read records the bytes it stopped after.
    fn still_there(&self, file: &mut std::fs::File) -> bool {
        self.witness.is_empty()
            || Self::bytes_before(file, self.offset)
                .map(|bytes| bytes == self.witness)
                .unwrap_or(false)
    }
}

/// Reads the messages appended since `tail`, leaving it after the last complete line.
///
/// The console polls every two seconds and the outbox only grows, so rereading it whole turns
/// each poll into O(history) work (SPEC defect C). A file that shrank, or that no longer holds
/// the bytes the last read stopped after, was rewritten: the read restarts.
pub fn replies_from(cfg: &service::Config, tail: &mut Tail) -> Vec<Reply> {
    let path = outbox(cfg);
    let Ok(meta) = std::fs::metadata(&path) else {
        return Vec::new();
    };
    let size = meta.len();
    if size < tail.offset {
        tail.restart();
    }
    let Ok(mut file) = std::fs::File::open(&path) else {
        return Vec::new();
    };
    if tail.offset > 0 && !tail.still_there(&mut file) {
        tail.restart();
    }
    if size == tail.offset {
        return Vec::new();
    }
    if file.seek(SeekFrom::Start(tail.offset)).is_err() {
        return Vec::new();
    }
    let mut raw = Vec::new();
    if file.read_to_end(&mut raw).is_err() {
        return Vec::new();
    }
    // Only whole lines count: a half-written record waits for the next poll.
    let Some(end) = raw.iter().rposition(|byte| *byte == b'\n').map(|at| at + 1) else {
        return Vec::new();
    };
    tail.offset += end as u64;
    match Tail::bytes_before(&mut file, tail.offset) {
        Some(bytes) => tail.witness = bytes,
        // Without a witness the next read cannot tell an append from a rewrite, so it starts
        // over rather than risk reading from the middle.
        None => tail.restart(),
    }
    parse_replies(&String::from_utf8_lossy(&raw[..end]))
}

fn parse_replies(text: &str) -> Vec<Reply> {
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

/// The Eval modes, in the order the choice offers them, with what each one admits.
pub const EVAL_MODES: &[(&str, &str)] = &[
    (
        "off",
        "only a listed person's explicit request becomes a ticket",
    ),
    (
        "observe",
        "inferred candidates are recorded, never accepted on their own",
    ),
    (
        "auto",
        "inferred candidates may be accepted without a person",
    ),
];

/// What `/eval <argument>` resolves to, given the mode in force.
///
/// Split out so the choice is testable without a service: `Ok(None)` means "show the modes",
/// `Ok(Some(mode))` means "switch to it", and the error names what was not a mode.
pub fn resolve_mode(argument: &str, current: &str) -> Result<Option<&'static str>, String> {
    let wanted = argument.trim().to_ascii_lowercase();
    if wanted.is_empty() {
        return Ok(None);
    }
    match EVAL_MODES.iter().find(|(mode, _)| *mode == wanted) {
        Some((mode, _)) if *mode == current => Ok(None),
        Some((mode, _)) => Ok(Some(mode)),
        None => Err(format!("{wanted} is not a mode; use off, observe or auto")),
    }
}

/// `/eval`: reads or changes the accept mode, the one setting that decides how much work
/// starts without a person.
pub fn eval(api: &Api, argument: &str) -> Result<Vec<L>, String> {
    let wanted = argument.trim().to_ascii_lowercase();
    let (value, _) = api.block("eval")?;
    let current = value
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("observe")
        .to_owned();
    if wanted.is_empty() {
        let mut lines = vec![section("eval")];
        for (mode, description) in EVAL_MODES {
            lines.push(Line::from(vec![
                Span::styled(
                    if *mode == current { "▶ " } else { "  " },
                    Style::default().fg(CYAN),
                ),
                Span::styled(format!("{mode:<9}"), Style::default().fg(Color::White)),
                Span::styled(*description, Style::default().fg(DIM)),
            ]));
        }
        lines.push(blank());
        lines.push(muted(&format!(
            "timeout {}s · /eval off | observe | auto changes the mode",
            value
                .get("timeoutSeconds")
                .and_then(Value::as_u64)
                .unwrap_or(120)
        )));
        return Ok(lines);
    }
    let Some(mode) = resolve_mode(&wanted, &current)? else {
        return Ok(vec![done(format!("eval already {current}"))]);
    };
    api.set("eval", &|block| block["mode"] = json!(mode))?;
    let mut lines = vec![done(format!("eval {current} → {mode}"))];
    if mode == "auto" {
        lines.push(muted(
            "inferred candidates can now become tickets without a person; the policy rules still decide which",
        ));
    }
    Ok(lines)
}

/// The lines typed in earlier console sessions, oldest first, capped at 500.
pub fn history(cfg: &service::Config) -> Vec<String> {
    let text = std::fs::read_to_string(cfg.state_dir.join("booth-history.txt")).unwrap_or_default();
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    lines[lines.len().saturating_sub(500)..].to_vec()
}

/// Appends a submitted line, rewriting the file once it holds twice the cap.
pub fn remember(cfg: &service::Config, line: &str) {
    let path = cfg.state_dir.join("booth-history.txt");
    let _ = std::fs::create_dir_all(&cfg.state_dir);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let count = existing.lines().count();
    if count >= 1000 {
        let kept: Vec<&str> = existing.lines().skip(count - 500).collect();
        let _ = std::fs::write(&path, format!("{}\n", kept.join("\n")));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{}", line.replace('\n', " "));
    }
}

/// The `/provider` view: which provider and model the agent runs with.
pub fn summary(catalog: &Catalog) -> Vec<L> {
    let mut lines = Vec::new();
    match &catalog.default {
        Some((provider, model)) => {
            lines.push(field("model", &format!("{provider} · {model}")));
            // The display name only adds something when it differs from the id.
            if let Some(choice) = catalog.current() {
                if choice.model_name != choice.model {
                    lines.push(field("name", &choice.model_name));
                }
            }
        }
        None if oauth::account_signed_in() => {
            lines.push(field("model", "the profile's own default (signed in)"));
        }
        None => lines.push(muted("no model yet — /login adds one")),
    }
    let providers = catalog.providers();
    if !providers.is_empty() {
        lines.push(field(
            "providers",
            &providers
                .iter()
                .map(|(id, name)| format!("{id} ({name})"))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    let stored: Vec<String> = catalog
        .providers()
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| crate::provider::has_ref(&crate::provider::key_ref(id)))
        .collect();
    if !stored.is_empty() {
        lines.push(field("keys", &format!("stored for {}", stored.join(", "))));
    }
    lines.push(field("source", "the DSH profile · no booth-side file"));
    lines.push(field("switch", "/model picks one, or adds a provider"));
    lines
}

/// `/setup`: names the repository, allows the people who may ask for work, and turns on the console's Source.
pub fn setup(api: &Api, cfg: &service::Config, argument: &str) -> Result<Vec<L>, String> {
    // Only the first word names the repository; everything after it is the directory, so a
    // path with spaces is kept whole instead of being cut at its first space.
    let argument = argument.trim();
    let (repository, directory) = match argument.split_once(char::is_whitespace) {
        Some((first, rest)) => (first, rest.trim()),
        None => (argument, ""),
    };
    let repository = match repository {
        "" => infer_repository().ok_or(
            "name the repository: /setup owner/repo — run it inside a checkout to infer it",
        )?,
        value if value.contains('/') => value.to_owned(),
        other => {
            return Err(format!(
                "{other} is not owner/name; use /setup owner/repo [directory]"
            ))
        }
    };
    let directory = (!directory.is_empty()).then(|| directory.to_owned());

    // A newer gh unlocks the GitHub front door; the system one still lets the agent push.
    // `BOOTH_NO_GH_INSTALL` keeps a test or an offline machine from a 13 MB download.
    let mut cli = std::env::var("BOOTH_GH").unwrap_or_else(|_| "gh".to_owned());
    let mut cli_note: Option<(String, bool)> = None;
    let download = std::env::var_os("BOOTH_NO_GH_INSTALL").is_none();
    let chosen = if std::env::var_os("BOOTH_GH").is_some() {
        Ok(Some(cli.clone()))
    } else {
        crate::gh::ensure(cfg, download)
    };
    match chosen {
        Ok(Some(path)) => {
            let version = crate::gh::version(std::path::Path::new(&path)).unwrap_or_default();
            cli = path.clone();
            cli_note = Some((format!("gh {version} → {path}"), true));
        }
        Ok(None) => {}
        Err(error) => cli_note = Some((error, false)),
    }
    let reads = crate::gh::capable(std::path::Path::new(&cli));
    let login = gh_login(&cli)?;
    // A fresh service answers /settings before every Source plugin has registered; wait for the
    // two this setup writes instead of reporting them missing.
    let installed = |settings: &Value, id: &str| {
        settings
            .get("sources")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find(|item| item.get("id") == Some(&json!(id))))
            .map(|source| {
                source
                    .get("installed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    };
    // The same knob as `wait_ready`: a caller can shorten both waits, and a test does.
    let registration = wait_budget().min(Duration::from_secs(30));
    let deadline = Instant::now() + registration;
    let settings = loop {
        // A restart mid-read answers with a transport error or a 5xx; that is the same "not
        // back yet" as an empty registry, so the loop retries instead of failing the setup.
        let settings = match api.settings() {
            Ok(settings) => settings,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                sleep(Duration::from_millis(500));
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        if installed(&settings, "github") && installed(&settings, "local") {
            break settings;
        }
        if Instant::now() > deadline {
            return Err(
                "the booth console Source is not installed; run `booth init` to install it".into(),
            );
        }
        sleep(Duration::from_millis(500));
    };
    let previous = settings
        .get("sources")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id") == Some(&json!("github")))
        })
        .and_then(|source| source.get("value"))
        .and_then(|value| value.get("config"))
        .and_then(|config| config.get("repository"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    // Saving a Source restarts dsh-superagent, and its registry is briefly empty: re-read the
    // revision and retry while the Source is not yet back instead of failing the whole setup.
    let save = |block: &str, change: &dyn Fn(&mut Value)| -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let settings = api.settings()?;
            let source = block.strip_prefix("source:");
            let entry = match source {
                Some(id) => settings
                    .get("sources")
                    .and_then(Value::as_array)
                    .and_then(|items| items.iter().find(|item| item.get("id") == Some(&json!(id)))),
                None => settings.get("blocks").and_then(|blocks| blocks.get(block)),
            };
            let Some(entry) = entry else {
                // The restart between two saves unregisters Sources for a moment.
                if Instant::now() > deadline {
                    return Err(match source {
                        Some(id) => format!(
                            "the Source `{id}` is not installed; run `booth init` to install the console Source"
                        ),
                        None => format!("the settings block `{block}` is missing"),
                    });
                }
                sleep(Duration::from_millis(600));
                continue;
            };
            let mut value = entry.get("value").cloned().unwrap_or_else(|| json!({}));
            let revision = entry.get("revision").and_then(Value::as_u64).unwrap_or(0);
            change(&mut value);
            match api.save(block, &value, revision) {
                Ok(_) => return wait_ready(api),
                Err(crate::api::ApiError::Status(code, message))
                    if ((code == 404 && message.contains("Source not found")) || code == 409)
                        && Instant::now() < deadline =>
                {
                    sleep(Duration::from_millis(600));
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    };

    save("source:github", &|github| {
        github["enabled"] = json!(reads);
        github["config"]["repository"] = json!(repository);
        github["config"]["cli"] = json!(cli);
        github["config"]["observeIssues"] = json!(true);
    })?;

    save("source:local", &|local| local["enabled"] = json!(true))?;

    save("policy", &|policy| {
        policy["sources"]["github"]["allowed"] = json!([{ "id": login }]);
        policy["sources"]["local"]["allowed"] = json!([{ "id": "operator", "name": "booth" }]);
    })?;

    if let Some(ref directory) = directory {
        save("executor", &|executor| {
            executor["directory"] = json!(directory)
        })?;
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
    if let Some((note, installed)) = cli_note {
        lines.push(Line::from(Span::styled(
            if installed {
                format!("✓ {note}")
            } else {
                format!("! GitHub front door unavailable: {note}")
            },
            Style::default().fg(if installed { GREEN } else { RED }),
        )));
    }
    if !reads {
        lines.push(Line::from(Span::styled(
            "! GitHub Issues are off: the system gh lacks `gh api --slurp`; run `booth gh --install`, then /setup again",
            Style::default().fg(RED),
        )));
    }
    let work = directory
        .clone()
        .unwrap_or_else(|| cfg.state_dir.join("work").display().to_string());
    lines.push(muted(&format!(
        "ticket directories {work} · panel → Execution gives each one a checkout"
    )));
    if !previous.is_empty() && previous != repository {
        lines.push(Line::from(Span::styled(
            format!(
                "! repository changed from {previous}; its tickets keep threads that no longer read — a different repository deserves a fresh BOOTH_STATE_DIR"
            ),
            Style::default().fg(RED),
        )));
    }
    lines.push(muted(&format!(
        "inbox {} · one instruction at a time",
        inbox(cfg).display()
    )));
    Ok(lines)
}

/// The pause before the single retry of a CLI the kernel refused to execute.
const RETRY_PAUSE: Duration = Duration::from_millis(50);

/// How long a settings change may take to answer; `BOOTH_WAIT_SECONDS` overrides it.
fn wait_budget() -> Duration {
    std::env::var("BOOTH_WAIT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(RESTART_TIMEOUT)
}

fn wait_ready(api: &Api) -> Result<(), String> {
    let budget = wait_budget();
    let deadline = Instant::now() + budget;
    loop {
        if api.state().is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!(
                "the service did not answer within {}s after the settings change",
                budget.as_secs()
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

fn gh_login(program: &str) -> Result<String, String> {
    // At most two attempts, as a range rather than a counter: the loop is bounded by its own
    // shape, so no rewrite of a guard can make it spin.
    let mut first: Option<std::io::Error> = None;
    for attempt in 0..2 {
        match Command::new(program)
            .args(["api", "user", "--jq", ".login"])
            .output()
        {
            Ok(output) => return account_of(output),
            Err(error)
                if attempt == 0 && error.kind() == std::io::ErrorKind::ExecutableFileBusy =>
            {
                // A CLI written moments ago can still be open elsewhere in the process, and the
                // kernel then refuses to execute it; the pause covers that window.
                first = Some(error);
                sleep(RETRY_PAUSE);
            }
            Err(error) => {
                // The second attempt, or a failure that was never transient.
                return Err(match first {
                    Some(_) => format!("cannot run gh: {error}; the CLI stayed busy"),
                    None => format!("cannot run gh: {error}; install the GitHub CLI"),
                });
            }
        }
    }
    Err("cannot run gh: it never started".into())
}

/// The account a successful `gh api user` printed, or why there is none.
fn account_of(output: std::process::Output) -> Result<String, String> {
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

fn section(text: &str) -> L {
    Line::from(Span::styled(
        text.to_owned(),
        Style::default().fg(Color::DarkGray),
    ))
}

fn blank() -> L {
    Line::from("")
}

fn done(text: impl Into<String>) -> L {
    Line::from(Span::styled(
        format!("✓ {}", text.into()),
        Style::default().fg(GREEN),
    ))
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
        send(&cfg, "first", None).unwrap();
        send(&cfg, "second", Some("t-1")).unwrap();
        let text = std::fs::read_to_string(inbox(&cfg)).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["text"], "first");
        assert_eq!(lines[1]["text"], "second");
        assert_eq!(lines[1]["thread"], "t-1");
        assert!(lines[0]["id"].as_str().unwrap().len() >= 16);
        assert!(send(&cfg, "  ", None).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_only_what_was_appended() {
        // SPEC scenario "文件里有 5000 条旧消息、新增 1 条".
        let dir = scratch("tail");
        let cfg = config(dir.clone());
        let mut body = String::new();
        for index in 0..5000 {
            body.push_str(&format!(
                "{{\"id\":\"old-{index}\",\"kind\":\"reply\",\"text\":\"x\"}}\n"
            ));
        }
        std::fs::write(outbox(&cfg), &body).unwrap();
        let mut tail = Tail::fresh();
        // The first read sees the history once, then the tail sits at the end.
        assert_eq!(replies_from(&cfg, &mut tail).len(), 5000);
        assert_eq!(tail.offset, body.len() as u64);
        assert!(replies_from(&cfg, &mut tail).is_empty());

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(outbox(&cfg))
            .unwrap();
        file.write_all(b"{\"id\":\"new-1\",\"kind\":\"reply\",\"text\":\"hello\"}\n")
            .unwrap();
        let fresh = replies_from(&cfg, &mut tail);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].id, "new-1");
        assert_eq!(fresh[0].text, "hello");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restart_skips_the_outbox_and_reads_only_what_arrives() {
        // The console starts with an empty transcript, so a restart must not replay the history
        // and must not parse it either: `at_end` reads only the last window to leave the offset
        // on a line boundary.
        let dir = scratch("at-end");
        let cfg = config(dir.clone());
        let body: String = (0..5000)
            .map(|index| format!("{{\"id\":\"old-{index}\",\"kind\":\"reply\",\"text\":\"x\"}}\n"))
            .collect();
        std::fs::write(outbox(&cfg), &body).unwrap();
        let mut tail = Tail::at_end(&cfg);
        assert!(replies_from(&cfg, &mut tail).is_empty());

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(outbox(&cfg))
            .unwrap();
        file.write_all(b"{\"id\":\"new-1\",\"kind\":\"reply\",\"text\":\"hello\"}\n")
            .unwrap();
        let fresh = replies_from(&cfg, &mut tail);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].id, "new-1");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restart_leaves_a_half_written_record_for_the_next_poll() {
        // `at_end` stops at the last newline, so a record the Source is still appending is read
        // once complete, not dropped and not parsed mid-way.
        let dir = scratch("at-end-partial");
        let cfg = config(dir.clone());
        std::fs::write(
            outbox(&cfg),
            "{\"id\":\"a\",\"kind\":\"reply\",\"text\":\"one\"}\n{\"id\":\"b\",\"kind\":\"re",
        )
        .unwrap();
        let mut tail = Tail::at_end(&cfg);
        assert!(replies_from(&cfg, &mut tail).is_empty());

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(outbox(&cfg))
            .unwrap();
        file.write_all(b"ply\",\"text\":\"two\"}\n").unwrap();
        let complete = replies_from(&cfg, &mut tail);
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].id, "b");
        assert_eq!(complete[0].text, "two");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restart_skips_a_record_longer_than_the_window() {
        // The 8 KiB window does not reach the newline before a long partial record; walking back
        // window by window still finds it, so the first poll reads the fragment, not the history.
        let dir = scratch("at-end-long-partial");
        let cfg = config(dir.clone());
        let mut body = String::from("{\"id\":\"old\",\"kind\":\"reply\",\"text\":\"x\"}\n");
        body.push_str(&"y".repeat(TAIL_WINDOW as usize * 2));
        std::fs::write(outbox(&cfg), &body).unwrap();
        let mut tail = Tail::at_end(&cfg);
        assert!(
            replies_from(&cfg, &mut tail).is_empty(),
            "the history is not replayed for a long partial record"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restart_with_no_outbox_reads_the_first_message() {
        let dir = scratch("at-end-missing");
        let cfg = config(dir.clone());
        let mut tail = Tail::at_end(&cfg);
        std::fs::write(
            outbox(&cfg),
            "{\"id\":\"first\",\"kind\":\"reply\",\"text\":\"hello\"}\n",
        )
        .unwrap();
        let fresh = replies_from(&cfg, &mut tail);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].id, "first");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unreadable_outbox_reads_nothing_instead_of_failing() {
        let dir = scratch("unreadable");
        let cfg = config(dir.clone());
        let path = outbox(&cfg);

        // A directory is readable by name and fails to read as a file.
        std::fs::create_dir_all(&path).unwrap();
        assert!(replies_from(&cfg, &mut Tail::fresh()).is_empty());
        std::fs::remove_dir_all(&path).unwrap();

        // A file with no read permission fails to open.
        std::fs::write(&path, "{\"id\":\"a\",\"kind\":\"reply\",\"text\":\"x\"}\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = replies_from(&cfg, &mut Tail::fresh());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        // Running as root ignores the mode, so only assert the ordinary case.
        if !unreadable.is_empty() {
            assert!(replies_from(&cfg, &mut Tail::fresh()).len() == 1);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_shrunk_outbox_starts_over_and_a_partial_line_waits() {
        // SPEC scenario "文件被截断或替换".
        let dir = scratch("shrink");
        let cfg = config(dir.clone());
        let first = "{\"id\":\"a\",\"kind\":\"reply\",\"text\":\"one\"}\n";
        std::fs::write(outbox(&cfg), first).unwrap();
        let mut tail = Tail::fresh();
        assert_eq!(replies_from(&cfg, &mut tail).len(), 1);
        assert_eq!(tail.offset, first.len() as u64);

        // Truncated and rewritten shorter than the offset: no panic, the new content is read.
        std::fs::write(
            outbox(&cfg),
            "{\"id\":\"b\",\"kind\":\"reply\",\"text\":\"two\"}\n",
        )
        .unwrap();
        let after = replies_from(&cfg, &mut tail);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, "b");

        // A half-written record stays unread until its newline arrives.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(outbox(&cfg))
            .unwrap();
        file.write_all(b"{\"id\":\"c\",\"kind\":\"re").unwrap();
        assert!(replies_from(&cfg, &mut tail).is_empty());
        file.write_all(b"ply\",\"text\":\"three\"}\n").unwrap();
        let complete = replies_from(&cfg, &mut tail);
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].id, "c");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_replaced_outbox_starts_over_even_when_it_grew() {
        // SPEC scenario "文件被截断或替换": a replaced file can be *longer* than the old one.
        // The offset then lands in the middle of a new record, so reading from it would drop
        // everything before that point.
        let dir = scratch("replace-grow");
        let cfg = config(dir.clone());
        let first = concat!(
            "{\"id\":\"a\",\"kind\":\"reply\",\"text\":\"one\"}\n",
            "{\"id\":\"b\",\"kind\":\"reply\",\"text\":\"two\"}\n",
        );
        std::fs::write(outbox(&cfg), first).unwrap();
        let mut tail = Tail::fresh();
        assert_eq!(replies_from(&cfg, &mut tail).len(), 2);
        assert_eq!(tail.offset, first.len() as u64);

        // A different, longer outbox replaces the file.
        let replaced = concat!(
            "{\"id\":\"x\",\"kind\":\"reply\",\"text\":\"a longer first record\"}\n",
            "{\"id\":\"y\",\"kind\":\"reply\",\"text\":\"two\"}\n",
            "{\"id\":\"z\",\"kind\":\"reply\",\"text\":\"three\"}\n",
        );
        assert!(replaced.len() > first.len());
        std::fs::write(outbox(&cfg), replaced).unwrap();
        let after = replies_from(&cfg, &mut tail);
        let ids: Vec<&str> = after.iter().map(|reply| reply.id.as_str()).collect();
        assert_eq!(ids, ["x", "y", "z"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_replaced_outbox_of_the_same_size_starts_over() {
        // Same length as the file before, so only the bytes can tell that it was rewritten.
        let dir = scratch("replace-same");
        let cfg = config(dir.clone());
        let first = "{\"id\":\"a\",\"kind\":\"reply\",\"text\":\"one\"}\n";
        std::fs::write(outbox(&cfg), first).unwrap();
        let mut tail = Tail::fresh();
        assert_eq!(replies_from(&cfg, &mut tail).len(), 1);

        let replaced = "{\"id\":\"b\",\"kind\":\"reply\",\"text\":\"two\"}\n";
        assert_eq!(replaced.len(), first.len());
        std::fs::write(outbox(&cfg), replaced).unwrap();
        let after = replies_from(&cfg, &mut tail);
        let ids: Vec<&str> = after.iter().map(|reply| reply.id.as_str()).collect();
        assert_eq!(ids, ["b"]);
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
        let messages = replies_from(&cfg, &mut Tail::fresh());
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].ticket.as_deref(), Some("t-1"));
        assert_eq!(messages[1].kind, "reply");
        assert!(replies_from(&config(dir.join("missing")), &mut Tail::fresh()).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remembers_prompt_history_across_sessions() {
        let dir = scratch("history");
        let cfg = config(dir.clone());
        assert!(history(&cfg).is_empty());
        remember(&cfg, "one");
        remember(&cfg, "two words");
        assert_eq!(
            history(&cfg),
            vec!["one".to_owned(), "two words".to_owned()]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn history_keeps_the_last_five_hundred_lines() {
        let dir = scratch("history-cap");
        let cfg = config(dir.clone());
        for index in 0..600 {
            remember(&cfg, &format!("line {index}"));
        }
        let lines = history(&cfg);
        assert_eq!(lines.len(), 500, "the cap is 500");
        assert_eq!(lines[0], "line 100");
        assert_eq!(lines[499], "line 599");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remember_rewrites_once_the_file_doubles_and_keeps_the_newest() {
        let dir = scratch("history-rewrite");
        let cfg = config(dir.clone());
        // Reach the 1000-line threshold, then one more append rewrites to the newest 500.
        for index in 0..1000 {
            remember(&cfg, &format!("l{index}"));
        }
        let path = cfg.state_dir.join("booth-history.txt");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            1000
        );
        remember(&cfg, "after");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 501, "rewritten to 500 plus the new line");
        assert_eq!(lines[0], "l500");
        assert_eq!(lines[500], "after");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remembers_the_ticket_it_continues() {
        let dir = scratch("follow");
        let cfg = config(dir.clone());
        assert!(follow(&cfg).is_none());
        set_follow(&cfg, Some(("t-1", "t-9")));
        assert_eq!(follow(&cfg), Some(("t-1".to_owned(), "t-9".to_owned())));
        set_follow(&cfg, None);
        assert!(follow(&cfg).is_none());
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
        // Each guard on the split result is reachable: no second segment, an empty first
        // segment, and a third segment.
        assert_eq!(repository_from_url("https://github.com/owner"), None);
        assert_eq!(repository_from_url("https://github.com//r"), None);
        assert_eq!(repository_from_url("https://github.com/o/r/extra"), None);
    }
}

#[cfg(test)]
mod eval_tests {
    use super::{resolve_mode, EVAL_MODES};

    #[test]
    fn an_unchanged_auto_mode_still_explains_what_auto_admits() {
        // The hint belongs to the mode, not to the change: asking for `auto` when it is already
        // in force says "already auto" and keeps quiet about inferred candidates.
        let text: String = super::EVAL_MODES
            .iter()
            .find(|(mode, _)| *mode == "auto")
            .map(|(_, hint)| (*hint).to_owned())
            .unwrap();
        assert!(text.contains("without a person"), "{text}");
    }

    #[test]
    fn resolves_a_mode_change_against_the_current_one() {
        assert_eq!(resolve_mode("", "observe").unwrap(), None);
        assert_eq!(resolve_mode("  AUTO ", "observe").unwrap(), Some("auto"));
        assert_eq!(resolve_mode("off", "observe").unwrap(), Some("off"));
        // Asking for the mode already in force is not a change.
        assert_eq!(resolve_mode("observe", "observe").unwrap(), None);
        let error = resolve_mode("maybe", "observe").unwrap_err();
        assert!(error.contains("maybe"), "{error}");
        assert!(error.contains("off"), "{error}");
    }

    #[test]
    fn every_mode_explains_itself() {
        let modes: Vec<&str> = EVAL_MODES.iter().map(|(mode, _)| *mode).collect();
        assert_eq!(modes, vec!["off", "observe", "auto"]);
        assert!(EVAL_MODES.iter().all(|(_, text)| !text.is_empty()));
        assert!(EVAL_MODES.iter().any(|(mode, _)| *mode == "observe"));
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use crate::testkit::FakeApi;

    /// `BOOTH_GH` and `BOOTH_WAIT_SECONDS` are process-wide, so tests that set them take turns.
    use crate::testkit::env_lock;

    fn cfg(dir: &std::path::Path) -> service::Config {
        service::Config {
            profile: "test".into(),
            port: 0,
            dsh_home: dir.to_path_buf(),
            state_dir: dir.to_path_buf(),
            bundle: dir.to_path_buf(),
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("booth-api-{name}-{}", oauth::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn wait_ready_passes_on_an_answering_service_and_fails_on_a_dead_port() {
        let _env = env_lock();
        // A service that answers: one round trip and back.
        let api = FakeApi::start();
        assert!(wait_ready(&Api::new(&api.base)).is_ok());

        // A port nothing listens on: the error names the budget it waited.
        std::env::set_var("BOOTH_WAIT_SECONDS", "1");
        let error = wait_ready(&Api::new("http://127.0.0.1:9")).unwrap_err();
        assert!(error.contains("1s"), "{error}");
        std::env::remove_var("BOOTH_WAIT_SECONDS");
    }

    #[test]
    fn the_transcript_helpers_keep_their_colours() {
        // These three render differently from a blank line only by colour, so a plain-text
        // assertion cannot tell them apart from `Line::default()`.
        for line in [section("eval"), muted("hint"), blank()] {
            let spans: Vec<_> = line.spans.iter().collect();
            let styled = spans.iter().any(|span| span.style.fg.is_some());
            assert!(styled || spans.is_empty(), "{line:?}");
        }
        assert!(section("eval").spans.iter().any(|s| s.style.fg.is_some()));
        assert!(muted("hint").spans.iter().any(|s| s.style.fg.is_some()));
    }

    #[test]
    fn eval_prints_the_modes_and_marks_the_current_one() {
        let api = FakeApi::start();
        let lines = eval(&Api::new(&api.base), "").unwrap();
        let text: String = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("observe"), "{text}");
        assert!(text.contains("▶ observe"), "{text}");
        assert!(api.put_count() == 0, "reading must not save");
    }

    #[test]
    fn eval_switches_the_mode_and_reads_the_revision_back() {
        let api = FakeApi::start();
        let lines = eval(&Api::new(&api.base), "auto").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("observe → auto"), "{text}");
        // The hint belongs to the target mode, so switching to auto says what it admits.
        assert!(text.contains("without a person"), "{text}");
        let (block, body) = api.last_put().unwrap();
        assert_eq!(block, "eval");
        assert_eq!(body["value"]["mode"], "auto");
        // The revision the fake can report is the one the console read.
        assert_eq!(
            api.settings.lock().unwrap()["blocks"]["eval"]["revision"],
            4
        );
    }

    #[test]
    fn eval_reports_an_unchanged_mode_without_saving() {
        let api = FakeApi::start();
        let text: String = eval(&Api::new(&api.base), "observe")
            .unwrap()
            .iter()
            .map(|l| l.to_string())
            .collect();
        assert!(text.contains("already observe"), "{text}");
        assert_eq!(api.put_count(), 0);
    }

    #[test]
    fn summary_shows_the_catalog_the_service_reports() {
        let catalog = Catalog::parse(&serde_json::json!({
            "default": { "provider": "deepseek-official", "model": "deepseek-flash" },
            "groups": [{
                "id": "deepseek-official",
                "name": "DeepSeek",
                "models": [{ "id": "deepseek-flash", "name": "DeepSeek-V4.1-Flash" }]
            }]
        }));
        let text: String = summary(&catalog)
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("deepseek-official · deepseek-flash"),
            "{text}"
        );
        assert!(text.contains("DeepSeek-V4.1-Flash"), "{text}");
        // The view names where it comes from, so nobody looks for a booth-side file.
        assert!(text.contains("no booth-side file"), "{text}");
    }

    #[test]
    fn summary_names_the_providers_whose_key_is_stored() {
        // `keys` tells an operator which endpoint already has a secret, without printing it.
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-keys-{}", oauth::uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("DSH_HOME", &home);
        std::fs::write(
            home.join(".credentials.yaml"),
            format!(
                "version: 1\nrefs:\n  {}: sk-x\n",
                crate::provider::key_ref("openrouter")
            ),
        )
        .unwrap();
        let catalog = Catalog::parse(&serde_json::json!({
            "default": { "provider": "openrouter", "model": "x/y" },
            "groups": [{ "id": "openrouter", "name": "OpenRouter",
                         "models": [{ "id": "x/y" }] }]
        }));
        let text: String = summary(&catalog)
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("stored for openrouter"), "{text}");
        assert!(
            !text.contains("sk-x"),
            "the secret is never printed: {text}"
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn summary_reports_a_signed_in_profile_with_no_override() {
        // After a sign-in there is no booth-side default: the profile's own rule applies.
        let _env = crate::testkit::env_lock();
        let home = std::env::temp_dir().join(format!("booth-signed-{}", oauth::uuid_v4()));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("DSH_HOME", &home);
        std::fs::write(
            home.join(".credentials.yaml"),
            "version: 1\nrecords:\n  deepseek-account-platform/default:\n    kind: grant\n    payload:\n      token: t\n",
        )
        .unwrap();
        let text: String = summary(&Catalog::default())
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("the profile's own default"), "{text}");
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn summary_says_so_when_the_catalog_is_empty() {
        let text: String = summary(&Catalog::default())
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        // Not signed in and no override: the console must say there is no model, not repeat the
        // signed-in wording.
        assert!(text.contains("no model yet"), "{text}");
        assert!(!text.contains("the profile's own default"), "{text}");
        assert!(!text.contains("providers"), "{text}");
    }

    #[test]
    fn setup_writes_every_block_and_reports_the_repository() {
        let _env = env_lock();
        // No download and no real `gh`: a fixture CLI the test writes itself, so the run needs
        // nothing from outside the repository.
        let dir = scratch("setup");
        let gh = dir.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --slurp  use with --paginate';;\n  *) echo tester;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        let api = FakeApi::start();
        let config = cfg(&dir);
        let lines = setup(&Api::new(&api.base), &config, "o/r").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("o/r"), "{text}");

        let puts = api.puts.lock().unwrap().clone();
        // The console percent-encodes the block name, since a Source block is `source:<id>`.
        let names: Vec<String> = puts
            .iter()
            .map(|(name, _)| name.replace("source%3A", "source:"))
            .collect();
        assert!(names.contains(&"source:github".to_owned()), "{names:?}");
        assert!(names.contains(&"source:local".to_owned()), "{names:?}");
        assert!(names.contains(&"policy".to_owned()), "{names:?}");
        let github = puts
            .iter()
            .find(|(name, _)| name.replace("source%3A", "source:") == "source:github")
            .unwrap();
        assert_eq!(github.1["value"]["config"]["repository"], "o/r");
        assert_eq!(github.1["value"]["enabled"], true);
        let policy = puts.iter().find(|(name, _)| name == "policy").unwrap();
        assert!(policy.1["value"]["sources"]["local"]["allowed"][0]["id"].is_string());
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
    }

    #[test]
    fn setup_keeps_a_directory_whose_path_has_spaces() {
        let _env = env_lock();
        let dir = scratch("setup-spaces");
        let gh = dir.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --slurp  use with --paginate';;\n  *) echo tester;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        let api = FakeApi::start();
        let lines = setup(&Api::new(&api.base), &cfg(&dir), "o/r /tmp/ticket work").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("/tmp/ticket work"), "{text}");
        let executor = api
            .puts
            .lock()
            .unwrap()
            .iter()
            .find(|(name, _)| name == "executor")
            .map(|(_, body)| body.clone())
            .unwrap();
        assert_eq!(executor["value"]["directory"], "/tmp/ticket work");
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
    }

    #[test]
    fn setup_retries_a_source_that_is_not_registered_yet() {
        let _env = env_lock();
        // The restart between two saves unregisters the Sources for a moment; setup must wait
        // rather than fail, so a first run configures in one go.
        let dir = scratch("setup-retry");
        let gh = dir.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --slurp';;\n  *) echo tester;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        let api = FakeApi::start();
        api.fail_next_puts(1, 404, "Source not found");
        let lines = setup(&Api::new(&api.base), &cfg(&dir), "o/r").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("o/r"), "{text}");
        // The retried PUT landed, so all three blocks were written.
        let names: Vec<String> = api
            .puts
            .lock()
            .unwrap()
            .iter()
            .map(|(name, _)| name.replace("source%3A", "source:"))
            .collect();
        assert!(names.contains(&"source:github".to_owned()), "{names:?}");
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
    }

    #[test]
    fn setup_stops_on_a_refusal_that_is_not_a_restart() {
        let _env = env_lock();
        let dir = scratch("setup-refused");
        let gh = dir.join("gh");
        std::fs::write(&gh, "#!/bin/sh\necho tester\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        let api = FakeApi::start();
        api.fail_next_puts(1, 400, "an invalid value");
        let error = setup(&Api::new(&api.base), &cfg(&dir), "o/r").unwrap_err();
        assert!(error.contains("invalid value"), "{error}");
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
    }

    #[test]
    fn setup_warns_when_the_repository_changes() {
        let _env = env_lock();
        let dir = scratch("setup-changed");
        let gh = dir.join("gh");
        std::fs::write(&gh, "#!/bin/sh\necho tester\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        let api = FakeApi::start();
        // The fixture already names o/r, so asking for another one is a change.
        let lines = setup(&Api::new(&api.base), &cfg(&dir), "other/repo").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("repository changed"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
    }

    #[test]
    fn gh_login_reports_a_missing_account_and_a_failed_cli() {
        let dir = scratch("gh-login");
        // Written under another name and renamed into place: a file some other thread still
        // holds open cannot be executed, and a rename never leaves one at the final path.
        let write = |name: &str, body: &str| {
            use std::os::unix::fs::PermissionsExt;
            let staged = dir.join(format!(".{name}.staged"));
            std::fs::write(&staged, body).unwrap();
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).unwrap();
            let path = dir.join(name);
            std::fs::rename(&staged, &path).unwrap();
            path
        };
        let ok = write("ok", "#!/bin/sh\necho tester\n");
        assert_eq!(gh_login(ok.to_str().unwrap()).unwrap(), "tester");

        let empty = write("empty", "#!/bin/sh\nexit 0\n");
        let error = gh_login(empty.to_str().unwrap()).unwrap_err();
        assert!(error.contains("did not report"), "{error}");

        let failing = write("failing", "#!/bin/sh\necho 'not signed in' >&2; exit 4\n");
        let error = gh_login(failing.to_str().unwrap()).unwrap_err();
        assert!(error.contains("not signed in"), "{error}");

        let missing = dir.join("absent");
        let error = gh_login(missing.to_str().unwrap()).unwrap_err();
        assert!(error.contains("cannot run"), "{error}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn gh_login_retries_a_cli_the_kernel_refuses_to_execute() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("gh-busy");
        let path = dir.join("gh");
        std::fs::write(&path, "#!/bin/sh\necho tester\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(gh_login(path.to_str().unwrap()).unwrap(), "tester");

        // Linux refuses to execute a file a process holds open for writing (ETXTBSY). Holding one
        // makes the first attempt fail; the retry is what the arm exists for, and the second
        // attempt reports the error. The pause is what proves the retry happened: without it the
        // error would come back at once, exactly as a non-busy failure does.
        let writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        let started = std::time::Instant::now();
        let error = gh_login(path.to_str().unwrap()).unwrap_err();
        let busy = started.elapsed();
        drop(writer);
        assert!(error.contains("cannot run gh"), "{error}");
        assert!(
            busy >= super::RETRY_PAUSE / 2,
            "a busy CLI is retried after a pause, not failed at once ({busy:?})"
        );

        // A missing binary is not busy, so it fails immediately: no pause, no retry.
        let started = std::time::Instant::now();
        let error = gh_login(dir.join("absent").to_str().unwrap()).unwrap_err();
        let missing = started.elapsed();
        assert!(error.contains("cannot run"), "{error}");
        assert!(
            missing < super::RETRY_PAUSE / 2,
            "only a busy CLI waits ({missing:?})"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn infers_the_repository_from_the_checkout_origin() {
        // Changing the working directory is process-wide, so this test takes its turn too.
        let _env = env_lock();
        let dir = scratch("infer");
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .expect("git must be installed")
        };
        run(&["init", "-q"]);
        // `infer_repository` reads the process's working directory, so the test runs there.
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let without = infer_repository();
        run(&["remote", "add", "origin", "git@github.com:owner/name.git"]);
        let with = infer_repository();
        std::env::set_current_dir(previous).unwrap();

        assert_eq!(without, None, "no origin means no guess");
        assert_eq!(with.as_deref(), Some("owner/name"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn setup_waits_for_a_service_that_is_still_restarting() {
        let _env = env_lock();
        let dir = scratch("setup-restarting");
        let gh = dir.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --slurp';;\n  *) echo tester;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        // A generous budget: the point is that setup waits instead of failing at once.
        std::env::set_var("BOOTH_WAIT_SECONDS", "20");
        let api = FakeApi::start();
        // Both reads a restart interrupts: the registry snapshot and the readiness answer.
        api.refuse_settings(2);
        api.refuse_state(2);
        let lines = setup(&Api::new(&api.base), &cfg(&dir), "o/r").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("o/r"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
        std::env::remove_var("BOOTH_WAIT_SECONDS");
    }

    #[test]
    fn setup_warns_when_the_github_cli_is_too_old() {
        let _env = env_lock();
        let dir = scratch("setup-old-gh");
        let gh = dir.join("gh");
        // No `--slurp`: the Source's reads would fail, so the door stays shut.
        std::fs::write(
            &gh,
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --paginate';;\n  *) echo tester;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        let api = FakeApi::start();
        let lines = setup(&Api::new(&api.base), &cfg(&dir), "o/r").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("GitHub Issues are off"), "{text}");
        // The GitHub Source is saved disabled, so it does not poll with a CLI that cannot read.
        let github = api
            .puts
            .lock()
            .unwrap()
            .iter()
            .find(|(name, _)| name.replace("source%3A", "source:") == "source:github")
            .map(|(_, body)| body.clone())
            .unwrap();
        assert_eq!(github["value"]["enabled"], false);
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
    }

    #[test]
    fn setup_stays_quiet_when_no_repository_was_set_before() {
        let _env = env_lock();
        let dir = scratch("setup-first");
        let gh = dir.join("gh");
        std::fs::write(&gh, "#!/bin/sh\necho tester\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        let api = FakeApi::start();
        // A fresh deployment names no repository, so naming one is not a change.
        api.set_sources(json!([
            { "id": "github", "kind": "github", "installed": true, "revision": 0,
              "value": { "enabled": true, "config": { "repository": "", "cli": "gh" } } },
            { "id": "local", "kind": "local", "installed": true, "revision": 0,
              "value": { "enabled": true, "config": {} } }
        ]));
        let lines = setup(&Api::new(&api.base), &cfg(&dir), "o/r").unwrap();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.contains("repository changed"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
    }

    #[test]
    fn setup_gives_up_when_the_service_never_answers() {
        let _env = env_lock();
        let dir = scratch("setup-silent");
        let gh = dir.join("gh");
        std::fs::write(&gh, "#!/bin/sh\necho tester\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        std::env::set_var("BOOTH_WAIT_SECONDS", "1");
        let api = FakeApi::start();
        api.refuse_settings(100);
        let error = setup(&Api::new(&api.base), &cfg(&dir), "o/r").unwrap_err();
        assert!(
            error.contains("restarting") || error.contains("503"),
            "{error}"
        );
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
        std::env::remove_var("BOOTH_WAIT_SECONDS");
    }

    #[test]
    fn setup_uses_a_capable_cli_installed_under_the_state_directory() {
        let _env = env_lock();
        let dir = scratch("setup-local-gh");
        std::env::remove_var("BOOTH_GH");
        // Whether this machine's own gh is new enough must not decide which CLI the test picks,
        // so a too-old fixture goes first on PATH and stands in for the system one. The machine's
        // PATH follows, so other processes a test spawns still resolve.
        let shadow = dir.join("path");
        std::fs::create_dir_all(&shadow).unwrap();
        let system = shadow.join("gh");
        std::fs::write(
            &system,
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --paginate';;\n  *) echo system-tester;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&system, std::fs::Permissions::from_mode(0o755)).unwrap();
        let previous = std::env::var_os("PATH");
        let mut search = vec![shadow];
        if let Some(path) = previous.as_deref() {
            search.extend(std::env::split_paths(path));
        }
        std::env::set_var("PATH", std::env::join_paths(search).unwrap());
        // A download would hide the branch under test and reach the network; the fixture below
        // is capable, so the offline branch is the one that must answer.
        std::env::set_var("BOOTH_NO_GH_INSTALL", "1");

        // `gh::ensure` looks here before offering a download, so a capable fixture means the
        // setup takes the real path with no network access.
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --slurp  use with --paginate';;\n  *) echo local-tester;;\nesac\n",
        )
        .unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();

        // The host may have a capable gh of its own; put an older one first on PATH so the
        // fallback this test names is the branch it runs, whatever the machine installed.
        struct PathGuard(Option<std::ffi::OsString>);
        impl Drop for PathGuard {
            fn drop(&mut self) {
                match &self.0 {
                    Some(value) => std::env::set_var("PATH", value),
                    None => std::env::remove_var("PATH"),
                }
            }
        }
        let saved = std::env::var_os("PATH");
        let older = dir.join("older");
        std::fs::create_dir_all(&older).unwrap();
        let old_gh = older.join("gh");
        std::fs::write(&old_gh, "#!/bin/sh\necho '      --paginate'\n").unwrap();
        std::fs::set_permissions(&old_gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut paths = vec![older];
        if let Some(saved) = &saved {
            paths.extend(std::env::split_paths(saved));
        }
        let _path = PathGuard(saved);
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());

        let api = FakeApi::start();
        let lines = setup(&Api::new(&api.base), &cfg(&dir), "o/r");

        // Restore the machine's PATH before asserting: a failed expectation must not leak into
        // every test that spawns a process after this one.
        match previous {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
        std::env::remove_var("BOOTH_NO_GH_INSTALL");

        let text: String = lines
            .unwrap()
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        // The CLI the Source was pointed at is the local one, and the GitHub door is open.
        assert!(
            text.contains("local-tester"),
            "the local CLI answered: {text}"
        );
        let github = api
            .puts
            .lock()
            .unwrap()
            .iter()
            .find(|(name, _)| name.replace("source%3A", "source:") == "source:github")
            .map(|(_, body)| body.clone())
            .unwrap();
        assert_eq!(github["value"]["config"]["cli"], gh.display().to_string());
        assert_eq!(github["value"]["enabled"], true);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn setup_says_which_source_is_missing() {
        let _env = env_lock();
        let dir = scratch("setup-missing-source");
        let gh = dir.join("gh");
        std::fs::write(&gh, "#!/bin/sh\necho tester\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("BOOTH_GH", &gh);
        std::env::set_var("BOOTH_WAIT_SECONDS", "1");
        let api = FakeApi::start();
        // Only the GitHub Source registered: the console one is not there yet, and never will be.
        api.set_sources(json!([
            { "id": "github", "kind": "github", "installed": true, "revision": 0,
              "value": { "enabled": true, "config": { "repository": "o/r", "cli": "gh" } } }
        ]));
        let error = setup(&Api::new(&api.base), &cfg(&dir), "o/r").unwrap_err();
        assert!(error.contains("booth init"), "{error}");
        assert_eq!(
            api.put_count(),
            0,
            "nothing is saved before the Sources exist"
        );
        std::fs::remove_dir_all(dir).unwrap();
        std::env::remove_var("BOOTH_GH");
        std::env::remove_var("BOOTH_WAIT_SECONDS");
    }

    #[test]
    fn setup_rejects_an_argument_that_is_not_owner_name() {
        let api = FakeApi::start();
        let dir = scratch("setup-bad");
        let error = setup(&Api::new(&api.base), &cfg(&dir), "notarepo").unwrap_err();
        assert!(error.contains("owner/name"), "{error}");
        assert_eq!(api.put_count(), 0, "a bad argument must not save anything");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

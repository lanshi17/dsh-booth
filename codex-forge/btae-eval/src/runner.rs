//! Linux-only, no credentials or model calls. This protocol is not a DSH adapter.
use crate::Manifest;
use crate::PlannedRun;
use crate::digest;
use crate::plan;
use clap::ValueEnum;
use nix::sys::signal::Signal;
use nix::sys::signal::killpg;
use nix::unistd::Pid;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs::File;
use std::fs::OpenOptions;
use std::fs::{self};
use std::io::Read;
use std::io::Write;
use std::io::{self};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    Success,
    Failure,
    Retry,
    Timeout,
    Descendant,
    LogFlood,
    Malformed,
    ForgedApproval,
    NonmonotonicUsage,
    Environment,
    Isolation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub wall_ms: u64,
    pub tokens: u64,
    pub attempts: u32,
    pub log_bytes: usize,
    pub message_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            wall_ms: 5000,
            tokens: 32,
            attempts: 2,
            log_bytes: 16384,
            message_bytes: 4096,
        }
    }
}

pub struct DiagnosticRequest {
    pub manifest: PathBuf,
    pub task_id: String,
    pub arm: String,
    pub repeat: u32,
    pub fixture_worker: PathBuf,
    pub output: PathBuf,
    pub scenario: Scenario,
    pub limits: Limits,
    /// Opaque paths/addresses for deliberate negative-control probes, never mounted.
    pub host_probe: Option<PathBuf>,
    pub host_address: Option<String>,
    /// Host-side cancellation sentinel; never mounted inside the worker sandbox.
    pub cancel_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicInput {
    pub protocol: u32,
    pub data_kind: String,
    pub run_id: String,
    pub task_request: String,
    pub arm_instructions: String,
    pub host_probe: Option<PathBuf>,
    pub host_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum FixtureEvent {
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        cached_input_tokens: u64,
        tools: u64,
    },
    Report {
        message: String,
    },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Counters {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub tools: u64,
}

impl Counters {
    fn total(&self) -> Option<u64> {
        self.input_tokens.checked_add(self.output_tokens)
    }
    fn add(&mut self, other: &Self) -> Option<()> {
        self.input_tokens = self.input_tokens.checked_add(other.input_tokens)?;
        self.output_tokens = self.output_tokens.checked_add(other.output_tokens)?;
        self.cached_input_tokens = self
            .cached_input_tokens
            .checked_add(other.cached_input_tokens)?;
        self.tools = self.tools.checked_add(other.tools)?;
        self.total()?;
        Some(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOutcome {
    Completed,
    WorkerFailed,
    TimedOut,
    TokenLimit,
    LogLimit,
    ProtocolError,
    InfrastructureError,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub attempt: u32,
    pub outcome: ProcessOutcome,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
    pub usage: Counters,
    pub log_bytes_received: u64,
    pub log_bytes_retained: usize,
    pub streams_closed: bool,
    pub note: Option<String>,
    pub workspace_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticRecord {
    pub schema_version: u32,
    pub data_kind: String,
    pub manifest_sha256: String,
    pub run: PlannedRun,
    pub scenario: Scenario,
    pub limits: Limits,
    pub fixture_sha256: String,
    pub bubblewrap_sha256: String,
    pub isolation: String,
    pub usage_source: String,
    pub outcome: ProcessOutcome,
    pub attempts: Vec<AttemptRecord>,
    pub usage_total: Counters,
    pub elapsed_ms: u64,
    pub files_sha256: BTreeMap<String, String>,
    pub hidden_test_result: Option<bool>,
    pub empirical_task_success: Option<bool>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_new(path, &bytes)?;
    Ok(())
}
fn normal_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}
fn plain_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Symlinks, special files and path traversal are never exported or inventoried.
pub fn inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let name = path
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or_else(|| invalid("non-UTF8 artifact path"))?
                    .to_string();
                let meta = entry.metadata()?;
                if meta.len() > 16 * 1024 * 1024 {
                    return Err(invalid("artifact file larger than 16 MiB").into());
                }
                hashes.insert(name, digest(&fs::read(path)?));
                if hashes.len() > 10000 {
                    return Err(invalid("too many artifact files").into());
                }
            } else {
                return Err(invalid("symlink or special file in artifacts").into());
            }
        }
    }
    Ok(hashes)
}

pub(crate) fn snapshot(source: &Path, destination: &Path) -> Result<String> {
    let hashes = inventory(source)?;
    fs::create_dir(destination)?;
    for name in hashes.keys() {
        let target = destination.join(name);
        fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| invalid("no snapshot parent"))?,
        )?;
        write_new(&target, &fs::read(source.join(name))?)?;
    }
    if inventory(destination)? != hashes {
        return Err(invalid("workspace changed during snapshot").into());
    }
    Ok(digest(&serde_json::to_vec(&hashes)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    algorithm: String,
    files: Vec<(String, String)>,
    snapshot_sha256: String,
}

pub(crate) fn export_seed(
    root: &Path,
    m: &Manifest,
    repository: &str,
    workspace: &Path,
) -> Result<()> {
    if !plain_id(repository) {
        return Err(invalid("invalid repository id").into());
    }
    let repo = m
        .repositories
        .iter()
        .find(|r| r.id == repository)
        .ok_or_else(|| invalid("missing repository"))?;
    let path = root.join(format!("datasets/{repository}-snapshot.json"));
    let snap: Snapshot = serde_json::from_slice(&fs::read(path)?)?;
    if snap.algorithm != "sha256(JSON compact sorted [relative_path,file_sha256] pairs)" {
        return Err(invalid("unknown snapshot algorithm").into());
    }
    let mut files = snap.files.clone();
    files.sort();
    if files != snap.files
        || digest(&serde_json::to_vec(&files)?) != repo.snapshot_sha256
        || snap.snapshot_sha256 != repo.snapshot_sha256
        || files.is_empty()
    {
        return Err(invalid("seed manifest hash/order mismatch").into());
    }
    let mut seen = BTreeSet::new();
    for (name, hash) in files {
        if !normal_relative(Path::new(&name)) || !seen.insert(name.clone()) {
            return Err(invalid("unsafe or duplicate export path").into());
        }
        let src = root.join("repositories").join(repository).join(&name);
        if !fs::symlink_metadata(&src)?.is_file() {
            return Err(invalid("source is not a regular file").into());
        }
        let repo_root = root.join("repositories").join(repository).canonicalize()?;
        if !src.canonicalize()?.starts_with(&repo_root) {
            return Err(invalid("source path escaped repository").into());
        }
        let bytes = fs::read(src)?;
        if bytes.len() > 1024 * 1024 || digest(&bytes) != hash {
            return Err(invalid("seed file content/hash mismatch").into());
        }
        let dest = workspace.join(name);
        fs::create_dir_all(dest.parent().ok_or_else(|| invalid("no export parent"))?)?;
        write_new(&dest, &bytes)?;
    }
    Ok(())
}

fn kill_tree(child: &mut Child) {
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
    }
    let _ = child.kill();
}

enum Chunk {
    Data(bool, Vec<u8>),
    Closed,
    ReadError(String),
}
fn reader(
    mut stream: impl Read + Send + 'static,
    stdout: bool,
    tx: mpsc::SyncSender<Chunk>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut bytes = [0; 2048];
        loop {
            match stream.read(&mut bytes) {
                Ok(0) => {
                    let _ = tx.send(Chunk::Closed);
                    break;
                }
                Ok(n) => {
                    if tx.send(Chunk::Data(stdout, bytes[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = tx.send(Chunk::ReadError(e.to_string()));
                    break;
                }
            }
        }
    })
}

#[derive(Default)]
struct ParserState {
    pending: Vec<u8>,
    usage: Counters,
    got_usage: bool,
    got_report: bool,
}
impl ParserState {
    fn bytes(
        &mut self,
        bytes: &[u8],
        limits: &Limits,
        already_spent: u64,
    ) -> Result<Option<ProcessOutcome>> {
        for byte in bytes {
            if *byte != b'\n' {
                if self.pending.len() >= limits.message_bytes {
                    return Err(invalid("JSONL message limit exceeded").into());
                }
                self.pending.push(*byte);
                continue;
            }
            if self.got_report {
                return Err(invalid("event after terminal report").into());
            }
            let event: FixtureEvent = serde_json::from_slice(&self.pending)?;
            self.pending.clear();
            match event {
                FixtureEvent::Usage {
                    input_tokens,
                    output_tokens,
                    cached_input_tokens,
                    tools,
                } => {
                    let next = Counters {
                        input_tokens,
                        output_tokens,
                        cached_input_tokens,
                        tools,
                    };
                    if cached_input_tokens > input_tokens
                        || input_tokens < self.usage.input_tokens
                        || output_tokens < self.usage.output_tokens
                        || tools < self.usage.tools
                        || cached_input_tokens < self.usage.cached_input_tokens
                    {
                        return Err(invalid("nonmonotonic or invalid usage counters").into());
                    }
                    let total = next
                        .total()
                        .and_then(|n| n.checked_add(already_spent))
                        .ok_or_else(|| invalid("token counter overflow"))?;
                    self.usage = next;
                    self.got_usage = true;
                    if total > limits.tokens {
                        return Ok(Some(ProcessOutcome::TokenLimit));
                    }
                }
                FixtureEvent::Report { .. } => {
                    self.got_report = true;
                }
            }
        }
        Ok(None)
    }
}

struct AttemptContext<'a> {
    worker: &'a Path,
    workspace: &'a Path,
    input: &'a Path,
    directory: &'a Path,
    scenario: Scenario,
    attempt: u32,
    limits: &'a Limits,
    episode_start: Instant,
    spent: u64,
    cancel_file: Option<&'a Path>,
}

fn attempt(ctx: AttemptContext<'_>) -> Result<AttemptRecord> {
    let start = Instant::now();
    let mut command = Command::new("/usr/bin/bwrap");
    command
        .args([
            "--unshare-all",
            "--die-with-parent",
            "--clearenv",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--dir",
            "/app",
            "--dir",
            "/input",
            "--ro-bind",
        ])
        .arg(ctx.worker)
        .arg("/app/worker")
        .arg("--ro-bind")
        .arg(ctx.input)
        .arg("/input/request.json")
        .arg("--bind")
        .arg(ctx.workspace)
        .arg("/workspace")
        .args([
            "--chdir",
            "/workspace",
            "--setenv",
            "PATH",
            "/usr/bin",
            "--setenv",
            "LANG",
            "C.UTF-8",
            "--setenv",
            "HOME",
            "/tmp",
            "--",
            "/app/worker",
            "--scenario",
        ])
        .arg(
            serde_json::to_value(ctx.scenario)?
                .as_str()
                .ok_or_else(|| invalid("invalid scenario"))?,
        )
        .arg("--attempt")
        .arg(ctx.attempt.to_string())
        .env_clear()
        .process_group(0)
        .stdin(File::open(ctx.input)?)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let stdout_path = ctx.directory.join("stdout.jsonl");
    let stderr_path = ctx.directory.join("stderr.txt");
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stdout_path)?;
    let mut err = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stderr_path)?;
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Ok(AttemptRecord {
                attempt: ctx.attempt,
                outcome: ProcessOutcome::InfrastructureError,
                exit_code: None,
                elapsed_ms: start.elapsed().as_millis() as u64,
                usage: Counters::default(),
                log_bytes_received: 0,
                log_bytes_retained: 0,
                streams_closed: true,
                note: Some(e.to_string()),
                workspace_sha256: digest(&serde_json::to_vec(&inventory(ctx.workspace)?)?),
            });
        }
    };
    let (tx, rx) = mpsc::sync_channel(16);
    let out_thread = reader(
        child
            .stdout
            .take()
            .ok_or_else(|| invalid("missing stdout"))?,
        true,
        tx.clone(),
    );
    let err_thread = reader(
        child
            .stderr
            .take()
            .ok_or_else(|| invalid("missing stderr"))?,
        false,
        tx,
    );
    let mut parsed = ParserState::default();
    let mut terminal: Option<ProcessOutcome> = None;
    let mut note = None;
    let mut exit: Option<ExitStatus> = None;
    let mut closed = 0;
    let mut received = 0u64;
    let mut retained = 0;
    let mut cleanup_start = None;
    loop {
        if exit.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    exit = Some(status);
                    kill_tree(&mut child);
                    cleanup_start = Some(Instant::now());
                }
                Ok(None) => (),
                Err(e) => {
                    terminal = Some(ProcessOutcome::InfrastructureError);
                    note = Some(e.to_string());
                    kill_tree(&mut child);
                }
            }
        }
        if exit.is_none() && terminal.is_none() && ctx.cancel_file.is_some_and(Path::exists) {
            terminal = Some(ProcessOutcome::Cancelled);
            kill_tree(&mut child);
        }
        if exit.is_none()
            && ctx.episode_start.elapsed().as_millis() >= ctx.limits.wall_ms as u128
            && terminal.is_none()
        {
            terminal = Some(ProcessOutcome::TimedOut);
            kill_tree(&mut child);
        }
        if let Some(since) = cleanup_start {
            if since.elapsed() > Duration::from_secs(2) && closed < 2 {
                terminal = Some(ProcessOutcome::InfrastructureError);
                note = Some("sandbox streams did not close after process-tree termination".into());
                break;
            }
        }
        if closed == 2 && exit.is_some() {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(Chunk::Closed) => closed += 1,
            Ok(Chunk::ReadError(e)) => {
                terminal = Some(ProcessOutcome::InfrastructureError);
                note = Some(e);
                kill_tree(&mut child);
                closed += 1;
            }
            Ok(Chunk::Data(is_stdout, bytes)) => {
                received = received.saturating_add(bytes.len() as u64);
                let n = bytes
                    .len()
                    .min(ctx.limits.log_bytes.saturating_sub(retained));
                let written = if is_stdout {
                    out.write_all(&bytes[..n])
                } else {
                    err.write_all(&bytes[..n])
                };
                if let Err(e) = written {
                    terminal = Some(ProcessOutcome::InfrastructureError);
                    note = Some(e.to_string());
                    kill_tree(&mut child);
                }
                retained += n;
                if received > ctx.limits.log_bytes as u64 && terminal.is_none() {
                    terminal = Some(ProcessOutcome::LogLimit);
                    kill_tree(&mut child);
                }
                if is_stdout && terminal.is_none() {
                    match parsed.bytes(&bytes, ctx.limits, ctx.spent) {
                        Ok(Some(reason)) => {
                            terminal = Some(reason);
                            kill_tree(&mut child);
                        }
                        Ok(None) => (),
                        Err(e) => {
                            terminal = Some(ProcessOutcome::ProtocolError);
                            note = Some(e.to_string());
                            kill_tree(&mut child);
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                closed = 2;
            }
        }
    }
    kill_tree(&mut child);
    let status = child.wait()?;
    drop(rx);
    if closed == 2 {
        let _ = out_thread.join();
        let _ = err_thread.join();
    }
    out.sync_all()?;
    err.sync_all()?;
    let outcome = terminal.unwrap_or_else(|| {
        if !status.success() {
            ProcessOutcome::WorkerFailed
        } else if !parsed.pending.is_empty() || !parsed.got_report || !parsed.got_usage {
            ProcessOutcome::ProtocolError
        } else {
            ProcessOutcome::Completed
        }
    });
    Ok(AttemptRecord {
        attempt: ctx.attempt,
        outcome,
        exit_code: status.code(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        usage: parsed.usage,
        log_bytes_received: received,
        log_bytes_retained: retained,
        streams_closed: closed == 2,
        note,
        workspace_sha256: digest(&serde_json::to_vec(&inventory(ctx.workspace)?)?),
    })
}

pub fn run_diagnostic(req: DiagnosticRequest) -> Result<DiagnosticRecord> {
    let bytes = fs::read(&req.manifest)?;
    let m: Manifest = serde_json::from_slice(&bytes)?;
    let schedule = plan(&m, &bytes)?;
    let planned = schedule
        .planned_runs
        .iter()
        .find(|r| r.task_id == req.task_id && r.arm == req.arm && r.repeat == req.repeat)
        .ok_or_else(|| invalid("requested diagnostic is not a planned task/arm/repeat"))?
        .clone();
    if planned.round != 1 {
        return Err(invalid("diagnostic export currently supports initial-round seeds only; no fake trajectory reset").into());
    }
    if req.limits.wall_ms == 0
        || req.limits.wall_ms > m.budget.wall_seconds.saturating_mul(1000)
        || req.limits.tokens == 0
        || req.limits.tokens > m.budget.total_tokens
        || req.limits.attempts == 0
        || req.limits.attempts > m.budget.attempts
        || !(128..=1024 * 1024).contains(&req.limits.log_bytes)
        || !(64..=req.limits.log_bytes).contains(&req.limits.message_bytes)
    {
        return Err(
            invalid("diagnostic limits must be positive and bounded by the study budget").into(),
        );
    }
    let manifest_path = req.manifest.canonicalize()?;
    let root = manifest_path
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| invalid("expected datasets/<manifest>"))?;
    let worker = req.fixture_worker.canonicalize()?;
    if !worker.is_file() {
        return Err(invalid("fixture must be a regular executable").into());
    }
    let worker_hash = digest(&fs::read(&worker)?);
    let bwrap_hash = digest(&fs::read("/usr/bin/bwrap")?);
    let instructions = fs::read_to_string(root.join(format!("baselines/{}.md", req.arm)))?;
    let task = m
        .tasks
        .iter()
        .find(|t| t.id == planned.task_id)
        .ok_or_else(|| invalid("missing task"))?;
    let input = PublicInput {
        protocol: 1,
        data_kind: "runner_diagnostic_only".into(),
        run_id: planned.run_id.clone(),
        task_request: task.request.clone(),
        arm_instructions: instructions,
        host_probe: req.host_probe,
        host_address: req.host_address,
    };
    let start = Instant::now();
    fs::create_dir(&req.output)?;
    let output = req.output.canonicalize()?;
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
    write_json(
        &output.join("started.json"),
        &serde_json::json!({
            "data_kind":"runner_diagnostic_only", "phase":"started", "run":planned,
            "manifest_sha256":schedule.manifest_sha256, "scenario":req.scenario,
            "limits":req.limits, "fixture_sha256":worker_hash,
            "note":"An absent diagnostic.json means setup failed or execution was interrupted; never infer task failure or success."
        }),
    )?;
    let workspace = output.join("workspace");
    fs::create_dir(&workspace)?;
    export_seed(root, &m, &planned.repository, &workspace)?;
    let input_path = output.join("public-input.json");
    write_json(&input_path, &input)?;
    let initial = inventory(&workspace)?;
    write_json(&output.join("initial-workspace.json"), &initial)?;
    let mut attempts = Vec::new();
    let mut usage = Counters::default();
    for n in 1..=req.limits.attempts {
        if start.elapsed().as_millis() >= req.limits.wall_ms as u128
            || usage
                .total()
                .ok_or_else(|| invalid("aggregate token overflow"))?
                >= req.limits.tokens
        {
            break;
        }
        let directory = output.join(format!("attempt-{n}"));
        fs::create_dir(&directory)?;
        let mut record = attempt(AttemptContext {
            worker: &worker,
            workspace: &workspace,
            input: &input_path,
            directory: &directory,
            scenario: req.scenario,
            attempt: n,
            limits: &req.limits,
            episode_start: start,
            spent: usage
                .total()
                .ok_or_else(|| invalid("aggregate token overflow"))?,
            cancel_file: req.cancel_file.as_deref(),
        })?;
        record.workspace_sha256 = snapshot(&workspace, &directory.join("workspace-snapshot"))?;
        usage
            .add(&record.usage)
            .ok_or_else(|| invalid("aggregate usage overflow"))?;
        write_json(&directory.join("attempt.json"), &record)?;
        let retry = record.outcome == ProcessOutcome::WorkerFailed;
        attempts.push(record);
        if !retry {
            break;
        }
    }
    let files = inventory(&output)?;
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let outcome = final_outcome(&attempts, &usage, &req.limits, elapsed_ms)?;
    let record = DiagnosticRecord { schema_version: 1, data_kind: "runner_diagnostic_only".into(),
        manifest_sha256: schedule.manifest_sha256, run: planned, scenario: req.scenario, limits: req.limits,
        fixture_sha256: worker_hash, bubblewrap_sha256: bwrap_hash,
        isolation: "bubblewrap user/pid/network/mount namespaces; public seed writable; input read-only; no host home/project mount".into(),
        usage_source: "fixture-self-reported counters; NOT provider billing or trusted model usage".into(),
        outcome, attempts, usage_total: usage, elapsed_ms,
        files_sha256: files, hidden_test_result: None, empirical_task_success: None };
    write_json(&output.join("diagnostic.json"), &record)?;
    Ok(record)
}

fn final_outcome(
    attempts: &[AttemptRecord],
    usage: &Counters,
    limits: &Limits,
    elapsed_ms: u64,
) -> Result<ProcessOutcome> {
    // Preserve the original stop cause. Budget exhaustion between retries only
    // replaces an otherwise retryable worker failure, never protocol/cancel errors.
    let last = attempts.last().map(|a| &a.outcome);
    if last.is_none_or(|s| matches!(*s, ProcessOutcome::WorkerFailed | ProcessOutcome::Completed)) {
        if elapsed_ms >= limits.wall_ms {
            return Ok(ProcessOutcome::TimedOut);
        }
        if last != Some(&ProcessOutcome::Completed)
            && usage
                .total()
                .ok_or_else(|| invalid("aggregate token overflow"))?
                >= limits.tokens
        {
            return Ok(ProcessOutcome::TokenLimit);
        }
    }
    Ok(last.cloned().unwrap_or(ProcessOutcome::InfrastructureError))
}

/// Accidental-change detection on a trusted host, not a signature or malicious-writer defense.
pub fn verify_diagnostic(directory: &Path, manifest_path: &Path) -> Result<DiagnosticRecord> {
    let bytes = fs::read(manifest_path)?;
    let m: Manifest = serde_json::from_slice(&bytes)?;
    let schedule = plan(&m, &bytes)?;
    let record: DiagnosticRecord =
        serde_json::from_slice(&fs::read(directory.join("diagnostic.json"))?)?;
    if record.schema_version != 1
        || record.data_kind != "runner_diagnostic_only"
        || record.hidden_test_result.is_some()
        || record.empirical_task_success.is_some()
        || record.manifest_sha256 != schedule.manifest_sha256
        || !schedule.planned_runs.contains(&record.run)
    {
        return Err(invalid("diagnostic provenance does not match the study plan").into());
    }
    let mut actual = inventory(directory)?;
    actual.remove("diagnostic.json");
    if actual != record.files_sha256 {
        return Err(invalid("artifact inventory/content mismatch").into());
    }
    let started: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("started.json"))?)?;
    for (field, value) in [
        ("data_kind", serde_json::to_value(&record.data_kind)?),
        (
            "manifest_sha256",
            serde_json::to_value(&record.manifest_sha256)?,
        ),
        ("run", serde_json::to_value(&record.run)?),
        ("limits", serde_json::to_value(&record.limits)?),
        ("scenario", serde_json::to_value(record.scenario)?),
        (
            "fixture_sha256",
            serde_json::to_value(&record.fixture_sha256)?,
        ),
    ] {
        if started.get(field) != Some(&value) {
            return Err(invalid(format!(
                "terminal metadata disagrees with startup receipt: {field}"
            ))
            .into());
        }
    }
    let mut usage = Counters::default();
    if record.attempts.len() > record.limits.attempts as usize {
        return Err(invalid("attempt count exceeds recorded limit").into());
    }
    for (i, a) in record.attempts.iter().enumerate() {
        if a.attempt as usize != i + 1
            || a.log_bytes_retained > record.limits.log_bytes
            || !a.streams_closed
        {
            return Err(invalid("attempt sequence/limit/cleanup inconsistent").into());
        }
        let saved: AttemptRecord = serde_json::from_slice(&fs::read(
            directory.join(format!("attempt-{}/attempt.json", a.attempt)),
        )?)?;
        if serde_json::to_value(saved)? != serde_json::to_value(a)? {
            return Err(invalid("attempt summary mismatch").into());
        }
        let attempt_dir = directory.join(format!("attempt-{}", a.attempt));
        let snapshot_hash = digest(&serde_json::to_vec(&inventory(
            &attempt_dir.join("workspace-snapshot"),
        )?)?);
        if snapshot_hash != a.workspace_sha256 {
            return Err(invalid("attempt workspace snapshot hash mismatch").into());
        }
        let retained = fs::metadata(attempt_dir.join("stdout.jsonl"))?
            .len()
            .checked_add(fs::metadata(attempt_dir.join("stderr.txt"))?.len())
            .ok_or_else(|| invalid("log length overflow"))?;
        if retained != a.log_bytes_retained as u64 || a.log_bytes_received < retained {
            return Err(invalid("saved log lengths do not match attempt record").into());
        }
        usage
            .add(&a.usage)
            .ok_or_else(|| invalid("aggregate usage overflow"))?;
    }
    if serde_json::to_value(usage)? != serde_json::to_value(&record.usage_total)? {
        return Err(invalid("aggregate usage mismatch").into());
    }
    if final_outcome(
        &record.attempts,
        &record.usage_total,
        &record.limits,
        record.elapsed_ms,
    )? != record.outcome
    {
        return Err(invalid("terminal outcome disagrees with attempts/limits").into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_deadline_without_a_worker_is_still_a_timeout() {
        let limits = Limits {
            wall_ms: 200,
            ..Default::default()
        };
        for elapsed_ms in [200, 250] {
            assert_eq!(
                final_outcome(&[], &Counters::default(), &limits, elapsed_ms).unwrap(),
                ProcessOutcome::TimedOut
            );
        }
    }

    #[test]
    fn sealing_work_counts_toward_deadline_without_overwriting_other_stop_causes() {
        let limits = Limits {
            wall_ms: 100,
            ..Default::default()
        };
        let mut record = AttemptRecord {
            attempt: 1,
            outcome: ProcessOutcome::Completed,
            exit_code: Some(0),
            elapsed_ms: 20,
            usage: Counters::default(),
            log_bytes_received: 0,
            log_bytes_retained: 0,
            streams_closed: true,
            note: None,
            workspace_sha256: String::new(),
        };
        assert_eq!(
            final_outcome(std::slice::from_ref(&record), &record.usage, &limits, 101).unwrap(),
            ProcessOutcome::TimedOut
        );
        record.outcome = ProcessOutcome::Cancelled;
        assert_eq!(
            final_outcome(std::slice::from_ref(&record), &record.usage, &limits, 101).unwrap(),
            ProcessOutcome::Cancelled
        );
        record.outcome = ProcessOutcome::ProtocolError;
        assert_eq!(
            final_outcome(std::slice::from_ref(&record), &record.usage, &limits, 101).unwrap(),
            ProcessOutcome::ProtocolError
        );
    }
}

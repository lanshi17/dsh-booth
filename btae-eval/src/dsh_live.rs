//! Linux process supervision for DSH JSONL. OS isolation is caller supplied.
//! No raw transcript, stderr, prompt or credential values are serialized.
use crate::dsh::DshAdapter;
use crate::dsh::DshLimits;
use crate::dsh::DshSummary;
use nix::fcntl::FcntlArg;
use nix::fcntl::OFlag;
use nix::fcntl::fcntl;
use nix::sys::signal::Signal;
use nix::sys::signal::killpg;
use nix::unistd::Pid;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::fs::{self};
use std::io::Read;
use std::io::Write;
use std::io::{self};
use std::os::fd::AsRawFd;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use thiserror::Error;

pub const MAX_PROMPT_BYTES: usize = 64 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveOutcome {
    Completed,
    Incomplete,
    WorkerFailed,
    TimedOut,
    Cancelled,
    OutputLimit,
    ProtocolError,
    InfrastructureError,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveLimits {
    pub wall_ms: u64,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub dsh: DshLimits,
}
impl Default for LiveLimits {
    fn default() -> Self {
        Self {
            wall_ms: 300_000,
            max_stdout_bytes: 16 * 1024 * 1024,
            max_stderr_bytes: 2 * 1024 * 1024,
            dsh: DshLimits::default(),
        }
    }
}

/// Trusted launcher inputs. `args` must never contain credential values. A
/// sandbox launcher such as bwrap can be selected as the executable.
pub struct LiveRequest {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub prompt: Vec<u8>,
    pub cwd: Option<PathBuf>,
    pub environment: BTreeMap<String, String>,
    pub cancel_file: Option<PathBuf>,
    pub limits: LiveLimits,
    pub output: Option<PathBuf>,
}
#[derive(Debug, Serialize)]
pub struct LiveRecord {
    pub schema_version: u32,
    pub data_kind: String,
    pub outcome: LiveOutcome,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub streams_closed: bool,
    pub prompt_delivered: bool,
    pub executable_sha256: String,
    pub argv_sha256: String,
    pub prompt_sha256: String,
    pub limits: LiveLimits,
    pub summary: Option<DshSummary>,
    /// Fixed categories only; no child-provided text or OS error payloads.
    pub note: Option<String>,
    pub network_isolation: String,
    pub billing_verified: bool,
    pub hidden_test_result: Option<bool>,
    pub empirical_task_success: Option<bool>,
}
#[derive(Debug, Error)]
pub enum LiveError {
    #[error("live DSH process I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("live DSH configuration is invalid: {0}")]
    Invalid(&'static str),
    #[error("live DSH evidence serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
type Result<T> = std::result::Result<T, LiveError>;

fn kill_tree(child: &mut Child) {
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
    }
    let _ = child.kill();
}
struct ManagedChild(Child);
impl Drop for ManagedChild {
    fn drop(&mut self) {
        kill_tree(&mut self.0);
        let _ = self.0.wait();
    }
}
fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
    let flags = fcntl(pipe.as_raw_fd(), FcntlArg::F_GETFL).map_err(io::Error::from)?;
    fcntl(
        pipe.as_raw_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .map_err(io::Error::from)?;
    Ok(())
}
fn validate(req: &LiveRequest) -> Result<()> {
    if !req.executable.is_absolute() {
        return Err(LiveError::Invalid("executable must be absolute"));
    }
    if req.prompt.len() > MAX_PROMPT_BYTES {
        return Err(LiveError::Invalid("prompt exceeds 64 KiB"));
    }
    if req.limits.wall_ms == 0
        || req.limits.wall_ms > 300_000
        || req.limits.max_stdout_bytes == 0
        || req.limits.max_stderr_bytes == 0
        || req.limits.dsh.max_total_bytes == 0
        || req.limits.dsh.max_line_bytes == 0
        || req.limits.dsh.max_events == 0
    {
        return Err(LiveError::Invalid("invalid process/parser limits"));
    }
    if req.args.iter().any(|s| s.contains('\0'))
        || req
            .environment
            .iter()
            .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
    {
        return Err(LiveError::Invalid("invalid argv/environment"));
    }
    Ok(())
}
fn write_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    serde_json::to_writer_pretty(&mut f, value)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}
#[derive(Default)]
struct Stream {
    hash: Sha256,
    bytes: u64,
    closed: bool,
}
impl Stream {
    fn poll(&mut self, pipe: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
        if self.closed {
            return Ok(None);
        }
        let mut buf = [0u8; 8192];
        match pipe.read(&mut buf) {
            Ok(0) => {
                self.closed = true;
                Ok(None)
            }
            Ok(n) => {
                self.hash.update(&buf[..n]);
                self.bytes = self.bytes.saturating_add(n as u64);
                Ok(Some(buf[..n].to_vec()))
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(e) => {
                self.closed = true;
                Err(e)
            }
        }
    }
    fn digest(&self) -> String {
        format!("{:x}", self.hash.clone().finalize())
    }
}

/// One engineering smoke. Child stdout is parsed incrementally, hashes cover
/// all bytes observed (including bytes drained after a stop), and bounded
/// nonblocking IO keeps stdin and descendant-held pipes inside the deadline.
/// This function does not provide OS isolation, token quotas or experiment data.
pub fn run_live(req: LiveRequest) -> Result<LiveRecord> {
    validate(&req)?;
    let executable_sha256 = crate::digest(&fs::read(&req.executable)?);
    let argv_sha256 = crate::digest(&serde_json::to_vec(&req.args)?);
    let prompt_sha256 = crate::digest(&req.prompt);
    let start = Instant::now();
    if let Some(output) = &req.output {
        fs::DirBuilder::new().mode(0o700).create(output)?;
        write_new(
            &output.join("started.json"),
            &serde_json::json!({"schema_version":1,"data_kind":"dsh_process_smoke_only","executable_sha256":executable_sha256,"argv_sha256":argv_sha256,"prompt_sha256":prompt_sha256,"environment_keys":req.environment.keys().collect::<Vec<_>>(),"limits":req.limits,"network_isolation":"caller_supplied"}),
        )?;
    }
    let mut command = Command::new(&req.executable);
    command
        .args(&req.args)
        .env_clear()
        .envs(&req.environment)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &req.cwd {
        command.current_dir(cwd);
    }
    let spawned = command.spawn();
    let mut outcome = None;
    let mut note = None;
    let mut stdout = Stream::default();
    let mut stderr = Stream::default();
    let mut status: Option<ExitStatus> = None;
    let mut delivered = 0usize;
    let mut adapter = Some(DshAdapter::new(req.limits.dsh));
    if let Ok(child) = spawned {
        let mut child = ManagedChild(child);
        let input = child
            .0
            .stdin
            .take()
            .ok_or(LiveError::Invalid("missing stdin"))?;
        let mut out = child
            .0
            .stdout
            .take()
            .ok_or(LiveError::Invalid("missing stdout"))?;
        let mut err = child
            .0
            .stderr
            .take()
            .ok_or(LiveError::Invalid("missing stderr"))?;
        nonblocking(&input)?;
        nonblocking(&out)?;
        nonblocking(&err)?;
        let mut input = Some(input);
        let mut cleanup: Option<Instant> = None;
        loop {
            // A stop remains terminal even if a valid final arrives while draining.
            if outcome.is_none() {
                if req.cancel_file.as_ref().is_some_and(|p| p.exists()) {
                    outcome = Some(LiveOutcome::Cancelled);
                } else if start.elapsed() >= Duration::from_millis(req.limits.wall_ms) {
                    outcome = Some(LiveOutcome::TimedOut);
                }
            }
            if status.is_none() {
                match child.0.try_wait() {
                    Ok(s) => status = s,
                    Err(_) => {
                        outcome.get_or_insert(LiveOutcome::InfrastructureError);
                        note = Some("wait_failed".into());
                    }
                }
            }
            if (status.is_some() || outcome.is_some()) && cleanup.is_none() {
                kill_tree(&mut child.0);
                input = None;
                cleanup = Some(Instant::now());
            }
            if outcome.is_none() && status.is_none() {
                if let Some(writer) = input.as_mut() {
                    if delivered == req.prompt.len() {
                        input = None;
                    } else {
                        match writer.write(&req.prompt[delivered..]) {
                            Ok(0) => {
                                outcome = Some(LiveOutcome::InfrastructureError);
                                note = Some("stdin_closed".into());
                            }
                            Ok(n) => {
                                delivered += n;
                                if delivered == req.prompt.len() {
                                    input = None;
                                }
                            }
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                                ) => {}
                            Err(_) => {
                                outcome = Some(LiveOutcome::InfrastructureError);
                                note = Some("stdin_write_failed".into());
                            }
                        }
                    }
                }
            }
            match stdout.poll(&mut out) {
                Ok(Some(bytes)) => {
                    if stdout.bytes > req.limits.max_stdout_bytes as u64 {
                        outcome.get_or_insert(LiveOutcome::OutputLimit);
                    }
                    if outcome.is_none()
                        && adapter
                            .as_mut()
                            .expect("adapter exists")
                            .feed(&bytes)
                            .is_err()
                    {
                        outcome = Some(LiveOutcome::ProtocolError);
                        note = Some("invalid_dsh_stream".into());
                        kill_tree(&mut child.0);
                    }
                }
                Ok(None) => (),
                Err(_) => {
                    outcome.get_or_insert(LiveOutcome::InfrastructureError);
                    note = Some("stdout_read_failed".into());
                }
            }
            if stderr.poll(&mut err).is_err() {
                outcome.get_or_insert(LiveOutcome::InfrastructureError);
                note = Some("stderr_read_failed".into());
            }
            if stderr.bytes > req.limits.max_stderr_bytes as u64 {
                outcome.get_or_insert(LiveOutcome::OutputLimit);
            }
            if status.is_some() && stdout.closed && stderr.closed {
                break;
            }
            if cleanup.is_some_and(|since| since.elapsed() > Duration::from_secs(2)) {
                outcome.get_or_insert(LiveOutcome::InfrastructureError);
                note = Some("pipes_not_closed".into());
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        kill_tree(&mut child.0);
        if status.is_none() {
            status = child.0.wait().ok();
        }
    } else {
        outcome = Some(LiveOutcome::InfrastructureError);
        note = Some("spawn_failed".into());
    }
    let summary = if outcome.is_none() {
        match adapter
            .take()
            .expect("adapter exists")
            .finish(status.as_ref().and_then(ExitStatus::code))
        {
            Ok(s) => Some(s),
            Err(_) => {
                outcome = Some(LiveOutcome::ProtocolError);
                note = Some("invalid_dsh_eof".into());
                None
            }
        }
    } else {
        None
    };
    let outcome = outcome.unwrap_or_else(|| {
        if status.is_some_and(|s| !s.success()) {
            LiveOutcome::WorkerFailed
        } else if summary.as_ref().is_some_and(|s| s.completed) && delivered == req.prompt.len() {
            LiveOutcome::Completed
        } else {
            LiveOutcome::Incomplete
        }
    });
    let record = LiveRecord {
        schema_version: 1,
        data_kind: "dsh_process_smoke_only".into(),
        outcome,
        exit_code: status.and_then(|s| s.code()),
        elapsed_ms: start.elapsed().as_millis() as u64,
        stdout_bytes: stdout.bytes,
        stderr_bytes: stderr.bytes,
        stdout_sha256: stdout.digest(),
        stderr_sha256: stderr.digest(),
        streams_closed: stdout.closed && stderr.closed,
        prompt_delivered: delivered == req.prompt.len(),
        executable_sha256,
        argv_sha256,
        prompt_sha256,
        limits: req.limits,
        summary,
        note,
        network_isolation: "caller_supplied".into(),
        billing_verified: false,
        hidden_test_result: None,
        empirical_task_success: None,
    };
    if let Some(output) = &req.output {
        write_new(&output.join("record.json"), &record)?;
    }
    Ok(record)
}

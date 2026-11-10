//! Process supervisor for worker executables.
//!
//! Lifecycle port remains for higher layers; F2 adds a concrete supervisor that:
//! - launches argv arrays (never a shell)
//! - clears the environment and re-applies an allowlist
//! - places the child in its own process group and kills the group on cancel/timeout
//! - optionally wraps Linux workers in bubblewrap (ro main repo, rw worktree, optional net ns)
//!
//! F3 adds bounded stdout/stderr capture and a shared CLI worker executor.
mod capture;
mod cli_executor;

pub use capture::CapturedOutput;
pub use capture::IoLimits;
pub use capture::version_probe_timeout;
pub use cli_executor::CliExecutorConfig;
pub use cli_executor::CliWorkerExecutor;
pub use cli_executor::EventParser;
pub use cli_executor::default_io_limits;
pub use cli_executor::supervisor_for_request;

use forge_agent_backend::AgentExecutor;
use forge_agent_backend::BackendFuture;
use forge_domain::AgentSession;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Child;
use tokio::process::ChildStderr;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

pub trait AgentRuntime: Send + Sync {
    fn executor(&self) -> &dyn AgentExecutor;
    fn recover(&self) -> BackendFuture<'_, Vec<AgentSession>>;
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("failed to spawn worker: {0}")]
    Spawn(String),
    #[error("worker timed out after {0:?}")]
    Timeout(Duration),
    #[error("worker was cancelled")]
    Cancelled,
    #[error("worker exited with status {0}")]
    ExitStatus(i32),
    #[error("stdout exceeded {0} bytes")]
    StdoutLimit(usize),
    #[error("stderr exceeded {0} bytes")]
    StderrLimit(usize),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    pub timeout: Duration,
    pub environment_allowlist: BTreeSet<String>,
    /// Extra env injected after clearing (values never logged by this crate).
    pub injected_env: BTreeMap<String, String>,
    pub network_allowed: bool,
    /// When set on Linux, wrap the worker with bubblewrap.
    pub sandbox: Option<SandboxMounts>,
    pub bwrap: PathBuf,
    pub kill: PathBuf,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(900),
            environment_allowlist: BTreeSet::from([
                "LANG".to_owned(),
                "LC_ALL".to_owned(),
                "PATH".to_owned(),
            ]),
            injected_env: BTreeMap::new(),
            network_allowed: false,
            sandbox: None,
            bwrap: PathBuf::from("bwrap"),
            kill: PathBuf::from("/bin/kill"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SandboxMounts {
    pub main_repo: PathBuf,
    pub worktree: PathBuf,
}

#[derive(Debug)]
pub struct ManagedProcess {
    pub id: Uuid,
    pub pid: u32,
    child: Child,
    kill: PathBuf,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) stdout: Option<ChildStdout>,
    pub(crate) stderr: Option<ChildStderr>,
}

impl ManagedProcess {
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

#[derive(Debug, Default)]
pub struct ProcessSupervisor {
    config: SupervisorConfig,
}

impl ProcessSupervisor {
    pub fn new(config: SupervisorConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SupervisorConfig {
        &self.config
    }

    pub async fn run(
        &self,
        executable: &Path,
        args: &[impl AsRef<OsStr>],
        cwd: &Path,
    ) -> Result<i32, SupervisorError> {
        let mut managed = self.spawn(executable, args, cwd).await?;
        self.wait(&mut managed).await
    }

    pub async fn spawn(
        &self,
        executable: &Path,
        args: &[impl AsRef<OsStr>],
        cwd: &Path,
    ) -> Result<ManagedProcess, SupervisorError> {
        self.spawn_with_stdin(executable, args, cwd, false).await
    }

    pub async fn spawn_with_stdin(
        &self,
        executable: &Path,
        args: &[impl AsRef<OsStr>],
        cwd: &Path,
        pipe_stdin: bool,
    ) -> Result<ManagedProcess, SupervisorError> {
        let mut command = self.build_command(executable, args, cwd, pipe_stdin)?;
        let mut child = command
            .spawn()
            .map_err(|err| SupervisorError::Spawn(err.to_string()))?;
        let pid = child
            .id()
            .ok_or_else(|| SupervisorError::Spawn("child has no pid".into()))?;
        Ok(ManagedProcess {
            id: Uuid::new_v4(),
            pid,
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
            child,
            kill: self.config.kill.clone(),
        })
    }

    pub async fn wait(&self, managed: &mut ManagedProcess) -> Result<i32, SupervisorError> {
        // Drain pipes so a chatty child cannot fill the OS buffer and deadlock.
        let _ = managed.stdout.take();
        let _ = managed.stderr.take();
        let wait = managed.child.wait();
        match timeout(self.config.timeout, wait).await {
            Ok(Ok(status)) => {
                if let Some(code) = status.code() {
                    if code == 0 {
                        Ok(0)
                    } else {
                        Err(SupervisorError::ExitStatus(code))
                    }
                } else {
                    Err(SupervisorError::ExitStatus(-1))
                }
            }
            Ok(Err(err)) => Err(SupervisorError::Io(err)),
            Err(_) => {
                let _ = self.kill_group(managed).await;
                Err(SupervisorError::Timeout(self.config.timeout))
            }
        }
    }

    pub async fn cancel(&self, managed: &mut ManagedProcess) -> Result<(), SupervisorError> {
        self.kill_group(managed).await
    }

    pub(crate) async fn kill_group(
        &self,
        managed: &mut ManagedProcess,
    ) -> Result<(), SupervisorError> {
        // Prefer whole-group signals, then fall back to per-PID KILL via pgrep.
        // Some CI environments deliver group signals unreliably for short-lived trees.
        let pgid = managed.pid;
        let group_arg = format!("-{pgid}");
        let kill_bin = managed.kill.clone();
        let signal_group = |signal: &str| {
            let kill_bin = kill_bin.clone();
            let group_arg = group_arg.clone();
            let signal = signal.to_owned();
            async move {
                let _ = Command::new(&kill_bin)
                    .args([&signal, &group_arg])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await;
            }
        };
        signal_group("-TERM").await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        signal_group("-KILL").await;
        for _ in 0..20 {
            if !process_group_alive(pgid) {
                break;
            }
            signal_group("-KILL").await;
            kill_group_members(&kill_bin, pgid).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let _ = managed.child.start_kill();
        let _ = managed.child.wait().await;
        Ok(())
    }

    fn build_command(
        &self,
        executable: &Path,
        args: &[impl AsRef<OsStr>],
        cwd: &Path,
        pipe_stdin: bool,
    ) -> Result<Command, SupervisorError> {
        let filtered = filter_env(&self.config.environment_allowlist);
        let mut command = if let Some(sandbox) = &self.config.sandbox {
            build_bwrap_command(
                &self.config.bwrap,
                sandbox,
                self.config.network_allowed,
                executable,
                args,
                cwd,
            )?
        } else {
            let mut command = Command::new(executable);
            command.args(args.iter().map(AsRef::as_ref));
            command.current_dir(cwd);
            command
        };
        command
            .env_clear()
            .envs(filtered)
            .envs(&self.config.injected_env)
            .stdin(if pipe_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            command.process_group(0);
        }
        Ok(command)
    }
}

fn filter_env(allowlist: &BTreeSet<String>) -> BTreeMap<OsString, OsString> {
    let mut out = BTreeMap::new();
    for (key, value) in std::env::vars_os() {
        if let Some(name) = key.to_str() {
            if allowlist.contains(name) {
                out.insert(key, value);
            }
        }
    }
    out
}

fn build_bwrap_command(
    bwrap: &Path,
    sandbox: &SandboxMounts,
    network_allowed: bool,
    executable: &Path,
    args: &[impl AsRef<OsStr>],
    cwd: &Path,
) -> Result<Command, SupervisorError> {
    let main_repo = sandbox
        .main_repo
        .canonicalize()
        .map_err(|err| SupervisorError::Spawn(err.to_string()))?;
    let worktree = sandbox
        .worktree
        .canonicalize()
        .map_err(|err| SupervisorError::Spawn(err.to_string()))?;
    let cwd = cwd
        .canonicalize()
        .map_err(|err| SupervisorError::Spawn(err.to_string()))?;
    let executable = executable
        .canonicalize()
        .map_err(|err| SupervisorError::Spawn(err.to_string()))?;
    if !cwd.starts_with(&worktree) {
        return Err(SupervisorError::Spawn(
            "cwd must be inside the sandboxed worktree".into(),
        ));
    }

    let mut command = Command::new(bwrap);
    command.args([
        "--die-with-parent",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/bin",
        "/bin",
        "--ro-bind",
        "/lib",
        "/lib",
    ]);
    if Path::new("/lib64").exists() {
        command.args(["--ro-bind", "/lib64", "/lib64"]);
    }
    if Path::new("/etc/alternatives").exists() {
        command.args(["--ro-bind", "/etc/alternatives", "/etc/alternatives"]);
    }
    // Worker binary may live under the Cargo target dir; bind it read-only.
    command.arg("--ro-bind");
    command.arg(&executable);
    command.arg(&executable);
    // Read-only main repository; read-write assignment worktree.
    command.arg("--ro-bind");
    command.arg(&main_repo);
    command.arg(&main_repo);
    command.arg("--bind");
    command.arg(&worktree);
    command.arg(&worktree);
    command.arg("--chdir");
    command.arg(&cwd);
    if !network_allowed {
        command.arg("--unshare-net");
    }
    command.arg("--");
    command.arg(&executable);
    command.args(args.iter().map(AsRef::as_ref));
    Ok(command)
}

async fn kill_group_members(kill_bin: &Path, pgid: u32) {
    let output = Command::new("pgrep")
        .args(["-g", &pgid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;
    let Ok(output) = output else {
        return;
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(pid) = line.trim().parse::<u32>() else {
            continue;
        };
        let _ = Command::new(kill_bin)
            .args(["-KILL", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}

/// Returns true when any non-zombie process with the given PGID is still alive.
///
/// Zombies keep a PGID until reaped; treating them as alive makes cancel/timeout
/// assertions flake on slower CI runners.
pub fn process_group_alive(pgid: u32) -> bool {
    // Prefer /proc: avoid `ps` column quirks across distros.
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return fallback_process_group_alive(pgid);
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        if !pid_str.as_bytes().iter().all(u8::is_ascii_digit) {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // /proc/<pid>/stat: pid (comm) state ppid pgrp ...
        let Some(close) = stat.rfind(')') else {
            continue;
        };
        let mut rest = stat[close + 1..].split_whitespace();
        let state = rest.next().unwrap_or("");
        let _ppid = rest.next();
        let Some(pgrp) = rest.next() else {
            continue;
        };
        if pgrp == pgid.to_string() && state != "Z" {
            return true;
        }
    }
    false
}

fn fallback_process_group_alive(pgid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-o", "pgid=,stat=", "-A"])
        .output();
    let Ok(output) = output else {
        return false;
    };
    let needle = format!("{pgid}");
    String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        let mut parts = line.split_whitespace();
        let Some(found) = parts.next() else {
            return false;
        };
        if found != needle {
            return false;
        }
        let stat = parts.next().unwrap_or("");
        !stat.contains('Z')
    })
}

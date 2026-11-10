//! Isolated installed-CLI fixtures and PID cleanup for Unix daemon integration tests.
//! Every test owns its home and CLI selection; only immutable executable contents are shared.

use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Weak;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use codex_utils_cargo_bin::copy_executable;
use serde_json::Value;
use tempfile::TempDir;

pub(super) struct TestDaemon {
    pub(super) home: TempDir,
    pub(super) codex: PathBuf,
    pub(super) unmanaged: Option<Child>,
    _executable: Arc<PreparedCodex>,
}

struct PreparedCodex {
    _directory: TempDir,
    executable: PathBuf,
}

// Bazel's executable may live on a different volume from /tmp, so linking it
// directly into each home can fall back to thirteen multi-GB copies. Prepare
// one immutable executable on the fixture volume and share only its contents.
// Every daemon still owns its installation path, state, and processes.
fn prepared_codex(source: &Path) -> Result<Arc<PreparedCodex>> {
    static PREPARED: OnceLock<Mutex<Weak<PreparedCodex>>> = OnceLock::new();
    let mut slot = PREPARED
        .get_or_init(|| Mutex::new(Weak::new()))
        .lock()
        .map_err(|_| anyhow::anyhow!("prepared daemon executable lock poisoned"))?;
    if let Some(prepared) = slot.upgrade() {
        return Ok(prepared);
    }
    let directory = tempfile::Builder::new()
        .prefix("codex-daemon-executable-")
        .tempdir_in("/tmp")?;
    let executable = directory.path().join("codex");
    // Translating a separate inode preserves the shared CLI's Rosetta cache.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    copy_executable(source, &executable)?;
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    if std::fs::hard_link(source, &executable).is_err() {
        copy_executable(source, &executable)?;
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    ensure!(
        Command::new(&executable)
            .env("CODEX_HOME", directory.path())
            .arg("--version")
            .output()?
            .status
            .success(),
        "failed to translate managed test executable"
    );
    let prepared = Arc::new(PreparedCodex {
        _directory: directory,
        executable,
    });
    *slot = Arc::downgrade(&prepared);
    Ok(prepared)
}

impl TestDaemon {
    pub(super) fn new() -> Result<Self> {
        let home = tempfile::Builder::new().tempdir_in("/tmp")?;
        let codex = codex_utils_cargo_bin::cargo_bin("codex")?;
        let codex_source = std::fs::canonicalize(&codex)?;
        let target = if cfg!(target_os = "macos") {
            format!("{}-apple-darwin", std::env::consts::ARCH)
        } else {
            format!("{}-unknown-linux-musl", std::env::consts::ARCH)
        };
        let standalone = home.path().join("packages/standalone");
        let release_name = format!("0.0.0-{target}");
        let managed = standalone
            .join("releases")
            .join(&release_name)
            .join("bin/codex");
        std::fs::create_dir_all(managed.parent().context("managed bin parent")?)?;
        let executable = prepared_codex(&codex_source)?;
        std::fs::hard_link(&executable.executable, &managed)?;
        std::fs::write(standalone.join("auto-update-version"), &release_name)?;
        std::os::unix::fs::symlink(
            PathBuf::from("releases").join(release_name),
            standalone.join("current"),
        )?;
        // Model a daemon that was previously launched and is currently stopped.
        let state = home.path().join("app-server-daemon");
        std::fs::create_dir(&state)?;
        std::fs::write(state.join("app-server.stderr.log"), b"")?;
        Ok(Self {
            home,
            codex,
            unmanaged: None,
            _executable: executable,
        })
    }

    pub(super) fn command(&self) -> Command {
        let mut command = Command::new(&self.codex);
        command.env("CODEX_HOME", self.home.path());
        command
    }

    pub(super) fn lifecycle(&self, action: &str) -> Result<Value> {
        let output = self
            .command()
            .args(["app-server", "daemon", action])
            .output()?;
        ensure!(
            output.status.success(),
            "daemon {action} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    pub(super) fn pid(&self, name: &str) -> Result<u32> {
        let record = std::fs::read(self.home.path().join("app-server-daemon").join(name))
            .with_context(|| format!("failed to read {name}"))?;
        Ok(serde_json::from_slice::<Value>(&record)?["pid"]
            .as_u64()
            .context("pid missing")? as u32)
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.unmanaged.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = self.lifecycle("stop");
        if let Ok(pid) = self.pid("app-server-updater.pid") {
            let _ = signal(pid, libc::SIGTERM);
        }
    }
}

pub(super) fn signal(pid: u32, signal: libc::c_int) -> Result<()> {
    let raw_pid = libc::pid_t::try_from(pid).context("pid out of range")?;
    ensure!(
        unsafe { libc::kill(raw_pid, signal) } == 0,
        "failed to signal pid {pid}: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}

pub(super) fn wait_for_exit(pid: u32) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let output = Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .context("failed to invoke ps")?;
        let state = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() || state.trim().starts_with('Z') {
            return Ok(());
        }
        ensure!(Instant::now() < deadline, "pid {pid} did not exit: {state}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

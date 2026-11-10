//! Bounded stdout/stderr capture for supervised workers.
use crate::ManagedProcess;
use crate::ProcessSupervisor;
use crate::SupervisorError;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::time::timeout;

#[derive(Debug, Clone, Copy)]
pub struct IoLimits {
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

#[derive(Debug)]
pub struct CapturedOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl ProcessSupervisor {
    /// Wait for exit while reading stdout/stderr up to hard limits.
    pub async fn wait_capture(
        &self,
        managed: &mut ManagedProcess,
        limits: IoLimits,
    ) -> Result<CapturedOutput, SupervisorError> {
        let stdout = managed
            .stdout
            .take()
            .ok_or_else(|| SupervisorError::Spawn("stdout pipe missing".into()))?;
        let stderr = managed
            .stderr
            .take()
            .ok_or_else(|| SupervisorError::Spawn("stderr pipe missing".into()))?;

        let collect = async {
            let stdout_task = read_limited(stdout, limits.max_stdout_bytes, true);
            let stderr_task = read_limited(stderr, limits.max_stderr_bytes, false);
            let status_task = managed.child.wait();
            let (stdout, stderr, status) = tokio::join!(stdout_task, stderr_task, status_task);
            let stdout = stdout?;
            let stderr = stderr?;
            let status = status.map_err(SupervisorError::Io)?;
            let exit_code = status.code().unwrap_or(-1);
            Ok::<CapturedOutput, SupervisorError>(CapturedOutput {
                exit_code,
                stdout,
                stderr,
            })
        };

        match timeout(self.config.timeout, collect).await {
            Ok(result) => result,
            Err(_) => {
                let _ = self.kill_group(managed).await;
                Err(SupervisorError::Timeout(self.config.timeout))
            }
        }
    }

    /// Spawn, optionally write stdin, then capture bounded output until exit.
    pub async fn run_capture(
        &self,
        executable: &std::path::Path,
        args: &[impl AsRef<std::ffi::OsStr>],
        cwd: &std::path::Path,
        stdin_bytes: Option<&[u8]>,
        limits: IoLimits,
    ) -> Result<CapturedOutput, SupervisorError> {
        let mut managed = self
            .spawn_with_stdin(executable, args, cwd, stdin_bytes.is_some())
            .await?;
        if let Some(bytes) = stdin_bytes {
            if let Some(mut stdin) = managed.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(bytes).await.map_err(SupervisorError::Io)?;
                stdin.shutdown().await.map_err(SupervisorError::Io)?;
            }
        }
        self.wait_capture(&mut managed, limits).await
    }
}

async fn read_limited(
    mut reader: impl AsyncReadExt + Unpin,
    limit: usize,
    is_stdout: bool,
) -> Result<Vec<u8>, SupervisorError> {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let n = reader.read(&mut chunk).await.map_err(SupervisorError::Io)?;
        if n == 0 {
            break;
        }
        if buf.len().saturating_add(n) > limit {
            return Err(if is_stdout {
                SupervisorError::StdoutLimit(limit)
            } else {
                SupervisorError::StderrLimit(limit)
            });
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

/// Convenience for short version probes.
pub fn version_probe_timeout() -> Duration {
    Duration::from_secs(5)
}

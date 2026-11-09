//! Shared CLI worker executor used by OMP/DSH adapters.
use crate::ManagedProcess;
use crate::ProcessSupervisor;
use crate::SupervisorConfig;
use crate::SupervisorError;
use crate::capture::IoLimits;
use forge_agent_backend::AdapterSpec;
use forge_agent_backend::AgentExecutor;
use forge_agent_backend::BackendError;
use forge_agent_backend::BackendFuture;
use forge_agent_protocol::ExecutionRequest;
use forge_agent_protocol::MAX_STDERR_BYTES;
use forge_agent_protocol::MAX_STDOUT_BYTES;
use forge_agent_protocol::PROTOCOL_VERSION;
use forge_agent_protocol::WorkerEvent;
use forge_agent_protocol::WorkerProtocolError;
use forge_agent_protocol::reject_stderr;
use forge_domain::AgentSession;
use forge_domain::BackendKind;
use forge_domain::ExecutionReport;
use forge_domain::ReportId;
use forge_domain::SessionId;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

pub type EventParser = fn(&[u8]) -> Result<(WorkerEvent, WorkerEvent), WorkerProtocolError>;

#[derive(Clone)]
pub struct CliExecutorConfig {
    pub kind: BackendKind,
    pub spec: AdapterSpec,
    pub version_prefix: String,
    pub version_args: Vec<String>,
    pub run_args: Vec<String>,
    pub parse_events: EventParser,
    pub supervisor: SupervisorConfig,
    pub io_limits: IoLimits,
}

struct RunningSession {
    process: ManagedProcess,
    request: ExecutionRequest,
}

/// Process-backed AgentExecutor with version probe, stdin request, and JSONL capture.
pub struct CliWorkerExecutor {
    config: CliExecutorConfig,
    supervisor: ProcessSupervisor,
    sessions: Mutex<HashMap<SessionId, RunningSession>>,
}

impl CliWorkerExecutor {
    pub fn new(config: CliExecutorConfig) -> Self {
        let supervisor = ProcessSupervisor::new(config.supervisor.clone());
        Self {
            config,
            supervisor,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    async fn probe_version(&self, cwd: &std::path::Path) -> Result<String, BackendError> {
        let output = self
            .supervisor
            .run_capture(
                &self.config.spec.executable,
                &self.config.version_args,
                cwd,
                None,
                IoLimits {
                    max_stdout_bytes: 16 << 10,
                    max_stderr_bytes: MAX_STDERR_BYTES,
                },
            )
            .await
            .map_err(map_supervisor)?;
        reject_stderr(&output.stderr).map_err(map_protocol)?;
        if output.exit_code != 0 {
            return Err(BackendError::Execution(format!(
                "version probe exited {}",
                output.exit_code
            )));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text.lines().next().unwrap_or("").trim();
        if !line.starts_with(&self.config.version_prefix) {
            return Err(BackendError::Execution(format!(
                "unexpected version banner: {line}"
            )));
        }
        Ok(line.to_owned())
    }

    async fn start_inner(&self, request: ExecutionRequest) -> Result<AgentSession, BackendError> {
        if request.version != PROTOCOL_VERSION {
            return Err(BackendError::Execution(
                "unsupported execution request version".into(),
            ));
        }
        if request.assignment.backend != self.config.kind {
            return Err(BackendError::Execution(
                "assignment backend does not match executor".into(),
            ));
        }
        request
            .assignment
            .workspace
            .validate()
            .map_err(|err| BackendError::Execution(err.to_string()))?;

        let cwd = &request.assignment.workspace.directory;
        let version = self.probe_version(cwd).await?;

        let payload =
            serde_json::to_vec(&request).map_err(|err| BackendError::Execution(err.to_string()))?;
        let mut args = self.config.run_args.clone();
        args.extend(self.config.spec.arguments.iter().cloned());

        let mut managed = self
            .supervisor
            .spawn_with_stdin(&self.config.spec.executable, &args, cwd, true)
            .await
            .map_err(map_supervisor)?;
        if let Some(mut stdin) = managed.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(&payload)
                .await
                .map_err(|err| BackendError::Execution(err.to_string()))?;
            stdin
                .shutdown()
                .await
                .map_err(|err| BackendError::Execution(err.to_string()))?;
        }

        let session = AgentSession {
            id: SessionId::new(),
            assignment_id: request.assignment.id,
            backend_session_id: Some(format!("{}:{}", managed.pid(), version)),
        };
        self.sessions
            .lock()
            .map_err(|_| BackendError::Execution("session mutex poisoned".into()))?
            .insert(
                session.id,
                RunningSession {
                    process: managed,
                    request,
                },
            );
        Ok(session)
    }

    async fn wait_inner(&self, session: &AgentSession) -> Result<ExecutionReport, BackendError> {
        let mut running = {
            let mut guard = self
                .sessions
                .lock()
                .map_err(|_| BackendError::Execution("session mutex poisoned".into()))?;
            guard
                .remove(&session.id)
                .ok_or_else(|| BackendError::Execution("unknown session".into()))?
        };
        let captured = self
            .supervisor
            .wait_capture(&mut running.process, self.config.io_limits)
            .await
            .map_err(map_supervisor)?;
        reject_stderr(&captured.stderr).map_err(map_protocol)?;
        let (_hello, result) =
            (self.config.parse_events)(&captured.stdout).map_err(map_protocol)?;
        let WorkerEvent::Result {
            exit_code,
            summary,
            head_commit,
            changed_files,
        } = result
        else {
            return Err(BackendError::Execution("missing result event".into()));
        };
        if exit_code != captured.exit_code {
            return Err(BackendError::Execution(format!(
                "result exit_code {exit_code} != process status {}",
                captured.exit_code
            )));
        }
        Ok(ExecutionReport {
            id: ReportId::new(),
            task_id: running.request.assignment.task_id,
            assignment_id: running.request.assignment.id,
            exit_code: captured.exit_code,
            summary,
            head_commit,
            changed_files,
        })
    }

    async fn cancel_inner(&self, session: &AgentSession) -> Result<(), BackendError> {
        let mut running = {
            let mut guard = self
                .sessions
                .lock()
                .map_err(|_| BackendError::Execution("session mutex poisoned".into()))?;
            guard
                .remove(&session.id)
                .ok_or_else(|| BackendError::Execution("unknown session".into()))?
        };
        self.supervisor
            .cancel(&mut running.process)
            .await
            .map_err(map_supervisor)?;
        Ok(())
    }
}

impl AgentExecutor for CliWorkerExecutor {
    fn kind(&self) -> BackendKind {
        self.config.kind
    }

    fn start(&self, request: ExecutionRequest) -> BackendFuture<'_, AgentSession> {
        Box::pin(self.start_inner(request))
    }

    fn wait<'a>(&'a self, session: &'a AgentSession) -> BackendFuture<'a, ExecutionReport> {
        Box::pin(self.wait_inner(session))
    }

    fn cancel<'a>(&'a self, session: &'a AgentSession) -> BackendFuture<'a, ()> {
        Box::pin(self.cancel_inner(session))
    }
}

fn map_supervisor(err: SupervisorError) -> BackendError {
    match err {
        SupervisorError::StdoutLimit(n) => {
            BackendError::Execution(WorkerProtocolError::StdoutLimit(n).to_string())
        }
        SupervisorError::StderrLimit(n) => {
            BackendError::Execution(WorkerProtocolError::StderrLimit(n).to_string())
        }
        SupervisorError::Cancelled => BackendError::Execution("worker was cancelled".into()),
        SupervisorError::Timeout(d) => {
            BackendError::Execution(format!("worker timed out after {d:?}"))
        }
        other => BackendError::Execution(other.to_string()),
    }
}

fn map_protocol(err: WorkerProtocolError) -> BackendError {
    BackendError::Execution(err.to_string())
}

pub fn default_io_limits() -> IoLimits {
    IoLimits {
        max_stdout_bytes: MAX_STDOUT_BYTES,
        max_stderr_bytes: MAX_STDERR_BYTES,
    }
}

pub fn supervisor_for_request(
    request: &ExecutionRequest,
    base: SupervisorConfig,
) -> SupervisorConfig {
    let mut config = base;
    config.timeout = Duration::from_secs(request.mandate.timeout_seconds.max(1));
    config.network_allowed = request.mandate.allow_network;
    if !request.mandate.environment_names.is_empty() {
        config.environment_allowlist = request.mandate.environment_names.clone();
    }
    config
}

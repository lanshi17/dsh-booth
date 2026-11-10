//! OMP CLI adapter. Speaks Forge canonical JSONL; never links Node.
use forge_agent_backend::AdapterSpec;
use forge_agent_backend::AgentExecutor;
use forge_agent_backend::BackendFuture;
use forge_agent_protocol::ExecutionRequest;
use forge_agent_protocol::parse_omp_jsonl;
use forge_agent_runtime::CliExecutorConfig;
use forge_agent_runtime::CliWorkerExecutor;
use forge_agent_runtime::SupervisorConfig;
use forge_agent_runtime::default_io_limits;
use forge_domain::AgentSession;
use forge_domain::BackendKind;
use forge_domain::ExecutionReport;
use std::path::PathBuf;

pub const CLI_VERSION_PREFIX: &str = "omp-cli ";
pub const FAKE_CLI_VERSION: &str = "omp-cli 1.0.0-forge";

pub fn specification(executable: PathBuf, arguments: Vec<String>) -> AdapterSpec {
    AdapterSpec {
        kind: BackendKind::Omp,
        executable,
        arguments,
    }
}

pub struct OmpExecutor {
    inner: CliWorkerExecutor,
}

impl OmpExecutor {
    pub fn new(spec: AdapterSpec, supervisor: SupervisorConfig) -> Self {
        debug_assert_eq!(spec.kind, BackendKind::Omp);
        Self {
            inner: CliWorkerExecutor::new(CliExecutorConfig {
                kind: BackendKind::Omp,
                spec,
                version_prefix: CLI_VERSION_PREFIX.to_owned(),
                version_args: vec!["--version".to_owned()],
                run_args: vec!["run".to_owned()],
                parse_events: parse_omp_jsonl,
                supervisor,
                io_limits: default_io_limits(),
            }),
        }
    }
}

impl AgentExecutor for OmpExecutor {
    fn kind(&self) -> BackendKind {
        BackendKind::Omp
    }

    fn start(&self, request: ExecutionRequest) -> BackendFuture<'_, AgentSession> {
        self.inner.start(request)
    }

    fn wait<'a>(&'a self, session: &'a AgentSession) -> BackendFuture<'a, ExecutionReport> {
        self.inner.wait(session)
    }

    fn cancel<'a>(&'a self, session: &'a AgentSession) -> BackendFuture<'a, ()> {
        self.inner.cancel(session)
    }
}

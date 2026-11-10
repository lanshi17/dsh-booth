use forge_agent_protocol::ExecutionRequest;
use forge_domain::AgentSession;
use forge_domain::BackendKind;
use forge_domain::ExecutionReport;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use thiserror::Error;

pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BackendError>> + Send + 'a>>;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend capability has not been implemented: {0}")]
    Unavailable(String),
    #[error("worker execution failed: {0}")]
    Execution(String),
}

/// Object-safe port. Implementations must use argv arrays and a host supervisor.
pub trait AgentExecutor: Send + Sync {
    fn kind(&self) -> BackendKind;
    fn start(&self, request: ExecutionRequest) -> BackendFuture<'_, AgentSession>;
    fn wait<'a>(&'a self, session: &'a AgentSession) -> BackendFuture<'a, ExecutionReport>;
    fn cancel<'a>(&'a self, session: &'a AgentSession) -> BackendFuture<'a, ()>;
}

#[derive(Debug, Clone)]
pub struct AdapterSpec {
    pub kind: BackendKind,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
}

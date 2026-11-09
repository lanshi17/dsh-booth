//! Framework-independent domain model. Worker completion never grants merge authority.
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

macro_rules! id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(pub Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}
id!(TaskId);
id!(AssignmentId);
id!(ReportId);
id!(SessionId);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PrincipalId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Omp,
    Dsh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Source {
    Cli,
    Mcp,
    Repository { location: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
    Manual,
    Schedule { key: String },
    Event { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMandate {
    pub allowed_backends: BTreeSet<BackendKind>,
    pub allow_network: bool,
    pub environment_names: BTreeSet<String>,
    pub timeout_seconds: u64,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLease {
    pub directory: PathBuf,
    pub branch: String,
    pub base_commit: String,
}

impl WorkspaceLease {
    pub fn validate(&self) -> Result<(), DomainError> {
        if !self.directory.is_absolute()
            || self.directory.parent().is_none()
            || !self.branch.starts_with("forge/")
            || !valid_commit(&self.base_commit)
        {
            return Err(DomainError::InvalidWorkspace);
        }
        Ok(())
    }
}

pub fn valid_commit(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64) && commit.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Accepted,
    Authorized,
    Running,
    AwaitingReview,
    Approved,
    Rejected,
    Failed,
    Cancelled,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: SessionId,
    pub assignment_id: AssignmentId,
    pub backend_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub id: AssignmentId,
    pub task_id: TaskId,
    pub backend: BackendKind,
    pub worker: PrincipalId,
    pub workspace: WorkspaceLease,
    pub status: AssignmentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub id: ReportId,
    pub task_id: TaskId,
    pub assignment_id: AssignmentId,
    pub exit_code: i32,
    pub summary: String,
    pub head_commit: String,
    pub changed_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub assignment_id: AssignmentId,
    pub report_id: ReportId,
    pub reviewed_head: String,
    pub reviewer: PrincipalId,
    pub decision: ReviewDecision,
    pub evidence: String,
}

/// Only application code may turn an authenticated request into a state transition.
/// Serialization for durable snapshots is separate from worker report ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    id: TaskId,
    description: String,
    source: Source,
    trigger: Trigger,
    status: TaskStatus,
    mandate: Option<TaskMandate>,
    assignments: Vec<Assignment>,
    reports: Vec<ExecutionReport>,
    reviews: Vec<ReviewRecord>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("transition is not valid for the current task state")]
    InvalidTransition,
    #[error("mandate is empty, expired, or does not permit this backend")]
    InvalidMandate,
    #[error("workspace must have an absolute directory, forge/ branch and base commit")]
    InvalidWorkspace,
    #[error("workspace directory or branch was already assigned")]
    WorkspaceReused,
    #[error("report does not belong to the active assignment or has an invalid commit")]
    InvalidReport,
    #[error("review must be independent and bind the current report and commit")]
    InvalidReview,
    #[error("merge is only allowed from an approved task that has not already merged")]
    InvalidMerge,
}

impl Task {
    pub fn accept(description: String, source: Source, trigger: Trigger) -> Self {
        Self {
            id: TaskId::new(),
            description,
            source,
            trigger,
            status: TaskStatus::Accepted,
            mandate: None,
            assignments: vec![],
            reports: vec![],
            reviews: vec![],
        }
    }
    pub fn id(&self) -> TaskId {
        self.id
    }
    pub fn status(&self) -> TaskStatus {
        self.status
    }
    pub fn assignments(&self) -> &[Assignment] {
        &self.assignments
    }
    pub fn reports(&self) -> &[ExecutionReport] {
        &self.reports
    }
    pub fn reviews(&self) -> &[ReviewRecord] {
        &self.reviews
    }

    pub fn authorize(&mut self, mandate: TaskMandate, now: u64) -> Result<(), DomainError> {
        if self.status != TaskStatus::Accepted {
            return Err(DomainError::InvalidTransition);
        }
        if mandate.allowed_backends.is_empty()
            || mandate.timeout_seconds == 0
            || mandate.expires_at_unix <= now
        {
            return Err(DomainError::InvalidMandate);
        }
        self.mandate = Some(mandate);
        self.status = TaskStatus::Authorized;
        Ok(())
    }

    pub fn assign(
        &mut self,
        backend: BackendKind,
        worker: PrincipalId,
        workspace: WorkspaceLease,
        now: u64,
    ) -> Result<Assignment, DomainError> {
        if !matches!(
            self.status,
            TaskStatus::Authorized | TaskStatus::Failed | TaskStatus::Rejected
        ) {
            return Err(DomainError::InvalidTransition);
        }
        let mandate = self.mandate.as_ref().ok_or(DomainError::InvalidMandate)?;
        if mandate.expires_at_unix <= now || !mandate.allowed_backends.contains(&backend) {
            return Err(DomainError::InvalidMandate);
        }
        workspace.validate()?;
        if self.assignments.iter().any(|a| {
            a.workspace.directory == workspace.directory || a.workspace.branch == workspace.branch
        }) {
            return Err(DomainError::WorkspaceReused);
        }
        let assignment = Assignment {
            id: AssignmentId::new(),
            task_id: self.id,
            backend,
            worker,
            workspace,
            status: AssignmentStatus::Running,
        };
        self.assignments.push(assignment.clone());
        self.status = TaskStatus::Running;
        Ok(assignment)
    }

    pub fn complete(&mut self, report: ExecutionReport) -> Result<(), DomainError> {
        if self.status != TaskStatus::Running {
            return Err(DomainError::InvalidTransition);
        }
        let assignment = self
            .assignments
            .last_mut()
            .ok_or(DomainError::InvalidReport)?;
        if report.task_id != self.id
            || report.assignment_id != assignment.id
            || !valid_commit(&report.head_commit)
            || self.reports.iter().any(|existing| existing.id == report.id)
        {
            return Err(DomainError::InvalidReport);
        }
        let succeeded = report.exit_code == 0;
        assignment.status = if succeeded {
            AssignmentStatus::Completed
        } else {
            AssignmentStatus::Failed
        };
        self.reports.push(report);
        self.status = if succeeded {
            TaskStatus::AwaitingReview
        } else {
            TaskStatus::Failed
        };
        Ok(())
    }

    pub fn review(&mut self, review: ReviewRecord) -> Result<(), DomainError> {
        if self.status != TaskStatus::AwaitingReview {
            return Err(DomainError::InvalidTransition);
        }
        let report = self.reports.last().ok_or(DomainError::InvalidReview)?;
        let assignment = self.assignments.last().ok_or(DomainError::InvalidReview)?;
        if review.reviewer == assignment.worker
            || review.assignment_id != assignment.id
            || review.report_id != report.id
            || review.reviewed_head != report.head_commit
            || review.evidence.trim().is_empty()
        {
            return Err(DomainError::InvalidReview);
        }
        self.status = match review.decision {
            ReviewDecision::Approve => TaskStatus::Approved,
            ReviewDecision::Reject => TaskStatus::Rejected,
        };
        self.reviews.push(review);
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), DomainError> {
        if !matches!(self.status, TaskStatus::Authorized | TaskStatus::Running) {
            return Err(DomainError::InvalidTransition);
        }
        if let Some(a) = self.assignments.last_mut() {
            a.status = AssignmentStatus::Cancelled;
        }
        self.status = TaskStatus::Cancelled;
        Ok(())
    }

    pub fn mark_merged(&mut self, merged_head: &str) -> Result<(), DomainError> {
        if self.status != TaskStatus::Approved {
            return Err(DomainError::InvalidMerge);
        }
        if !valid_commit(merged_head) {
            return Err(DomainError::InvalidMerge);
        }
        self.status = TaskStatus::Merged;
        Ok(())
    }
}

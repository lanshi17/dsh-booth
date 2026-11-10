//! Trusted application boundary. Caller identities must come from host authentication.
use forge_domain::*;
use forge_identity::MergeAuthClaims;
use forge_identity::MergeAuthKeyPair;
use forge_persistence::MergeAuthRecord;
use forge_persistence::TursoTaskRepository;
use forge_review::MergeError;
use forge_review::ReviewError;
use forge_review::ReviewEvidence;
use forge_review::merge_approved_branch;
use forge_review::push_branch;
use forge_review::verify_merge_cas;
use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct Policy {
    pub authorizers: BTreeSet<PrincipalId>,
    pub reviewers: BTreeSet<PrincipalId>,
    pub merge_authorizers: BTreeSet<PrincipalId>,
}

pub struct TaskService {
    policy: Policy,
    /// When set, merge grants are Ed25519-signed and verified before consume.
    merge_keys: Option<Arc<MergeAuthKeyPair>>,
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("caller is not permitted to perform this action")]
    Forbidden,
    #[error("merge authorization signature is invalid or missing")]
    InvalidSignature,
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Review(#[from] ReviewError),
    #[error(transparent)]
    Merge(#[from] MergeError),
    #[error("storage error: {0}")]
    Storage(#[from] io::Error),
}

/// Inputs for issuing a time-limited merge grant.
#[derive(Debug, Clone, Copy)]
pub struct MergeAuthRequest<'a> {
    pub base_commit: &'a str,
    pub target_branch: &'a str,
    pub now: u64,
    pub ttl_seconds: u64,
}

/// Inputs for consuming a grant and merging (optionally pushing).
#[derive(Debug, Clone, Copy)]
pub struct MergeExecuteRequest<'a> {
    pub auth_id: Uuid,
    pub repo: &'a Path,
    pub now: u64,
    pub remote: Option<&'a str>,
}

/// Time-limited, single-use merge grant. Not a git operation by itself.
#[derive(Debug, Clone)]
pub struct MergeAuthorization {
    pub id: Uuid,
    pub task_id: TaskId,
    pub assignment_id: AssignmentId,
    pub head_commit: String,
    pub base_commit: String,
    pub target_branch: String,
    pub source_branch: String,
    pub authorized_by: PrincipalId,
    pub expires_at_unix: u64,
    /// Base64url Ed25519 signature over canonical claims; required when keys are configured.
    pub signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredMergeAuthorization {
    pub auth: MergeAuthorization,
    pub consumed: bool,
}

pub trait MergeAuthStore: Send + Sync {
    fn insert(&self, auth: &MergeAuthorization) -> Result<(), ApplicationError>;
    fn get(&self, id: Uuid) -> Result<Option<StoredMergeAuthorization>, ApplicationError>;
    /// CAS: consume only if present, unexpired, and not yet consumed.
    fn try_consume(&self, id: Uuid, now: u64) -> Result<MergeAuthorization, ApplicationError>;
}

/// In-memory store used by tests and single-process hosts.
#[derive(Default)]
pub struct MemoryMergeAuthStore {
    inner: std::sync::Mutex<std::collections::HashMap<Uuid, StoredMergeAuthorization>>,
}

impl MergeAuthStore for MemoryMergeAuthStore {
    fn insert(&self, auth: &MergeAuthorization) -> Result<(), ApplicationError> {
        let mut guard = self.inner.lock().map_err(|_| ApplicationError::Forbidden)?;
        if guard.contains_key(&auth.id) {
            return Err(MergeError::Consumed.into());
        }
        guard.insert(
            auth.id,
            StoredMergeAuthorization {
                auth: auth.clone(),
                consumed: false,
            },
        );
        Ok(())
    }

    fn get(&self, id: Uuid) -> Result<Option<StoredMergeAuthorization>, ApplicationError> {
        let guard = self.inner.lock().map_err(|_| ApplicationError::Forbidden)?;
        Ok(guard.get(&id).cloned())
    }

    fn try_consume(&self, id: Uuid, now: u64) -> Result<MergeAuthorization, ApplicationError> {
        let mut guard = self.inner.lock().map_err(|_| ApplicationError::Forbidden)?;
        let stored = guard.get_mut(&id).ok_or(MergeError::MissingAuth)?;
        if stored.consumed {
            return Err(MergeError::Consumed.into());
        }
        if stored.auth.expires_at_unix <= now {
            return Err(MergeError::Expired.into());
        }
        stored.consumed = true;
        Ok(stored.auth.clone())
    }
}

/// Durable merge-auth store backed by Turso (`merge_authorizations` table).
pub struct DurableMergeAuthStore {
    repo: Arc<TursoTaskRepository>,
}

impl DurableMergeAuthStore {
    pub fn new(repo: Arc<TursoTaskRepository>) -> Self {
        Self { repo }
    }
}

impl MergeAuthStore for DurableMergeAuthStore {
    fn insert(&self, auth: &MergeAuthorization) -> Result<(), ApplicationError> {
        self.repo
            .insert_merge_auth(&to_record(auth))
            .map_err(map_merge_store_err)
    }

    fn get(&self, id: Uuid) -> Result<Option<StoredMergeAuthorization>, ApplicationError> {
        Ok(self
            .repo
            .get_merge_auth(id)
            .map_err(map_merge_store_err)?
            .map(|stored| StoredMergeAuthorization {
                auth: from_record(stored.record),
                consumed: stored.consumed,
            }))
    }

    fn try_consume(&self, id: Uuid, now: u64) -> Result<MergeAuthorization, ApplicationError> {
        Ok(from_record(
            self.repo
                .try_consume_merge_auth(id, now)
                .map_err(map_merge_store_err)?,
        ))
    }
}

fn to_record(auth: &MergeAuthorization) -> MergeAuthRecord {
    MergeAuthRecord {
        id: auth.id,
        task_id: auth.task_id,
        assignment_id: auth.assignment_id,
        head_commit: auth.head_commit.clone(),
        base_commit: auth.base_commit.clone(),
        target_branch: auth.target_branch.clone(),
        source_branch: auth.source_branch.clone(),
        authorized_by: auth.authorized_by.clone(),
        expires_at_unix: auth.expires_at_unix,
        signature: auth.signature.clone(),
    }
}

fn from_record(record: MergeAuthRecord) -> MergeAuthorization {
    MergeAuthorization {
        id: record.id,
        task_id: record.task_id,
        assignment_id: record.assignment_id,
        head_commit: record.head_commit,
        base_commit: record.base_commit,
        target_branch: record.target_branch,
        source_branch: record.source_branch,
        authorized_by: record.authorized_by,
        expires_at_unix: record.expires_at_unix,
        signature: record.signature,
    }
}

fn claims_from(auth: &MergeAuthorization) -> MergeAuthClaims {
    MergeAuthClaims {
        id: auth.id,
        task_id: auth.task_id.0,
        assignment_id: auth.assignment_id.0,
        head_commit: auth.head_commit.clone(),
        base_commit: auth.base_commit.clone(),
        target_branch: auth.target_branch.clone(),
        source_branch: auth.source_branch.clone(),
        authorized_by: auth.authorized_by.0.clone(),
        expires_at_unix: auth.expires_at_unix,
    }
}

fn map_merge_store_err(err: io::Error) -> ApplicationError {
    match err.kind() {
        io::ErrorKind::NotFound => MergeError::MissingAuth.into(),
        io::ErrorKind::AlreadyExists => MergeError::Consumed.into(),
        io::ErrorKind::TimedOut => MergeError::Expired.into(),
        _ => ApplicationError::Storage(err),
    }
}

impl TaskService {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            merge_keys: None,
        }
    }

    pub fn with_merge_keys(mut self, keys: Arc<MergeAuthKeyPair>) -> Self {
        self.merge_keys = Some(keys);
        self
    }

    pub fn authorize(
        &self,
        caller: &PrincipalId,
        task: &mut Task,
        mandate: TaskMandate,
        now: u64,
    ) -> Result<(), ApplicationError> {
        if !self.policy.authorizers.contains(caller) {
            return Err(ApplicationError::Forbidden);
        }
        Ok(task.authorize(mandate, now)?)
    }

    pub fn review(
        &self,
        caller: &PrincipalId,
        task: &mut Task,
        review: ReviewRecord,
        evidence: &ReviewEvidence,
    ) -> Result<(), ApplicationError> {
        if !self.policy.reviewers.contains(caller) || caller != &review.reviewer {
            return Err(ApplicationError::Forbidden);
        }
        match review.decision {
            ReviewDecision::Approve => evidence.validate(&review.reviewed_head)?,
            ReviewDecision::Reject => evidence.bind_commit(&review.reviewed_head)?,
        }
        Ok(task.review(review)?)
    }

    pub fn complete(
        &self,
        caller: &PrincipalId,
        task: &mut Task,
        report: ExecutionReport,
    ) -> Result<(), ApplicationError> {
        if task
            .assignments()
            .last()
            .is_none_or(|a| &a.worker != caller)
        {
            return Err(ApplicationError::Forbidden);
        }
        Ok(task.complete(report)?)
    }

    pub fn authorize_merge(
        &self,
        caller: &PrincipalId,
        task: &Task,
        request: MergeAuthRequest<'_>,
        store: &dyn MergeAuthStore,
    ) -> Result<MergeAuthorization, ApplicationError> {
        if !self.policy.merge_authorizers.contains(caller) || task.status() != TaskStatus::Approved
        {
            return Err(ApplicationError::Forbidden);
        }
        if request.ttl_seconds == 0 || !valid_commit(request.base_commit) {
            return Err(ApplicationError::Forbidden);
        }
        let review = task.reviews().last().ok_or(ApplicationError::Forbidden)?;
        let assignment = task
            .assignments()
            .last()
            .ok_or(ApplicationError::Forbidden)?;
        let mut auth = MergeAuthorization {
            id: Uuid::new_v4(),
            task_id: task.id(),
            assignment_id: review.assignment_id,
            head_commit: review.reviewed_head.clone(),
            base_commit: request.base_commit.to_owned(),
            target_branch: request.target_branch.to_owned(),
            source_branch: assignment.workspace.branch.clone(),
            authorized_by: caller.clone(),
            expires_at_unix: request.now.saturating_add(request.ttl_seconds),
            signature: None,
        };
        if let Some(keys) = &self.merge_keys {
            let sig = keys
                .sign(&claims_from(&auth))
                .map_err(|_| ApplicationError::InvalidSignature)?;
            auth.signature = Some(sig);
        }
        store.insert(&auth)?;
        Ok(auth)
    }

    fn verify_auth_signature(&self, auth: &MergeAuthorization) -> Result<(), ApplicationError> {
        match &self.merge_keys {
            None => Ok(()),
            Some(keys) => {
                let Some(sig) = auth.signature.as_deref() else {
                    return Err(ApplicationError::InvalidSignature);
                };
                keys.verify(&claims_from(auth), sig)
                    .map_err(|_| ApplicationError::InvalidSignature)
            }
        }
    }

    /// Consume a one-time authorization and perform a real Git merge under HEAD/base CAS.
    ///
    /// When `request.remote` is set, also push the target branch via argv-only `git push`.
    pub fn execute_merge(
        &self,
        caller: &PrincipalId,
        task: &mut Task,
        request: MergeExecuteRequest<'_>,
        store: &dyn MergeAuthStore,
    ) -> Result<String, ApplicationError> {
        if !self.policy.merge_authorizers.contains(caller) {
            return Err(ApplicationError::Forbidden);
        }
        if task.status() != TaskStatus::Approved {
            return Err(DomainError::InvalidMerge.into());
        }
        let stored = store.get(request.auth_id)?.ok_or(MergeError::MissingAuth)?;
        if stored.auth.authorized_by != *caller || stored.auth.task_id != task.id() {
            return Err(ApplicationError::Forbidden);
        }
        if stored.consumed {
            return Err(MergeError::Consumed.into());
        }
        if stored.auth.expires_at_unix <= request.now {
            return Err(MergeError::Expired.into());
        }
        self.verify_auth_signature(&stored.auth)?;
        // Validate CAS before consuming so failed attempts do not burn the grant.
        verify_merge_cas(
            request.repo,
            &stored.auth.target_branch,
            &stored.auth.source_branch,
            &stored.auth.base_commit,
            &stored.auth.head_commit,
        )?;
        let auth = store.try_consume(request.auth_id, request.now)?;
        // Re-verify after consume against the consumed record (signature must still match).
        self.verify_auth_signature(&auth)?;
        let merged = merge_approved_branch(
            request.repo,
            &auth.target_branch,
            &auth.source_branch,
            &auth.base_commit,
            &auth.head_commit,
        )?;
        if let Some(remote) = request.remote {
            push_branch(request.repo, remote, &auth.target_branch)?;
        }
        task.mark_merged(&merged)?;
        Ok(merged)
    }
}

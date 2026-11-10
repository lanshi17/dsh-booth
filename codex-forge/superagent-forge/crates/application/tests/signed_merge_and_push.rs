use forge_application::*;
use forge_domain::*;
use forge_identity::MergeAuthKeyPair;
use forge_review::CheckResult;
use forge_review::ReviewEvidence;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;
use uuid::Uuid;

fn principal(name: &str) -> PrincipalId {
    PrincipalId(name.into())
}

fn service_with(keys: Arc<MergeAuthKeyPair>) -> TaskService {
    TaskService::new(Policy {
        authorizers: BTreeSet::from([principal("owner")]),
        reviewers: BTreeSet::from([principal("reviewer")]),
        merge_authorizers: BTreeSet::from([principal("owner")]),
    })
    .with_merge_keys(keys)
}

fn git(cwd: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .unwrap()
            .success()
    );
}

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn approved_task(svc: &TaskService, repo: &Path, base: &str, branch: &str, head: &str) -> Task {
    let mut task = Task::accept("signed".into(), Source::Cli, Trigger::Manual);
    svc.authorize(
        &principal("owner"),
        &mut task,
        TaskMandate {
            allowed_backends: BTreeSet::from([BackendKind::Omp]),
            allow_network: false,
            environment_names: BTreeSet::new(),
            timeout_seconds: 30,
            expires_at_unix: 10_000,
        },
        1,
    )
    .unwrap();
    task.assign(
        BackendKind::Omp,
        principal("worker"),
        WorkspaceLease {
            directory: repo.to_path_buf(),
            branch: branch.to_owned(),
            base_commit: base.to_owned(),
        },
        2,
    )
    .unwrap();
    let report = ExecutionReport {
        id: ReportId::new(),
        task_id: task.id(),
        assignment_id: task.assignments()[0].id,
        exit_code: 0,
        summary: "ok".into(),
        head_commit: head.to_owned(),
        changed_files: vec![],
    };
    svc.complete(&principal("worker"), &mut task, report)
        .unwrap();
    let evidence = ReviewEvidence {
        observed_head: head.to_owned(),
        observed_base: base.to_owned(),
        diff_inspected: true,
        diff_stat: "ok".into(),
        checks: vec![CheckResult {
            command: vec!["true".into()],
            exit_code: 0,
        }],
    };
    let review = ReviewRecord {
        assignment_id: task.assignments()[0].id,
        report_id: task.reports()[0].id,
        reviewed_head: head.to_owned(),
        reviewer: principal("reviewer"),
        decision: ReviewDecision::Approve,
        evidence: "host".into(),
    };
    svc.review(&principal("reviewer"), &mut task, review, &evidence)
        .unwrap();
    task
}

#[derive(Default)]
struct TamperStore {
    inner: Mutex<HashMap<Uuid, StoredMergeAuthorization>>,
}

impl MergeAuthStore for TamperStore {
    fn insert(&self, auth: &MergeAuthorization) -> Result<(), ApplicationError> {
        self.inner.lock().unwrap().insert(
            auth.id,
            StoredMergeAuthorization {
                auth: auth.clone(),
                consumed: false,
            },
        );
        Ok(())
    }

    fn get(&self, id: Uuid) -> Result<Option<StoredMergeAuthorization>, ApplicationError> {
        Ok(self.inner.lock().unwrap().get(&id).cloned())
    }

    fn try_consume(&self, id: Uuid, now: u64) -> Result<MergeAuthorization, ApplicationError> {
        let mut guard = self.inner.lock().unwrap();
        let stored = guard
            .get_mut(&id)
            .ok_or(forge_review::MergeError::MissingAuth)?;
        if stored.consumed {
            return Err(forge_review::MergeError::Consumed.into());
        }
        if stored.auth.expires_at_unix <= now {
            return Err(forge_review::MergeError::Expired.into());
        }
        stored.consumed = true;
        Ok(stored.auth.clone())
    }
}

#[test]
fn signed_merge_auth_required_and_tamper_rejected() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-b", "master"]);
    git(&repo, &["config", "user.email", "forge@example.com"]);
    git(&repo, &["config", "user.name", "Forge"]);
    fs::write(repo.join("README"), "base\n").unwrap();
    git(&repo, &["add", "README"]);
    git(&repo, &["commit", "-m", "base"]);
    let base = git_stdout(&repo, &["rev-parse", "HEAD"]);
    let branch = format!("forge/{}", Uuid::new_v4());
    git(&repo, &["checkout", "-b", &branch]);
    fs::write(repo.join("feature.txt"), "x\n").unwrap();
    git(&repo, &["add", "feature.txt"]);
    git(&repo, &["commit", "-m", "feature"]);
    let head = git_stdout(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "master"]);

    let keys = Arc::new(MergeAuthKeyPair::generate());
    let svc = service_with(keys);
    let mut task = approved_task(&svc, &repo, &base, &branch, &head);
    let store = TamperStore::default();
    let auth = svc
        .authorize_merge(
            &principal("owner"),
            &task,
            MergeAuthRequest {
                base_commit: &base,
                target_branch: "master",
                now: 100,
                ttl_seconds: 60,
            },
            &store,
        )
        .unwrap();
    assert!(auth.signature.is_some());

    store
        .inner
        .lock()
        .unwrap()
        .get_mut(&auth.id)
        .unwrap()
        .auth
        .signature = Some("deadbeef".into());

    let err = svc
        .execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id: auth.id,
                repo: &repo,
                now: 110,
                remote: None,
            },
            &store,
        )
        .unwrap_err();
    assert!(matches!(err, ApplicationError::InvalidSignature));
}

#[test]
fn merge_and_push_updates_bare_remote() {
    let dir = tempdir().unwrap();
    let bare = dir.path().join("remote.git");
    let repo = dir.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--bare", "-b", "master"])
            .arg(&bare)
            .status()
            .unwrap()
            .success()
    );
    git(&repo, &["init", "-b", "master"]);
    git(&repo, &["config", "user.email", "forge@example.com"]);
    git(&repo, &["config", "user.name", "Forge"]);
    fs::write(repo.join("README"), "base\n").unwrap();
    git(&repo, &["add", "README"]);
    git(&repo, &["commit", "-m", "base"]);
    let base = git_stdout(&repo, &["rev-parse", "HEAD"]);
    assert!(
        Command::new("git")
            .args(["remote", "add", "origin"])
            .arg(&bare)
            .current_dir(&repo)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .unwrap()
            .success()
    );
    git(&repo, &["push", "-u", "origin", "master"]);

    let branch = format!("forge/{}", Uuid::new_v4());
    git(&repo, &["checkout", "-b", &branch]);
    fs::write(repo.join("feature.txt"), "push-me\n").unwrap();
    git(&repo, &["add", "feature.txt"]);
    git(&repo, &["commit", "-m", "feature"]);
    let head = git_stdout(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "master"]);

    let keys = Arc::new(MergeAuthKeyPair::generate());
    let svc = service_with(keys);
    let mut task = approved_task(&svc, &repo, &base, &branch, &head);
    let store = MemoryMergeAuthStore::default();
    let auth = svc
        .authorize_merge(
            &principal("owner"),
            &task,
            MergeAuthRequest {
                base_commit: &base,
                target_branch: "master",
                now: 100,
                ttl_seconds: 60,
            },
            &store,
        )
        .unwrap();
    let merged = svc
        .execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id: auth.id,
                repo: &repo,
                now: 110,
                remote: Some("origin"),
            },
            &store,
        )
        .unwrap();
    assert_eq!(task.status(), TaskStatus::Merged);
    assert_eq!(git_stdout(&bare, &["rev-parse", "master"]), merged);
}

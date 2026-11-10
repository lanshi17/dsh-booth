use forge_application::*;
use forge_domain::*;
use forge_review::CheckResult;
use forge_review::ReviewEvidence;
use forge_review::collect_evidence;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;
use uuid::Uuid;

fn principal(name: &str) -> PrincipalId {
    PrincipalId(name.into())
}

fn service() -> TaskService {
    TaskService::new(Policy {
        authorizers: BTreeSet::from([principal("owner")]),
        reviewers: BTreeSet::from([principal("reviewer")]),
        merge_authorizers: BTreeSet::from([principal("owner")]),
    })
}

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn init_main(repo: &Path) -> String {
    fs::create_dir_all(repo).unwrap();
    git(repo, &["init", "-b", "master"]);
    git(repo, &["config", "user.email", "forge@example.com"]);
    git(repo, &["config", "user.name", "Forge"]);
    fs::write(repo.join("README"), "base\n").unwrap();
    git(repo, &["add", "README"]);
    git(repo, &["commit", "-m", "base"]);
    git_stdout(repo, &["rev-parse", "HEAD"])
}

fn prepare_assignment(repo: &Path, base: &str) -> (String, String, WorkspaceLease) {
    let branch = format!("forge/{}", Uuid::new_v4());
    git(repo, &["checkout", "-b", &branch]);
    fs::write(repo.join("feature.txt"), "feature\n").unwrap();
    git(repo, &["add", "feature.txt"]);
    git(repo, &["commit", "-m", "feature"]);
    let head = git_stdout(repo, &["rev-parse", "HEAD"]);
    git(repo, &["checkout", "master"]);
    let lease = WorkspaceLease {
        directory: repo.to_path_buf(),
        branch: branch.clone(),
        base_commit: base.to_owned(),
    };
    (branch, head, lease)
}

fn awaiting_review(lease: WorkspaceLease, head: &str) -> Task {
    let mut task = Task::accept("merge me".into(), Source::Cli, Trigger::Manual);
    let svc = service();
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
    task.assign(BackendKind::Omp, principal("worker"), lease, 2)
        .unwrap();
    let report = ExecutionReport {
        id: ReportId::new(),
        task_id: task.id(),
        assignment_id: task.assignments()[0].id,
        exit_code: 0,
        summary: "ok".into(),
        head_commit: head.to_owned(),
        changed_files: vec!["feature.txt".into()],
    };
    svc.complete(&principal("worker"), &mut task, report)
        .unwrap();
    task
}

#[test]
fn failed_host_checks_block_approval() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let base = init_main(&repo);
    let (branch, head, lease) = prepare_assignment(&repo, &base);
    git(&repo, &["checkout", &branch]);
    let failed = collect_evidence(&repo, &base, &[vec!["false".into()]]).unwrap();
    assert_eq!(failed.checks[0].exit_code, 1);
    let mut task = awaiting_review(lease, &head);
    let approve = ReviewRecord {
        assignment_id: task.assignments()[0].id,
        report_id: task.reports()[0].id,
        reviewed_head: head,
        reviewer: principal("reviewer"),
        decision: ReviewDecision::Approve,
        evidence: "host".into(),
    };
    assert!(
        service()
            .review(&principal("reviewer"), &mut task, approve, &failed)
            .is_err()
    );
    assert_eq!(task.status(), TaskStatus::AwaitingReview);
}

#[test]
fn real_merge_after_auth_rejects_drift_duplicate_and_expiry() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let base = init_main(&repo);
    let (branch, head, lease) = prepare_assignment(&repo, &base);
    git(&repo, &["checkout", &branch]);
    let evidence = collect_evidence(&repo, &base, &[vec!["true".into()]]).unwrap();
    let mut task = awaiting_review(lease, &head);
    let svc = service();
    let review = ReviewRecord {
        assignment_id: task.assignments()[0].id,
        report_id: task.reports()[0].id,
        reviewed_head: head.clone(),
        reviewer: principal("reviewer"),
        decision: ReviewDecision::Approve,
        evidence: evidence.diff_stat.clone(),
    };
    svc.review(&principal("reviewer"), &mut task, review, &evidence)
        .unwrap();
    assert_eq!(task.status(), TaskStatus::Approved);

    let store = MemoryMergeAuthStore::default();
    git(&repo, &["checkout", "master"]);
    let auth = svc
        .authorize_merge(
            &principal("owner"),
            &task,
            MergeAuthRequest {
                base_commit: &base,
                target_branch: "master",
                now: 100,
                ttl_seconds: 30,
            },
            &store,
        )
        .unwrap();

    // Post-review drift should fail CAS and leave the grant reusable.
    git(&repo, &["checkout", &branch]);
    fs::write(repo.join("drift.txt"), "drift\n").unwrap();
    git(&repo, &["add", "drift.txt"]);
    git(&repo, &["commit", "-m", "drift"]);
    git(&repo, &["checkout", "master"]);
    let drift = svc
        .execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id: auth.id,
                repo: &repo,
                now: 100,
                remote: None,
            },
            &store,
        )
        .unwrap_err();
    assert!(
        drift.to_string().to_ascii_lowercase().contains("head"),
        "{drift}"
    );
    assert!(!store.get(auth.id).unwrap().unwrap().consumed);

    // Restore reviewed tip and merge for real.
    git(&repo, &["checkout", &branch]);
    git(&repo, &["reset", "--hard", &head]);
    git(&repo, &["checkout", "master"]);
    let merged = svc
        .execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id: auth.id,
                repo: &repo,
                now: 101,
                remote: None,
            },
            &store,
        )
        .unwrap();
    assert_eq!(task.status(), TaskStatus::Merged);
    assert_eq!(git_stdout(&repo, &["rev-parse", "master"]), merged);
    assert!(repo.join("feature.txt").exists());

    // Duplicate merge rejected.
    assert!(
        svc.execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id: auth.id,
                repo: &repo,
                now: 102,
                remote: None,
            },
            &store,
        )
        .is_err()
    );
}

#[test]
fn expired_authorization_cannot_merge() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let base = init_main(&repo);
    let (branch, head, lease) = prepare_assignment(&repo, &base);
    git(&repo, &["checkout", &branch]);
    let evidence = ReviewEvidence {
        observed_head: head.clone(),
        observed_base: base.clone(),
        diff_inspected: true,
        diff_stat: "ok".into(),
        checks: vec![CheckResult {
            command: vec!["true".into()],
            exit_code: 0,
        }],
    };
    let mut task = awaiting_review(lease, &head);
    let svc = service();
    let review = ReviewRecord {
        assignment_id: task.assignments()[0].id,
        report_id: task.reports()[0].id,
        reviewed_head: head,
        reviewer: principal("reviewer"),
        decision: ReviewDecision::Approve,
        evidence: "host".into(),
    };
    svc.review(&principal("reviewer"), &mut task, review, &evidence)
        .unwrap();
    let store = MemoryMergeAuthStore::default();
    let auth = svc
        .authorize_merge(
            &principal("owner"),
            &task,
            MergeAuthRequest {
                base_commit: &base,
                target_branch: "master",
                now: 50,
                ttl_seconds: 5,
            },
            &store,
        )
        .unwrap();
    let err = svc
        .execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id: auth.id,
                repo: &repo,
                now: 60,
                remote: None,
            },
            &store,
        )
        .unwrap_err();
    assert!(err.to_string().to_ascii_lowercase().contains("expir"));
}

use forge_application::*;
use forge_domain::*;
use forge_persistence::TursoTaskRepository;
use forge_review::CheckResult;
use forge_review::ReviewEvidence;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
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

#[test]
fn durable_merge_auth_survives_reopen_and_merges_once() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("forge.db");
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    git(&repo_path, &["init", "-b", "master"]);
    git(&repo_path, &["config", "user.email", "forge@example.com"]);
    git(&repo_path, &["config", "user.name", "Forge"]);
    fs::write(repo_path.join("README"), "base\n").unwrap();
    git(&repo_path, &["add", "README"]);
    git(&repo_path, &["commit", "-m", "base"]);
    let base = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    let branch = format!("forge/{}", Uuid::new_v4());
    git(&repo_path, &["checkout", "-b", &branch]);
    fs::write(repo_path.join("feature.txt"), "x\n").unwrap();
    git(&repo_path, &["add", "feature.txt"]);
    git(&repo_path, &["commit", "-m", "feature"]);
    let head = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    git(&repo_path, &["checkout", "master"]);

    let mut task = Task::accept("durable".into(), Source::Cli, Trigger::Manual);
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
    task.assign(
        BackendKind::Omp,
        principal("worker"),
        WorkspaceLease {
            directory: repo_path.clone(),
            branch: branch.clone(),
            base_commit: base.clone(),
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
        head_commit: head.clone(),
        changed_files: vec![],
    };
    svc.complete(&principal("worker"), &mut task, report)
        .unwrap();
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

    let auth_id = {
        let repo = Arc::new(TursoTaskRepository::open(&db).unwrap());
        let store = DurableMergeAuthStore::new(repo);
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
        assert!(auth.signature.is_none());
        auth.id
    };

    // Crash/reopen: durable store still holds the unconsumed grant.
    let repo = Arc::new(TursoTaskRepository::reopen(&db).unwrap());
    let store = DurableMergeAuthStore::new(repo);
    assert!(!store.get(auth_id).unwrap().unwrap().consumed);
    let merged = svc
        .execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id,
                repo: &repo_path,
                now: 110,
                remote: None,
            },
            &store,
        )
        .unwrap();
    assert_eq!(task.status(), TaskStatus::Merged);
    assert_eq!(git_stdout(&repo_path, &["rev-parse", "master"]), merged);
    assert!(
        svc.execute_merge(
            &principal("owner"),
            &mut task,
            MergeExecuteRequest {
                auth_id,
                repo: &repo_path,
                now: 120,
                remote: None,
            },
            &store,
        )
        .is_err()
    );
}

use forge_application::*;
use forge_domain::*;
use forge_review::CheckResult;
use forge_review::ReviewEvidence;
use std::collections::BTreeSet;

fn principal(name: &str) -> PrincipalId {
    PrincipalId(name.into())
}
fn service() -> TaskService {
    TaskService::new(Policy {
        authorizers: BTreeSet::from([principal("owner")]),
        reviewers: BTreeSet::from([principal("reviewer"), principal("worker")]),
        merge_authorizers: BTreeSet::from([principal("owner")]),
    })
}
fn mandate() -> TaskMandate {
    TaskMandate {
        allowed_backends: BTreeSet::from([BackendKind::Omp]),
        allow_network: false,
        environment_names: BTreeSet::new(),
        timeout_seconds: 30,
        expires_at_unix: 100,
    }
}
fn workspace(label: &str) -> WorkspaceLease {
    WorkspaceLease {
        directory: std::env::temp_dir().join("forge-tests").join(label),
        branch: format!("forge/{label}"),
        base_commit: "a".repeat(40),
    }
}
fn accepted() -> Task {
    Task::accept("implement feature".into(), Source::Cli, Trigger::Manual)
}
fn running() -> Task {
    let mut task = accepted();
    service()
        .authorize(&principal("owner"), &mut task, mandate(), 1)
        .unwrap();
    task.assign(BackendKind::Omp, principal("worker"), workspace("first"), 2)
        .unwrap();
    task
}
fn report(task: &Task, exit_code: i32) -> ExecutionReport {
    ExecutionReport {
        id: ReportId::new(),
        task_id: task.id(),
        assignment_id: task.assignments().last().unwrap().id,
        exit_code,
        summary: "worker result".into(),
        head_commit: "b".repeat(40),
        changed_files: vec![],
    }
}
fn review(task: &Task, who: &str, decision: ReviewDecision) -> ReviewRecord {
    let report = task.reports().last().unwrap();
    ReviewRecord {
        assignment_id: report.assignment_id,
        report_id: report.id,
        reviewed_head: report.head_commit.clone(),
        reviewer: principal(who),
        decision,
        evidence: "host inspected diff and ran tests".into(),
    }
}
fn evidence() -> ReviewEvidence {
    ReviewEvidence {
        observed_head: "b".repeat(40),
        observed_base: "a".repeat(40),
        diff_inspected: true,
        diff_stat: "1 file changed".into(),
        checks: vec![CheckResult {
            command: vec!["cargo".into(), "test".into()],
            exit_code: 0,
        }],
    }
}
fn auth_merge(
    svc: &TaskService,
    caller: &str,
    task: &Task,
    store: &MemoryMergeAuthStore,
) -> Result<MergeAuthorization, ApplicationError> {
    svc.authorize_merge(
        &principal(caller),
        task,
        MergeAuthRequest {
            base_commit: &"a".repeat(40),
            target_branch: "master",
            now: 10,
            ttl_seconds: 60,
        },
        store,
    )
}

#[test]
fn accepting_does_not_authorize_execution_or_worker_privilege_escalation() {
    let mut task = accepted();
    assert!(
        task.assign(BackendKind::Omp, principal("worker"), workspace("first"), 1)
            .is_err()
    );
    assert!(
        service()
            .authorize(&principal("worker"), &mut task, mandate(), 1)
            .is_err()
    );
    assert_eq!(task.status(), TaskStatus::Accepted);
    assert!(task.assignments().is_empty());
}

#[test]
fn mandate_restricts_backend_and_expiration() {
    let mut task = accepted();
    service()
        .authorize(&principal("owner"), &mut task, mandate(), 1)
        .unwrap();
    assert_eq!(
        task.assign(BackendKind::Dsh, principal("worker"), workspace("first"), 2)
            .unwrap_err(),
        DomainError::InvalidMandate
    );
    assert_eq!(
        task.assign(
            BackendKind::Omp,
            principal("worker"),
            workspace("first"),
            100
        )
        .unwrap_err(),
        DomainError::InvalidMandate
    );
    assert!(task.assignments().is_empty());
}

#[test]
fn completion_requires_independent_review_then_explicit_merge_authority() {
    let mut task = running();
    let store = MemoryMergeAuthStore::default();
    let result = report(&task, 0);
    service()
        .complete(&principal("worker"), &mut task, result)
        .unwrap();
    assert_eq!(task.status(), TaskStatus::AwaitingReview);
    assert!(auth_merge(&service(), "owner", &task, &store).is_err());
    // Even a worker accidentally granted the reviewer role cannot review its own work.
    let self_review = review(&task, "worker", ReviewDecision::Approve);
    assert!(
        service()
            .review(&principal("worker"), &mut task, self_review, &evidence())
            .is_err()
    );
    let independent = review(&task, "reviewer", ReviewDecision::Approve);
    service()
        .review(&principal("reviewer"), &mut task, independent, &evidence())
        .unwrap();
    assert_eq!(task.status(), TaskStatus::Approved);
    assert!(auth_merge(&service(), "worker", &task, &store).is_err());
    let grant = auth_merge(&service(), "owner", &task, &store).unwrap();
    assert_eq!(grant.head_commit, "b".repeat(40));
    assert_eq!(grant.assignment_id, task.assignments()[0].id);
    assert!(grant.expires_at_unix > 10);
}

#[test]
fn wrong_worker_or_assignment_cannot_submit_a_result() {
    let mut task = running();
    let valid = report(&task, 0);
    assert!(
        service()
            .complete(&principal("stranger"), &mut task, valid.clone())
            .is_err()
    );
    let mut wrong = valid;
    wrong.assignment_id = AssignmentId::new();
    assert!(
        service()
            .complete(&principal("worker"), &mut task, wrong)
            .is_err()
    );
    assert_eq!(task.status(), TaskStatus::Running);
    assert!(task.reports().is_empty());
}

#[test]
fn review_cannot_substitute_another_commit_or_failed_tests() {
    let mut task = running();
    let result = report(&task, 0);
    service()
        .complete(&principal("worker"), &mut task, result)
        .unwrap();
    let mut stale = review(&task, "reviewer", ReviewDecision::Approve);
    stale.reviewed_head = "c".repeat(40);
    assert!(
        service()
            .review(&principal("reviewer"), &mut task, stale, &evidence())
            .is_err()
    );
    let mut failed = evidence();
    failed.checks[0].exit_code = 1;
    let good = review(&task, "reviewer", ReviewDecision::Approve);
    assert!(
        service()
            .review(&principal("reviewer"), &mut task, good, &failed)
            .is_err()
    );
    assert_eq!(task.status(), TaskStatus::AwaitingReview);
}

#[test]
fn failed_assignment_can_retry_but_old_report_and_workspace_are_rejected() {
    let mut task = running();
    let old = report(&task, 1);
    service()
        .complete(&principal("worker"), &mut task, old.clone())
        .unwrap();
    assert_eq!(task.status(), TaskStatus::Failed);
    assert_eq!(
        task.assign(BackendKind::Omp, principal("worker"), workspace("first"), 3)
            .unwrap_err(),
        DomainError::WorkspaceReused
    );
    task.assign(
        BackendKind::Omp,
        principal("worker"),
        workspace("second"),
        3,
    )
    .unwrap();
    assert_eq!(task.assignments().len(), 2);
    assert!(
        service()
            .complete(&principal("worker"), &mut task, old)
            .is_err()
    );
    assert_eq!(task.status(), TaskStatus::Running);
}

#[test]
fn rejected_review_is_retained_and_does_not_authorize_merge() {
    let mut task = running();
    let store = MemoryMergeAuthStore::default();
    let result = report(&task, 0);
    service()
        .complete(&principal("worker"), &mut task, result)
        .unwrap();
    let rejected = review(&task, "reviewer", ReviewDecision::Reject);
    service()
        .review(&principal("reviewer"), &mut task, rejected, &evidence())
        .unwrap();
    assert!(auth_merge(&service(), "owner", &task, &store).is_err());
    task.assign(
        BackendKind::Omp,
        principal("worker"),
        workspace("second"),
        4,
    )
    .unwrap();
    assert_eq!(task.reviews().len(), 1);
    assert_eq!(task.status(), TaskStatus::Running);
}

#[test]
fn late_completion_cannot_resurrect_cancelled_assignment() {
    let mut task = running();
    let late = report(&task, 0);
    task.cancel().unwrap();
    assert!(
        service()
            .complete(&principal("worker"), &mut task, late)
            .is_err()
    );
    assert_eq!(task.status(), TaskStatus::Cancelled);
    assert_eq!(task.assignments()[0].status, AssignmentStatus::Cancelled);
}

#[test]
fn independent_reviewer_can_reject_failing_checks() {
    let mut task = running();
    let store = MemoryMergeAuthStore::default();
    let result = report(&task, 0);
    service()
        .complete(&principal("worker"), &mut task, result)
        .unwrap();
    let rejected = review(&task, "reviewer", ReviewDecision::Reject);
    let mut failed = evidence();
    failed.checks[0].exit_code = 1;
    service()
        .review(&principal("reviewer"), &mut task, rejected, &failed)
        .unwrap();
    assert_eq!(task.status(), TaskStatus::Rejected);
    assert!(auth_merge(&service(), "owner", &task, &store).is_err());
}

use forge_agent_backend::AgentExecutor;
use forge_agent_omp::OmpExecutor;
use forge_agent_omp::specification;
use forge_agent_protocol::ExecutionRequest;
use forge_agent_protocol::PROTOCOL_VERSION;
use forge_agent_runtime::SupervisorConfig;
use forge_domain::Assignment;
use forge_domain::AssignmentId;
use forge_domain::AssignmentStatus;
use forge_domain::BackendKind;
use forge_domain::PrincipalId;
use forge_domain::TaskId;
use forge_domain::TaskMandate;
use forge_domain::WorkspaceLease;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;

fn fake_omp() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_forge-fake-omp"))
}

fn request(cwd: PathBuf, prompt: &str) -> ExecutionRequest {
    let commit = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned();
    ExecutionRequest {
        version: PROTOCOL_VERSION,
        assignment: Assignment {
            id: AssignmentId::new(),
            task_id: TaskId::new(),
            backend: BackendKind::Omp,
            worker: PrincipalId("omp-worker".into()),
            workspace: WorkspaceLease {
                directory: cwd,
                branch: format!("forge/{}", uuid::Uuid::new_v4()),
                base_commit: commit,
            },
            status: AssignmentStatus::Running,
        },
        mandate: TaskMandate {
            allowed_backends: BTreeSet::from([BackendKind::Omp]),
            allow_network: false,
            environment_names: BTreeSet::from(["PATH".to_owned(), "LANG".to_owned()]),
            timeout_seconds: 30,
            expires_at_unix: u64::MAX,
        },
        prompt: prompt.to_owned(),
    }
}

fn executor(extra_args: Vec<String>) -> OmpExecutor {
    let supervisor = SupervisorConfig {
        timeout: Duration::from_secs(5),
        ..SupervisorConfig::default()
    };
    OmpExecutor::new(specification(fake_omp(), extra_args), supervisor)
}

#[tokio::test]
async fn start_wait_success_and_version_probe() {
    let dir = tempdir().unwrap();
    let exec = executor(vec![]);
    let req = request(dir.path().to_path_buf(), "hello");
    let session = exec.start(req).await.unwrap();
    assert!(
        session
            .backend_session_id
            .as_deref()
            .unwrap()
            .contains("omp-cli 1.0.0-forge")
    );
    let report = exec.wait(&session).await.unwrap();
    assert_eq!(report.exit_code, 0);
    assert!(report.summary.contains("omp:hello"));
    assert_eq!(report.changed_files, vec![PathBuf::from("omp.txt")]);
}

#[tokio::test]
async fn cancel_running_worker() {
    let dir = tempdir().unwrap();
    let exec = executor(vec!["--mode".into(), "sleep".into()]);
    let session = exec
        .start(request(dir.path().to_path_buf(), "sleep"))
        .await
        .unwrap();
    exec.cancel(&session).await.unwrap();
    let err = exec.wait(&session).await.unwrap_err();
    assert!(err.to_string().contains("unknown session"));
}

#[tokio::test]
async fn rejects_stderr_bad_json_and_huge_logs() {
    let dir = tempdir().unwrap();
    for mode in ["stderr", "bad-json", "huge"] {
        let exec = executor(vec!["--mode".into(), mode.into()]);
        let session = exec
            .start(request(dir.path().to_path_buf(), mode))
            .await
            .unwrap();
        let err = exec.wait(&session).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("stderr")
                || msg.contains("json")
                || msg.contains("exceeded")
                || msg.contains("Stdout"),
            "mode {mode} unexpected error: {msg}"
        );
    }
}

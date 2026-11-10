use forge_agent_runtime::ProcessSupervisor;
use forge_agent_runtime::SandboxMounts;
use forge_agent_runtime::SupervisorConfig;
use forge_agent_runtime::SupervisorError;
use forge_agent_runtime::process_group_alive;
use forge_domain::AssignmentId;
use forge_worktree::GitWorktreeManager;
use forge_worktree::WorktreeManager;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::process::Stdio;
use std::time::Duration;
use tempfile::tempdir;

fn fake_worker() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_forge-fake-worker"))
}

fn init_repo(root: &Path) -> String {
    for args in [
        ["init"][..].as_ref(),
        ["config", "user.email", "forge@example.com"].as_ref(),
        ["config", "user.name", "Forge"].as_ref(),
    ] {
        assert!(
            StdCommand::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
    }
    fs::write(root.join("protected.txt"), "main-secret\n").unwrap();
    assert!(
        StdCommand::new("git")
            .args(["add", "protected.txt"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        StdCommand::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    let output = StdCommand::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
async fn success_and_failure_exits() {
    let dir = tempdir().unwrap();
    let supervisor = ProcessSupervisor::new(SupervisorConfig {
        timeout: Duration::from_secs(5),
        ..SupervisorConfig::default()
    });
    assert_eq!(
        supervisor
            .run(&fake_worker(), &["--exit", "0"], dir.path())
            .await
            .unwrap(),
        0
    );
    let err = supervisor
        .run(&fake_worker(), &["--exit", "7"], dir.path())
        .await
        .unwrap_err();
    assert!(matches!(err, SupervisorError::ExitStatus(7)));
}

#[tokio::test]
async fn timeout_and_cancel_kill_process_group() {
    let dir = tempdir().unwrap();
    let supervisor = ProcessSupervisor::new(SupervisorConfig {
        timeout: Duration::from_millis(200),
        ..SupervisorConfig::default()
    });
    let err = supervisor
        .run(&fake_worker(), &["--spawn-orphan-ms", "20000"], dir.path())
        .await
        .unwrap_err();
    assert!(matches!(err, SupervisorError::Timeout(_)));
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut managed = ProcessSupervisor::new(SupervisorConfig {
        timeout: Duration::from_secs(30),
        ..SupervisorConfig::default()
    })
    .spawn(&fake_worker(), &["--spawn-orphan-ms", "20000"], dir.path())
    .await
    .unwrap();
    let pgid = managed.pid();
    ProcessSupervisor::new(SupervisorConfig::default())
        .cancel(&mut managed)
        .await
        .unwrap();
    let mut alive = true;
    for _ in 0..40 {
        if !process_group_alive(pgid) {
            alive = false;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if alive {
        let dump = StdCommand::new("ps")
            .args(["-o", "pid=,pgid=,stat=,args=", "-A"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        panic!("process group {pgid} still alive after cancel; ps dump:\n{dump}");
    }
}

#[tokio::test]
async fn env_allowlist_drops_parent_vars_and_keeps_injection() {
    let dir = tempdir().unwrap();
    let supervisor = ProcessSupervisor::new(SupervisorConfig {
        timeout: Duration::from_secs(5),
        // Empty allowlist: parent PATH/HOME/etc must not reach the worker.
        environment_allowlist: Default::default(),
        injected_env: [("FORGE_INJECTED".to_owned(), "ok".to_owned())]
            .into_iter()
            .collect(),
        ..SupervisorConfig::default()
    });
    let missing = supervisor
        .run(&fake_worker(), &["--env-get", "PATH"], dir.path())
        .await
        .unwrap_err();
    assert!(matches!(missing, SupervisorError::ExitStatus(3)));
    assert_eq!(
        supervisor
            .run(&fake_worker(), &["--env-get", "FORGE_INJECTED"], dir.path(),)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn sandbox_blocks_writes_to_main_repo() {
    if !cfg!(target_os = "linux") {
        return;
    }
    if StdCommand::new("bwrap")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!("skipping sandbox test: bwrap not available");
        return;
    }
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let trees = dir.path().join("trees");
    fs::create_dir_all(&repo).unwrap();
    let head = init_repo(&repo);
    let manager = GitWorktreeManager::new(&repo, &trees).unwrap();
    let assignment = AssignmentId(uuid::Uuid::new_v4());
    let lease = manager.allocate(assignment, &head).unwrap();

    let supervisor = ProcessSupervisor::new(SupervisorConfig {
        timeout: Duration::from_secs(5),
        network_allowed: false,
        sandbox: Some(SandboxMounts {
            main_repo: repo.clone(),
            worktree: lease.directory.clone(),
        }),
        ..SupervisorConfig::default()
    });

    supervisor
        .run(
            &fake_worker(),
            &["--write", "worker-out.txt", "ok"],
            &lease.directory,
        )
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(lease.directory.join("worker-out.txt")).unwrap(),
        "ok"
    );
    assert!(!repo.join("worker-out.txt").exists());

    let target = repo.join("protected.txt");
    let err = supervisor
        .run(
            &fake_worker(),
            &["--write-abs", target.to_str().unwrap(), "pwned"],
            &lease.directory,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, SupervisorError::ExitStatus(_)),
        "expected worker failure when writing main, got {err:?}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "main-secret\n");

    manager.release(assignment).unwrap();
}

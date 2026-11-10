use super::spawn_git_command;
use super::wait_for_git_command_with_timeout_output;
use pretty_assertions::assert_eq;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Clone, Copy)]
enum GitWrapperLifetime {
    WaitForChild,
    ExitBeforeTimeout,
}

async fn assert_timed_out_git_wrapper_does_not_leave_child_process_running(
    wrapper_lifetime: GitWrapperLifetime,
) {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let child_pid_file = temp_dir.path().join("child.pid");
    let child_ready_file = temp_dir.path().join("child-ready");
    let release_child_file = temp_dir.path().join("release-child");
    let child_survived_file = temp_dir.path().join("child-survived");
    let release_wrapper_file = temp_dir.path().join("release-wrapper");
    let mut command = {
        // Re-execute this Rust fixture so the process-tree assertion does not
        // depend on PowerShell startup or Start-Process console inheritance.
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args(["--exact", FIXTURE_TEST, "--nocapture"])
            .env("CODEX_GIT_PROCESS_FIXTURE", "wrapper")
            .env(
                "CODEX_GIT_WRAPPER_LIFETIME",
                match wrapper_lifetime {
                    GitWrapperLifetime::WaitForChild => "wait",
                    GitWrapperLifetime::ExitBeforeTimeout => "exit",
                },
            );
        command
    };
    command
        .env("CHILD_PID_FILE", &child_pid_file)
        .env("CHILD_READY_FILE", &child_ready_file)
        .env("RELEASE_CHILD_FILE", &release_child_file)
        .env("CHILD_SURVIVED_FILE", &child_survived_file)
        .env("RELEASE_WRAPPER_FILE", &release_wrapper_file);

    let (mut wrapper, process_tree) = spawn_git_command(&mut command).expect("spawn Git wrapper");
    let child_pid = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(child_pid) = std::fs::read_to_string(&child_pid_file)
                && !child_pid.trim().is_empty()
                && child_ready_file.exists()
            {
                break child_pid.trim().to_string();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("wait for Git wrapper child readiness");

    if matches!(wrapper_lifetime, GitWrapperLifetime::ExitBeforeTimeout) {
        std::fs::write(&release_wrapper_file, "release").expect("release Git wrapper");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if wrapper
                    .try_wait()
                    .expect("check Git wrapper state")
                    .is_some()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("wait for Git wrapper exit");
    }

    let output =
        wait_for_git_command_with_timeout_output(wrapper, process_tree, Duration::from_millis(100))
            .await;
    assert_eq!(output, None);

    std::fs::write(&release_child_file, "release").expect("release Git wrapper child");
    tokio::time::sleep(Duration::from_secs(3)).await;
    if !child_survived_file.exists() {
        return;
    }

    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &child_pid])
        .status();
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &child_pid, "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    panic!("Git wrapper child process {child_pid} survived timeout cleanup");
}

#[tokio::test]
async fn timed_out_git_wrapper_does_not_leave_child_process_running() {
    assert_timed_out_git_wrapper_does_not_leave_child_process_running(
        GitWrapperLifetime::WaitForChild,
    )
    .await;
}

#[tokio::test]
async fn timed_out_exited_git_wrapper_does_not_leave_child_process_running() {
    assert_timed_out_git_wrapper_does_not_leave_child_process_running(
        GitWrapperLifetime::ExitBeforeTimeout,
    )
    .await;
}

const FIXTURE_TEST: &str = "git_process::tests::git_process_fixture";

/// The child inherits the wrapper's process group/job and pipes, like a Git helper.
#[test]
fn git_process_fixture() {
    fn wait_for_file(variable: &str) {
        let path = std::env::var_os(variable).expect("fixture marker path");
        while !std::path::Path::new(&path).exists() {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    match std::env::var("CODEX_GIT_PROCESS_FIXTURE").as_deref() {
        Ok("wrapper") => {
            #[expect(
                clippy::zombie_processes,
                reason = "The early-exit fixture deliberately leaves a descendant for the supervisor to clean up"
            )]
            let mut child =
                std::process::Command::new(std::env::current_exe().expect("test executable"))
                    .args(["--exact", FIXTURE_TEST, "--nocapture"])
                    .env("CODEX_GIT_PROCESS_FIXTURE", "child")
                    .stdin(Stdio::null())
                    .spawn()
                    .expect("spawn fixture child");
            std::fs::write(
                std::env::var_os("CHILD_PID_FILE").expect("child pid path"),
                child.id().to_string(),
            )
            .expect("write child pid");
            if std::env::var("CODEX_GIT_WRAPPER_LIFETIME").as_deref() == Ok("wait") {
                assert!(child.wait().expect("wait for fixture child").success());
            } else {
                wait_for_file("RELEASE_WRAPPER_FILE");
            }
        }
        Ok("child") => {
            std::fs::write(
                std::env::var_os("CHILD_READY_FILE").expect("child ready path"),
                "ready",
            )
            .expect("write child readiness");
            wait_for_file("RELEASE_CHILD_FILE");
            std::thread::sleep(Duration::from_secs(1));
            std::fs::write(
                std::env::var_os("CHILD_SURVIVED_FILE").expect("survival marker path"),
                "survived",
            )
            .expect("write survival marker");
            std::thread::sleep(Duration::from_secs(60));
        }
        _ => {}
    }
}

#![cfg(target_os = "linux")]
use btae_eval::runner::*;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "btae-runner-{}-{timestamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("datasets/pilot.json")
}
fn request(tmp: &Temp, scenario: Scenario) -> DiagnosticRequest {
    DiagnosticRequest {
        manifest: manifest(),
        task_id: "queue-r01".into(),
        arm: "E".into(),
        repeat: 1,
        fixture_worker: env!("CARGO_BIN_EXE_btae-fixture-worker").into(),
        output: tmp.0.join("run"),
        scenario,
        limits: Limits {
            wall_ms: 3000,
            ..Default::default()
        },
        host_probe: None,
        host_address: None,
        cancel_file: None,
    }
}

#[test]
fn successful_fixture_binds_plan_but_never_becomes_empirical_success() {
    let tmp = Temp::new();
    let record = run_diagnostic(request(&tmp, Scenario::Success)).unwrap();
    assert_eq!(
        record.outcome,
        ProcessOutcome::Completed,
        "{:?}",
        record.attempts
    );
    assert_eq!(record.attempts.len(), 1);
    assert!(record.hidden_test_result.is_none() && record.empirical_task_success.is_none());
    assert_eq!(record.data_kind, "runner_diagnostic_only");
    assert!(
        serde_json::from_value::<btae_eval::Observation>(serde_json::to_value(&record).unwrap())
            .is_err()
    );
    assert_eq!(record.usage_total.input_tokens, 3);
    assert_eq!(record.usage_total.output_tokens, 2);
    verify_diagnostic(&tmp.0.join("run"), &manifest()).unwrap();
}

#[test]
fn failed_attempts_are_retained_and_retry_consumes_one_shared_budget() {
    let tmp = Temp::new();
    let record = run_diagnostic(request(&tmp, Scenario::Failure)).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::WorkerFailed);
    assert_eq!(record.attempts.len(), 2);
    assert_eq!(record.usage_total.input_tokens, 6);
    assert!(tmp.0.join("run/attempt-1/attempt.json").exists());
    assert!(tmp.0.join("run/attempt-2/attempt.json").exists());
    verify_diagnostic(&tmp.0.join("run"), &manifest()).unwrap();
}

#[test]
fn retry_keeps_modified_workspace_and_original_failure() {
    let tmp = Temp::new();
    let record = run_diagnostic(request(&tmp, Scenario::Retry)).unwrap();
    assert_eq!(
        record.outcome,
        ProcessOutcome::Completed,
        "{:?}",
        record.attempts
    );
    assert_eq!(record.attempts.len(), 2);
    assert_eq!(record.attempts[0].outcome, ProcessOutcome::WorkerFailed);
    assert!(tmp.0.join("run/workspace/first-attempt.txt").exists());
    assert_eq!(
        record.usage_total.input_tokens + record.usage_total.output_tokens,
        10
    );
}

#[test]
fn aggregate_token_budget_is_not_reset_on_retry() {
    let tmp = Temp::new();
    let mut req = request(&tmp, Scenario::Retry);
    req.limits.tokens = 8;
    let record = run_diagnostic(req).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::TokenLimit);
    assert_eq!(record.attempts.len(), 2);
    // Overshoot is retained honestly, not clipped to the cap.
    assert_eq!(
        record.usage_total.input_tokens + record.usage_total.output_tokens,
        10
    );
}

#[test]
fn timeout_stops_without_retrying_or_resetting_the_clock() {
    let tmp = Temp::new();
    let mut req = request(&tmp, Scenario::Timeout);
    req.limits.wall_ms = 200;
    let record = run_diagnostic(req).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::TimedOut);
    // Seed export and evidence preparation consume the same episode budget.
    // A loaded host may exhaust it before starting the first worker.
    assert!(record.attempts.len() <= 1);
    assert!(record.elapsed_ms >= record.limits.wall_ms);
    assert!(record.elapsed_ms < 2500);
    if let Some(attempt) = record.attempts.first() {
        assert_eq!(attempt.outcome, ProcessOutcome::TimedOut);
        assert_eq!(attempt.exit_code, None, "worker must be killed by signal");
        assert!(attempt.streams_closed);
    } else {
        assert!(!tmp.0.join("run/attempt-1").exists());
        assert_eq!(record.usage_total.input_tokens, 0);
        assert_eq!(record.usage_total.output_tokens, 0);
    }
    assert!(!tmp.0.join("run/attempt-2").exists());
    verify_diagnostic(&tmp.0.join("run"), &manifest()).unwrap();
}

#[test]
fn timeout_kills_the_descendant_not_just_direct_child() {
    let tmp = Temp::new();
    let mut req = request(&tmp, Scenario::Descendant);
    req.limits.wall_ms = 700;
    let record = run_diagnostic(req).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::TimedOut);
    assert_eq!(record.attempts.len(), 1);
    assert_eq!(record.attempts[0].outcome, ProcessOutcome::TimedOut);
    assert_eq!(record.attempts[0].exit_code, None);
    assert!(record.elapsed_ms >= record.limits.wall_ms);
    let heartbeat = tmp.0.join("run/workspace/heartbeat.txt");
    let size = fs::metadata(&heartbeat).unwrap().len();
    assert!(size > 0);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        size,
        fs::metadata(&heartbeat).unwrap().len(),
        "descendant is still running"
    );
    assert!(record.attempts[0].streams_closed);
}

#[test]
fn host_cancellation_terminates_the_process_tree_and_is_sealed() {
    let tmp = Temp::new();
    let sentinel = tmp.0.join("cancel");
    let writer = sentinel.clone();
    let mut req = request(&tmp, Scenario::Descendant);
    req.cancel_file = Some(sentinel);
    let ready = tmp.0.join("run/workspace/heartbeat.txt");
    let trigger = thread::spawn(move || {
        for _ in 0..400 {
            if ready.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(writer, b"cancel").unwrap();
    });
    let record = run_diagnostic(req).unwrap();
    trigger.join().unwrap();
    assert_eq!(record.outcome, ProcessOutcome::Cancelled);
    let heartbeat = tmp.0.join("run/workspace/heartbeat.txt");
    let size = fs::metadata(&heartbeat).unwrap().len();
    thread::sleep(Duration::from_millis(80));
    assert_eq!(size, fs::metadata(heartbeat).unwrap().len());
    verify_diagnostic(&tmp.0.join("run"), &manifest()).unwrap();
}

#[test]
fn log_flood_is_bounded_and_stopped() {
    let tmp = Temp::new();
    let mut req = request(&tmp, Scenario::LogFlood);
    req.limits.log_bytes = 4096;
    req.limits.message_bytes = 1024;
    let record = run_diagnostic(req).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::LogLimit);
    assert!(record.attempts[0].log_bytes_retained <= 4096);
    let a = fs::metadata(tmp.0.join("run/attempt-1/stdout.jsonl"))
        .unwrap()
        .len();
    let b = fs::metadata(tmp.0.join("run/attempt-1/stderr.txt"))
        .unwrap()
        .len();
    assert!(a + b <= 4096);
}

#[test]
fn malformed_json_and_self_approval_are_protocol_errors() {
    for scenario in [
        Scenario::Malformed,
        Scenario::ForgedApproval,
        Scenario::NonmonotonicUsage,
    ] {
        let tmp = Temp::new();
        let record = run_diagnostic(request(&tmp, scenario)).unwrap();
        assert_eq!(
            record.outcome,
            ProcessOutcome::ProtocolError,
            "{scenario:?} {:?}",
            record.attempts
        );
        assert_eq!(record.attempts.len(), 1);
        assert!(record.empirical_task_success.is_none());
    }
}

#[test]
fn host_private_files_readonly_input_and_network_are_isolated() {
    let tmp = Temp::new();
    let canary = tmp.0.join("private.txt");
    fs::write(&canary, b"private evaluator canary").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut req = request(&tmp, Scenario::Isolation);
    req.host_probe = Some(canary.clone());
    req.host_address = Some(listener.local_addr().unwrap().to_string());
    let record = run_diagnostic(req).unwrap();
    assert_eq!(
        record.outcome,
        ProcessOutcome::Completed,
        "{:?}",
        record.attempts
    );
    assert_eq!(fs::read(canary).unwrap(), b"private evaluator canary");
}

#[test]
fn host_environment_is_cleared_even_when_cli_inherits_a_secret_canary() {
    let tmp = Temp::new();
    let output = Command::new(env!("CARGO_BIN_EXE_btae-eval"))
        .args(["diagnose", "--manifest"])
        .arg(manifest())
        .arg("--fixture-worker")
        .arg(env!("CARGO_BIN_EXE_btae-fixture-worker"))
        .arg("--output")
        .arg(tmp.0.join("run"))
        .args(["--scenario", "environment", "--wall-ms", "3000"])
        .env("BTAE_PRIVATE_CANARY", "must-not-pass-to-worker")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: DiagnosticRecord = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        record.outcome,
        ProcessOutcome::Completed,
        "{:?}",
        record.attempts
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("must-not-pass-to-worker"));
}

#[test]
fn artifact_edits_missing_files_extra_files_and_plan_tampering_are_detected() {
    let tmp = Temp::new();
    run_diagnostic(request(&tmp, Scenario::Success)).unwrap();
    let dir = tmp.0.join("run");
    let artifact = dir.join("workspace/fixture-output.txt");
    let original = fs::read(&artifact).unwrap();
    fs::write(&artifact, b"edited").unwrap();
    assert!(verify_diagnostic(&dir, &manifest()).is_err());
    fs::write(&artifact, &original).unwrap();
    let extra = dir.join("extra");
    fs::write(&extra, b"unlisted").unwrap();
    assert!(verify_diagnostic(&dir, &manifest()).is_err());
    fs::remove_file(extra).unwrap();
    fs::remove_file(&artifact).unwrap();
    assert!(verify_diagnostic(&dir, &manifest()).is_err());
    fs::write(&artifact, original).unwrap();
    let path = dir.join("diagnostic.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let unchanged = value.clone();
    value["run"]["arm"] = "A".into();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(verify_diagnostic(&dir, &manifest()).is_err());
    let mut changed_limits = unchanged;
    changed_limits["limits"]["tokens"] = 31.into();
    fs::write(&path, serde_json::to_vec(&changed_limits).unwrap()).unwrap();
    assert!(verify_diagnostic(&dir, &manifest()).is_err());
}

#[test]
fn existing_output_is_never_overwritten_and_noninitial_rounds_are_not_reset() {
    let tmp = Temp::new();
    run_diagnostic(request(&tmp, Scenario::Success)).unwrap();
    let record = fs::read(tmp.0.join("run/diagnostic.json")).unwrap();
    assert!(run_diagnostic(request(&tmp, Scenario::Success)).is_err());
    assert_eq!(record, fs::read(tmp.0.join("run/diagnostic.json")).unwrap());
    let other = Temp::new();
    let mut req = request(&other, Scenario::Success);
    req.task_id = "queue-r02".into();
    assert!(run_diagnostic(req).is_err());
    assert!(!other.0.join("run").exists());
}

#[test]
fn killing_supervisor_cleans_namespace_and_leaves_an_incomplete_receipt() {
    let tmp = Temp::new();
    let dir = tmp.0.join("interrupted");
    let mut child = Command::new(env!("CARGO_BIN_EXE_btae-eval"))
        .args(["diagnose", "--manifest"])
        .arg(manifest())
        .arg("--fixture-worker")
        .arg(env!("CARGO_BIN_EXE_btae-fixture-worker"))
        .arg("--output")
        .arg(&dir)
        .args(["--scenario", "descendant", "--wall-ms", "5000"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let heartbeat = dir.join("workspace/heartbeat.txt");
    for _ in 0..400 {
        if heartbeat.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let started = heartbeat.exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        started,
        "fixture did not start before supervisor interruption"
    );
    thread::sleep(Duration::from_millis(150));
    let size = fs::metadata(&heartbeat).unwrap().len();
    thread::sleep(Duration::from_millis(80));
    assert_eq!(
        size,
        fs::metadata(&heartbeat).unwrap().len(),
        "namespace survived supervisor death"
    );
    assert!(dir.join("started.json").is_file());
    assert!(!dir.join("diagnostic.json").exists());
    assert!(verify_diagnostic(&dir, &manifest()).is_err());
}

#[test]
fn export_rejects_changed_seed_and_keeps_setup_failure_receipt() {
    let tmp = Temp::new();
    let root = tmp.0.join("data");
    fs::create_dir_all(root.join("datasets")).unwrap();
    fs::create_dir_all(root.join("baselines")).unwrap();
    fs::create_dir_all(root.join("repositories/queue")).unwrap();
    let original = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in [
        "datasets/pilot.json",
        "datasets/queue-snapshot.json",
        "baselines/E.md",
    ] {
        fs::copy(original.join(name), root.join(name)).unwrap();
    }
    for entry in fs::read_dir(original.join("repositories/queue")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(
                entry.path(),
                root.join("repositories/queue").join(entry.file_name()),
            )
            .unwrap();
        }
    }
    fs::write(
        root.join("repositories/queue/service.py"),
        b"changed after dataset freeze",
    )
    .unwrap();
    let mut req = request(&tmp, Scenario::Success);
    req.manifest = root.join("datasets/pilot.json");
    assert!(run_diagnostic(req).is_err());
    assert!(tmp.0.join("run/started.json").exists());
    assert!(!tmp.0.join("run/diagnostic.json").exists());
    assert!(!tmp.0.join("run/attempt-1").exists());
}

#![cfg(target_os = "linux")]
use btae_eval::dsh_live::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
fn exe() -> PathBuf {
    env!("CARGO_BIN_EXE_dsh-json-fixture").into()
}
fn request(scenario: &str) -> LiveRequest {
    LiveRequest {
        executable: exe(),
        args: vec!["--scenario".into(), scenario.into()],
        prompt: b"task\n".to_vec(),
        cwd: None,
        environment: BTreeMap::from([(String::from("PATH"), String::from("/usr/bin"))]),
        cancel_file: None,
        output: None,
        limits: LiveLimits {
            wall_ms: 1000,
            max_stdout_bytes: 1024 * 1024,
            max_stderr_bytes: 1024 * 1024,
            ..Default::default()
        },
    }
}
#[test]
fn successful_jsonl_process_is_parsed_and_not_billed() {
    let record = run_live(request("success")).unwrap();
    assert_eq!(record.outcome, LiveOutcome::Completed);
    assert_eq!(record.exit_code, Some(0));
    let summary = record.summary.unwrap();
    assert!(summary.completed);
    assert_eq!(summary.total_events(), 7);
    assert!(!record.billing_verified);
    assert_eq!(record.network_isolation, "caller_supplied");
}
#[test]
fn environment_is_cleared_and_only_allowlist_is_visible() {
    let mut req = request("env");
    req.environment.insert("BTAE_ALLOWED".into(), "yes".into());
    assert_eq!(run_live(req).unwrap().outcome, LiveOutcome::Completed);
}

#[test]
fn malformed_transcript_is_protocol_error() {
    let mut req = request("malformed");
    req.limits.wall_ms = 10_000;
    let deadline_ms = req.limits.wall_ms;
    let record = run_live(req).unwrap();
    assert_eq!(record.outcome, LiveOutcome::ProtocolError);
    // Verify termination of a worker that never exits by itself, rather than
    // imposing a subsecond benchmark on hashing and shared-runner scheduling.
    assert_eq!(record.exit_code, None);
    assert!(record.streams_closed);
    assert_eq!(record.stdout_bytes, b"not-json\n".len() as u64);
    assert!(record.elapsed_ms < deadline_ms);
    assert!(record.summary.is_none());
    assert!(record.note.unwrap() == "invalid_dsh_stream");
}
#[test]
fn timeout_kills_process_and_returns_no_summary() {
    let mut req = request("timeout");
    req.limits.wall_ms = 80;
    let record = run_live(req).unwrap();
    assert_eq!(record.outcome, LiveOutcome::TimedOut);
    assert!(record.summary.is_none());
}
#[test]
fn cancellation_kills_process_group() {
    let dir = std::env::temp_dir().join(format!("btae-live-cancel-{}", std::process::id()));
    let _ = fs::create_dir(&dir);
    let cancel = dir.join("cancel");
    let mut req = request("timeout");
    req.limits.wall_ms = 1000;
    req.cancel_file = Some(cancel.clone());
    let (tx, rx) = mpsc::channel();
    let c = cancel.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(40));
        fs::write(c, b"cancel").unwrap();
        tx.send(()).unwrap();
    });
    let record = run_live(req).unwrap();
    rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(record.outcome, LiveOutcome::Cancelled);
    let _ = fs::remove_dir_all(dir);
}
#[test]
fn exact_stdout_limit_is_inclusive() {
    let fixture = include_bytes!("fixtures/dsh-live-smoke.jsonl");
    let mut req = request("success");
    req.limits.max_stdout_bytes = fixture.len();
    let record = run_live(req).unwrap();
    assert_eq!(record.outcome, LiveOutcome::Completed);
    assert_eq!(record.stdout_bytes, fixture.len() as u64);
}

#[test]
fn stderr_output_limit_is_bounded() {
    let mut req = request("flood");
    req.limits.max_stderr_bytes = 1024;
    let record = run_live(req).unwrap();
    assert_eq!(record.outcome, LiveOutcome::OutputLimit);
    assert!(record.stderr_bytes > 1024);
}

#[test]
fn evidence_directory_is_created_once_and_sealed_without_payloads() {
    let dir = std::env::temp_dir().join(format!("btae-live-evidence-{}", std::process::id()));
    let mut req = request("success");
    // The started journal is fsynced inside the wall budget before spawning.
    // This checks sealing and payload redaction, not shared-runner fsync latency.
    req.limits.wall_ms = 10_000;
    req.output = Some(dir.clone());
    let marker = "LIVE_PAYLOAD_CANARY_20261103";
    req.prompt = marker.as_bytes().to_vec();
    req.environment.insert("BTAE_CANARY".into(), marker.into());
    let record = run_live(req).unwrap();
    assert_eq!(record.outcome, LiveOutcome::Completed);
    let started = fs::read_to_string(dir.join("started.json")).unwrap();
    let sealed = fs::read_to_string(dir.join("record.json")).unwrap();
    assert!(started.contains("prompt_sha256") && sealed.contains("stdout_sha256"));
    assert!(!sealed.contains("task\n"));
    assert!(!started.contains(marker));
    assert!(!sealed.contains(marker));
    let mut collision = request("success");
    collision.output = Some(dir.clone());
    assert!(run_live(collision).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn exit_zero_without_final_never_counts_as_completed() {
    let record = run_live(request("incomplete")).unwrap();
    assert_ne!(record.outcome, LiveOutcome::Completed);
    assert!(!record.summary.unwrap().completed);
}
#[test]
fn valid_transcript_with_nonzero_exit_is_worker_failure() {
    let record = run_live(request("failure")).unwrap();
    assert_eq!(record.outcome, LiveOutcome::WorkerFailed);
    assert_eq!(record.exit_code, Some(7));
    assert!(!record.summary.unwrap().completed);
}
#[test]
fn blocked_stdin_still_obeys_wall_deadline() {
    let mut req = request("stdin-blocked");
    req.prompt = vec![b'x'; 64 * 1024];
    req.limits.wall_ms = 80;
    let start = std::time::Instant::now();
    let record = run_live(req).unwrap();
    assert_eq!(record.outcome, LiveOutcome::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn exited_parent_does_not_leave_descendant_pipes_or_work_running() {
    let dir = std::env::temp_dir().join(format!("btae-live-descendant-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let mut req = request("descendant");
    req.cwd = Some(dir.clone());
    req.limits.wall_ms = 1500;
    let record = run_live(req).unwrap();
    assert_eq!(record.outcome, LiveOutcome::Completed);
    assert!(record.streams_closed);
    assert!(record.elapsed_ms < 1200);
    let before = fs::metadata(dir.join("heartbeat")).unwrap().len();
    thread::sleep(Duration::from_millis(60));
    assert_eq!(fs::metadata(dir.join("heartbeat")).unwrap().len(), before);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn prompt_and_zero_limits_are_rejected_before_spawning() {
    let mut req = request("success");
    req.prompt = vec![b'x'; 64 * 1024 + 1];
    assert!(run_live(req).is_err());
    let mut req = request("success");
    req.limits.dsh.max_events = 0;
    assert!(run_live(req).is_err());
}
#[test]
fn spawn_failure_leaves_a_sealed_failure_record() {
    let dir = std::env::temp_dir().join(format!("btae-live-spawn-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let exe = dir.join("not-executable");
    fs::write(&exe, b"not executable").unwrap();
    let mut req = request("success");
    req.executable = exe;
    req.output = Some(dir.join("evidence"));
    let r = run_live(req).unwrap();
    assert_eq!(r.outcome, LiveOutcome::InfrastructureError);
    assert!(dir.join("evidence/record.json").is_file());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cli_collect_has_success_and_failure_exit_codes_without_claiming_observation() {
    use std::process::Command;
    use std::process::Stdio;
    for (scenario, expected) in [("success", 0), ("incomplete", 1), ("malformed", 1)] {
        let child = Command::new(env!("CARGO_BIN_EXE_btae-eval"))
            .args(["collect", "--executable"])
            .arg(exe())
            .args([
                "--arg=--scenario",
                &format!("--arg={scenario}"),
                "--wall-ms",
                "1000",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(expected));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["data_kind"], "dsh_process_smoke_only");
        assert!(value["empirical_task_success"].is_null());
        assert!(serde_json::from_value::<btae_eval::Observation>(value).is_err());
    }
}

#![cfg(target_os = "linux")]
use btae_eval::dsh_live::LiveLimits;
use btae_eval::task_collect::*;
use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "btae-task-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
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
fn request(t: &Temp) -> TaskRequest {
    TaskRequest {
        manifest: manifest(),
        task_id: "queue-r01".into(),
        arm: "E".into(),
        repeat: 1,
        executable: env!("CARGO_BIN_EXE_dsh-json-fixture").into(),
        args: vec!["--scenario".into(), "success".into()],
        bundle: None,
        profile: None,
        environment: BTreeMap::new(),
        share_network: false,
        output: t.0.join("run"),
        cancel_file: None,
        limits: LiveLimits {
            wall_ms: 1500,
            ..Default::default()
        },
    }
}
#[test]
fn success_binds_plan_seed_and_snapshot_without_observation() {
    let t = Temp::new();
    let r = collect_task(request(&t)).unwrap();
    assert_eq!(r.outcome, "completed");
    assert_eq!(r.network, "disabled");
    assert_eq!(r.run.task_id, "queue-r01");
    assert_eq!(r.data_kind, "dsh_task_engineering_only");
    assert!(
        r.hidden_test_result.is_none() && r.empirical_task_success.is_none() && !r.billing_verified
    );
    assert!(
        serde_json::from_value::<btae_eval::Observation>(serde_json::to_value(&r).unwrap())
            .is_err()
    );
    let verified = verify_task(&t.0.join("run"), &manifest()).unwrap();
    assert_eq!(verified.run, r.run);
    assert!(t.0.join("run/workspace-snapshot/api.py").is_file());
}
#[test]
fn isolated_worker_edits_only_export_and_cannot_read_host_or_connect() {
    let t = Temp::new();
    let probe = t.0.join("host-canary");
    fs::write(&probe, "HOST_ONLY_CANARY").unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let before =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("repositories/queue/api.py")).unwrap();
    let mut req = request(&t);
    req.args[1] = "task-isolation".into();
    req.environment
        .insert("BTAE_HOST_PROBE".into(), probe.to_str().unwrap().into());
    req.environment.insert(
        "BTAE_HOST_ADDRESS".into(),
        server.local_addr().unwrap().to_string(),
    );
    req.environment.insert("BTAE_ALLOWED".into(), "yes".into());
    let r = collect_task(req).unwrap();
    assert_eq!(r.outcome, "completed");
    assert_eq!(
        fs::read_to_string(t.0.join("run/workspace-snapshot/worker-edit.txt")).unwrap(),
        "isolated edit"
    );
    assert_eq!(
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("repositories/queue/api.py")).unwrap(),
        before
    );
    verify_task(&t.0.join("run"), &manifest()).unwrap();
}
#[test]
fn failure_timeout_cancel_and_bad_protocol_are_sealed() {
    for (scenario, expected) in [
        ("failure", "worker_failed"),
        ("timeout", "timed_out"),
        ("malformed", "protocol_error"),
    ] {
        let t = Temp::new();
        let mut req = request(&t);
        req.args[1] = scenario.into();
        // Only the hanging fixture needs a short deadline. Failure/protocol
        // cases must have time to emit their result on a loaded CI host.
        if scenario == "timeout" {
            req.limits.wall_ms = 80;
        }
        let r = collect_task(req).unwrap();
        assert_eq!(r.outcome, expected);
        let process: serde_json::Value =
            serde_json::from_slice(&fs::read(t.0.join("run/process/record.json")).unwrap())
                .unwrap();
        assert_eq!(process["streams_closed"], true);
        if scenario == "failure" {
            assert_eq!(process["exit_code"], 7);
        } else {
            assert!(
                process["exit_code"].is_null(),
                "hanging worker must be killed"
            );
        }
        verify_task(&t.0.join("run"), &manifest()).unwrap();
    }
    let t = Temp::new();
    let cancel = t.0.join("cancel");
    fs::write(&cancel, "cancel").unwrap();
    let mut req = request(&t);
    req.cancel_file = Some(cancel);
    let r = collect_task(req).unwrap();
    assert_eq!(r.outcome, "cancelled");
    verify_task(&t.0.join("run"), &manifest()).unwrap();
}
#[test]
fn later_rounds_and_existing_outputs_are_rejected() {
    let t = Temp::new();
    collect_task(request(&t)).unwrap();
    assert!(
        collect_task(request(&t))
            .unwrap_err()
            .to_string()
            .contains("exist")
    );
    let t = Temp::new();
    let mut req = request(&t);
    req.task_id = "queue-r02".into();
    assert!(
        collect_task(req)
            .unwrap_err()
            .to_string()
            .contains("initial-round")
    );
    assert!(!t.0.join("run").exists());
}
#[test]
fn modified_seed_leaves_started_but_never_a_completed_task() {
    let t = Temp::new();
    let root = t.0.join("source");
    fs::create_dir_all(root.join("datasets")).unwrap();
    fs::copy(manifest(), root.join("datasets/pilot.json")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("datasets/queue-snapshot.json"),
        root.join("datasets/queue-snapshot.json"),
    )
    .unwrap();
    fs::create_dir_all(root.join("baselines")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("baselines/E.md"),
        root.join("baselines/E.md"),
    )
    .unwrap();
    fs::create_dir_all(root.join("repositories/queue")).unwrap();
    fs::write(root.join("repositories/queue/README.md"), "changed").unwrap();
    let mut req = request(&t);
    req.manifest = root.join("datasets/pilot.json");
    assert!(collect_task(req).is_err());
    assert!(t.0.join("run/started.json").is_file());
    assert!(!t.0.join("run/task.json").exists());
    assert!(verify_task(&t.0.join("run"), &manifest()).is_err());
}
#[test]
fn tampered_missing_extra_and_symlink_artifacts_are_rejected() {
    for mode in ["edit", "missing", "extra", "symlink"] {
        let t = Temp::new();
        collect_task(request(&t)).unwrap();
        let root = t.0.join("run");
        match mode {
            "edit" => fs::write(root.join("workspace-snapshot/api.py"), "changed").unwrap(),
            "missing" => fs::remove_file(root.join("workspace-snapshot/api.py")).unwrap(),
            "extra" => fs::write(root.join("unexpected"), "extra").unwrap(),
            _ => std::os::unix::fs::symlink("/etc/passwd", root.join("linked")).unwrap(),
        }
        assert!(verify_task(&root, &manifest()).is_err(), "{mode}");
    }
}
#[test]
fn wrong_plan_receipt_and_sop_are_rejected() {
    let t = Temp::new();
    collect_task(request(&t)).unwrap();
    let p = t.0.join("run/task.json");
    let original = fs::read(&p).unwrap();
    for field in ["run", "outcome", "network"] {
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        if field == "run" {
            value["run"]["arm"] = "A".into();
        } else {
            value[field] = "forged".into();
        }
        fs::write(&p, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            verify_task(&t.0.join("run"), &manifest()).is_err(),
            "{field}"
        );
    }
}
#[test]
fn unsafe_profile_environment_and_overlapping_output_are_rejected() {
    let t = Temp::new();
    let p = t.0.join("profile");
    fs::create_dir(&p).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", p.join("cordis.yml")).unwrap();
    fs::write(p.join("cordis.patch.yml"), "[]").unwrap();
    let mut req = request(&t);
    req.profile = Some(p);
    assert!(
        collect_task(req)
            .unwrap_err()
            .to_string()
            .contains("profile")
    );
    let mut req = request(&t);
    req.environment.insert("HOME".into(), "/host".into());
    assert!(
        collect_task(req)
            .unwrap_err()
            .to_string()
            .contains("reserved")
    );
    let mut req = request(&t);
    req.output = Path::new(env!("CARGO_MANIFEST_DIR")).join("repositories/queue/unsafe-output");
    assert!(
        collect_task(req)
            .unwrap_err()
            .to_string()
            .contains("overlap")
    );
}

#[test]
fn cli_collects_verifies_and_returns_nonzero_for_worker_failure() {
    use std::process::Command;
    for (scenario, expected) in [("success", 0), ("failure", 1)] {
        let t = Temp::new();
        let output = t.0.join("run");
        let r = Command::new(env!("CARGO_BIN_EXE_btae-eval"))
            .args(["collect-task", "--manifest"])
            .arg(manifest())
            .args([
                "--executable",
                env!("CARGO_BIN_EXE_dsh-json-fixture"),
                "--arg=--scenario",
                &format!("--arg={scenario}"),
                "--output",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(
            r.status.code(),
            Some(expected),
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(value["data_kind"], "dsh_task_engineering_only");
        let verified = Command::new(env!("CARGO_BIN_EXE_btae-eval"))
            .arg("verify-task")
            .arg(&output)
            .arg(manifest())
            .output()
            .unwrap();
        assert!(
            verified.status.success(),
            "{}",
            String::from_utf8_lossy(&verified.stderr)
        );
    }
}

#[test]
fn deployment_bundle_cannot_mount_the_project_or_host_home() {
    let t = Temp::new();
    let mut req = request(&t);
    req.bundle = Some(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf());
    assert!(
        collect_task(req)
            .unwrap_err()
            .to_string()
            .contains("bundle")
    );
    assert!(!t.0.join("run").exists());
    if let Some(home) = std::env::var_os("HOME") {
        let mut req = request(&t);
        req.bundle = Some(home.into());
        assert!(
            collect_task(req)
                .unwrap_err()
                .to_string()
                .contains("bundle")
        );
    }
}
#[test]
fn profile_export_is_explicit_and_environment_values_stay_out_of_receipts() {
    use std::os::unix::fs::PermissionsExt;
    let t = Temp::new();
    let profile = t.0.join("profile");
    fs::create_dir(&profile).unwrap();
    for name in ["cordis.yml", "cordis.patch.yml"] {
        fs::write(profile.join(name), "public configuration").unwrap();
    }
    fs::write(
        profile.join("unselected-private-file"),
        "LOCAL_PROFILE_CANARY",
    )
    .unwrap();
    let mut req = request(&t);
    req.profile = Some(profile);
    req.environment
        .insert("BTAE_ALLOWED".into(), "ENV_VALUE_CANARY_20261103".into());
    collect_task(req).unwrap();
    let root = t.0.join("run");
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(root.join("state/profiles/headless/cordis.yml").is_file());
    assert!(
        !root
            .join("state/profiles/headless/unselected-private-file")
            .exists()
    );
    for file in [
        "started.json",
        "task.json",
        "process/started.json",
        "process/record.json",
    ] {
        let contents = fs::read_to_string(root.join(file)).unwrap();
        assert!(!contents.contains("ENV_VALUE_CANARY_20261103"));
        assert!(!contents.contains("LOCAL_PROFILE_CANARY"));
    }
    verify_task(&root, &manifest()).unwrap();
}
#[test]
fn a_changed_sop_or_manifest_is_rejected_by_offline_verification() {
    let t = Temp::new();
    let source = t.0.join("source");
    for (from, to) in [
        (manifest(), source.join("datasets/pilot.json")),
        (
            Path::new(env!("CARGO_MANIFEST_DIR")).join("baselines/E.md"),
            source.join("baselines/E.md"),
        ),
    ] {
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(from, to).unwrap();
    }
    collect_task(request(&t)).unwrap();
    let path = source.join("datasets/pilot.json");
    verify_task(&t.0.join("run"), &path).unwrap();
    fs::write(source.join("baselines/E.md"), "changed SOP").unwrap();
    assert!(
        verify_task(&t.0.join("run"), &path)
            .unwrap_err()
            .to_string()
            .contains("SOP")
    );
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["seed"] = 123.into();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(verify_task(&t.0.join("run"), &path).is_err());
}

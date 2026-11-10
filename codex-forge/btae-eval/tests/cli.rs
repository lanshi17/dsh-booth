use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn binary() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_btae-eval"));
    c.current_dir(env!("CARGO_MANIFEST_DIR"));
    c
}

#[test]
fn cli_plans_without_launching_and_refuses_unready_study() {
    let planned = binary()
        .args(["plan", "datasets/pilot.json"])
        .output()
        .unwrap();
    assert!(planned.status.success());
    let value: Value = serde_json::from_slice(&planned.stdout).unwrap();
    assert_eq!(value["status"], "PLANNING_ONLY_NO_AGENT_EXECUTED");
    assert_eq!(value["planned_runs"].as_array().unwrap().len(), 360);
    let refused = binary()
        .args(["check-ready", "datasets/pilot.json"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert!(!value["blockers"].as_array().unwrap().is_empty());
    assert_eq!(value["declarations_complete"], false);
}

#[test]
fn cli_fixture_score_is_math_and_bootstrap_cannot_announce_superiority() {
    let scored = binary()
        .args(["score", "examples/scorer-fixture.json"])
        .output()
        .unwrap();
    assert!(scored.status.success());
    let value: Value = serde_json::from_slice(&scored.stdout).unwrap();
    assert_eq!(value["impact"]["precision"], 0.875);
    assert_eq!(value["impact"]["recall"], 0.7);
    let sampled = binary()
        .args(["bootstrap", "examples/bootstrap-fixture.json"])
        .output()
        .unwrap();
    assert!(sampled.status.success());
    let value: Value = serde_json::from_slice(&sampled.stdout).unwrap();
    assert_eq!(
        value["analysis_kind"],
        "EXPLORATORY_CLUSTER_BOOTSTRAP_NO_SUPERIORITY_DECISION"
    );
    assert_eq!(value["clusters"], 4);
    assert!(value.get("p_value").is_none());
}

#[test]
fn cli_has_no_unimplemented_execute_command() {
    assert_eq!(
        binary().arg("execute").output().unwrap().status.code(),
        Some(2)
    );
}

#[test]
fn diagnostic_seed_files_match_frozen_snapshot_manifests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest: btae_eval::Manifest =
        serde_json::from_slice(&std::fs::read(root.join("datasets/pilot.json")).unwrap()).unwrap();
    for repo in &manifest.repositories {
        let snapshot: Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("datasets/{}-snapshot.json", repo.id))).unwrap(),
        )
        .unwrap();
        let files = snapshot["files"].as_array().unwrap();
        let mut sorted = Vec::new();
        for pair in files {
            let path = pair[0].as_str().unwrap();
            let hash = pair[1].as_str().unwrap();
            let bytes = std::fs::read(root.join("repositories").join(&repo.id).join(path)).unwrap();
            assert_eq!(btae_eval::digest(&bytes), hash);
            sorted.push((path.to_string(), hash.to_string()));
        }
        sorted.sort();
        assert_eq!(
            btae_eval::digest(&serde_json::to_vec(&sorted).unwrap()),
            repo.snapshot_sha256
        );
        assert_eq!(snapshot["snapshot_sha256"], repo.snapshot_sha256);
    }
}

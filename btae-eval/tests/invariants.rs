use btae_eval::*;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

fn manifest() -> Manifest {
    serde_json::from_str(include_str!("../datasets/pilot.json")).unwrap()
}
fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}
fn observation() -> Observation {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../examples/scorer-fixture.json")).unwrap();
    serde_json::from_value(value["observation"].clone()).unwrap()
}

#[test]
fn schedule_is_balanced_reproducible_and_round_ordered() {
    let m = manifest();
    let bytes = include_bytes!("../datasets/pilot.json");
    let a = plan(&m, bytes).unwrap();
    let b = plan(&m, bytes).unwrap();
    assert_eq!(a.planned_runs, b.planned_runs);
    assert_eq!(a.planned_runs.len(), 360);
    let mut cells: BTreeMap<(&str, u32), BTreeSet<&str>> = BTreeMap::new();
    let mut trajectories: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut paired_seeds: BTreeMap<(&str, u32), BTreeSet<u64>> = BTreeMap::new();
    for row in &a.planned_runs {
        assert!(ids.insert(&row.run_id));
        cells
            .entry((&row.task_id, row.repeat))
            .or_default()
            .insert(&row.arm);
        trajectories
            .entry(&row.trajectory_id)
            .or_default()
            .push(row.round);
        paired_seeds
            .entry((&row.task_id, row.repeat))
            .or_default()
            .insert(row.model_seed);
    }
    assert_eq!(cells.len(), 72);
    assert!(cells.values().all(|s| s.len() == 5));
    assert_eq!(trajectories.len(), 60);
    assert!(trajectories.values().all(|v| *v == vec![1, 2, 3, 4, 5, 6]));
    assert!(paired_seeds.values().all(|s| s.len() == 1));
    let mut different = m;
    different.seed += 1;
    assert!(plan(&different, bytes).is_err());
    assert_ne!(
        a.planned_runs,
        plan(&different, &serde_json::to_vec(&different).unwrap())
            .unwrap()
            .planned_runs
    );
}

#[test]
fn malformed_or_incomplete_designs_do_not_silently_plan() {
    let mut m = manifest();
    m.tasks.push(m.tasks[0].clone());
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.tasks[0].repository = "missing".into();
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.tasks[0].round = 40;
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.repeats = 0;
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.repositories[0].snapshot_sha256 = "latest".into();
    assert!(m.validate().is_err());
}

#[test]
fn draft_is_valid_but_cannot_launch() {
    let m = manifest();
    assert!(m.validate().is_ok());
    let blockers = m.blockers();
    for key in [
        "draft",
        "ground-truth",
        "M1-M6",
        "medium",
        "immutable",
        "hidden tests",
    ] {
        assert!(blockers.iter().any(|b| b.contains(key)), "{key}");
    }
}

#[test]
fn precision_recall_and_f1_match_known_example() {
    let expected = set(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]);
    let predicted = set(&["1", "2", "3", "4", "5", "6", "7", "wrong"]);
    let s = set_score(&expected, &predicted);
    assert_eq!(
        (s.true_positive, s.false_positive, s.false_negative),
        (7, 1, 3)
    );
    assert_eq!(s.precision, Some(0.875));
    assert_eq!(s.recall, Some(0.7));
    assert!((s.f1.unwrap() - 14.0 / 18.0).abs() < 1e-12);
}

#[test]
fn empty_denominators_are_missing_not_perfect_scores() {
    let empty = BTreeSet::new();
    let one = set(&["one"]);
    let s = set_score(&empty, &empty);
    assert!(s.precision.is_none() && s.recall.is_none() && s.f1.is_none());
    let s = set_score(&one, &empty);
    assert_eq!(s.precision, None);
    assert_eq!(s.recall, Some(0.0));
    assert_eq!(s.f1, Some(0.0));
    let s = set_score(&empty, &one);
    assert_eq!(s.recall, None);
    assert_eq!(s.f1, Some(0.0));
}

#[test]
fn architecture_requires_real_opportunities_and_unique_finite_rules() {
    assert_eq!(architecture_score(&[]).unwrap(), None);
    let valid = RuleCount {
        rule: "dependency".into(),
        weight: 3.0,
        violations: 2,
        opportunities: 10,
    };
    assert_eq!(
        architecture_score(std::slice::from_ref(&valid)).unwrap(),
        Some(0.2)
    );
    assert!(architecture_score(&[valid.clone(), valid.clone()]).is_err());
    for weight in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        let mut c = valid.clone();
        c.weight = weight;
        assert!(architecture_score(&[c]).is_err());
    }
    let mut impossible = valid;
    impossible.opportunities = 1;
    assert!(architecture_score(&[impossible]).is_err());
}

#[test]
fn only_verified_completed_within_budget_counts_as_success() {
    let m = manifest();
    let mut o = observation();
    assert!(
        score(&o, &set(&["a"]), &m.budget)
            .unwrap()
            .functional_success
    );
    o.hidden_test_passed = None;
    let s = score(&o, &set(&["a"]), &m.budget).unwrap();
    assert!(!s.functional_success && !s.hidden_test_result_recorded);
    o.hidden_test_passed = Some(true);
    for status in [
        TerminalStatus::Failed,
        TerminalStatus::TimedOut,
        TerminalStatus::BudgetExhausted,
        TerminalStatus::InfrastructureError,
    ] {
        o.status = status;
        assert!(
            !score(&o, &set(&["a"]), &m.budget)
                .unwrap()
                .functional_success
        );
    }
    o.status = TerminalStatus::Completed;
    o.usage.input_tokens = m.budget.total_tokens;
    assert!(
        !score(&o, &set(&["a"]), &m.budget)
            .unwrap()
            .functional_success
    );
}

#[test]
fn self_reports_negative_costs_invalid_cache_and_overflow_are_rejected() {
    let m = manifest();
    let mut o = observation();
    o.data_kind = "worker_report".into();
    assert!(score(&o, &set(&[]), &m.budget).is_err());
    let mut o = observation();
    o.usage.human_cost_usd = -1.0;
    assert!(score(&o, &set(&[]), &m.budget).is_err());
    let mut o = observation();
    o.usage.cached_input_tokens = o.usage.input_tokens + 1;
    assert!(score(&o, &set(&[]), &m.budget).is_err());
    let mut o = observation();
    o.usage.input_tokens = u64::MAX;
    assert!(score(&o, &set(&[]), &m.budget).is_err());
    let mut o = observation();
    o.usage.attempts = 0;
    assert!(score(&o, &set(&[]), &m.budget).is_err());
}

#[test]
fn cost_counts_failed_work_and_does_not_double_bill_cached_tokens() {
    let m = manifest();
    let mut o = observation();
    o.status = TerminalStatus::Failed;
    let s = score(&o, &set(&[]), &m.budget).unwrap();
    assert!(!s.functional_success);
    assert!((s.total_cost_usd - 0.06).abs() < 1e-12);
    // Cached tokens are a subset of input, not additional tokens or another cost item.
    o.usage.cached_input_tokens = 0;
    assert_eq!(
        s.total_cost_usd,
        score(&o, &set(&[]), &m.budget).unwrap().total_cost_usd
    );
}

#[test]
fn bootstrap_clusters_repositories_and_is_order_invariant() {
    let pairs: Vec<_> = (0..4)
        .map(|i| ClusterPair {
            repository: format!("r{i}"),
            treatment: 0.75,
            baseline: 0.5,
        })
        .collect();
    let a = cluster_bootstrap(&pairs, 42, 1000).unwrap();
    assert_eq!(a.clusters, 4);
    assert_eq!(a.difference, 0.25);
    assert_eq!(a.percentile_ci_95, [0.25, 0.25]);
    let mut reverse = pairs.clone();
    reverse.reverse();
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(cluster_bootstrap(&reverse, 42, 1000).unwrap()).unwrap()
    );
    assert!(cluster_bootstrap(&pairs[..1], 42, 1000).is_err());
    let mut duplicate = pairs.clone();
    duplicate[1].repository = duplicate[0].repository.clone();
    assert!(cluster_bootstrap(&duplicate, 42, 1000).is_err());
}

#[test]
fn sha256_is_standard_and_unknown_json_fields_are_not_ignored() {
    assert_eq!(
        digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let mut value = serde_json::to_value(manifest()).unwrap();
    value["quiet_extra_budget"] = true.into();
    assert!(serde_json::from_value::<Manifest>(value).is_err());
}

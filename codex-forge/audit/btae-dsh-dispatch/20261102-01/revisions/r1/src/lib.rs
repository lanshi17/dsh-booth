//! Research preparation/scoring and explicitly separate sandboxed fixture diagnostics.
pub mod dsh;
#[cfg(target_os = "linux")]
pub mod runner;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const ARMS: [&str; 5] = ["A", "B", "C", "D", "E"];

#[derive(Debug, Error)]
#[error("invalid experiment input: {0}")]
pub struct EvalError(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub provider: String,
    pub name: String,
    pub immutable_revision: Option<String>,
    pub framework: String,
    pub framework_version: Option<String>,
    pub profile_sha256: Option<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub seed_supported: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub total_tokens: u64,
    pub wall_seconds: u64,
    /// Includes the first attempt; all retries consume the same total budget.
    pub attempts: u32,
    pub agents: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub id: String,
    pub snapshot_sha256: String,
    pub scale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSpec {
    pub id: String,
    pub repository: String,
    pub round: u32,
    pub kind: String,
    pub request: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readiness {
    pub method_frozen: bool,
    pub baselines_validated: bool,
    pub mechanisms_verified: bool,
    pub evaluator_isolation_verified: bool,
    pub independent_gold_reviewers: BTreeSet<String>,
    pub annotation_agreement_recorded: bool,
    pub hidden_tests_validated: bool,
    pub environment_image_digest: Option<String>,
    pub profile_sanitization_reviewed: bool,
    pub pricing_frozen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub study_id: String,
    pub status: String,
    pub seed: u64,
    pub repeats: u32,
    pub model: Model,
    pub budget: Budget,
    pub repositories: Vec<Repository>,
    pub tasks: Vec<TaskSpec>,
    pub readiness: Readiness,
}

fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn sha256_string(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}

impl Manifest {
    pub fn validate(&self) -> Result<(), EvalError> {
        let fail = |s: &str| Err(EvalError(s.into()));
        if self.schema_version != 1
            || !nonempty(&self.study_id)
            || !matches!(self.status.as_str(), "draft" | "frozen")
        {
            return fail("unknown schema/status or empty study id");
        }
        if self.repeats == 0
            || self.repeats > 100
            || self.budget.total_tokens == 0
            || self.budget.wall_seconds == 0
            || self.budget.agents == 0
            || self.budget.attempts == 0
        {
            return fail("repeats and all budgets must be positive (repeats <= 100)");
        }
        if !nonempty(&self.model.provider)
            || !nonempty(&self.model.name)
            || !nonempty(&self.model.framework)
        {
            return fail("model/framework identity must be present");
        }
        if self
            .model
            .temperature
            .is_some_and(|v| !v.is_finite() || !(0.0..=2.0).contains(&v))
            || self
                .model
                .top_p
                .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1.0)
        {
            return fail("invalid model sampling parameters");
        }
        let mut repositories = BTreeSet::new();
        for repo in &self.repositories {
            if !nonempty(&repo.id)
                || !repositories.insert(&repo.id)
                || !sha256_string(&repo.snapshot_sha256)
                || !matches!(repo.scale.as_str(), "small" | "medium" | "large")
            {
                return fail("repository id, scale or snapshot digest is invalid");
            }
        }
        if repositories.is_empty() || self.tasks.is_empty() {
            return fail("empty experiment");
        }
        let mut ids = BTreeSet::new();
        let mut rounds: BTreeMap<&str, BTreeSet<u32>> = BTreeMap::new();
        for task in &self.tasks {
            if !nonempty(&task.id)
                || !ids.insert(&task.id)
                || !repositories.contains(&task.repository)
                || task.round == 0
                || !nonempty(&task.request)
                || !matches!(task.kind.as_str(), "T1" | "T2" | "T3" | "T4" | "T5")
            {
                return fail("duplicate task, missing repository, invalid kind/round/request");
            }
            if !rounds
                .entry(&task.repository)
                .or_default()
                .insert(task.round)
            {
                return fail("only one task per repository round is supported");
            }
        }
        for repo in &repositories {
            let sequence = rounds
                .get(repo.as_str())
                .ok_or_else(|| EvalError("repository without tasks".into()))?;
            if sequence.iter().copied().ne(1..=sequence.len() as u32) {
                return fail("trajectory rounds must be contiguous from 1");
            }
        }
        Ok(())
    }

    /// Declarations are required planning metadata, not proof that controls were enforced.
    pub fn blockers(&self) -> Vec<String> {
        let mut b = Vec::new();
        if let Err(e) = self.validate() {
            b.push(e.to_string());
        }
        if self.status != "frozen" {
            b.push("protocol and dataset are draft".into());
        }
        let r = &self.readiness;
        for (flag, name) in [
            (r.method_frozen, "BTAE method not frozen"),
            (
                r.baselines_validated,
                "baseline implementations not validated",
            ),
            (
                r.mechanisms_verified,
                "M1-M6 actual enforcement not verified",
            ),
            (
                r.evaluator_isolation_verified,
                "private evaluator isolation not verified",
            ),
            (
                r.annotation_agreement_recorded,
                "annotation agreement/adjudication absent",
            ),
            (
                r.hidden_tests_validated,
                "hidden tests/reference repairs not validated",
            ),
            (
                r.profile_sanitization_reviewed,
                "common profile secret/leakage review absent",
            ),
            (r.pricing_frozen, "pricing and human-time rates not frozen"),
        ] {
            if !flag {
                b.push(name.into());
            }
        }
        if r.independent_gold_reviewers.len() < 2
            || r.independent_gold_reviewers.iter().any(|s| !nonempty(s))
        {
            b.push("need two distinct independent ground-truth reviewers".into());
        }
        if !r
            .environment_image_digest
            .as_deref()
            .is_some_and(sha256_string)
        {
            b.push("container image digest missing or invalid".into());
        }
        if !self
            .model
            .immutable_revision
            .as_deref()
            .is_some_and(nonempty)
        {
            b.push("provider model alias is not an immutable model revision".into());
        }
        if self.model.temperature.is_none()
            || self.model.top_p.is_none()
            || self.model.seed_supported.is_none()
        {
            b.push("sampling parameters and provider seed support not frozen".into());
        }
        if !self
            .model
            .framework_version
            .as_deref()
            .is_some_and(nonempty)
            || !self
                .model
                .profile_sha256
                .as_deref()
                .is_some_and(sha256_string)
        {
            b.push("framework version or sanitized profile digest missing".into());
        }
        if !self.repositories.iter().any(|r| r.scale == "medium") {
            b.push(
                "small diagnostic repositories do not cover the planned medium-project stratum"
                    .into(),
            );
        }
        b
    }
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Portable SplitMix64 with rejection sampling; used for allocation, never security.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    fn index(&mut self, n: usize) -> usize {
        let n = n as u64;
        let threshold = n.wrapping_neg() % n;
        loop {
            let value = self.next();
            if value >= threshold {
                return (value % n) as usize;
            }
        }
    }
    fn shuffle<T>(&mut self, values: &mut [T]) {
        for i in (1..values.len()).rev() {
            values.swap(i, self.index(i + 1));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedRun {
    pub order: usize,
    pub run_id: String,
    pub repository: String,
    pub task_id: String,
    pub round: u32,
    pub repeat: u32,
    pub arm: String,
    pub trajectory_id: String,
    pub model_seed: u64,
}

#[derive(Debug, Serialize)]
pub struct Schedule {
    pub status: &'static str,
    pub manifest_sha256: String,
    pub blockers: Vec<String>,
    pub planned_runs: Vec<PlannedRun>,
}

pub fn plan(manifest: &Manifest, manifest_bytes: &[u8]) -> Result<Schedule, EvalError> {
    manifest.validate()?;
    let parsed: Manifest = serde_json::from_slice(manifest_bytes)
        .map_err(|e| EvalError(format!("manifest bytes cannot be parsed: {e}")))?;
    if serde_json::to_value(&parsed).map_err(|e| EvalError(e.to_string()))?
        != serde_json::to_value(manifest).map_err(|e| EvalError(e.to_string()))?
    {
        return Err(EvalError(
            "manifest object differs from the source bytes to be hashed".into(),
        ));
    }
    let manifest_hash = digest(manifest_bytes);
    let mut rng = Rng(manifest.seed);
    let mut rows = Vec::new();
    let rounds = manifest.tasks.iter().map(|t| t.round).max().unwrap_or(0);
    for round in 1..=rounds {
        let mut block = Vec::new();
        for task in manifest.tasks.iter().filter(|t| t.round == round) {
            for repeat in 1..=manifest.repeats {
                // Same seed within a matched task/repetition block; provider support is recorded separately.
                let model_seed = rng.next();
                for arm in ARMS {
                    let key = format!(
                        "{manifest_hash}:{}:{}:{arm}:{repeat}",
                        manifest.study_id, task.id
                    );
                    let trajectory_key = format!(
                        "{manifest_hash}:{}:{}:{arm}:{repeat}",
                        manifest.study_id, task.repository
                    );
                    block.push(PlannedRun {
                        order: 0,
                        run_id: digest(key.as_bytes()),
                        repository: task.repository.clone(),
                        task_id: task.id.clone(),
                        round,
                        repeat,
                        arm: arm.into(),
                        trajectory_id: digest(trajectory_key.as_bytes()),
                        model_seed,
                    });
                }
            }
        }
        rng.shuffle(&mut block);
        rows.extend(block);
    }
    for (i, row) in rows.iter_mut().enumerate() {
        row.order = i + 1;
    }
    Ok(Schedule {
        status: "PLANNING_ONLY_NO_AGENT_EXECUTED",
        manifest_sha256: digest(manifest_bytes),
        blockers: manifest.blockers(),
        planned_runs: rows,
    })
}

#[derive(Debug, Serialize)]
pub struct SetScore {
    pub true_positive: usize,
    pub false_positive: usize,
    pub false_negative: usize,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub f1: Option<f64>,
}

pub fn set_score(expected: &BTreeSet<String>, predicted: &BTreeSet<String>) -> SetScore {
    let tp = expected.intersection(predicted).count();
    let fp = predicted.difference(expected).count();
    let fn_ = expected.difference(predicted).count();
    let ratio = |a, b| {
        if b == 0 {
            None
        } else {
            Some(a as f64 / b as f64)
        }
    };
    SetScore {
        true_positive: tp,
        false_positive: fp,
        false_negative: fn_,
        precision: ratio(tp, tp + fp),
        recall: ratio(tp, tp + fn_),
        f1: ratio(2 * tp, 2 * tp + fp + fn_),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleCount {
    pub rule: String,
    pub weight: f64,
    pub violations: u64,
    pub opportunities: u64,
}

pub fn architecture_score(counts: &[RuleCount]) -> Result<Option<f64>, EvalError> {
    let mut seen = BTreeSet::new();
    let mut v = 0.0;
    let mut r = 0.0;
    for c in counts {
        if !nonempty(&c.rule)
            || !seen.insert(&c.rule)
            || !c.weight.is_finite()
            || c.weight <= 0.0
            || c.violations > c.opportunities
        {
            return Err(EvalError("invalid architecture rule counts/weights".into()));
        }
        v += c.weight * c.violations as f64;
        r += c.weight * c.opportunities as f64;
    }
    if !v.is_finite() || !r.is_finite() {
        return Err(EvalError("architecture weight overflow".into()));
    }
    Ok(if r == 0.0 { None } else { Some(v / r) })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    Completed,
    Failed,
    TimedOut,
    BudgetExhausted,
    InfrastructureError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub wall_seconds: f64,
    pub human_minutes: f64,
    pub model_cost_usd: f64,
    pub compute_cost_usd: f64,
    pub human_cost_usd: f64,
    pub attempts: u32,
    pub tools: u64,
}

impl Usage {
    fn validate(&self) -> Result<(), EvalError> {
        if self.cached_input_tokens > self.input_tokens
            || self.attempts == 0
            || self.input_tokens.checked_add(self.output_tokens).is_none()
            || [
                self.wall_seconds,
                self.human_minutes,
                self.model_cost_usd,
                self.compute_cost_usd,
                self.human_cost_usd,
            ]
            .iter()
            .any(|x| !x.is_finite() || *x < 0.0)
            || !(self.model_cost_usd + self.compute_cost_usd + self.human_cost_usd).is_finite()
        {
            return Err(EvalError("invalid cost/usage values".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Must be evaluator-authored; this tag is provenance, not authentication.
    pub data_kind: String,
    pub run: PlannedRun,
    pub status: TerminalStatus,
    pub usage: Usage,
    pub hidden_test_passed: Option<bool>,
    pub predictions_before_edit: BTreeSet<String>,
    pub architecture: Vec<RuleCount>,
}

#[derive(Debug, Serialize)]
pub struct ObservationScore {
    pub functional_success: bool,
    pub within_budget: bool,
    /// Records presence only. Caller-supplied JSON is not authenticated evidence.
    pub hidden_test_result_recorded: bool,
    pub impact: SetScore,
    pub architecture_drift: Option<f64>,
    pub total_cost_usd: f64,
    pub architecture_counts: Vec<RuleCount>,
}

pub fn score(
    observation: &Observation,
    expected: &BTreeSet<String>,
    budget: &Budget,
) -> Result<ObservationScore, EvalError> {
    if !matches!(
        observation.data_kind.as_str(),
        "scorer_fixture" | "evaluator_observation"
    ) {
        return Err(EvalError(
            "worker self-reports cannot be evaluation observations".into(),
        ));
    }
    observation.usage.validate()?;
    let u = &observation.usage;
    let within_budget = u.input_tokens + u.output_tokens <= budget.total_tokens
        && u.wall_seconds <= budget.wall_seconds as f64
        && u.attempts <= budget.attempts;
    Ok(ObservationScore {
        functional_success: within_budget
            && matches!(observation.status, TerminalStatus::Completed)
            && observation.hidden_test_passed == Some(true),
        within_budget,
        hidden_test_result_recorded: observation.hidden_test_passed.is_some(),
        impact: set_score(expected, &observation.predictions_before_edit),
        architecture_drift: architecture_score(&observation.architecture)?,
        total_cost_usd: u.model_cost_usd + u.compute_cost_usd + u.human_cost_usd,
        architecture_counts: observation.architecture.clone(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterPair {
    pub repository: String,
    /// Means of task-level means over repeats/rounds, precomputed with equal task weights.
    pub treatment: f64,
    pub baseline: f64,
}

#[derive(Debug, Serialize)]
pub struct BootstrapResult {
    pub analysis_kind: &'static str,
    pub clusters: usize,
    pub difference: f64,
    pub percentile_ci_95: [f64; 2],
    pub seed: u64,
    pub resamples: usize,
    pub warning: &'static str,
}

/// Exploratory repository-cluster bootstrap; never treats repeats/rounds as independent n.
pub fn cluster_bootstrap(
    pairs: &[ClusterPair],
    seed: u64,
    resamples: usize,
) -> Result<BootstrapResult, EvalError> {
    let mut ids = BTreeSet::new();
    if pairs.len() < 2
        || !(100..=1_000_000).contains(&resamples)
        || pairs.iter().any(|p| {
            !nonempty(&p.repository)
                || !ids.insert(&p.repository)
                || !p.treatment.is_finite()
                || !p.baseline.is_finite()
                || !((p.treatment - p.baseline).is_finite())
        })
    {
        return Err(EvalError(
            "bootstrap needs distinct finite repository pairs and 100..1000000 resamples".into(),
        ));
    }
    // Sort first so serialization order does not change the seeded analysis.
    let mut ordered: Vec<_> = pairs.iter().collect();
    ordered.sort_by_key(|p| &p.repository);
    let differences: Vec<_> = ordered.iter().map(|p| p.treatment - p.baseline).collect();
    let mean = |xs: &[f64]| xs.iter().map(|x| x / xs.len() as f64).sum::<f64>();
    let effect = mean(&differences);
    if !effect.is_finite() {
        return Err(EvalError("effect overflow".into()));
    }
    let mut rng = Rng(seed);
    let mut samples = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let sample: Vec<_> = (0..differences.len())
            .map(|_| differences[rng.index(differences.len())])
            .collect();
        let value = mean(&sample);
        if !value.is_finite() {
            return Err(EvalError("bootstrap overflow".into()));
        }
        samples.push(value);
    }
    samples.sort_by(f64::total_cmp);
    Ok(BootstrapResult {
        analysis_kind: "EXPLORATORY_CLUSTER_BOOTSTRAP_NO_SUPERIORITY_DECISION",
        clusters: pairs.len(),
        difference: effect,
        percentile_ci_95: [
            samples[((resamples - 1) as f64 * 0.025).floor() as usize],
            samples[((resamples - 1) as f64 * 0.975).ceil() as usize],
        ],
        seed,
        resamples,
        warning: "Few repositories yield unstable intervals. No p-value, power guarantee, or BTAE superiority follows. Inputs must retain all scheduled failures.",
    })
}

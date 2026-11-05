//! Wire types for the dsh-superagent management API (`/orchestrator/api/*`).
//!
//! The views are plain JSON built by the bundle's `src/app/views/*.ts`. Fields that this
//! console does not render are left as `Value` on purpose: the bundle is a research
//! prototype and its views grow, so decoding must not break on an unknown shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Health {
    pub state: String,
    pub detail: String,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Load {
    pub running: u32,
    pub queued: u32,
    pub max_workers: u32,
    pub idle: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SetupProblem {
    pub detail: String,
    pub code: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Accounts {
    pub repository: Option<String>,
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Setup {
    pub problem: Option<SetupProblem>,
    pub accounts: Option<Accounts>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StateView {
    pub paused: bool,
    pub master_id: Option<String>,
    pub health: BTreeMap<String, Health>,
    pub load: Load,
    pub setup: Setup,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Link {
    pub source: String,
    pub url: Option<String>,
    /// The bundle sends a localized label; older routes send a plain string.
    #[serde(default, deserialize_with = "localized_or_text")]
    pub label: Option<Localized>,
}

fn localized_or_text<'de, D>(deserializer: D) -> Result<Option<Localized>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Text(String),
        Map(Localized),
    }
    Ok(
        Option::<Raw>::deserialize(deserializer)?.map(|raw| match raw {
            Raw::Text(text) => Localized {
                zh: Some(text),
                en: None,
            },
            Raw::Map(map) => map,
        }),
    )
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadFacts {
    pub key: String,
    pub source: String,
    pub title: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Actions {
    pub retry: bool,
    pub stop: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketRow {
    pub id: String,
    pub title: String,
    pub state: String,
    pub stage: String,
    pub action: Option<String>,
    pub note: Option<String>,
    pub summary: Option<String>,
    pub directory: Option<String>,
    pub session_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub assignments: u32,
    pub inputs: u32,
    pub undelivered: u32,
    pub attention: Option<Value>,
    pub actions: Option<Actions>,
    pub strength: Option<Value>,
    pub threads: Vec<ThreadFacts>,
    pub reply: Option<Link>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Flow {
    pub received: u32,
    pub filtered: u32,
    pub triggers: u32,
    pub judged: u32,
    pub accepted: u32,
    pub evaluating: u32,
    pub dropped: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Overview {
    pub problems: Vec<Value>,
    pub person: Vec<TicketRow>,
    pub executor: Load,
    pub flow: Flow,
    pub policy_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Latest {
    pub actor: Option<Value>,
    pub human: bool,
    pub at: Option<String>,
    pub preview: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadTicket {
    pub id: String,
    pub title: String,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadRow {
    pub key: String,
    pub source: String,
    pub title: String,
    pub stage: String,
    pub last_at: Option<String>,
    pub created_at: Option<String>,
    pub subscriptions: u32,
    pub pending: u32,
    pub url: Option<String>,
    pub tickets: Vec<ThreadTicket>,
    pub latest: Option<Latest>,
    pub evaluating: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u32,
    #[serde(rename = "next_offset")]
    pub next_offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Mandate {
    pub actor: Option<Value>,
    pub strength: Option<Value>,
    pub rules: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Basis {
    pub facts: Vec<Value>,
    pub legacy: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Report {
    pub state: String,
    pub at: Option<String>,
    pub summary: Option<String>,
    pub question: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Assignment {
    pub id: String,
    pub state: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    #[serde(rename = "ref")]
    pub session_ref: Option<String>,
    pub report: Option<Report>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InputTrigger {
    pub id: String,
    pub actor: Option<Value>,
    pub human: bool,
    pub preview: String,
    pub thread: String,
    pub handback: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Input {
    pub id: String,
    pub kind: String,
    pub delivered: bool,
    pub text: Option<String>,
    pub trigger: Option<InputTrigger>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Decision {
    pub id: String,
    pub kind: String,
    pub outcome: String,
    pub reason: Value,
    pub at: Option<String>,
    pub strength: Option<Value>,
    pub ticket_id: Option<String>,
    pub ticket_title: Option<String>,
    pub thread: Option<String>,
    pub thread_title: Option<String>,
    pub preview: Option<String>,
    pub dismissed: Option<bool>,
    pub rules: Vec<Value>,
    /// The Triggers this decision judged; the console matches the instructions it sent by these.
    pub trigger_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DecisionList {
    pub items: Vec<Decision>,
    pub total: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketDetail {
    pub id: String,
    pub title: String,
    pub state: String,
    pub stage: String,
    pub action: Option<String>,
    pub note: Option<String>,
    pub summary: Option<String>,
    pub directory: Option<String>,
    pub session_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub strength: Option<Value>,
    pub attention: Option<Value>,
    pub actions: Option<Actions>,
    pub mandate: Option<Mandate>,
    pub basis: Option<Basis>,
    pub paused: bool,
    pub channels: Vec<Link>,
    pub decision: Option<Decision>,
    pub assignments: Vec<Assignment>,
    pub inputs: Vec<Input>,
    pub threads: Vec<ThreadRow>,
    pub environment: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TimelineItem {
    pub id: String,
    pub source: String,
    pub actor: Option<Value>,
    pub human: bool,
    pub at: Option<String>,
    pub preview: String,
    pub text: Option<String>,
    pub url: Option<String>,
    pub actions: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Correlated {
    pub key: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadDetail {
    pub key: String,
    pub source: String,
    pub title: String,
    pub stage: String,
    pub last_at: Option<String>,
    pub created_at: Option<String>,
    pub subscriptions: u32,
    pub pending: u32,
    pub url: Option<String>,
    pub context: Vec<String>,
    pub correlated: Vec<Correlated>,
    pub tickets: Vec<TicketRow>,
    pub timeline: Vec<TimelineItem>,
    pub decisions: Vec<Decision>,
    pub earlier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Localized {
    pub zh: Option<String>,
    pub en: Option<String>,
}

impl Localized {
    pub fn text(&self) -> String {
        self.zh
            .clone()
            .or_else(|| self.en.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceStats {
    pub received: u32,
    pub filtered: u32,
    pub last_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceStatus {
    pub id: String,
    pub kind: String,
    pub title: Localized,
    pub state: Option<String>,
    pub enabled: bool,
    pub installed: bool,
    pub health: Vec<Health>,
    pub stats: Option<SourceStats>,
    pub problem: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchView {
    pub tickets: Vec<TicketRow>,
    pub threads: Vec<ThreadRow>,
    pub items: Vec<Value>,
}

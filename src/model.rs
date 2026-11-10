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
    /// The bundle's machine-readable code, such as `pollFailures` or `setupPending`.
    pub code: Option<String>,
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
    /// Empty when the bundle has no address to link to.
    #[serde(default, deserialize_with = "empty_url_as_none")]
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

/// An optional link, where the bundle serves `""` for "there is no link".
///
/// `channelLink()` returns `url: ""` when the deployment lacks the facts a link needs, and a
/// Thread's `url` falls back to that same value. Read as `Some("")`, that empty address made
/// the console print a blank `link` line and an empty channel link instead of omitting the
/// line or saying `(no link)`; an empty served URL is the model's "no link".
fn empty_url_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.filter(|url| !url.is_empty()))
}

/// The Console's one-line note on a ticket, as the bundle's `ticketNote()` builds it.
///
/// Current routes send `{ "text": …, "error": … }`; older routes and the repository's fixture
/// send a plain string. Both become the text this console renders, because a person's pending
/// question is no reason for every list that carries the ticket to fail to decode.
fn note_or_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Text(String),
        Note { text: String },
    }
    Ok(
        Option::<Raw>::deserialize(deserializer)?.map(|raw| match raw {
            Raw::Text(text) => text,
            Raw::Note { text } => text,
        }),
    )
}

/// Why a Source filtered a message, as the reason object `/search` serves for a message that
/// reached no Thread, or as a plain string from an older route.
///
/// The bundle puts `disposition.filtered` on the search hit, and that value is a `Reason`
/// (`{ "code": …, "values": … }`), not the string this console used to expect. One filtered
/// message therefore failed the whole search view as `invalid type: map, expected a string`.
fn filtered_or_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Text(String),
        Reason {
            code: String,
            #[serde(default)]
            values: BTreeMap<String, Value>,
        },
    }
    Ok(
        Option::<Raw>::deserialize(deserializer)?.map(|raw| match raw {
            Raw::Text(text) => text,
            Raw::Reason { code, values } => {
                if values.is_empty() {
                    code
                } else {
                    let shown = values
                        .iter()
                        .map(|(name, value)| {
                            format!(
                                "{name}={}",
                                value
                                    .as_str()
                                    .map(str::to_string)
                                    .unwrap_or_else(|| value.to_string())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{code} ({shown})")
                }
            }
        }),
    )
}

#[cfg(test)]
mod note_tests {
    use super::{TicketDetail, TicketRow};
    use serde_json::json;

    #[test]
    fn a_ticket_note_object_decodes_to_the_text_the_console_renders() {
        // `ticketSummary` sends `ticketNote()`, which is `{ text, error }`; a person waiting
        // on an answer used to fail the whole ticket list as `invalid type: map, expected a
        // string`.
        let served = json!({
            "id": "t1",
            "title": "a ticket",
            "note": { "text": "需要确认按钮颜色", "error": false }
        });
        let ticket: TicketRow =
            serde_json::from_value(served).expect("a ticket with a note object decodes");
        assert_eq!(ticket.note.as_deref(), Some("需要确认按钮颜色"));
    }

    #[test]
    fn an_older_ticket_note_string_still_decodes() {
        // `dev/mock-api.py` and routes older than `ticketNote()` send a plain string.
        let served = json!({ "id": "t1", "title": "a ticket", "note": "waiting for your answer" });
        let ticket: TicketRow =
            serde_json::from_value(served).expect("a ticket with a text note decodes");
        assert_eq!(ticket.note.as_deref(), Some("waiting for your answer"));
    }

    #[test]
    fn a_ticket_detail_keeps_a_served_note_object() {
        let served = json!({
            "id": "t1",
            "title": "a ticket",
            "note": { "text": "the delivery failed", "error": true }
        });
        let ticket: TicketDetail =
            serde_json::from_value(served).expect("a ticket detail with a note object decodes");
        assert_eq!(ticket.note.as_deref(), Some("the delivery failed"));
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadFacts {
    pub key: String,
    pub source: String,
    pub title: String,
    /// Empty when the bundle has no address to link to.
    #[serde(default, deserialize_with = "empty_url_as_none")]
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
    /// The bundle's `{ text, error }` note, or a plain string from an older route.
    #[serde(default, deserialize_with = "note_or_text")]
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
    /// Empty when the bundle has no address to link to.
    #[serde(default, deserialize_with = "empty_url_as_none")]
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
    /// `/decisions` pages like the other lists (`{ items, next_offset }`) and has no `total`;
    /// this console used to print `total: 0` and drop the cursor from `--json`.
    #[serde(rename = "next_offset")]
    pub next_offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketDetail {
    pub id: String,
    pub title: String,
    pub state: String,
    pub stage: String,
    pub action: Option<String>,
    /// The bundle's `{ text, error }` note, or a plain string from an older route.
    #[serde(default, deserialize_with = "note_or_text")]
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
    /// Empty when the bundle has no address to link to.
    #[serde(default, deserialize_with = "empty_url_as_none")]
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
    /// Empty when the bundle has no address to link to.
    #[serde(default, deserialize_with = "empty_url_as_none")]
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

/// One message `/search` matched, as the bundle reports it in `SearchView.items`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchItem {
    pub id: String,
    pub source: String,
    /// The person or bot the bundle resolved, or its raw id.
    pub actor: Option<Value>,
    pub human: bool,
    pub at: Option<String>,
    pub preview: String,
    /// The thread's title, when the bundle could name it.
    pub thread_title: Option<String>,
    /// Why a Source filtered the message, when it did not reach a thread: the bundle's reason
    /// object, or a plain string from an older route.
    #[serde(default, deserialize_with = "filtered_or_text")]
    pub filtered: Option<String>,
    /// The rest of the served view, not rendered here but still printed by `--json`.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchView {
    pub tickets: Vec<TicketRow>,
    pub threads: Vec<ThreadRow>,
    pub items: Vec<SearchItem>,
}

#[cfg(test)]
mod tests {
    use super::{DecisionList, SearchView, ThreadDetail, TicketDetail};
    use serde_json::json;

    #[test]
    fn an_empty_served_link_is_no_link() {
        // `channelLink()` serves `url: ""` when the deployment has no address to link to; the
        // console printed a blank channel link instead of `(no link)`.
        let ticket: TicketDetail = serde_json::from_value(json!({
            "id": "t1", "title": "a local ticket", "state": "open", "stage": "person",
            "channels": [{ "source": "local", "url": "", "label": { "zh": "booth", "en": "booth" } }],
            "threads": [{ "key": "local:t-demo", "source": "local", "title": "a local thread",
                "stage": "subscribed", "url": "" }]
        }))
        .expect("a ticket whose local channel has no address decodes");
        assert_eq!(ticket.channels[0].url, None);
        assert_eq!(ticket.threads[0].url, None);
    }

    #[test]
    fn an_empty_served_thread_link_is_no_link() {
        // A Thread's `url` falls back to its first Trigger's `channelLink()`, so a local Thread
        // serves `""` too; the console printed a `link` line with nothing after it.
        let view: ThreadDetail = serde_json::from_value(json!({
            "key": "local:t-demo", "source": "local", "title": "a local thread",
            "stage": "subscribed", "url": "",
            "tickets": [{ "id": "t1", "title": "a ticket", "state": "open",
                "threads": [{ "key": "local:t-demo", "source": "local",
                    "title": "a local thread", "url": "" }] }],
            "timeline": [{ "id": "e1", "source": "local", "human": true,
                "preview": "hello", "url": "" }]
        }))
        .expect("a local thread without an address decodes");
        assert_eq!(view.url, None);
        assert_eq!(view.tickets[0].threads[0].url, None);
        assert_eq!(view.timeline[0].url, None);
    }

    #[test]
    fn a_served_link_keeps_its_address() {
        let ticket: TicketDetail = serde_json::from_value(json!({
            "id": "t1", "title": "a github ticket", "state": "open", "stage": "person",
            "channels": [{ "source": "github",
                "url": "https://github.com/o/r/issues/12", "label": "Issue #12" }]
        }))
        .expect("a ticket with a real channel link decodes");
        assert_eq!(
            ticket.channels[0].url.as_deref(),
            Some("https://github.com/o/r/issues/12")
        );
    }

    #[test]
    fn a_decision_page_keeps_the_served_cursor_and_has_no_total() {
        // The bundle's `decisionList()` answers `{ items, next_offset }`; the wire model used to
        // read a `total` the route never sends, so `--json` printed `total: 0` and dropped the
        // cursor a caller needs to ask for the next page.
        let served = json!({
            "items": [{
                "id": "d1",
                "kind": "accept",
                "outcome": "accept",
                "reason": [{ "code": "explicitRouted" }],
                "at": "2026-11-09T11:09:38.753Z"
            }],
            "next_offset": 30
        });
        let view: DecisionList =
            serde_json::from_value(served).expect("a served decision page decodes");
        assert_eq!(view.next_offset, Some(30));
        let printed = serde_json::to_value(&view).expect("the page serializes for --json");
        assert_eq!(printed["next_offset"], 30);
        assert!(
            printed.get("total").is_none(),
            "the route has no total; --json must not invent one"
        );
    }

    #[test]
    fn a_final_decision_page_keeps_a_null_cursor() {
        let served = json!({ "items": [], "next_offset": null });
        let view: DecisionList =
            serde_json::from_value(served).expect("a final decision page decodes");
        assert_eq!(view.next_offset, None);
    }

    #[test]
    fn a_filtered_search_hit_decodes_the_reason_object_the_bundle_sends() {
        // A Source-filtered message has no Thread, so `search()` puts the disposition's
        // `Reason` object in the hit's `filtered`; treating it as a string made the whole
        // view fail as `invalid type: map, expected a string`.
        let served = json!({
            "tickets": [],
            "threads": [],
            "items": [{
                "id": "e9",
                "source": "feishu",
                "human": true,
                "preview": "hello",
                "filtered": { "code": "chat_not_allowed", "values": { "chat": "oc_other" } }
            }, {
                "id": "e10",
                "source": "feishu",
                "human": true,
                "preview": "hi",
                "filtered": { "code": "not_a_member" }
            }]
        });
        let view: SearchView =
            serde_json::from_value(served).expect("a filtered search hit decodes");
        assert_eq!(
            view.items[0].filtered.as_deref(),
            Some("chat_not_allowed (chat=oc_other)")
        );
        assert_eq!(view.items[1].filtered.as_deref(), Some("not_a_member"));
    }

    #[test]
    fn an_older_filtered_search_hit_string_still_decodes() {
        // The fixture and routes before the structured Reason sent the sentence as one string.
        let served = json!({
            "tickets": [],
            "threads": [],
            "items": [{
                "id": "e9",
                "source": "github",
                "human": true,
                "preview": "hello",
                "filtered": "sender is not on the allow-list"
            }]
        });
        let view: SearchView =
            serde_json::from_value(served).expect("a textual filter reason decodes");
        assert_eq!(
            view.items[0].filtered.as_deref(),
            Some("sender is not on the allow-list")
        );
    }

    #[test]
    fn a_search_item_keeps_the_served_fields_this_view_does_not_render() {
        let served = json!({
            "tickets": [],
            "threads": [],
            "items": [{
                "id": "local-abc123",
                "source": "local",
                "human": true,
                "preview": "retry",
                "actions": { "requeue": true },
                "text": "the full served text"
            }]
        });
        let view: SearchView =
            serde_json::from_value(served).expect("a served search view decodes");
        let printed = serde_json::to_value(&view).expect("the view serializes for --json");
        assert_eq!(printed["items"][0]["actions"]["requeue"], true);
        assert_eq!(printed["items"][0]["text"], "the full served text");
    }
}

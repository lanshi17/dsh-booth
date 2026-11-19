//! CSS Agent interoperability.
//!
//! CSS Agent (the research checkout beside this repository) publishes a read-only,
//! transport-neutral `subagent/1` contract at `GET /v1/health` and
//! `GET /v1/subagent/manifest`.  A CSS host may also publish the durable snapshot
//! `SubagentProtocol.snapshot()` returns; booth reads that file so the console can
//! show CSS tickets beside the team's own workers.  There is no shared database and
//! no token: both reads are loopback and read-only.
//!
//! The feed path is a contract, not a guess: `BOOTH_CSS_FEED` names the file, and
//! when `BOOTH_CSS_DIR` names the checkout the default is
//! `<checkout>/.css-runs/booth/subagents.json`.  A missing file means "not
//! published", which is a normal state and not an error.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_URL: &str = "http://127.0.0.1:8765";
pub const URL_ENV: &str = "BOOTH_CSS_URL";
pub const DIR_ENV: &str = "BOOTH_CSS_DIR";
pub const FEED_ENV: &str = "BOOTH_CSS_FEED";

/// Where the CSS Agent service and its optional snapshot live.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub base: String,
    pub dir: Option<PathBuf>,
    pub feed: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base: DEFAULT_URL.to_owned(),
            dir: None,
            feed: None,
        }
    }
}

impl Config {
    /// Read the three overrides. A blank value is unset, as with the other booth
    /// environment settings.
    pub fn from_env() -> Self {
        let base = std::env::var(URL_ENV)
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_URL.to_owned());
        let dir = std::env::var_os(DIR_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let feed = feed_path(
            dir.as_deref(),
            std::env::var_os(FEED_ENV)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        );
        Self { base, dir, feed }
    }

    /// The command a person can run to start the service from this checkout.
    pub fn start_hint(&self) -> String {
        let mut command = String::from("python3 -m css_agent api");
        if let Some(port) = self
            .base
            .rsplit(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
        {
            command.push_str(&format!(" --port {port}"));
        }
        match &self.dir {
            Some(dir) => format!("cd {} && {command}", dir.display()),
            None => format!("{command}   (run in the CSS Agent checkout, then set {DIR_ENV})"),
        }
    }
}

/// An explicit feed wins; otherwise the checkout's conventional run directory.
pub fn feed_path(dir: Option<&std::path::Path>, explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(|| dir.map(|dir| dir.join(".css-runs").join("booth").join("subagents.json")))
}

/// What one probe of the service found.
#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub base: String,
    pub reachable: bool,
    pub version: Option<String>,
    pub protocol: Option<String>,
    pub compatible: Vec<String>,
    pub roles: Vec<String>,
    pub report_states: Vec<String>,
    pub max_active: Option<u64>,
    pub max_depth: Option<u64>,
    /// Why a read failed; the service can be reachable while its manifest is not.
    pub error: Option<String>,
}

impl Status {
    fn unreachable(base: &str, error: String) -> Self {
        Self {
            base: base.to_owned(),
            reachable: false,
            version: None,
            protocol: None,
            compatible: Vec::new(),
            roles: Vec::new(),
            report_states: Vec::new(),
            max_active: None,
            max_depth: None,
            error: Some(error),
        }
    }

    /// Merge the two views. Kept pure so the parser is testable without a server.
    pub fn from_views(base: &str, health: &Value, manifest: &Value) -> Self {
        let version = health
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let strings = |value: Option<&Value>| -> Vec<String> {
            value
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let activation = manifest.get("activation");
        let limit = |name: &str| {
            activation
                .and_then(|activation| activation.get("limits"))
                .and_then(|limits| limits.get(name))
                .and_then(Value::as_u64)
        };
        Self {
            base: base.to_owned(),
            reachable: true,
            version,
            protocol: manifest
                .get("protocolVersion")
                .and_then(Value::as_str)
                .map(str::to_owned),
            compatible: strings(manifest.get("compatibleProtocols")),
            roles: strings(manifest.get("roles")),
            report_states: strings(manifest.get("reportStates")),
            max_active: limit("maxActiveSubagents"),
            max_depth: limit("maxDepth"),
            error: None,
        }
    }
}

/// One CSS ticket from a published snapshot.
#[derive(Clone, Debug, Serialize)]
pub struct Ticket {
    pub id: String,
    pub title: String,
    /// `working`, `person`, `idle` or `ended`.
    pub stage: String,
    pub summary: String,
    pub question: Option<String>,
    pub updated_at: Option<String>,
}

/// The snapshot read, or why there is none.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Feed {
    pub configured: bool,
    pub path: String,
    pub protocol: Option<String>,
    pub tickets: Vec<Ticket>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    #[serde(default)]
    protocol_version: Option<String>,
    #[serde(default)]
    tickets: Vec<RawTicket>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTicket {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    ended: Option<Value>,
    #[serde(default)]
    attention: Option<Value>,
    #[serde(default)]
    assignments: Vec<RawAssignment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAssignment {
    #[serde(default)]
    state: String,
    #[serde(default)]
    report: Option<RawReport>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReport {
    #[serde(default)]
    state: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    question: Option<String>,
}

fn latest_report(ticket: &RawTicket) -> Option<&RawReport> {
    ticket
        .assignments
        .iter()
        .rev()
        .find_map(|assignment| assignment.report.as_ref())
}

fn stage_of(ticket: &RawTicket) -> &'static str {
    if ticket.ended.is_some() {
        return "ended";
    }
    if ticket.attention.is_some() {
        return "person";
    }
    if let Some(report) = latest_report(ticket) {
        return match report.state.as_str() {
            "progress" => "working",
            "waiting" | "blocked" | "failed" => "person",
            // The protocol ends a ticket on `ready` or `dropped`; a snapshot written
            // just before the `ended` record still should not look live.
            "ready" | "dropped" => "ended",
            _ => "idle",
        };
    }
    if ticket
        .assignments
        .iter()
        .any(|item| item.state == "running")
    {
        "working"
    } else {
        "idle"
    }
}

impl Feed {
    /// Parse one published snapshot. Tolerant by design: a field this console does
    /// not render stays out of the row rather than failing the whole feed.
    pub fn parse(path: &str, body: &str) -> Self {
        let snapshot: Snapshot = match serde_json::from_str(body) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Self {
                    configured: true,
                    path: path.to_owned(),
                    protocol: None,
                    tickets: Vec::new(),
                    error: Some(format!("the CSS snapshot is not valid JSON: {error}")),
                }
            }
        };
        let tickets = snapshot
            .tickets
            .iter()
            .map(|ticket| {
                let report = latest_report(ticket);
                let question = ticket
                    .attention
                    .as_ref()
                    .and_then(|attention| attention.get("question"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| report.and_then(|report| report.question.clone()));
                Ticket {
                    id: ticket.id.clone(),
                    title: ticket.title.clone(),
                    stage: stage_of(ticket).to_owned(),
                    summary: report
                        .map(|report| report.summary.clone())
                        .unwrap_or_default(),
                    question,
                    updated_at: ticket.updated_at.clone(),
                }
            })
            .collect();
        Self {
            configured: true,
            path: path.to_owned(),
            protocol: snapshot.protocol_version,
            tickets,
            error: None,
        }
    }

    /// Read the configured feed. "Not configured" is a normal state; an unreadable
    /// configured path is reported so `/css` can explain it.
    pub fn read(config: &Config) -> Self {
        let Some(path) = &config.feed else {
            return Self::default();
        };
        let shown = path.display().to_string();
        if !path.exists() {
            return Self {
                configured: true,
                path: shown,
                protocol: None,
                tickets: Vec::new(),
                error: Some("no snapshot published yet".into()),
            };
        }
        match std::fs::read_to_string(path) {
            Ok(body) => Self::parse(&shown, &body),
            Err(error) => Self {
                configured: true,
                path: shown,
                protocol: None,
                tickets: Vec::new(),
                error: Some(format!("cannot read the CSS snapshot: {error}")),
            },
        }
    }

    pub fn live(&self) -> impl Iterator<Item = &Ticket> {
        self.tickets.iter().filter(|ticket| ticket.stage != "ended")
    }
}

/// A short-deadline, read-only client for the CSS Agent API.
pub struct Client {
    base: String,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(base: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_millis(750))
            .timeout(Duration::from_secs(2))
            .build();
        Self {
            base: base.trim_end_matches('/').to_owned(),
            agent,
        }
    }

    fn get(&self, path: &str) -> Result<Value, String> {
        let response = self
            .agent
            .get(&format!("{}{path}", self.base))
            .set("accept", "application/json")
            .call();
        match response {
            Ok(response) => {
                let body = response
                    .into_string()
                    .map_err(|error| format!("cannot read the answer: {error}"))?;
                serde_json::from_str(&body)
                    .map_err(|error| format!("the answer is not JSON: {error}"))
            }
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(format!("HTTP {code}: {}", body.trim()))
            }
            Err(ureq::Error::Transport(error)) => Err(error.to_string()),
        }
    }

    /// Health first, then the manifest. A reachable service with an unreadable
    /// manifest keeps `reachable = true` and names the second failure.
    pub fn probe(&self) -> Status {
        let health = match self.get("/v1/health") {
            Ok(health) => health,
            Err(error) => return Status::unreachable(&self.base, error),
        };
        match self.get("/v1/subagent/manifest") {
            Ok(manifest) => Status::from_views(&self.base, &health, &manifest),
            Err(error) => {
                let mut status = Status::from_views(&self.base, &health, &Value::Null);
                status.error = Some(format!("the subagent manifest is unavailable: {error}"));
                status
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_manifest_names_the_protocol_and_its_limits() {
        let status = Status::from_views(
            "http://127.0.0.1:8765",
            &json!({ "status": "ok", "service": "css-agent", "version": "0.1.0" }),
            &json!({
                "protocolVersion": "subagent/1",
                "compatibleProtocols": ["subagent/1", "dsh-superagent/1"],
                "roles": ["orchestrator", "worker"],
                "reportStates": ["progress", "waiting", "ready", "blocked", "failed", "dropped"],
                "activation": { "limits": { "maxActiveSubagents": 8, "maxDepth": 1 } }
            }),
        );
        assert!(status.reachable);
        assert_eq!(status.version.as_deref(), Some("0.1.0"));
        assert_eq!(status.protocol.as_deref(), Some("subagent/1"));
        assert_eq!(status.compatible, ["subagent/1", "dsh-superagent/1"]);
        assert_eq!(status.roles, ["orchestrator", "worker"]);
        assert_eq!(status.report_states.len(), 6);
        assert_eq!(status.max_active, Some(8));
        assert_eq!(status.max_depth, Some(1));
    }

    #[test]
    fn a_snapshot_becomes_tickets_with_the_latest_report() {
        let feed = Feed::parse(
            "feed.json",
            &json!({
                "protocolVersion": "subagent/1",
                "tickets": [
                    {
                        "id": "css-1", "title": "check the install guide", "updatedAt": "2026-11-19T10:00:00Z",
                        "assignments": [
                            { "id": "a1", "state": "running",
                              "report": { "state": "progress", "summary": "reading the installer" } }
                        ]
                    },
                    {
                        "id": "css-2", "title": "review the release",
                        "attention": { "id": "n1", "question": "which tag?" },
                        "assignments": [{ "id": "a2", "state": "ended",
                                          "report": { "state": "waiting", "summary": "waiting for a tag" } }]
                    },
                    {
                        "id": "css-3", "title": "done", "ended": { "outcome": "completed", "by": "executor", "at": "2026-11-19T09:00:00Z" }
                    }
                ]
            })
            .to_string(),
        );
        assert_eq!(feed.protocol.as_deref(), Some("subagent/1"));
        assert_eq!(feed.tickets.len(), 3);
        assert_eq!(feed.tickets[0].stage, "working");
        assert_eq!(feed.tickets[0].summary, "reading the installer");
        assert_eq!(feed.tickets[1].stage, "person");
        assert_eq!(feed.tickets[1].question.as_deref(), Some("which tag?"));
        assert_eq!(feed.tickets[2].stage, "ended");
        assert_eq!(feed.live().count(), 2);
    }

    #[test]
    fn an_unconfigured_or_missing_feed_is_explained() {
        let unconfigured = Feed::read(&Config::default());
        assert!(!unconfigured.configured);
        assert!(unconfigured.error.is_none());

        let config = Config {
            base: DEFAULT_URL.into(),
            dir: None,
            feed: Some(PathBuf::from("/nonexistent/booth/css-feed.json")),
        };
        let missing = Feed::read(&config);
        assert!(missing.configured);
        assert!(missing.tickets.is_empty());
        assert!(
            missing
                .error
                .as_deref()
                .is_some_and(|error| error.contains("no snapshot published")),
            "{:?}",
            missing.error
        );
    }

    #[test]
    fn a_malformed_snapshot_is_reported_not_fatal() {
        let feed = Feed::parse("feed.json", "{not json");
        assert!(feed.tickets.is_empty());
        assert!(feed
            .error
            .as_deref()
            .is_some_and(|error| error.contains("not valid JSON")));
    }

    #[test]
    fn the_feed_defaults_under_the_css_checkout() {
        assert_eq!(
            feed_path(Some(std::path::Path::new("/src/css-agent")), None),
            Some(PathBuf::from(
                "/src/css-agent/.css-runs/booth/subagents.json"
            ))
        );
        assert_eq!(
            feed_path(None, Some(PathBuf::from("/tmp/feed.json"))),
            Some(PathBuf::from("/tmp/feed.json"))
        );
        assert_eq!(feed_path(None, None), None);
        let config = Config {
            base: "http://127.0.0.1:9000".into(),
            dir: Some(PathBuf::from("/src/css-agent")),
            feed: None,
        };
        assert!(config
            .start_hint()
            .contains("cd /src/css-agent && python3 -m css_agent api --port 9000"));
    }
}

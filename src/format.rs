//! Turns API views into terminal lines. Everything here is pure and testable.

use crate::model::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;

pub type L = Line<'static>;

fn span(text: impl Into<String>, style: Style) -> Span<'static> {
    Span::styled(text.into(), style)
}

pub fn muted(text: impl Into<String>) -> L {
    Line::from(span(text, Style::default().fg(Color::DarkGray)))
}

pub fn section(text: impl Into<String>) -> L {
    Line::from(span(
        text,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

pub fn blank() -> L {
    Line::default()
}

pub fn kv(key: &str, value: impl Into<String>) -> L {
    Line::from(vec![
        span(format!("{key:<14}"), Style::default().fg(Color::DarkGray)),
        span(value.into(), Style::default()),
    ])
}

pub fn bullet(text: impl Into<String>) -> L {
    Line::from(vec![
        span("  · ", Style::default().fg(Color::DarkGray)),
        span(text.into(), Style::default()),
    ])
}

pub fn clip(text: &str, max: usize) -> String {
    let text = text.replace(['\n', '\r'], " ");
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

/// `2026-11-04T10:20:30.000Z` -> `11-04 10:20`, local wall clock is not needed for a glance.
pub fn short_at(iso: &str) -> String {
    if iso.len() >= 16 && iso.as_bytes().get(10) == Some(&b'T') {
        format!("{} {}", &iso[5..10], &iso[11..16])
    } else {
        iso.chars().take(16).collect()
    }
}

pub fn actor(value: &Value) -> String {
    if let Some(name) = value.get("name") {
        if let Some(zh) = name.get("zh").and_then(Value::as_str) {
            return zh.to_string();
        }
        if let Some(en) = name.get("en").and_then(Value::as_str) {
            return en.to_string();
        }
    }
    value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("someone")
        .to_string()
}

pub fn strength(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str) {
        Some("hard") => "hard".into(),
        Some("soft") => "soft".into(),
        Some(other) => other.to_string(),
        None => "-".into(),
    }
}

pub fn reason(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Some(items) = value.as_array() {
        return items.iter().map(reason).collect::<Vec<_>>().join(" · ");
    }
    let code = value
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("reason");
    // `said` renders as the agent's own sentence, without a code in front of it.
    if code == "said" {
        if let Some(text) = value.get("values").and_then(|values| values.get("text")) {
            return text.as_str().unwrap_or_default().to_string();
        }
    }
    match value.get("values") {
        Some(Value::Object(map)) if !map.is_empty() => {
            let parts: Vec<String> = map
                .iter()
                .map(|(key, item)| {
                    let shown = item
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| item.to_string());
                    format!("{key}={shown}")
                })
                .collect();
            format!("{code} ({})", parts.join(", "))
        }
        _ => code.to_string(),
    }
}

fn fact(value: &Value) -> String {
    let name = value.get("name").and_then(Value::as_str).unwrap_or("fact");
    let shown = match value.get("value") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .or_else(|| item.as_str())
                    .unwrap_or("?")
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(", "),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    if shown.is_empty() {
        name.to_string()
    } else {
        format!("{name}: {shown}")
    }
}

pub fn stage_color(stage: &str) -> Color {
    match stage {
        "person" => Color::Yellow,
        "working" => Color::Cyan,
        "idle" => Color::Blue,
        "ended" => Color::DarkGray,
        "error" => Color::Red,
        _ => Color::Gray,
    }
}

fn health_color(state: &str) -> Color {
    match state {
        "ready" => Color::Green,
        "paused" => Color::Yellow,
        "error" => Color::Red,
        _ => Color::Gray,
    }
}

pub fn ticket_line(ticket: &TicketRow) -> L {
    let id: String = ticket.id.chars().take(10).collect();
    let at = ticket
        .updated_at
        .as_deref()
        .map(short_at)
        .unwrap_or_default();
    let mut spans = vec![
        span(
            format!("{:<8}", ticket.stage),
            Style::default().fg(stage_color(&ticket.stage)),
        ),
        span(format!("{id:<10} "), Style::default().fg(Color::DarkGray)),
        span(clip(&ticket.title, 64), Style::default()),
    ];
    if !at.is_empty() {
        spans.push(span(
            format!("  {at}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

fn ticket_lines(ticket: &TicketRow) -> Vec<L> {
    let mut lines = vec![ticket_line(ticket)];
    if let Some(note) = &ticket.note {
        lines.push(Line::from(vec![
            span("           ", Style::default()),
            span(clip(note, 100), Style::default().fg(Color::Yellow)),
        ]));
    }
    lines
}

fn flow_line(flow: &Flow) -> L {
    muted(format!(
        "24h: received {} · filtered {} · judged {} · accepted {} · evaluating {} · dropped {}",
        flow.received, flow.filtered, flow.judged, flow.accepted, flow.evaluating, flow.dropped
    ))
}

fn problem_line(problem: &Value) -> L {
    let kind = problem
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("problem");
    let detail = problem
        .get("detail")
        .and_then(Value::as_str)
        .or_else(|| {
            problem
                .get("source")
                .and_then(|source| source.get("health"))
                .and_then(|health| health.get("detail"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            problem
                .get("thread")
                .and_then(|thread| thread.get("title"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            problem
                .get("outgoing")
                .and_then(|outgoing| outgoing.get("error"))
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    let label = match kind {
        "setup" => "setup",
        "no-source" => "no Source is running",
        "no-allowed" => "no allowed sender: nobody can ask for work",
        "source-error" => "Source error",
        "executor-paused" => "executor paused",
        "eval-overdue" => "Eval overdue",
        "eval-unavailable" => "Eval unavailable",
        "runtime-error" => "runtime error",
        "outgoing" => "outgoing message failed",
        other => other,
    };
    if detail.is_empty() {
        bullet(label.to_string())
    } else {
        bullet(format!("{label}: {}", clip(detail, 110)))
    }
}

/// `/agents`: who does what, how many run at once, and where their memory lives.
///
/// dsh-superagent already models the arrangement — a main Session that routes, one Executor
/// Session per Ticket, a concurrency limit, and a store that outlives both — so this view
/// reports it instead of inventing a second one.
pub fn agents(view: &Overview, base: &str, sessions: usize, memory: &str) -> Vec<L> {
    let mut lines = vec![
        section("agents"),
        kv("service", base.to_string()),
        blank(),
        section("how work moves"),
        bullet("you → instruction → the console Source"),
        bullet("router · the main Session decides: accept, attach, stop, ignore"),
        bullet("worker · one Session per ticket, named worker-<ticket>"),
        bullet(format!(
            "parallel · up to {} workers at once, one extra slot for a person's message",
            view.executor.max_workers
        )),
        bullet("report · the worker answers on the ticket's thread, the console shows it"),
        blank(),
        section("now"),
        kv(
            "workers",
            format!(
                "{} running · {} queued · {} idle",
                view.executor.running, view.executor.queued, view.executor.idle
            ),
        ),
        kv("need a person", view.person.len().to_string()),
        kv("sessions", sessions.to_string()),
        blank(),
        section("memory"),
        kv("sessions", memory.to_string()),
        bullet("the router Session keeps the tickets, threads and decisions it judged"),
        bullet("a worker keeps its own conversation when a later message resumes its ticket"),
        bullet("both outlive the console: restart it and the history is still there"),
        blank(),
        section("when a person is told"),
        bullet("the agent works free of a person; a ticket reaches `person` only for an answer or a review"),
        bullet("/overview lists exactly those; everything else stays quiet"),
        blank(),
        section("what starts work"),
        bullet("console instructions, GitHub Issues and comments, Feishu messages"),
        bullet("/requeue <id> puts a dropped or unavailable instruction back"),
    ];
    if !view.problems.is_empty() {
        lines.push(blank());
        lines.push(section(format!("problems ({})", view.problems.len())));
        lines.extend(view.problems.iter().map(problem_line));
    }
    lines
}

pub fn overview(view: &Overview, base: &str) -> Vec<L> {
    let mut lines = vec![
        section("overview"),
        kv("service", base.to_string()),
        kv(
            "executor",
            format!(
                "{} running / {} max · {} queued · {} idle",
                view.executor.running,
                view.executor.max_workers,
                view.executor.queued,
                view.executor.idle
            ),
        ),
        kv(
            "policy",
            view.policy_version
                .as_deref()
                .map(|version| version.chars().take(10).collect::<String>())
                .unwrap_or_else(|| "-".into()),
        ),
        blank(),
        section(format!("problems ({})", view.problems.len())),
    ];
    if view.problems.is_empty() {
        lines.push(bullet("none"));
    } else {
        lines.extend(view.problems.iter().map(problem_line));
    }
    lines.push(blank());
    lines.push(section(format!(
        "waiting for a person ({})",
        view.person.len()
    )));
    if view.person.is_empty() {
        lines.push(bullet("none"));
    } else {
        for ticket in &view.person {
            lines.extend(ticket_lines(ticket));
        }
    }
    lines.push(blank());
    lines.push(flow_line(&view.flow));
    lines
}

pub fn status(view: &StateView, base: &str) -> Vec<L> {
    let mut lines = vec![
        section("status"),
        kv("service", base.to_string()),
        kv("paused", if view.paused { "yes" } else { "no" }.to_string()),
        kv("master", view.master_id.clone().unwrap_or_default()),
        kv(
            "setup",
            view.setup
                .problem
                .as_ref()
                .map(|problem| clip(&problem.detail, 100))
                .unwrap_or_else(|| "ok".into()),
        ),
        blank(),
        section("health"),
    ];
    if view.health.is_empty() {
        lines.push(bullet("no entries"));
    }
    for (name, health) in &view.health {
        lines.push(Line::from(vec![
            span(
                format!("  {name:<18}"),
                Style::default().fg(Color::DarkGray),
            ),
            span(
                format!("{:<8}", health.state),
                Style::default().fg(health_color(&health.state)),
            ),
            span(clip(&health.detail, 90), Style::default()),
        ]));
    }
    if let Some(accounts) = &view.setup.accounts {
        lines.push(blank());
        lines.push(section("accounts"));
        lines.push(kv(
            "repository",
            accounts.repository.clone().unwrap_or_default(),
        ));
        lines.push(kv("agent", accounts.agent.clone().unwrap_or_default()));
    }
    lines
}

pub fn ticket_list(view: &Page<TicketRow>, state: &str) -> Vec<L> {
    let mut lines = vec![section(format!("tickets ({state}) · {} total", view.total))];
    if view.items.is_empty() {
        lines.push(bullet("none"));
    }
    for ticket in &view.items {
        lines.extend(ticket_lines(ticket));
    }
    if let Some(next) = view.next_offset {
        lines.push(muted(format!("next offset {next}")));
    }
    lines
}

pub fn ticket_detail(view: &TicketDetail) -> Vec<L> {
    let mut lines = vec![
        section(clip(&view.title, 90)),
        Line::from(vec![
            span(
                format!("{:<8}", view.stage),
                Style::default().fg(stage_color(&view.stage)),
            ),
            span(
                format!("{} ", view.state),
                Style::default().fg(Color::DarkGray),
            ),
            span(view.id.clone(), Style::default().fg(Color::DarkGray)),
        ]),
    ];
    if let Some(note) = &view.note {
        lines.push(Line::from(span(
            clip(note, 110),
            Style::default().fg(Color::Yellow),
        )));
    }
    if let Some(summary) = &view.summary {
        if !summary.is_empty() {
            lines.push(blank());
            lines.push(muted(clip(summary, 300)));
        }
    }
    lines.push(blank());
    if let Some(mandate) = &view.mandate {
        lines.push(kv(
            "mandate",
            format!(
                "{} · {}",
                mandate
                    .actor
                    .as_ref()
                    .map(actor)
                    .unwrap_or_else(|| "-".into()),
                strength(mandate.strength.as_ref())
            ),
        ));
    }
    if let Some(basis) = &view.basis {
        if !basis.facts.is_empty() {
            lines.push(kv(
                "basis",
                basis.facts.iter().map(fact).collect::<Vec<_>>().join(" · "),
            ));
        }
    }
    let actions = view.actions.clone().unwrap_or_default();
    lines.push(kv(
        "actions",
        format!("stop {} · retry {}", actions.stop, actions.retry),
    ));
    if let Some(directory) = &view.directory {
        lines.push(kv("directory", directory.clone()));
    }
    if let Some(session) = &view.session_id {
        if !session.is_empty() {
            lines.push(kv("session", session.clone()));
        }
    }
    if !view.channels.is_empty() {
        lines.push(blank());
        lines.push(section("channels"));
        for channel in &view.channels {
            lines.push(bullet(format!(
                "{}{}",
                channel.url.clone().unwrap_or_else(|| "(no link)".into()),
                channel
                    .label
                    .as_ref()
                    .map(|label| format!("  {}", label.text()))
                    .unwrap_or_default()
            )));
        }
    }
    lines.push(blank());
    lines.push(section(format!("assignments ({})", view.assignments.len())));
    if view.assignments.is_empty() {
        lines.push(bullet("none"));
    }
    for assignment in &view.assignments {
        let at = assignment
            .started_at
            .as_deref()
            .map(short_at)
            .unwrap_or_default();
        lines.push(bullet(format!(
            "{} · {} {}",
            assignment.id.chars().take(12).collect::<String>(),
            assignment.state,
            at
        )));
        if let Some(report) = &assignment.report {
            if let Some(summary) = &report.summary {
                lines.push(muted(format!("      {}", clip(summary, 120))));
            }
            if let Some(question) = &report.question {
                let text = question
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| question.to_string());
                lines.push(Line::from(span(
                    format!("      ? {}", clip(&text, 120)),
                    Style::default().fg(Color::Yellow),
                )));
            }
        }
    }
    lines.push(blank());
    lines.push(section(format!("inputs ({})", view.inputs.len())));
    if view.inputs.is_empty() {
        lines.push(bullet("none"));
    }
    for input in &view.inputs {
        let delivered = if input.delivered {
            "delivered"
        } else {
            "waiting"
        };
        let body = input
            .text
            .clone()
            .or_else(|| {
                input
                    .trigger
                    .as_ref()
                    .map(|trigger| trigger.preview.clone())
            })
            .unwrap_or_default();
        lines.push(bullet(format!(
            "{} · {} · {}",
            input.kind,
            delivered,
            clip(&body, 100)
        )));
    }
    if !view.threads.is_empty() {
        lines.push(blank());
        lines.push(section("threads"));
        for thread in &view.threads {
            lines.push(bullet(format!(
                "{} · {}",
                clip(&thread.title, 70),
                thread.url.clone().unwrap_or_else(|| thread.key.clone())
            )));
        }
    }
    lines
}

pub fn thread_list(view: &Page<ThreadRow>, filter: &str) -> Vec<L> {
    let mut lines = vec![section(format!(
        "threads ({filter}) · {} total",
        view.total
    ))];
    if view.items.is_empty() {
        lines.push(bullet("none"));
    }
    for thread in &view.items {
        let at = thread.last_at.as_deref().map(short_at).unwrap_or_default();
        lines.push(Line::from(vec![
            span(
                format!("{:<10}", thread.stage),
                Style::default().fg(stage_color(match thread.stage.as_str() {
                    "evaluating" | "queued" => "working",
                    "subscribed" => "idle",
                    "unsubscribed" => "ended",
                    _ => "person",
                })),
            ),
            span(format!(" {}", clip(&thread.title, 64)), Style::default()),
            span(
                format!("  {} tickets · {at}", thread.subscriptions),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    lines
}

pub fn thread_detail(view: &ThreadDetail) -> Vec<L> {
    let mut lines = vec![
        section(clip(&view.title, 90)),
        Line::from(vec![
            span(
                format!("{:<10}", view.stage),
                Style::default().fg(Color::Cyan),
            ),
            span(
                format!(" {}", view.key),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    if let Some(url) = &view.url {
        lines.push(kv("link", url.clone()));
    }
    lines.push(kv(
        "tickets",
        format!(
            "{} subscribed · {} pending",
            view.subscriptions, view.pending
        ),
    ));
    if !view.context.is_empty() {
        lines.push(kv("context", view.context.join(", ")));
    }
    if !view.correlated.is_empty() {
        lines.push(kv(
            "correlated",
            view.correlated
                .iter()
                .map(|item| clip(&item.title, 40))
                .collect::<Vec<_>>()
                .join(" · "),
        ));
    }
    lines.push(blank());
    lines.push(section(format!("timeline ({})", view.timeline.len())));
    for item in &view.timeline {
        // The bundle projects an automated notice as an actor with an empty id and no name;
        // `actor` cannot name that, so fall back to the Source that sent it.
        let who = item
            .actor
            .as_ref()
            .map(actor)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| item.source.clone());
        let at = item.at.as_deref().map(short_at).unwrap_or_default();
        lines.push(Line::from(vec![
            span(format!("{at:<12}"), Style::default().fg(Color::DarkGray)),
            span(
                format!("{:<14}", clip(&who, 13)),
                Style::default().fg(if item.human {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
            span(clip(&item.preview, 90), Style::default()),
        ]));
    }
    if let Some(earlier) = &view.earlier {
        lines.push(muted(format!("earlier: {earlier}")));
    }
    if !view.decisions.is_empty() {
        lines.push(blank());
        lines.push(section(format!("decisions ({})", view.decisions.len())));
        for decision in &view.decisions {
            lines.push(decision_line(decision));
        }
    }
    lines
}

fn decision_line(decision: &Decision) -> L {
    let at = decision.at.as_deref().map(short_at).unwrap_or_default();
    Line::from(vec![
        span(
            format!("{:<11}", decision.outcome),
            Style::default().fg(match decision.outcome.as_str() {
                "accept" => Color::Green,
                "stop" | "ignore" => Color::DarkGray,
                "unavailable" => Color::Red,
                _ => Color::Yellow,
            }),
        ),
        span(format!("{at}  "), Style::default().fg(Color::DarkGray)),
        span(clip(&reason(&decision.reason), 70), Style::default()),
    ])
}

pub fn decisions(view: &DecisionList) -> Vec<L> {
    let mut lines = vec![section(format!("decisions · {} shown", view.items.len()))];
    if view.items.is_empty() {
        lines.push(bullet("none"));
    }
    for decision in &view.items {
        let mut line = vec![
            span(
                format!("{:<11}", decision.outcome),
                Style::default().fg(match decision.outcome.as_str() {
                    "accept" => Color::Green,
                    "stop" | "ignore" => Color::DarkGray,
                    "unavailable" => Color::Red,
                    _ => Color::Yellow,
                }),
            ),
            span(
                format!(
                    "{}  ",
                    decision.at.as_deref().map(short_at).unwrap_or_default()
                ),
                Style::default().fg(Color::DarkGray),
            ),
            span(clip(&reason(&decision.reason), 60), Style::default()),
        ];
        if let Some(title) = &decision.thread_title {
            line.push(span(
                format!("  · {}", clip(title, 40)),
                Style::default().fg(Color::DarkGray),
            ));
        }
        lines.push(Line::from(line));
    }
    lines
}

pub fn sources(view: &[SourceStatus]) -> Vec<L> {
    let mut lines = vec![section(format!("sources ({})", view.len()))];
    for source in view {
        lines.push(Line::from(vec![
            span(
                format!("{:<10}", source.state.as_deref().unwrap_or("?")),
                Style::default().fg(health_color(source.state.as_deref().unwrap_or(""))),
            ),
            span(format!("{:<10} ", source.id), Style::default()),
            span(
                format!(
                    "{} · {}{}",
                    source.title.text(),
                    if source.kind.is_empty() {
                        &source.id
                    } else {
                        &source.kind
                    },
                    if source.enabled { "" } else { " · disabled" }
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        if let Some(problem) = &source.problem {
            let detail = problem
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("setup problem");
            lines.push(bullet(format!("setup: {}", clip(detail, 100))));
        }
        for health in &source.health {
            if health.state == "error" || health.state == "paused" {
                lines.push(bullet(format!(
                    "{}: {}",
                    health.state,
                    clip(&health.detail, 100)
                )));
            }
        }
    }
    lines
}

pub fn search(view: &SearchView) -> Vec<L> {
    let mut lines = vec![section(format!(
        "search · {} tickets · {} threads",
        view.tickets.len(),
        view.threads.len()
    ))];
    for ticket in &view.tickets {
        lines.extend(ticket_lines(ticket));
    }
    if !view.threads.is_empty() {
        lines.push(blank());
        lines.push(section("threads"));
        for thread in &view.threads {
            lines.push(bullet(format!(
                "{}  {}",
                clip(&thread.title, 70),
                thread.key
            )));
        }
    }
    if view.tickets.is_empty() && view.threads.is_empty() {
        lines.push(bullet("nothing matched"));
    }
    lines
}

pub fn help() -> Vec<L> {
    let mut lines = vec![section("commands")];
    for &(name, description) in crate::commands::PALETTE {
        lines.push(Line::from(vec![
            span(format!("  {name:<20}"), Style::default().fg(Color::Cyan)),
            span(description, Style::default().fg(Color::DarkGray)),
        ]));
    }
    lines.push(blank());
    lines.push(muted(
        "Bare text sends an instruction to the agent; /send does the same. Ctrl+P palette · Ctrl+R history · /keys lists the shortcuts.",
    ));
    lines
}
#[cfg(test)]
mod tests {
    use super::{clip, reason, short_at, thread_detail};
    use crate::model::ThreadDetail;
    use serde_json::json;

    #[test]
    fn clips_by_characters_not_bytes() {
        assert_eq!(clip("ab", 4), "ab");
        assert_eq!(clip("abcdef", 3), "abc…");
        assert_eq!(clip("中文标题", 2), "中文…");
    }

    #[test]
    fn shortens_iso_timestamps() {
        assert_eq!(short_at("2026-11-04T10:20:30.000Z"), "11-04 10:20");
        assert_eq!(short_at("not-a-date"), "not-a-date");
    }

    #[test]
    fn an_automated_notice_names_its_source_in_the_timeline() {
        let view: ThreadDetail = serde_json::from_value(json!({
            "key": "github:o/r#12",
            "source": "github",
            "title": "Issue #12 flaky retry",
            "stage": "subscribed",
            "timeline": [{
                "id": "tr-1",
                "source": "github",
                "actor": { "source": "github", "id": "alice", "name": { "zh": "爱丽丝", "en": "Alice" } },
                "human": true,
                "at": "2026-11-04T09:00:00.000Z",
                "preview": "@agent please fix this"
            }, {
                // The bundle projects an automated notice as an actor with an empty id and no
                // name. It is not "some actor with no name": the source is the sender we know.
                "id": "tr-2",
                "source": "github",
                "actor": { "source": "github", "id": "" },
                "human": false,
                "at": "2026-11-04T09:05:00.000Z",
                "preview": "workflow retry failed"
            }],
            "decisions": []
        }))
        .expect("the bundle's thread view decodes");
        let shown: Vec<String> = thread_detail(&view)
            .iter()
            .map(ToString::to_string)
            .collect();
        let notice = shown
            .iter()
            .find(|line| line.contains("workflow retry failed"))
            .expect("the automated notice is shown");
        assert!(
            notice.contains("github"),
            "an automated notice must name its Source, not leave the sender blank: {notice:?}"
        );
        let human = shown
            .iter()
            .find(|line| line.contains("@agent please fix this"))
            .expect("the human message is shown");
        assert!(
            human.contains("爱丽丝"),
            "a named actor still wins over the Source: {human:?}"
        );
    }

    #[test]
    fn words_a_reason_code_and_its_values() {
        assert_eq!(reason(&json!({"code": "timeout"})), "timeout");
        assert_eq!(
            reason(&json!({"code": "observed", "values": {"text": "later"}})),
            "observed (text=later)"
        );
        // `said` is the agent's own sentence: no code in front of it.
        assert_eq!(
            reason(&json!({"code": "said", "values": {"text": "later"}})),
            "later"
        );
        assert_eq!(
            reason(&json!([{"code": "said", "values": {"text": "greeting"}}])),
            "greeting"
        );
    }
}

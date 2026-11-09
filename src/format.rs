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
    // The slices are only valid on a byte boundary; a timestamp the service sends is ASCII, but
    // this view is tolerant of whatever JSON arrived and must not panic on the rest.
    match (iso.as_bytes().get(10), iso.get(5..10), iso.get(11..16)) {
        (Some(b'T'), Some(date), Some(time)) => format!("{date} {time}"),
        _ => iso.chars().take(16).collect(),
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

/// What an open `person` ticket waits for, or a queued `working` ticket. The bundle splits the
/// stage into `stage` plus `action` (docs/003): printing only the stage made every `person`
/// ticket look alike and called a ticket the executor had not started `working`.
fn status_label<'a>(stage: &'a str, action: Option<&'a str>) -> &'a str {
    match (stage, action) {
        ("person", Some(action)) => action,
        ("working", Some("queued")) => "queued",
        _ => stage,
    }
}

fn status_color(stage: &str, action: Option<&str>) -> Color {
    match (stage, action) {
        ("person", Some("failed")) => stage_color("error"),
        ("working", Some("queued")) => stage_color("idle"),
        _ => stage_color(stage),
    }
}

pub fn ticket_line(ticket: &TicketRow) -> L {
    let id: String = ticket.id.chars().take(10).collect();
    let status = status_label(&ticket.stage, ticket.action.as_deref());
    let at = ticket
        .updated_at
        .as_deref()
        .map(short_at)
        .unwrap_or_default();
    let mut spans = vec![
        span(
            format!("{status:<9}"),
            Style::default().fg(status_color(&ticket.stage, ticket.action.as_deref())),
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
    let status = status_label(&view.stage, view.action.as_deref());
    let mut lines = vec![
        section(clip(&view.title, 90)),
        Line::from(vec![
            span(
                format!("{status:<9}"),
                Style::default().fg(status_color(&view.stage, view.action.as_deref())),
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
        // An input can be delivered and still have been handed back by this ticket: the bundle
        // keeps the worker's reason beside the trigger. Without it the text view looks as if the
        // message simply went to the worker, hiding what happened to it.
        if let Some(reason) = input
            .trigger
            .as_ref()
            .and_then(|trigger| trigger.handback.as_ref())
            .and_then(|handback| handback.get("reason"))
            .and_then(Value::as_str)
            .filter(|reason| !reason.is_empty())
        {
            lines.push(muted(format!("      handed back: {}", clip(reason, 100))));
        }
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
    // The counts above are not the list: `/thread` promises the subscribed tickets too, and
    // the bundle sends them, so a thread whose tickets were only counted left the operator
    // without the one thing needed to open the work.
    if !view.tickets.is_empty() {
        lines.push(blank());
        lines.push(section(format!("tickets ({})", view.tickets.len())));
        for ticket in &view.tickets {
            lines.extend(ticket_lines(ticket));
        }
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
            format!("{:<11} ", decision.outcome),
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
                format!("{:<11} ", decision.outcome),
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
        "search · {} tickets · {} threads · {} messages",
        view.tickets.len(),
        view.threads.len(),
        view.items.len()
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
    // The bundle searches Triggers too, not just the tickets and threads they reached. A hit
    // only in a message is still a hit, so dropping `items` made `/search` report silence.
    if !view.items.is_empty() {
        lines.push(blank());
        lines.push(section("messages"));
        lines.extend(view.items.iter().map(message_line));
    }
    if view.tickets.is_empty() && view.threads.is_empty() && view.items.is_empty() {
        lines.push(bullet("nothing matched"));
    }
    lines
}

/// One matched message: when, who, its preview, and the thread or filter that explains it.
fn message_line(item: &SearchItem) -> L {
    let at = item.at.as_deref().map(short_at).unwrap_or_default();
    let who = item
        .actor
        .as_ref()
        .map(actor)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| item.source.clone());
    let mut spans = vec![
        span(format!("{at:<12}"), Style::default().fg(Color::DarkGray)),
        span(
            format!("{:<14}", clip(&who, 13)),
            Style::default().fg(if item.human {
                Color::Green
            } else {
                Color::DarkGray
            }),
        ),
        span(clip(&item.preview, 80), Style::default()),
    ];
    if let Some(title) = item
        .thread_title
        .as_deref()
        .filter(|title| !title.is_empty())
    {
        spans.push(span(
            format!("  · {}", clip(title, 40)),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(reason) = item.filtered.as_deref().filter(|reason| !reason.is_empty()) {
        spans.push(span(
            format!("  · filtered: {}", clip(reason, 40)),
            Style::default().fg(Color::Yellow),
        ));
    }
    Line::from(spans)
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
    use super::{clip, reason, short_at, thread_detail, ticket_detail, ticket_list, SearchView};
    use crate::model::{DecisionList, Page, ThreadDetail, TicketDetail, TicketRow};
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
    fn a_multibyte_timestamp_does_not_panic() {
        // The `T` at byte 10 is not enough to make 5..10 and 11..16 char boundaries: the emoji
        // starts at 13, so the old slice panicked. Any non-ASCII value now falls back.
        assert_eq!(short_at("1234567890T01😀"), "1234567890T01😀");
        // The fallback counts characters, so a long non-ASCII value truncates at 16 of them.
        assert_eq!(short_at(&"中".repeat(20)), "中".repeat(16));
    }

    #[test]
    fn a_ticket_status_names_what_it_waits_for() {
        // The bundle sends a `person` stage with the action that asks for a person, and
        // `working` with `queued` before a slot starts it (docs/003). Those actions were
        // decoded but never printed: a queued ticket read as working, and a review showed
        // as a plain `person` row with no note, like an unanswered question.
        let rows: Page<TicketRow> = serde_json::from_value(json!({
            "total": 6,
            "items": [
                {"id": "t-review", "title": "Review it", "state": "open", "stage": "person", "action": "review"},
                {"id": "t-queued", "title": "Start it", "state": "open", "stage": "working", "action": "queued"},
                {"id": "t-answer", "title": "Answer it", "state": "open", "stage": "person", "action": "answer"},
                {"id": "t-delivery", "title": "Deliver it", "state": "open", "stage": "person", "action": "delivery"},
                {"id": "t-working", "title": "Running", "state": "open", "stage": "working"},
                {"id": "t-person", "title": "Old shape", "state": "open", "stage": "person"}
            ]
        }))
        .expect("the bundle's ticket list decodes");
        let shown: Vec<String> = ticket_list(&rows, "open")
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(shown[1].starts_with("review   t-review"), "{shown:?}");
        assert!(shown[2].starts_with("queued   t-queued"), "{shown:?}");
        assert!(shown[3].starts_with("answer   t-answer"), "{shown:?}");
        // `delivery` is the longest action: the column still leaves a space before the id.
        assert!(shown[4].starts_with("delivery t-delivery"), "{shown:?}");
        // Without an action the stage is still the only thing to show.
        assert!(shown[5].starts_with("working  t-working"), "{shown:?}");
        assert!(shown[6].starts_with("person   t-person"), "{shown:?}");

        let detail: TicketDetail = serde_json::from_value(json!({
            "id": "t-review", "title": "Review it", "state": "open", "stage": "person", "action": "review"
        }))
        .expect("the bundle's ticket detail decodes");
        let header = ticket_detail(&detail)[1].to_string();
        assert!(header.starts_with("review   open"), "{header:?}");
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

    #[test]
    fn search_shows_a_message_that_no_ticket_or_thread_matched() {
        let view: SearchView = serde_json::from_value(json!({
            "tickets": [],
            "threads": [],
            "items": [{
                "id": "local-abc123",
                "source": "local",
                "actor": { "id": "operator", "name": { "zh": "操作员", "en": "operator" } },
                "human": true,
                "at": "2026-11-07T03:42:18.025Z",
                "preview": "请重试一次部署",
                "threads": ["local:t-local-abc123"],
                "threadTitle": "重试部署"
            }, {
                "id": "local-def456",
                "source": "github",
                "human": false,
                "preview": "a message with no actor or thread name",
                "filtered": "sender is not on the allow-list"
            }]
        }))
        .expect("the bundle's search view decodes");
        let text: Vec<String> = super::search(&view)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            text.iter().any(|line| line.contains("请重试一次部署")),
            "{text:?}"
        );
        assert!(text.iter().any(|line| line.contains("操作员")), "{text:?}");
        assert!(
            text.iter().any(|line| line.contains("11-07 03:42")),
            "the time is shown: {text:?}"
        );
        assert!(
            text.iter().any(|line| line.contains("重试部署")),
            "{text:?}"
        );
        // Without an actor the source stands in, and a message with no thread name gets no
        // dangling separator.
        let bare = text
            .iter()
            .find(|line| line.contains("no actor or thread name"))
            .expect("the second message is shown");
        assert!(bare.contains("github"), "{bare:?}");
        assert!(
            bare.contains("no actor or thread name  · filtered: sender is not on the allow-list"),
            "the preview runs straight into the filter reason, with no empty title: {bare:?}"
        );
        assert!(
            !text.iter().any(|line| line.contains("nothing matched")),
            "a message hit is a hit: {text:?}"
        );
    }

    #[test]
    fn search_says_nothing_matched_only_when_nothing_did() {
        let text: Vec<String> = super::search(&SearchView::default())
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(text.last().map(String::as_str), Some("  · nothing matched"));
    }

    #[test]
    fn a_decision_outcome_and_its_time_stay_separate_fields() {
        // "unavailable" and "wouldAccept" are the longest real outcomes, eleven characters.
        // The fixed eleven-wide outcome column therefore never padded them, and the timestamp
        // started in the same cell: `unavailable11-04 10:20` instead of two fields.
        let view: DecisionList = serde_json::from_value(json!({
            "items": [
                {
                    "id": "d-1",
                    "kind": "ignore",
                    "outcome": "unavailable",
                    "reason": { "code": "timeout" },
                    "at": "2026-11-04T10:20:30.000Z"
                },
                {
                    "id": "d-2",
                    "kind": "ignore",
                    "outcome": "wouldAccept",
                    "reason": [{ "code": "addressed" }],
                    "at": "2026-11-04T09:00:00.000Z"
                }
            ]
        }))
        .expect("the bundle's decision list decodes");
        let shown: Vec<String> = super::decisions(&view)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            shown[1].starts_with("unavailable 11-04 10:20"),
            "the outcome and its time must be separate fields: {shown:?}"
        );
        assert!(
            shown[2].starts_with("wouldAccept 11-04 09:00"),
            "the outcome and its time must be separate fields: {shown:?}"
        );

        // `/thread` renders its decisions with the same column, so it needs the same fix.
        let detail: ThreadDetail = serde_json::from_value(json!({
            "key": "github:o/r#12",
            "source": "github",
            "title": "Issue #12 flaky retry",
            "stage": "subscribed",
            "timeline": [],
            "decisions": [{
                "id": "d-1",
                "kind": "ignore",
                "outcome": "unavailable",
                "reason": { "code": "timeout" },
                "at": "2026-11-04T10:20:30.000Z"
            }]
        }))
        .expect("the bundle's thread view decodes");
        let shown: Vec<String> = thread_detail(&detail)
            .iter()
            .map(ToString::to_string)
            .collect();
        let decision = shown
            .iter()
            .find(|line| line.contains("timeout"))
            .expect("the thread's decision is shown");
        assert!(
            decision.starts_with("unavailable 11-04 10:20"),
            "the thread view uses the same separated column: {decision:?}"
        );
    }
}

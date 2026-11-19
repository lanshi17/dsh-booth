//! Who is doing what: the team's worker Sessions plus CSS Agent's tickets.
//!
//! The management API already answers this for the team: `/tickets?state=working`
//! carries each ticket's stage, latest worker report summary, worker Session id and
//! last update. CSS Agent's published snapshot carries the same facts for its own
//! `subagent/1` tickets. This module only merges the two into one board; it does
//! not decide anything or add a second task model.

use crate::api::Api;
use crate::cssagent;
use crate::model::TicketRow;
use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A dsh-superagent worker Session (`worker-<ticket>`).
    Team,
    /// A CSS Agent `subagent/1` ticket.
    Css,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::Css => "css",
        }
    }
}

/// One row on the board. `stage` uses the service's spelling (`working`, `person`,
/// `idle`, `ended`) and CSS Agent's is normalized to the same four.
#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub source: Source,
    pub id: String,
    pub title: String,
    pub stage: String,
    pub action: Option<String>,
    pub summary: String,
    pub question: Option<String>,
    pub session: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Snapshot {
    pub rows: Vec<Row>,
    pub total: u32,
    pub error: Option<String>,
    pub css: Option<cssagent::Status>,
    pub feed: Option<cssagent::Feed>,
}

impl Snapshot {
    pub fn running(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.stage == "working")
            .count()
            + self
                .feed
                .as_ref()
                .map(|feed| {
                    feed.live()
                        .filter(|ticket| ticket.stage == "working")
                        .count()
                })
                .unwrap_or(0)
    }

    pub fn team_running(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.stage == "working")
            .count()
    }

    pub fn css_running(&self) -> usize {
        self.feed
            .as_ref()
            .map(|feed| {
                feed.live()
                    .filter(|ticket| ticket.stage == "working")
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn css_waiting(&self) -> usize {
        self.feed
            .as_ref()
            .map(|feed| {
                feed.live()
                    .filter(|ticket| ticket.stage == "person")
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn waiting(&self) -> usize {
        self.rows.iter().filter(|row| row.stage == "person").count()
            + self
                .feed
                .as_ref()
                .map(|feed| {
                    feed.live()
                        .filter(|ticket| ticket.stage == "person")
                        .count()
                })
                .unwrap_or(0)
    }

    /// The row the working line should name: whoever is actually producing output.
    pub fn focus(&self) -> Option<&Row> {
        self.rows
            .iter()
            .filter(|row| row.stage == "working")
            .max_by_key(|row| u8::from(!row.summary.is_empty()))
            .or_else(|| self.rows.iter().find(|row| row.stage == "person"))
    }

    pub fn css_live(&self) -> usize {
        self.feed
            .as_ref()
            .map(|feed| feed.live().count())
            .unwrap_or(0)
    }
}

/// The team rows a ticket page stands for. A row with no stage is skipped: the
/// console cannot say what such a worker is doing, and inventing a stage would
/// make the board lie.
pub fn rows_from_tickets(tickets: &[TicketRow]) -> Vec<Row> {
    tickets
        .iter()
        .filter(|ticket| !ticket.stage.is_empty())
        .map(|ticket| Row {
            source: Source::Team,
            id: ticket.id.clone(),
            title: ticket.title.clone(),
            stage: ticket.stage.clone(),
            action: ticket.action.clone(),
            summary: ticket.summary.clone().unwrap_or_default(),
            question: if ticket.stage == "person" {
                ticket.note.clone()
            } else {
                None
            },
            session: ticket.session_id.clone().filter(|id| !id.is_empty()),
            updated_at: ticket.updated_at.clone(),
        })
        .collect()
}

/// CSS tickets are ordered newest first by update time when the snapshot has it.
pub fn rows_from_feed(feed: &cssagent::Feed) -> Vec<Row> {
    let mut tickets: Vec<&cssagent::Ticket> = feed.live().collect();
    tickets.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    tickets
        .into_iter()
        .map(|ticket| Row {
            source: Source::Css,
            id: ticket.id.clone(),
            title: ticket.title.clone(),
            stage: ticket.stage.clone(),
            action: None,
            summary: ticket.summary.clone(),
            question: ticket.question.clone(),
            session: None,
            updated_at: ticket.updated_at.clone(),
        })
        .collect()
}

/// Read the whole board: every working and waiting team ticket, then CSS Agent's
/// service and published snapshot. A CSS outage is reported in `css`, not in the
/// team's rows, so one optional integration cannot hide the team's own work.
pub fn collect(api: &Api, config: &cssagent::Config) -> Snapshot {
    let mut rows = Vec::new();
    let mut total = 0;
    let mut errors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for state in ["working", "person"] {
        match api.all_tickets(state) {
            Ok(page) => {
                total += page.total;
                // One ticket has one stage, but a tolerant fixture that ignores the
                // filter would otherwise put it on the board twice.
                rows.extend(
                    rows_from_tickets(&page.items)
                        .into_iter()
                        .filter(|row| seen.insert(row.id.clone())),
                );
            }
            Err(error) => errors.push(format!("the {state} tickets are unavailable: {error}")),
        }
    }
    Snapshot {
        rows,
        total,
        error: (!errors.is_empty()).then(|| errors.join(" · ")),
        css: Some(cssagent::Client::new(&config.base).probe()),
        feed: Some(cssagent::Feed::read(config)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, stage: &str, summary: &str) -> TicketRow {
        TicketRow {
            id: id.into(),
            title: format!("ticket {id}"),
            state: "open".into(),
            stage: stage.into(),
            summary: (!summary.is_empty()).then(|| summary.to_owned()),
            session_id: Some(format!("worker-{id}")),
            ..TicketRow::default()
        }
    }

    #[test]
    fn working_tickets_become_rows_with_their_latest_report() {
        let rows = rows_from_tickets(&[
            row("t-1", "working", "reading the installer"),
            row("t-2", "person", "which tag?"),
            TicketRow {
                id: "t-3".into(),
                stage: String::new(),
                ..TicketRow::default()
            },
        ]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].source, Source::Team);
        assert_eq!(rows[0].summary, "reading the installer");
        assert_eq!(rows[0].session.as_deref(), Some("worker-t-1"));
        assert_eq!(rows[1].question, None, "a summary is not a question");
    }

    #[test]
    fn the_focus_prefers_progress_over_a_silent_worker() {
        let snapshot = Snapshot {
            rows: rows_from_tickets(&[
                row("silent", "working", ""),
                row("busy", "working", "writing tests"),
            ]),
            ..Snapshot::default()
        };
        assert_eq!(snapshot.focus().map(|row| row.id.as_str()), Some("busy"));
        assert_eq!(snapshot.running(), 2);
    }

    #[test]
    fn css_tickets_are_merged_but_keep_their_own_source() {
        let snapshot = Snapshot {
            rows: rows_from_tickets(&[row("t-1", "working", "team work")]),
            feed: Some(cssagent::Feed::parse(
                "feed.json",
                &serde_json::json!({
                    "tickets": [
                        { "id": "css-1", "title": "css work", "assignments": [
                            { "id": "a1", "state": "running", "report": { "state": "progress", "summary": "compiling" } }
                        ] },
                        { "id": "css-2", "title": "done", "ended": { "outcome": "completed", "by": "executor", "at": "2026-11-19T09:00:00Z" } }
                    ]
                })
                .to_string(),
            )),
            ..Snapshot::default()
        };
        let css_rows = rows_from_feed(snapshot.feed.as_ref().unwrap());
        assert_eq!(css_rows.len(), 1);
        assert_eq!(css_rows[0].source, Source::Css);
        assert_eq!(snapshot.running(), 2);
        assert_eq!(snapshot.css_live(), 1);
    }
}

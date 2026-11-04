//! One command grammar for both the TUI palette and `booth <command>`.

use crate::api::Api;
use crate::format::{self, L};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// The palette's entries; `format::help` and the completer read the same table.
pub const PALETTE: &[(&str, &str)] = &[
    ("/overview", "what needs a person, problems, executor load"),
    ("/status", "health entries, pause state, accounts"),
    ("/tickets [state]", "open | person | working | idle | ended"),
    ("/ticket <id>", "mandate, assignments, inputs, threads"),
    (
        "/threads [filter]",
        "recent | subscribed | evaluating | all",
    ),
    ("/thread <key>", "timeline, decisions, subscribed tickets"),
    ("/decisions", "the last 50 decisions and why"),
    ("/sources", "each Source's state and problems"),
    ("/search <text>", "also runs for bare text"),
    ("/stop <id>", "withdraw a ticket's mandate"),
    ("/retry <id>", "start a fresh assignment"),
    ("/pause", "stop the executor from taking work"),
    ("/resume", "let the executor take work"),
    ("/refresh", "reload the header"),
    ("/clear", "clear the transcript"),
    ("/quit", "leave the console"),
];

fn need<'a>(command: &str, argument: &'a str) -> Result<&'a str, String> {
    if argument.is_empty() {
        Err(format!("{command} needs an argument"))
    } else {
        Ok(argument)
    }
}

fn done(text: impl Into<String>) -> L {
    Line::from(Span::styled(
        format!("✓ {}", text.into()),
        Style::default().fg(Color::Green),
    ))
}

/// Runs one command and returns the lines to append to the transcript.
pub fn execute(api: &Api, raw: &str) -> Result<Vec<L>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(format::help());
    }
    // Bare text is a search: the console has no "create work" route, so typing a word should
    // find the record that already exists instead of failing.
    if !raw.starts_with('/') {
        return Ok(format::search(&api.search(raw)?));
    }
    let mut split = raw.splitn(2, char::is_whitespace);
    let command = split.next().unwrap_or_default();
    let argument = split.next().unwrap_or_default().trim();
    let lines = match command {
        "/help" | "/?" => format::help(),
        "/overview" => format::overview(&api.overview()?, api.base()),
        "/status" => format::status(&api.state()?, api.base()),
        "/tickets" => {
            let state = if argument.is_empty() {
                "open"
            } else {
                argument
            };
            format::ticket_list(&api.tickets(state)?, state)
        }
        "/ticket" => format::ticket_detail(&api.ticket(need("/ticket", argument)?)?),
        "/threads" => {
            let filter = if argument.is_empty() {
                "recent"
            } else {
                argument
            };
            format::thread_list(&api.threads(filter)?, filter)
        }
        "/thread" => format::thread_detail(&api.thread(need("/thread", argument)?)?),
        "/decisions" => format::decisions(&api.decisions()?),
        "/sources" => format::sources(&api.sources()?),
        "/search" => format::search(&api.search(need("/search", argument)?)?),
        "/stop" => {
            let id = need("/stop", argument)?;
            api.stop(id)?;
            vec![done(format!("stopped {id}"))]
        }
        "/retry" => {
            let id = need("/retry", argument)?;
            api.retry(id)?;
            vec![done(format!("retried {id}"))]
        }
        "/requeue" => {
            let id = need("/requeue", argument)?;
            api.requeue(id)?;
            vec![done(format!("requeued {id}"))]
        }
        "/dismiss" => {
            let id = need("/dismiss", argument)?;
            api.dismiss(id)?;
            vec![done(format!("dismissed {id}"))]
        }
        "/unsubscribe" => {
            let key = need("/unsubscribe", argument)?;
            api.unsubscribe(key)?;
            vec![done(format!("unsubscribed {key}"))]
        }
        "/pause" => {
            api.pause(true)?;
            vec![done("executor paused")]
        }
        "/resume" => {
            api.pause(false)?;
            vec![done("executor resumed")]
        }
        other => {
            return Err(format!("unknown command {other}; /help lists them"));
        }
    };
    Ok(lines)
}

/// Palette entries whose first word starts with what the user typed.
pub fn matches(input: &str) -> Vec<(&'static str, &'static str)> {
    let token = input
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    PALETTE
        .iter()
        .filter(|(name, _)| {
            let first = name.split_whitespace().next().unwrap_or("");
            token == "/" || first.starts_with(&token)
        })
        .copied()
        .collect()
}

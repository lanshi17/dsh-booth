//! One command grammar for both the TUI palette and `booth <command>`.

use crate::api::Api;
use crate::format::{self, L};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// The palette's entries; `format::help` and the completer read the same table.
pub const PALETTE: &[(&str, &str)] = &[
    ("/overview", "what needs a person, problems, executor load"),
    ("/status", "health entries, pause state, accounts"),
    ("/agents", "how work is routed, run in parallel, remembered"),
    ("/model", "switch the worker model, or add a provider"),
    ("/provider", "which provider the agent runs with"),
    (
        "/setup [owner/repo]",
        "name the repository and who may ask for work",
    ),
    (
        "/login [provider]",
        "sign in: DeepSeek account, or an API-key provider",
    ),
    ("/tickets [state]", "open | person | working | idle | ended"),
    ("/ticket <id>", "mandate, assignments, inputs, threads"),
    (
        "/threads [filter]",
        "recent | subscribed | evaluating | all",
    ),
    ("/thread <key>", "timeline, decisions, subscribed tickets"),
    ("/decisions", "the last 50 decisions and why"),
    ("/sources", "each Source's state and problems"),
    (
        "/send <text>",
        "send an instruction; bare text does the same",
    ),
    ("/search <text>", "find tickets, threads and messages"),
    ("/stop <id>", "withdraw a ticket's mandate"),
    ("/retry <id>", "start a fresh assignment"),
    (
        "/eval [mode]",
        "off | observe | auto — what may start without a person",
    ),
    ("/pause", "stop the executor from taking work"),
    ("/resume", "let the executor take work"),
    ("/keys", "the keyboard shortcuts, on one screen"),
    ("/history", "search what you typed before (Ctrl+R)"),
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

/// The ticket stages `/tickets` can list. The service coerces an unknown `state` to `open`
/// (`z.enum(TICKET_FILTERS).catch('open')`), so forwarding a typo would print a heading for a
/// filter the rows below it do not use; the console rejects it instead.
const TICKET_STATES: [&str; 5] = ["open", "person", "working", "idle", "ended"];

/// The thread lists `/threads` can ask for. The service coerces an unknown `filter` to
/// `subscribed`, which would contradict the printed heading in the same way.
const THREAD_FILTERS: [&str; 4] = ["recent", "subscribed", "evaluating", "all"];

/// One of `allowed`, with `default` for the bare command; the comparison ignores case so a
/// capitalised filter reaches the service in its canonical spelling.
fn choice(
    argument: &str,
    default: &'static str,
    allowed: &[&'static str],
    what: &str,
) -> Result<&'static str, String> {
    if argument.is_empty() {
        return Ok(default);
    }
    let wanted = argument.to_ascii_lowercase();
    allowed
        .iter()
        .copied()
        .find(|value| *value == wanted)
        .ok_or_else(|| format!("unknown {what} {argument:?}; use {}", allowed.join(" | ")))
}

/// Runs one command and returns the lines to append to the transcript.
pub fn execute(api: &Api, raw: &str) -> Result<Vec<L>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(format::help());
    }
    // Bare text is an instruction; the console hands it to the agent before reaching this grammar.
    if !raw.starts_with('/') {
        return Err("bare text sends an instruction; use /search to look something up".into());
    }
    let mut split = raw.splitn(2, char::is_whitespace);
    let command = split.next().unwrap_or_default();
    let argument = split.next().unwrap_or_default().trim();
    let lines = match command {
        "/help" | "/?" => format::help(),
        "/overview" => format::overview(&api.overview()?, api.base()),
        "/status" => format::status(&api.state()?, api.base()),
        "/agents" => {
            let view = api.overview()?;
            let cfg = crate::service::Config::from_env().ok();
            let (sessions, memory) = match &cfg {
                Some(cfg) => {
                    let work = crate::service::execution_root(api, cfg);
                    (
                        crate::service::session_count(cfg, &work),
                        crate::service::session_root(cfg).display().to_string(),
                    )
                }
                None => (0, "unknown".into()),
            };
            format::agents(&view, api.base(), sessions, &memory)
        }
        "/tickets" => {
            let state = choice(argument, "open", &TICKET_STATES, "ticket state")?;
            format::ticket_list(&api.tickets(state)?, state)
        }
        "/ticket" => format::ticket_detail(&api.ticket(need("/ticket", argument)?)?),
        "/threads" => {
            let filter = choice(argument, "recent", &THREAD_FILTERS, "thread filter")?;
            format::thread_list(&api.threads(filter)?, filter)
        }
        "/thread" => format::thread_detail(&api.thread(need("/thread", argument)?)?),
        "/decisions" => format::decisions(&api.decisions()?),
        "/sources" => format::sources(&api.sources()?),
        "/search" => format::search(&api.search(need("/search", argument)?)?),
        "/provider" => {
            // DSH's own catalog, not a booth-side list.
            let catalog = crate::provider::Catalog::parse(&api.models()?);
            crate::intake::summary(&catalog)
        }
        "/send" => {
            let cfg = crate::service::Config::from_env()?;
            let id = crate::intake::send(&cfg, need("/send", argument)?, None)?;
            vec![done(format!(
                "sent {id} — the agent picks it up within a second"
            ))]
        }
        "/setup" => {
            let cfg = crate::service::Config::from_env()?;
            crate::intake::setup(api, &cfg, argument)?
        }
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
        "/eval" => crate::intake::eval(api, argument)?,
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
    let token = query(input);
    if token.is_empty() {
        return PALETTE.to_vec();
    }
    let mut scored: Vec<(i32, usize, &'static str, &'static str)> = PALETTE
        .iter()
        .enumerate()
        .filter_map(|(index, (name, description))| {
            let word = name.split_whitespace().next().unwrap_or("");
            fuzzy(word, &token).map(|(score, _)| (score, index, *name, *description))
        })
        .collect();
    // A higher score first; equal scores keep the palette's order.
    scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    scored
        .into_iter()
        .map(|(_, _, name, description)| (name, description))
        .collect()
}

/// The part of a line the palette filters on: the command word, without its slash.
pub fn query(input: &str) -> String {
    input
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

/// Subsequence match of `query` in `candidate`, scored for consecutive and word-start hits.
/// Returns the score and the matched char indices, or `None` when a char is missing.
pub fn fuzzy(candidate: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let chars: Vec<char> = candidate.chars().collect();
    let wanted: Vec<char> = query.chars().collect();
    let mut indices = Vec::with_capacity(wanted.len());
    let mut score = 0i32;
    let mut last: Option<usize> = None;
    let mut from = 0usize;
    for (position, want) in wanted.iter().enumerate() {
        let lower = want.to_ascii_lowercase();
        let found = chars
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, ch)| ch.to_ascii_lowercase() == lower)
            .map(|(index, _)| index)?;
        score += match last {
            Some(previous) if found == previous + 1 => 6,
            Some(previous) if found == previous + 2 => 2,
            _ => 0,
        };
        // The first letter of a word and the first letter of the command score best.
        if found == 0 || !chars[found - 1].is_alphanumeric() {
            score += 8;
        }
        if position == 0 {
            score += if found == 0 { 10 } else { 4 };
        }
        score -= found as i32 / 4;
        indices.push(found);
        last = Some(found);
        from = found + 1;
    }
    Some((score, indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_subsequences_with_their_indices() {
        let (score, indices) = fuzzy("/ticket", "tkt").unwrap();
        assert_eq!(indices, vec![1, 4, 6]);
        assert!(score > 0);
        assert!(fuzzy("/ticket", "zzz").is_none());
    }

    #[test]
    fn a_consecutive_match_beats_a_scattered_one() {
        let (consecutive, _) = fuzzy("/status", "st").unwrap();
        let (scattered, _) = fuzzy("/status", "ts").unwrap();
        assert!(consecutive > scattered, "{consecutive} vs {scattered}");
    }

    #[test]
    fn the_palette_ranks_and_filters() {
        let names: Vec<&str> = matches("/tick").iter().map(|(name, _)| *name).collect();
        assert!(
            names.iter().any(|name| name.starts_with("/ticket")),
            "{names:?}"
        );
        assert!(
            !names.iter().any(|name| name.starts_with("/pause")),
            "{names:?}"
        );
        assert_eq!(matches("/").len(), PALETTE.len());
        assert_eq!(matches("   ").len(), PALETTE.len());
    }

    #[test]
    fn query_strips_the_slash_and_arguments() {
        assert_eq!(query("/ticket abc"), "ticket");
        assert_eq!(query("TiCk"), "tick");
        assert_eq!(query("/"), "");
    }

    #[test]
    fn a_list_filter_is_canonical_or_rejected() {
        assert_eq!(
            choice("", "open", &TICKET_STATES, "ticket state").unwrap(),
            "open"
        );
        assert_eq!(
            choice("PERSON", "open", &TICKET_STATES, "ticket state").unwrap(),
            "person"
        );
        let error = choice("persno", "open", &TICKET_STATES, "ticket state").unwrap_err();
        assert!(
            error.contains("persno") && error.contains("person"),
            "{error}"
        );
        assert_eq!(
            choice("Recent", "recent", &THREAD_FILTERS, "thread filter").unwrap(),
            "recent"
        );
        assert!(choice("lately", "recent", &THREAD_FILTERS, "thread filter").is_err());
    }

    #[test]
    fn a_misspelled_list_filter_fails_before_the_request() {
        // Port 9 is not listening: reaching it would produce a transport error, not this message.
        let api = Api::new("http://127.0.0.1:9");
        let error = execute(&api, "/tickets persno").unwrap_err();
        assert!(error.contains("persno"), "{error}");
        let error = execute(&api, "/threads lately").unwrap_err();
        assert!(error.contains("lately"), "{error}");
    }
}

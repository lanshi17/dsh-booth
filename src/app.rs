//! The terminal UI: one transcript, one input line, and a palette that filters as you type.

use crate::api::Api;
use crate::commands;
use crate::format::{self, L};
use crate::intake;
use crate::oauth;
use crate::provider::{self, Provider, Store};
use crate::service;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io::{self, IsTerminal, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

// The omp palette: blue frame, white copy, muted hints, a pink-to-blue wordmark.
const CYAN: Color = Color::Rgb(0x00, 0xaf, 0xff);
const DIM: Color = Color::Rgb(0xa8, 0xa8, 0xa8);
const MUTED: Color = Color::Rgb(0xa8, 0xa8, 0xa8);
const FAINT: Color = Color::Rgb(0x76, 0x76, 0x76);
const TEXT: Color = Color::Rgb(0xe4, 0xe4, 0xe4);
const TIP: Color = Color::Rgb(0xd7, 0xaf, 0x87);
const RED: Color = Color::Rgb(0xff, 0x5f, 0x5f);
const GREEN: Color = Color::Rgb(0x5f, 0xd7, 0x87);

const LOGO: [&str; 6] = [
    "  ██████╗  ██████╗  ██████╗ ████████╗██╗  ██╗",
    "  ██╔══██╗██╔═══██╗██╔═══██╗╚══██╔══╝██║  ██║",
    "  ██████╔╝██║   ██║██║   ██║   ██║   ███████║",
    "  ██╔══██╗██║   ██║██║   ██║   ██║   ██╔══██║",
    "  ██████╔╝╚██████╔╝╚██████╔╝   ██║   ██║  ██║",
    "  ╚═════╝  ╚═════╝  ╚═════╝    ╚═╝   ╚═╝  ╚═╝",
];

const LOGO_COLORS: [Color; 6] = [
    Color::Rgb(0xff, 0x5f, 0xd7),
    Color::Rgb(0xd7, 0x5f, 0xd7),
    Color::Rgb(0xaf, 0x5f, 0xd7),
    Color::Rgb(0x87, 0x5f, 0xff),
    Color::Rgb(0x5f, 0x87, 0xff),
    Color::Rgb(0x5f, 0xaf, 0xd7),
];

const TIPS: [&str; 4] = [
    "Bare text is an instruction: the agent turns it into a ticket.",
    "/setup names the repository, once; then instructions just work.",
    "/provider shows the model the agent runs with; /model switches it.",
    "Ctrl+P opens the palette, Ctrl+R searches history; /keys lists them all.",
];

/// `/login` starts here: the account sign-in first, then API-key providers.
const LOGIN_METHODS: &[(&str, &str)] = &[
    ("DeepSeek account", "browser sign-in, no API key"),
    ("DeepSeek API key", "platform.deepseek.com key"),
    ("OpenAI API key", "api.openai.com"),
    ("OpenRouter API key", "openrouter.ai"),
    ("Custom endpoint", "any OpenAI-compatible base URL"),
];

enum Msg {
    Lines(Vec<L>),
    Error(String),
    Status(Status),
    Replies(Vec<intake::Reply>),
    Decisions(Vec<crate::model::Decision>),
    Tickets(Vec<crate::model::TicketRow>),
    OAuthReady(Box<oauth::Attempt>),
    OAuthCode(String),
    OAuthResult(Result<(), String>),
}

enum Modal {
    LoginList {
        selected: usize,
    },
    Login(LoginForm),
    OAuth(OAuthState),
    Model(ModelPick),
    /// Ctrl+R: filter the command history and put the chosen line back in the composer.
    History {
        query: String,
        selected: usize,
    },
    /// `/keys`: the shortcuts, one screen.
    Keys,
}

/// Browser sign-in state. `attempt` arrives once `auth_init` answers; `paste` accepts the
/// callback URL for machines where the browser cannot reach the loopback port.
struct OAuthState {
    url: String,
    status: String,
    paste: String,
    cursor: usize,
    attempt: Option<oauth::Attempt>,
    cancel: Option<Arc<AtomicBool>>,
}

/// The `/login` form: preset, id, base URL, model, api, key. `field` is the focused row.
struct LoginForm {
    preset: usize,
    id: String,
    base: String,
    model: String,
    api: String,
    key: String,
    field: usize,
    cursor: usize,
    error: Option<String>,
}

struct ModelPick {
    selected: usize,
}

impl LoginForm {
    fn new(preset: usize) -> Self {
        let item = &provider::PRESETS[preset.min(provider::PRESETS.len() - 1)];
        Self {
            preset,
            id: item.id.to_owned(),
            base: item.base.to_owned(),
            model: item.model.to_owned(),
            api: item.api.to_owned(),
            key: String::new(),
            field: 1,
            cursor: 0,
            error: None,
        }
    }

    fn value(&self) -> &str {
        match self.field {
            1 => &self.id,
            2 => &self.base,
            3 => &self.model,
            5 => &self.key,
            _ => "",
        }
    }

    fn focus(&mut self, field: usize) {
        self.field = field;
        let length = self.value().chars().count();
        self.cursor = length;
    }
}

#[derive(Clone)]
struct Status {
    base: String,
    runtime: String,
    paused: bool,
    running: u32,
    workers: u32,
    queued: u32,
    errors: usize,
    setup: bool,
    note: Option<String>,
    account: bool,
    repository: Option<String>,
}

impl Status {
    fn connecting(base: &str) -> Self {
        Self {
            base: base.to_owned(),
            runtime: "connecting".into(),
            paused: false,
            running: 0,
            workers: 0,
            queued: 0,
            errors: 0,
            setup: false,
            note: None,
            account: false,
            repository: None,
        }
    }

    fn down(base: &str, note: String) -> Self {
        Self {
            runtime: "down".into(),
            note: Some(note),
            ..Self::connecting(base)
        }
    }

    fn from_state(api: &Api, state: &crate::model::StateView) -> Self {
        Self {
            base: api.base().to_owned(),
            runtime: state
                .health
                .get("runtime")
                .map(|health| health.state.clone())
                .unwrap_or_else(|| "?".into()),
            paused: state.paused,
            running: state.load.running,
            workers: state.load.max_workers,
            queued: state.load.queued,
            errors: state
                .health
                .values()
                .filter(|health| health.state == "error")
                .count(),
            setup: state.setup.problem.is_some(),
            note: None,
            account: oauth::account_signed_in(),
            repository: state
                .setup
                .accounts
                .as_ref()
                .and_then(|accounts| accounts.repository.clone()),
        }
    }
}

/// The ticket the next instruction continues, and the thread that reaches it.
struct Follow {
    thread: String,
    ticket: String,
}

/// What the console has already said about one ticket it opened.
struct Tracked {
    stage: String,
}

pub struct App {
    api: Api,
    lines: Vec<L>,
    input: String,
    /// Cursor position in characters, not bytes.
    cursor: usize,
    history: Vec<String>,
    hist: Option<usize>,
    selection: usize,
    busy: bool,
    follow: bool,
    scroll: u16,
    quit: bool,
    status: Status,
    started: Instant,
    config: Option<service::Config>,
    store: Store,
    modal: Option<Modal>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Agent messages already shown; the outbox is reread whole on each poll.
    seen: std::collections::HashSet<String>,
    /// Trigger ids of the instructions this console sent and has not seen judged yet, with when
    /// they were sent and whether the console already said the judgement is slow.
    pending: std::collections::HashMap<String, (Instant, bool)>,
    /// Tickets this console opened, with what it has already shown about each.
    tracked: std::collections::HashMap<String, Tracked>,
    /// Set after an accept: the next instruction goes to that ticket's thread instead of a new one.
    continues: Option<Follow>,
    /// Tickets the agent already wrote a reply about; their reports need no second telling.
    replied: std::collections::HashSet<String>,
    /// When the transcript last polled the service for status and replies.
    polled: Instant,
    /// One poll at a time: a slow or unreachable service must not stack polling threads.
    polling: Arc<AtomicBool>,
    /// The last killed text, for Ctrl+Y, as in omp's editor.
    kill: String,
    /// Lines that arrived while the transcript was scrolled up.
    unread: usize,
    /// When the current activity began, for the working row's elapsed time.
    work_since: Option<Instant>,
    /// Open tickets and those waiting for a person, from the last ticket poll.
    open: u32,
    person: u32,
}

pub fn run(api: Api) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "the console needs a terminal; use `booth <command>` for one-shot output".into(),
        );
    }
    let mut terminal = setup_terminal().map_err(|error| error.to_string())?;
    let mut app = App::new(api);
    app.bootstrap();
    let result = app
        .event_loop(&mut terminal)
        .map_err(|error| error.to_string());
    restore_terminal(&mut terminal).map_err(|error| error.to_string())?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

impl App {
    fn new(api: Api) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Status::connecting(api.base());
        let config = service::Config::from_env().ok();
        let store = config.as_ref().map(Store::load).unwrap_or_default();
        let history = config.as_ref().map(intake::history).unwrap_or_default();
        let continues = config
            .as_ref()
            .and_then(intake::follow)
            .map(|(ticket, thread)| Follow { thread, ticket });
        // A restart shows the recent conversation, not the whole outbox.
        let mut seen = std::collections::HashSet::new();
        if let Some(config) = &config {
            let replies = intake::replies(config);
            let skip = replies.len().saturating_sub(20);
            for reply in replies.into_iter().take(skip) {
                seen.insert(reply.id);
            }
        }
        Self {
            api,
            config,
            store,
            modal: None,
            lines: Vec::new(),
            input: String::new(),
            cursor: 0,
            history,
            hist: None,
            selection: 0,
            busy: false,
            follow: true,
            scroll: 0,
            quit: false,
            status,
            started: Instant::now(),
            tx,
            rx,
            seen,
            pending: std::collections::HashMap::new(),
            tracked: std::collections::HashMap::new(),
            continues,
            replied: std::collections::HashSet::new(),
            polled: Instant::now(),
            polling: Arc::new(AtomicBool::new(false)),
            kill: String::new(),
            unread: 0,
            work_since: None,
            open: 0,
            person: 0,
        }
    }

    fn bootstrap(&mut self) {
        if let Some(follow) = &self.continues {
            let short = &follow.ticket[..follow.ticket.len().min(8)];
            self.push(Line::from(vec![
                Span::styled("  ↳ ", Style::default().fg(CYAN)),
                Span::styled(
                    format!("continuing ticket {short}"),
                    Style::default().fg(DIM),
                ),
                Span::styled("  · /new starts a new ticket", Style::default().fg(FAINT)),
            ]));
        }
        self.refresh_status();
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        loop {
            self.drain();
            terminal.draw(|frame| self.draw(frame))?;
            if self.quit {
                return Ok(());
            }
            if self.polled.elapsed() > Duration::from_secs(2) {
                self.polled = Instant::now();
                self.check_pending();
                self.refresh_status();
            }
            if event::poll(Duration::from_millis(120))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.on_key(key);
                    }
                }
            }
        }
    }

    fn drain(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Msg::Lines(lines) => {
                    self.busy = false;
                    self.push_lines(lines);
                    self.refresh_status();
                }
                Msg::Error(error) => {
                    self.busy = false;
                    self.push(Line::from(Span::styled(
                        format!("! {error}"),
                        Style::default().fg(RED),
                    )));
                }
                Msg::Status(status) => self.status = status,
                Msg::Decisions(decisions) => {
                    for decision in decisions {
                        let Some(trigger) = decision
                            .trigger_ids
                            .iter()
                            .find(|trigger| self.pending.contains_key(*trigger))
                            .cloned()
                        else {
                            continue;
                        };
                        self.pending.remove(&trigger);
                        // An accept is already answered by the acknowledgement in the outbox; the
                        // ticket it opened is followed here for its progress and final report.
                        if decision.outcome == "accept" {
                            if let Some(ticket) = &decision.ticket_id {
                                self.tracked.insert(
                                    ticket.clone(),
                                    Tracked {
                                        stage: "new".into(),
                                    },
                                );
                                if let Some(thread) = decision
                                    .thread
                                    .as_deref()
                                    .and_then(|key| key.strip_prefix("local:"))
                                {
                                    self.continues = Some(Follow {
                                        thread: thread.to_owned(),
                                        ticket: ticket.clone(),
                                    });
                                    if let Some(config) = &self.config {
                                        intake::set_follow(config, Some((ticket.as_str(), thread)));
                                    }
                                }
                            }
                        } else {
                            self.push_decision(&decision);
                        }
                    }
                }
                Msg::Tickets(rows) => self.show_tickets(rows),
                Msg::Replies(replies) => {
                    let mut fresh = Vec::new();
                    for reply in replies {
                        if self.seen.insert(reply.id.clone()) {
                            fresh.push(reply);
                        }
                    }
                    for reply in fresh {
                        self.push_agent(reply);
                    }
                }
                Msg::OAuthReady(attempt) => {
                    if let Some(Modal::OAuth(state)) = &mut self.modal {
                        state.url = attempt.url.clone();
                        state.attempt = Some(*attempt);
                        state.status = "waiting for the browser…".into();
                    }
                }
                Msg::OAuthCode(code) => {
                    let attempt = match &self.modal {
                        Some(Modal::OAuth(state)) => state.attempt.clone(),
                        _ => None,
                    };
                    if let Some(attempt) = attempt {
                        self.exchange_oauth(attempt, code);
                    }
                }
                Msg::OAuthResult(result) => match result {
                    Ok(()) => {
                        self.modal = None;
                        self.push(Line::from(vec![
                            Span::styled("signed in ", Style::default().fg(GREEN)),
                            Span::styled(
                                "DeepSeek account · restarting the service",
                                Style::default().fg(DIM),
                            ),
                        ]));
                        if let Some(cfg) = &self.config {
                            let _ = std::fs::remove_file(provider::overlay(cfg));
                        }
                        self.restart_service();
                    }
                    Err(error) => {
                        if let Some(Modal::OAuth(state)) = &mut self.modal {
                            state.status = error;
                        } else {
                            self.push(Line::from(Span::styled(
                                format!("! {error}"),
                                Style::default().fg(RED),
                            )));
                        }
                    }
                },
            }
        }
    }

    fn push(&mut self, line: L) {
        self.push_lines(vec![line]);
    }

    /// Reports the stage of the tickets this console opened, once per change; a report the agent
    /// did not also send as a reply is shown as a short fallback.
    fn show_tickets(&mut self, rows: Vec<crate::model::TicketRow>) {
        self.open = rows
            .iter()
            .filter(|row| row.state == "open")
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        self.person = rows
            .iter()
            .filter(|row| row.state == "open" && row.stage == "person")
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        let by_id: std::collections::HashMap<&str, &crate::model::TicketRow> =
            rows.iter().map(|row| (row.id.as_str(), row)).collect();
        let replied = self.replied.clone();
        let mut lines: Vec<L> = Vec::new();
        let mut done: Vec<String> = Vec::new();
        for (id, tracked) in self.tracked.iter_mut() {
            let short = id[..id.len().min(8)].to_owned();
            let Some(row) = by_id.get(id.as_str()) else {
                if tracked.stage != "closed" {
                    tracked.stage = "closed".into();
                    lines.push(ticket_line(&format!("{short} · closed")));
                }
                done.push(id.clone());
                continue;
            };
            let stage = if row.state == "ended" {
                "closed"
            } else {
                row.stage.as_str()
            };
            // "new" is the placeholder between the accept and the first poll.
            if stage != tracked.stage && !(tracked.stage == "new" && stage == "idle") {
                let label = match stage {
                    "person" => row
                        .action
                        .as_deref()
                        .map(|action| format!("needs a person · {action}"))
                        .unwrap_or_else(|| "needs a person".into()),
                    "working" => "working".into(),
                    "idle" => "waiting".into(),
                    "closed" => "closed".into(),
                    other => other.to_owned(),
                };
                lines.push(ticket_line(&format!("{short} · {label}")));
                tracked.stage = stage.to_owned();
            }
            // The agent's own reply is the report; only a silent report falls back to the summary.
            if stage == "closed" && !replied.contains(id.as_str()) {
                if let Some(summary) = row.summary.as_deref().filter(|text| !text.is_empty()) {
                    lines.push(ticket_line(&format!("{short} · report")));
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(format::clip(summary, 400), Style::default().fg(TEXT)),
                    ]));
                }
            }
            if stage == "closed" {
                done.push(id.clone());
            }
        }
        self.push_lines(lines);
        for id in done {
            self.tracked.remove(&id);
            if self
                .continues
                .as_ref()
                .map(|follow| follow.ticket == id)
                .unwrap_or(false)
            {
                self.continues = None;
                if let Some(config) = &self.config {
                    intake::set_follow(config, None);
                }
                self.push(Line::from(Span::styled(
                    "  · that ticket is closed; the next instruction starts a new one",
                    Style::default().fg(FAINT),
                )));
            }
        }
    }

    /// Shows why the agent did not take an instruction; silence is the one answer a console cannot show.
    fn push_decision(&mut self, decision: &crate::model::Decision) {
        let label = match decision.outcome.as_str() {
            "ignore" => "agent · not taken",
            "unavailable" => "agent · could not judge",
            "stop" => "agent · stopped",
            other => other,
        };
        self.push(Line::from(vec![
            Span::styled("  ◇ ", Style::default().fg(FAINT)),
            Span::styled(label.to_owned(), Style::default().fg(FAINT)),
            Span::styled(
                format!("  {}", format::reason(&decision.reason)),
                Style::default().fg(MUTED),
            ),
        ]));
    }

    /// Shows one message the agent wrote back: an acknowledgement, a reply, or a notice.
    fn push_agent(&mut self, reply: intake::Reply) {
        if let Some(ticket) = &reply.ticket {
            self.replied.insert(ticket.clone());
        }
        let label = match reply.kind.as_str() {
            "acknowledgement" => "agent · accepted",
            "notice" => "agent · notice",
            _ => "agent",
        };
        let mut lines = Vec::new();
        let head = vec![
            Span::styled("  ◆ ", Style::default().fg(GREEN)),
            Span::styled(label.to_owned(), Style::default().fg(FAINT)),
            match &reply.ticket {
                Some(ticket) => Span::styled(
                    format!("  {}", &ticket[..ticket.len().min(8)]),
                    Style::default().fg(FAINT),
                ),
                None => Span::raw(""),
            },
        ];
        lines.push(Line::from(head));
        for line in reply.text.lines() {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(line.to_owned(), Style::default().fg(TEXT)),
            ]));
        }
        self.push_lines(lines);
    }

    fn push_lines(&mut self, lines: Vec<L>) {
        if !self.follow && !lines.is_empty() {
            self.unread += lines.len();
        }
        self.lines.extend(lines);
        let overflow = self.lines.len().saturating_sub(4000);
        if overflow > 0 {
            self.lines.drain(0..overflow);
            self.scroll = self.scroll.saturating_sub(overflow as u16);
        }
        if self.follow {
            self.scroll = u16::MAX;
        }
    }

    fn spawn(&mut self, line: String) {
        self.busy = true;
        let api = self.api.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let message = match commands::execute(&api, &line) {
                Ok(lines) => Msg::Lines(lines),
                Err(error) => Msg::Error(error),
            };
            let _ = tx.send(message);
        });
    }

    /// Hands one instruction to the agent; the Source picks it up on its next poll.
    fn instruct(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            self.push(warn_line(
                "/send needs an instruction; bare text sends one too",
            ));
            return;
        }
        let Some(config) = self.config.clone() else {
            self.push(warn_line(
                "the console cannot find its state directory; set BOOTH_STATE_DIR",
            ));
            return;
        };
        let follow = self.continues.as_ref().map(|follow| follow.thread.clone());
        match intake::send(&config, text, follow.as_deref()) {
            Ok(id) => {
                let lines = match &self.continues {
                    Some(follow) => vec![Line::from(vec![
                        Span::styled("  ↳ ", Style::default().fg(CYAN)),
                        Span::styled("follow-up to ", Style::default().fg(DIM)),
                        Span::styled(
                            follow.ticket[..follow.ticket.len().min(8)].to_owned(),
                            Style::default().fg(CYAN),
                        ),
                        Span::styled("  · /new starts a new ticket", Style::default().fg(FAINT)),
                    ])],
                    None => {
                        // A follow-up fans out to its ticket without a decision, so only a new
                        // instruction is awaited as one.
                        self.pending
                            .insert(format!("local-{id}"), (Instant::now(), false));
                        vec![Line::from(vec![
                            Span::styled("  ↑ ", Style::default().fg(GREEN)),
                            Span::styled("sent to the agent", Style::default().fg(GREEN)),
                            Span::styled(
                                format!("  {} · picks it up within a second", &id[..8]),
                                Style::default().fg(FAINT),
                            ),
                        ])]
                    }
                };
                self.push_lines(lines);
                if self.status.runtime == "down" {
                    self.push(warn_line(
                        "the service is down; the instruction waits in the queue — `booth up`",
                    ));
                }
                self.refresh_status();
            }
            Err(error) => self.push(warn_line(&error)),
        }
    }

    fn spawn_setup(&mut self, argument: String) {
        self.busy = true;
        let api = self.api.clone();
        let config = self.config.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let message = match config {
                Some(config) => match intake::setup(&api, &config, &argument) {
                    Ok(lines) => Msg::Lines(lines),
                    Err(error) => Msg::Error(error),
                },
                None => Msg::Error("the console cannot find its state directory".into()),
            };
            let _ = tx.send(message);
        });
    }

    /// Says so when a sent instruction is not judged, instead of leaving "sent" as the last word.
    fn check_pending(&mut self) {
        let now = Instant::now();
        let slow: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, (sent, warned))| {
                !warned && now.duration_since(*sent) > Duration::from_secs(20)
            })
            .map(|(trigger, _)| trigger.clone())
            .collect();
        for trigger in slow {
            if let Some((_, warned)) = self.pending.get_mut(&trigger) {
                *warned = true;
            }
            self.push(Line::from(Span::styled(
                "  ⋯ no decision yet · /sources shows whether the source is polling",
                Style::default().fg(FAINT),
            )));
        }
        let stale: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, (sent, _))| now.duration_since(*sent) > Duration::from_secs(240))
            .map(|(trigger, _)| trigger.clone())
            .collect();
        for trigger in stale {
            self.pending.remove(&trigger);
            self.push(warn_line(
                "the agent never judged that instruction; /sources and /status explain why",
            ));
        }
    }

    fn refresh_status(&self) {
        if self.polling.swap(true, Ordering::SeqCst) {
            return;
        }
        let polling = self.polling.clone();
        let api = self.api.clone();
        let tx = self.tx.clone();
        let config = self.config.clone();
        let pending = !self.pending.is_empty();
        let tracked: Vec<String> = self.tracked.keys().cloned().collect();
        let polled = self.api.clone();
        std::thread::spawn(move || {
            if pending {
                if let Ok(view) = polled.decisions() {
                    let _ = tx.send(Msg::Decisions(view.items));
                }
            }
            // Replies first: the ticket poll below then knows a report was already spoken.
            if let Some(config) = &config {
                let replies = intake::replies(config);
                if !replies.is_empty() {
                    let _ = tx.send(Msg::Replies(replies));
                }
            }
            // Open tickets are the header's counts as well as the followed tickets' stages.
            let mut rows = Vec::new();
            let mut fetched = false;
            if let Ok(page) = polled.tickets("open") {
                rows.extend(page.items);
                fetched = true;
            }
            if !tracked.is_empty() {
                if let Ok(page) = polled.tickets("ended") {
                    rows.extend(page.items);
                }
            }
            if fetched {
                let _ = tx.send(Msg::Tickets(rows));
            }
            let status = match api.state() {
                Ok(state) => Status::from_state(&api, &state),
                Err(error) => {
                    let text = error.to_string();
                    let note = if text.contains("cannot reach") {
                        format!("{} · booth up", clip_error(&text))
                    } else {
                        clip_error(&text)
                    };
                    Status::down(api.base(), note)
                }
            };
            let _ = tx.send(Msg::Status(status));
            polling.store(false, Ordering::SeqCst);
        });
    }

    fn palette(&self) -> Option<Vec<(&'static str, &'static str)>> {
        if !self.input.starts_with('/') {
            return None;
        }
        let matches = commands::matches(&self.input);
        if matches.is_empty() {
            None
        } else {
            Some(matches)
        }
    }

    fn complete(&mut self) {
        if let Some(matches) = self.palette() {
            let index = self.selection.min(matches.len().saturating_sub(1));
            let name = matches[index].0.split_whitespace().next().unwrap_or("");
            self.input = format!("{name} ");
            self.cursor = self.input.chars().count();
            self.selection = 0;
            self.hist = None;
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if self.modal.is_some() {
            self.modal_key(key);
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => self.quit = true,
                KeyCode::Char('l') => {
                    self.lines.clear();
                    self.scroll = 0;
                }
                KeyCode::Char('p') => {
                    // An empty composer opens the palette; a draft is never replaced by it.
                    if self.input.is_empty() {
                        self.input = "/".into();
                        self.cursor = 1;
                        self.selection = 0;
                        self.follow = true;
                    }
                }
                KeyCode::Char('a') => self.cursor = 0,
                KeyCode::Char('e') => self.cursor = self.input.chars().count(),
                KeyCode::Char('w') => self.delete_word_back(),
                KeyCode::Char('u') => self.delete_to_start(),
                KeyCode::Char('k') => self.delete_to_end(),
                KeyCode::Char('y') => self.yank(),
                KeyCode::Char('r') => self.open_history(),
                KeyCode::Left => self.word_left(),
                KeyCode::Right => self.word_right(),
                KeyCode::Backspace => self.delete_word_back(),
                _ => {}
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char('b') => self.word_left(),
                KeyCode::Char('f') => self.word_right(),
                KeyCode::Backspace => self.delete_word_back(),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Enter => {
                if self.input.trim() == "/" {
                    self.complete();
                } else {
                    self.submit();
                }
            }
            KeyCode::Tab => self.complete(),
            KeyCode::Esc => {
                self.input.clear();
                self.cursor = 0;
                self.selection = 0;
                self.follow = true;
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => {
                if self.cursor < self.input.chars().count() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Up => self.up(),
            KeyCode::Down => self.down(),
            KeyCode::PageUp => {
                self.follow = false;
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(10);
            }
            KeyCode::Char(ch) => self.insert(ch),
            _ => {}
        }
    }

    fn insert(&mut self, ch: char) {
        let byte = char_to_byte(&self.input, self.cursor);
        self.input.insert(byte, ch);
        self.cursor += 1;
        self.selection = 0;
        self.hist = None;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let byte = char_to_byte(&self.input, self.cursor - 1);
        self.input.remove(byte);
        self.cursor -= 1;
        self.selection = 0;
    }

    fn delete(&mut self) {
        if self.cursor >= self.input.chars().count() {
            return;
        }
        let byte = char_to_byte(&self.input, self.cursor);
        self.input.remove(byte);
    }

    fn word_left(&mut self) {
        let chars: Vec<char> = self.input.chars().collect();
        let mut index = self.cursor;
        while index > 0 && chars[index - 1].is_whitespace() {
            index -= 1;
        }
        while index > 0 && !chars[index - 1].is_whitespace() {
            index -= 1;
        }
        self.cursor = index;
    }

    fn word_right(&mut self) {
        let chars: Vec<char> = self.input.chars().collect();
        let mut index = self.cursor;
        while index < chars.len() && !chars[index].is_whitespace() {
            index += 1;
        }
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        self.cursor = index;
    }

    fn delete_word_back(&mut self) {
        let end = self.cursor;
        self.word_left();
        let start = self.cursor;
        if start == end {
            return;
        }
        self.kill =
            self.input[char_to_byte(&self.input, start)..char_to_byte(&self.input, end)].to_owned();
        let range = char_to_byte(&self.input, start)..char_to_byte(&self.input, end);
        self.input.replace_range(range, "");
        self.cursor = start;
        self.selection = 0;
    }

    fn delete_to_start(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.kill = self.input[..char_to_byte(&self.input, self.cursor)].to_owned();
        let rest = self.input[char_to_byte(&self.input, self.cursor)..].to_owned();
        self.input = rest;
        self.cursor = 0;
        self.selection = 0;
    }

    fn delete_to_end(&mut self) {
        if self.cursor >= self.input.chars().count() {
            return;
        }
        self.kill = self.input[char_to_byte(&self.input, self.cursor)..].to_owned();
        let kept = self.input[..char_to_byte(&self.input, self.cursor)].to_owned();
        self.input = kept;
        self.selection = 0;
    }

    fn yank(&mut self) {
        if self.kill.is_empty() {
            return;
        }
        let byte = char_to_byte(&self.input, self.cursor);
        let killed = self.kill.clone();
        self.input.insert_str(byte, &killed);
        self.cursor += killed.chars().count();
        self.selection = 0;
    }

    fn up(&mut self) {
        if let Some(matches) = self.palette() {
            self.selection = self.selection.saturating_sub(1).min(matches.len() - 1);
            return;
        }
        if self.history.is_empty() {
            return;
        }
        let next = match self.hist {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.hist = Some(next);
        self.input = self.history[next].clone();
        self.cursor = self.input.chars().count();
    }

    fn down(&mut self) {
        if let Some(matches) = self.palette() {
            self.selection = (self.selection + 1).min(matches.len() - 1);
            return;
        }
        match self.hist {
            Some(index) if index + 1 < self.history.len() => {
                self.hist = Some(index + 1);
                self.input = self.history[index + 1].clone();
                self.cursor = self.input.chars().count();
            }
            _ => {
                self.hist = None;
                self.input.clear();
                self.cursor = 0;
            }
        }
    }

    fn submit(&mut self) {
        let line = self.input.trim().to_string();
        self.input.clear();
        self.cursor = 0;
        self.selection = 0;
        self.hist = None;
        if line.is_empty() {
            return;
        }
        self.history.push(line.clone());
        if self.history.len() > 500 {
            self.history.remove(0);
        }
        if let Some(config) = &self.config {
            intake::remember(config, &line);
        }
        if line == "/model" {
            self.open_model();
            return;
        }
        if line == "/keys" || line == "/hotkeys" {
            self.push(Line::from(vec![
                Span::styled("  ❯ ", Style::default().fg(CYAN)),
                Span::styled(line.clone(), Style::default().fg(TEXT)),
            ]));
            self.modal = Some(Modal::Keys);
            return;
        }
        if line == "/history" {
            self.open_history();
            return;
        }
        if line == "/new" || line == "/reset" {
            self.push(Line::from(vec![
                Span::styled("  ❯ ", Style::default().fg(CYAN)),
                Span::styled(line.clone(), Style::default().fg(TEXT)),
            ]));
            if self.continues.take().is_some() {
                if let Some(config) = &self.config {
                    intake::set_follow(config, None);
                }
                self.push(Line::from(Span::styled(
                    "✓ the next instruction starts a new ticket",
                    Style::default().fg(GREEN),
                )));
            } else {
                self.push(Line::from(Span::styled(
                    "each instruction already starts a new ticket",
                    Style::default().fg(DIM),
                )));
            }
            return;
        }
        if line == "/provider" {
            self.push(Line::from(vec![
                Span::styled("  ❯ ", Style::default().fg(CYAN)),
                Span::styled(line.clone(), Style::default().fg(TEXT)),
            ]));
            self.push_lines(intake::summary(&self.store));
            return;
        }
        if let Some(rest) = line.strip_prefix("/send") {
            if !rest.is_empty() && !rest.starts_with(' ') {
                // `/sender` is not `/send`: fall through to the command grammar.
            } else {
                self.push(Line::from(vec![
                    Span::styled("  ❯ ", Style::default().fg(CYAN)),
                    Span::styled(line.clone(), Style::default().fg(TEXT)),
                ]));
                self.instruct(rest.trim());
                return;
            }
        }
        if line == "/setup" || line.starts_with("/setup ") {
            self.push(Line::from(vec![
                Span::styled("  ❯ ", Style::default().fg(CYAN)),
                Span::styled(line.clone(), Style::default().fg(TEXT)),
            ]));
            self.spawn_setup(line.trim_start_matches("/setup").trim().to_owned());
            return;
        }
        if line == "/login" {
            self.modal = Some(Modal::LoginList { selected: 0 });
            return;
        }
        if let Some(rest) = line.strip_prefix("/login ") {
            let rest = rest.trim();
            if rest == "oauth" || rest == "deepseek-account" {
                self.start_oauth();
            } else {
                self.open_login(rest);
            }
            return;
        }
        match line.as_str() {
            "/quit" | "/q" => {
                self.quit = true;
                return;
            }
            "/clear" => {
                self.lines.clear();
                self.scroll = 0;
                self.follow = true;
                return;
            }
            "/refresh" => {
                self.refresh_status();
                return;
            }
            _ => {}
        }
        self.push(Line::from(vec![
            Span::styled("  ❯ ", Style::default().fg(CYAN)),
            Span::styled(line.clone(), Style::default().fg(TEXT)),
        ]));
        if line == "/help" || line == "/?" {
            self.push_lines(format::help());
            return;
        }
        // Bare text is an instruction for the agent; the commands above are the console's own.
        if !line.starts_with('/') {
            self.instruct(&line);
            return;
        }
        self.spawn(line);
    }

    fn open_login(&mut self, preset: &str) {
        let index = provider::PRESETS
            .iter()
            .position(|item| item.id == preset || item.label.eq_ignore_ascii_case(preset))
            .unwrap_or(0);
        self.modal = Some(Modal::Login(LoginForm::new(index)));
    }

    fn open_model(&mut self) {
        let selected = self
            .store
            .providers
            .iter()
            .position(|item| item.id == self.store.current)
            .unwrap_or(0);
        self.modal = Some(Modal::Model(ModelPick { selected }));
    }

    fn open_history(&mut self) {
        if self.history.is_empty() {
            self.push(Line::from(Span::styled(
                "  no history yet",
                Style::default().fg(FAINT),
            )));
            return;
        }
        self.modal = Some(Modal::History {
            query: String::new(),
            selected: 0,
        });
    }

    /// Newest first, filtered by the same fuzzy subsequence the picker draws.
    fn history_matches(&self, query: &str) -> Vec<&String> {
        matching_history(&self.history, query)
    }

    fn history_key(&mut self, key: KeyEvent, mut query: String, mut selected: usize) {
        let count = self.history_matches(&query).len();
        // Ctrl+R again walks to the next older match, as readline's reverse search does.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            selected = (selected + 1).min(count.saturating_sub(1));
            self.modal = Some(Modal::History { query, selected });
            return;
        }
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Up => selected = selected.saturating_sub(1),
            KeyCode::Down => selected = (selected + 1).min(count.saturating_sub(1)),
            KeyCode::Enter => {
                let chosen = self
                    .history_matches(&query)
                    .get(selected.min(count.saturating_sub(1)))
                    .map(|line| (*line).clone());
                if let Some(line) = chosen {
                    self.cursor = line.chars().count();
                    self.input = line;
                }
                return;
            }
            KeyCode::Backspace => {
                query.pop();
                selected = 0;
            }
            KeyCode::Char(ch) => {
                query.push(ch);
                selected = 0;
            }
            _ => {}
        }
        self.modal = Some(Modal::History { query, selected });
    }

    fn modal_key(&mut self, key: KeyEvent) {
        match self.modal.take() {
            Some(Modal::LoginList { selected }) => self.login_list_key(key, selected),
            Some(Modal::Login(form)) => self.login_key(key, form),
            Some(Modal::OAuth(state)) => self.oauth_key(key, state),
            Some(Modal::Model(pick)) => self.model_key(key, pick),
            Some(Modal::History { query, selected }) => self.history_key(key, query, selected),
            Some(Modal::Keys) => {}
            None => {}
        }
    }

    fn login_list_key(&mut self, key: KeyEvent, mut selected: usize) {
        let count = LOGIN_METHODS.len();
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Up => selected = selected.saturating_sub(1),
            KeyCode::Down => selected = (selected + 1).min(count - 1),
            KeyCode::Enter => {
                if selected == 0 {
                    self.start_oauth();
                } else {
                    self.modal = Some(Modal::Login(LoginForm::new(selected - 1)));
                }
                return;
            }
            _ => {}
        }
        self.modal = Some(Modal::LoginList { selected });
    }

    fn start_oauth(&mut self) {
        let port = std::env::var("BOOTH_OAUTH_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(37029);
        let cancel = Arc::new(AtomicBool::new(false));
        self.modal = Some(Modal::OAuth(OAuthState {
            url: String::new(),
            status: "asking platform.deepseek.com…".into(),
            paste: String::new(),
            cursor: 0,
            attempt: None,
            cancel: Some(cancel.clone()),
        }));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            // A busy default port (say, another DSH web) falls back to an ephemeral one.
            let started = oauth::begin(port).or_else(|first| {
                if port == 0 {
                    Err(first)
                } else {
                    oauth::begin(0).map_err(|second| format!("{first}; {second}"))
                }
            });
            match started {
                Ok((attempt, listener)) => {
                    let _ = tx.send(Msg::OAuthReady(Box::new(attempt.clone())));
                    match oauth::wait_code(
                        &listener,
                        &attempt.state,
                        &cancel,
                        Duration::from_secs(600),
                    ) {
                        Ok(code) => {
                            let _ = tx.send(Msg::OAuthCode(code));
                        }
                        Err(error) => {
                            let _ = tx.send(Msg::OAuthResult(Err(error)));
                        }
                    }
                }
                Err(error) => {
                    let _ = tx.send(Msg::OAuthResult(Err(error)));
                }
            }
        });
    }

    fn exchange_oauth(&mut self, attempt: oauth::Attempt, code: String) {
        if let Some(Modal::OAuth(state)) = &mut self.modal {
            state.status = "exchanging the code…".into();
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = oauth::exchange(&attempt, &code);
            let _ = tx.send(Msg::OAuthResult(result));
        });
    }

    fn oauth_key(&mut self, key: KeyEvent, mut state: OAuthState) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            if !state.url.is_empty() {
                let _ = std::process::Command::new("xdg-open")
                    .arg(&state.url)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            self.modal = Some(Modal::OAuth(state));
            return;
        }
        match key.code {
            KeyCode::Esc => {
                if let Some(cancel) = &state.cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
                return;
            }
            KeyCode::Enter => {
                let url = state.paste.trim().to_owned();
                let attempt = state.attempt.clone();
                if let (Some(attempt), false) = (attempt, url.is_empty()) {
                    match oauth::code_from_url(&url, &attempt.state) {
                        Ok(code) => {
                            self.exchange_oauth(attempt, code);
                            return;
                        }
                        Err(error) => state.status = error,
                    }
                }
                self.modal = Some(Modal::OAuth(state));
                return;
            }
            KeyCode::Backspace => {
                let cursor = state.cursor;
                state.cursor = remove_before(&mut state.paste, cursor);
            }
            KeyCode::Left => state.cursor = state.cursor.saturating_sub(1),
            KeyCode::Right => {
                state.cursor = (state.cursor + 1).min(state.paste.chars().count());
            }
            KeyCode::Char(ch) => {
                insert_char(&mut state.paste, state.cursor, ch);
                state.cursor += 1;
            }
            _ => {}
        }
        self.modal = Some(Modal::OAuth(state));
    }

    fn login_key(&mut self, key: KeyEvent, mut form: LoginForm) {
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Tab => {
                let next = (form.field + 1) % 6;
                form.focus(next);
            }
            KeyCode::BackTab => {
                let next = (form.field + 5) % 6;
                form.focus(next);
            }
            KeyCode::Enter => {
                if form.field == 5 {
                    self.commit_login(form);
                    return;
                }
                let next = form.field + 1;
                form.focus(next);
            }
            KeyCode::Left | KeyCode::Right => {
                let step = if key.code == KeyCode::Left { -1 } else { 1 };
                match form.field {
                    0 => {
                        let count = provider::PRESETS.len() as i32;
                        let next = (form.preset as i32 + step).rem_euclid(count) as usize;
                        form = LoginForm::new(next);
                    }
                    4 => {
                        let count = provider::APIS.len() as i32;
                        let at = provider::APIS
                            .iter()
                            .position(|item| *item == form.api)
                            .unwrap_or(0) as i32;
                        form.api =
                            provider::APIS[(at + step).rem_euclid(count) as usize].to_owned();
                    }
                    _ => {
                        let length = form.value().chars().count();
                        form.cursor = if step < 0 {
                            form.cursor.saturating_sub(1)
                        } else {
                            (form.cursor + 1).min(length)
                        };
                    }
                }
            }
            KeyCode::Backspace => {
                let cursor = form.cursor;
                match form.field {
                    1 => form.cursor = remove_before(&mut form.id, cursor),
                    2 => form.cursor = remove_before(&mut form.base, cursor),
                    3 => form.cursor = remove_before(&mut form.model, cursor),
                    5 => form.cursor = remove_before(&mut form.key, cursor),
                    _ => {}
                }
            }
            KeyCode::Delete => {
                let cursor = form.cursor;
                match form.field {
                    1 => form.cursor = remove_at(&mut form.id, cursor),
                    2 => form.cursor = remove_at(&mut form.base, cursor),
                    3 => form.cursor = remove_at(&mut form.model, cursor),
                    5 => form.cursor = remove_at(&mut form.key, cursor),
                    _ => {}
                }
            }
            KeyCode::Char(ch) => match form.field {
                1 => {
                    insert_char(&mut form.id, form.cursor, ch);
                    form.cursor += 1;
                }
                2 => {
                    insert_char(&mut form.base, form.cursor, ch);
                    form.cursor += 1;
                }
                3 => {
                    insert_char(&mut form.model, form.cursor, ch);
                    form.cursor += 1;
                }
                5 => {
                    insert_char(&mut form.key, form.cursor, ch);
                    form.cursor += 1;
                }
                _ => {}
            },
            _ => {}
        }
        self.modal = Some(Modal::Login(form));
    }

    fn model_key(&mut self, key: KeyEvent, mut pick: ModelPick) {
        let count = self.store.providers.len() + 1;
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Up => pick.selected = pick.selected.saturating_sub(1),
            KeyCode::Down => pick.selected = (pick.selected + 1).min(count - 1),
            KeyCode::Enter => {
                if pick.selected >= self.store.providers.len() {
                    self.open_login("");
                    return;
                }
                let chosen = self.store.providers[pick.selected].clone();
                let Some(cfg) = self.config.clone() else {
                    return;
                };
                let mut store = self.store.clone();
                store.current = chosen.id.clone();
                match store.save(&cfg) {
                    Ok(()) => {
                        self.store = store;
                        self.push(Line::from(vec![
                            Span::styled("model ", Style::default().fg(GREEN)),
                            Span::styled(
                                format!("{} · {}", chosen.id, chosen.model),
                                Style::default(),
                            ),
                        ]));
                        self.restart_service();
                    }
                    Err(error) => {
                        self.push(Line::from(Span::styled(
                            format!("! {error}"),
                            Style::default().fg(RED),
                        )));
                        self.modal = Some(Modal::Model(pick));
                    }
                }
                return;
            }
            _ => {}
        }
        self.modal = Some(Modal::Model(pick));
    }

    fn commit_login(&mut self, mut form: LoginForm) {
        let id = form.id.trim().to_lowercase().replace(' ', "-");
        let base = form.base.trim().trim_end_matches('/').to_owned();
        let model = form.model.trim().to_owned();
        let key = form.key.trim().to_owned();
        let problem = if id.is_empty() {
            Some("provider id is empty")
        } else if !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            Some("provider id: letters, digits, - and _ only")
        } else if !base.starts_with("http") {
            Some("base URL must start with http")
        } else if model.is_empty() {
            Some("model is empty")
        } else if key.is_empty() {
            Some("API key is empty")
        } else {
            None
        };
        if let Some(problem) = problem {
            form.error = Some(problem.to_owned());
            self.modal = Some(Modal::Login(form));
            return;
        }
        let Some(cfg) = self.config.clone() else {
            form.error = Some("no DSH home found".to_owned());
            self.modal = Some(Modal::Login(form));
            return;
        };
        let mut store = self.store.clone();
        store.upsert(Provider {
            id,
            base,
            api: form.api.clone(),
            model,
            key,
        });
        match store.save(&cfg) {
            Ok(()) => {
                let saved = store.current().cloned().unwrap_or_default();
                self.store = store;
                self.modal = None;
                self.push(Line::from(vec![
                    Span::styled("provider ", Style::default().fg(GREEN)),
                    Span::styled(format!("{} · {}", saved.id, saved.model), Style::default()),
                ]));
                self.restart_service();
            }
            Err(error) => {
                form.error = Some(error);
                self.modal = Some(Modal::Login(form));
            }
        }
    }

    fn restart_service(&mut self) {
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut lines = Vec::new();
            let result = std::env::current_exe()
                .map_err(|error| error.to_string())
                .and_then(|exe| {
                    std::process::Command::new(exe)
                        .arg("restart")
                        .output()
                        .map_err(|error| error.to_string())
                });
            match result {
                Ok(output) => {
                    for line in String::from_utf8_lossy(&output.stdout).lines().take(6) {
                        lines.push(Line::from(Span::styled(
                            line.to_owned(),
                            Style::default().fg(DIM),
                        )));
                    }
                    if !output.status.success() {
                        lines.push(Line::from(Span::styled(
                            format!("! {}", String::from_utf8_lossy(&output.stderr).trim()),
                            Style::default().fg(RED),
                        )));
                    }
                }
                Err(error) => lines.push(Line::from(Span::styled(
                    format!("! {error}"),
                    Style::default().fg(RED),
                ))),
            }
            let _ = tx.send(Msg::Lines(lines));
        });
    }

    fn model_label(&self) -> String {
        if let Some(current) = self.store.current() {
            format!("{}/{}", current.id, current.model)
        } else if self.status.account {
            "deepseek account".into()
        } else {
            "no provider".into()
        }
    }

    fn alert_text(&self, width: u16) -> Option<String> {
        let room = (width as usize).saturating_sub(4);
        if let Some(note) = &self.status.note {
            return Some(format::clip(note, room));
        }
        if self.status.setup {
            return Some("setup pending — /setup owner/repo".into());
        }
        if self.status.errors > 0 {
            return Some(format!("{} health error(s) — /status", self.status.errors));
        }
        if self.unread > 0 && !self.follow {
            return Some(format!("{} new line(s) · PgDn follows", self.unread));
        }
        None
    }

    fn draw_composer(&self, frame: &mut Frame, top: Rect, bottom: Rect) {
        let runtime_color = match self.status.runtime.as_str() {
            "ready" => GREEN,
            "paused" => Color::Yellow,
            "connecting" => DIM,
            _ => RED,
        };
        let mut segments = vec![
            Span::styled(
                "◆ booth",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(self.model_label(), Style::default().fg(TEXT)),
            Span::styled(
                self.status.runtime.clone(),
                Style::default().fg(runtime_color),
            ),
            Span::styled(
                format!(
                    "{}/{} · {}q",
                    self.status.running,
                    self.status.workers.max(self.status.running),
                    self.status.queued
                ),
                Style::default().fg(MUTED),
            ),
        ];
        if self.open > 0 {
            segments.push(Span::styled(
                format!("{} open", self.open),
                Style::default().fg(MUTED),
            ));
        }
        if self.person > 0 {
            segments.push(Span::styled(
                format!("{} need you", self.person),
                Style::default().fg(TIP),
            ));
        }
        if let Some(repository) = &self.status.repository {
            segments.push(Span::styled(repository.clone(), Style::default().fg(FAINT)));
        }
        if self.status.paused {
            segments.push(Span::styled("paused", Style::default().fg(Color::Yellow)));
        }
        if let Some(follow) = &self.continues {
            segments.push(Span::styled(
                format!("↳ {}", &follow.ticket[..follow.ticket.len().min(8)]),
                Style::default().fg(CYAN),
            ));
        }
        let right = Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(FAINT),
        );
        frame.render_widget(
            Paragraph::new(compose_top(segments, Some(right), top.width)),
            top,
        );

        let hint = if self.busy {
            Some(Span::styled("working…", Style::default().fg(Color::Yellow)))
        } else if bottom.width >= 58 {
            Some(Span::styled(
                "Enter run · / commands",
                Style::default().fg(FAINT),
            ))
        } else {
            None
        };
        frame.render_widget(
            Paragraph::new(compose_bottom(&self.input, self.cursor, bottom.width, hint)),
            bottom,
        );

        if self.modal.is_none() {
            let before: String = self.input.chars().take(self.cursor).collect();
            let x = bottom.x + 5 + UnicodeWidthStr::width(before.as_str()) as u16;
            let max = bottom.x + bottom.width.saturating_sub(2);
            frame.set_cursor_position(Position::new(x.min(max), bottom.y));
        }
    }

    /// What the console is waiting on, in omp's working-row shape: spinner, elapsed, label.
    fn activity(&mut self) -> Option<(String, Instant)> {
        let active = if self.busy {
            Some("working".to_owned())
        } else if self.status.running > 0 {
            Some(format!(
                "the agent is working · {} running",
                self.status.running
            ))
        } else if self.status.queued > 0 {
            Some(format!("{} ticket(s) queued", self.status.queued))
        } else if !self.pending.is_empty() {
            Some("waiting for the agent's decision".to_owned())
        } else if self
            .tracked
            .values()
            .any(|tracked| tracked.stage == "working")
        {
            Some("the agent is working".to_owned())
        } else {
            None
        };
        match active {
            Some(label) => {
                let since = *self.work_since.get_or_insert_with(Instant::now);
                Some((label, since))
            }
            None => {
                self.work_since = None;
                None
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let alert = self.alert_text(area.width);
        let alert_height = u16::from(alert.is_some());
        let working = self.activity();
        // The palette owns the rows above the composer while it is open, spinner included.
        let palette_open = self.palette().is_some();
        let working_height = if palette_open {
            0
        } else {
            u16::from(working.is_some())
        };
        let [alert_area, body, working_area, top, bottom] = Layout::vertical([
            Constraint::Length(alert_height),
            Constraint::Min(3),
            Constraint::Length(working_height),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        if let Some((label, since)) = working.filter(|_| !palette_open) {
            draw_working(frame, working_area, &label, since);
        }

        if let Some(text) = alert {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" ! ", Style::default().fg(RED)),
                    Span::styled(text, Style::default().fg(TIP)),
                ])),
                alert_area,
            );
        }

        let height = body.height.max(1);
        let total = self.lines.len() as u16;
        let max_scroll = total.saturating_sub(height);
        if self.follow {
            self.scroll = max_scroll;
        } else {
            self.scroll = self.scroll.min(max_scroll);
            if self.scroll >= max_scroll {
                self.follow = true;
                self.unread = 0;
            }
        }
        if self.lines.is_empty() && !self.busy {
            draw_splash(
                frame,
                body,
                &self.status,
                self.started.elapsed(),
                self.store.current().is_none() && !self.status.account,
            );
        } else {
            frame.render_widget(
                Paragraph::new(self.lines.clone())
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0)),
                body,
            );
        }

        self.draw_composer(frame, top, bottom);

        // The palette floats above the composer: painting it after the composer lets it cover
        // the working row instead of sharing its cells.
        if let Some(matches) = self.palette() {
            let room = top.y.saturating_sub(area.y) as usize;
            let shown = matches.len().min(8).min(room.max(1));
            let selected = self.selection.min(matches.len().saturating_sub(1));
            // Keep the selection in view when the list is longer than its window (omp's list scrolls).
            let offset = selected.saturating_sub(shown.saturating_sub(1));
            let width = area.width.saturating_sub(4).clamp(24, 82).min(area.width);
            let height = shown as u16;
            let rect = Rect {
                x: area.x + 2,
                y: top.y.saturating_sub(height),
                width,
                height,
            };
            frame.render_widget(Clear, rect);
            let token = commands::query(&self.input);
            let lines: Vec<L> = matches
                .iter()
                .enumerate()
                .skip(offset)
                .take(shown)
                .map(|(index, (name, description))| {
                    let active = index == selected;
                    let word = name.split_whitespace().next().unwrap_or("");
                    let hit = commands::fuzzy(word, &token)
                        .map(|(_, indices)| indices)
                        .unwrap_or_default();
                    let mut spans = vec![Span::styled(
                        if active { "❯ " } else { "  " },
                        Style::default().fg(CYAN),
                    )];
                    // The command word highlights the letters the fuzzy filter matched.
                    for (position, ch) in name.chars().enumerate() {
                        let matched = hit.contains(&position);
                        let style = if matched {
                            Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                        } else if active {
                            Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(MUTED)
                        };
                        spans.push(Span::styled(ch.to_string(), style));
                    }
                    let pad = 18usize.saturating_sub(UnicodeWidthStr::width(*name));
                    spans.push(Span::raw(" ".repeat(pad.max(1))));
                    spans.push(Span::styled(
                        *description,
                        Style::default().fg(if active { MUTED } else { FAINT }),
                    ));
                    Line::from(spans)
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), rect);
        }

        if let Some(modal) = &self.modal {
            draw_modal(frame, area, modal, &self.store, &self.history);
        }
    }
}

fn draw_modal(frame: &mut Frame, area: Rect, modal: &Modal, store: &Store, history: &[String]) {
    match modal {
        Modal::LoginList { selected } => draw_login_list(frame, area, *selected),
        Modal::Login(form) => draw_login(frame, area, form),
        Modal::OAuth(state) => draw_oauth(frame, area, state),
        Modal::Model(pick) => draw_model(frame, area, pick, store),
        Modal::History { query, selected } => draw_history(frame, area, query, *selected, history),
        Modal::Keys => draw_keys(frame, area),
    }
}

/// The shortcuts, as omp lists its hotkeys.
const KEYS: &[(&str, &str)] = &[
    ("Enter", "run the line; bare text goes to the agent"),
    ("Tab", "complete the selected command"),
    ("Ctrl+P", "open the command palette"),
    ("Ctrl+R", "search the history"),
    ("Ctrl+A / Ctrl+E", "start / end of the line"),
    ("Alt+B / Alt+F", "move by word"),
    ("Ctrl+W", "delete the word before"),
    ("Ctrl+U / Ctrl+K", "delete to the start / end"),
    ("Ctrl+Y", "paste the last deleted text"),
    ("↑ ↓", "pick in the palette, or walk the history"),
    ("PgUp / PgDn", "scroll the transcript"),
    ("Ctrl+L", "clear the transcript"),
    ("Ctrl+C", "quit"),
];

fn draw_keys(frame: &mut Frame, area: Rect) {
    let width = area.width.saturating_sub(8).clamp(40, 66).min(area.width);
    let height = (KEYS.len() as u16 + 3).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(" keys ", Style::default().fg(MUTED)));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let mut lines: Vec<L> = KEYS
        .iter()
        .map(|(key, description)| {
            Line::from(vec![
                Span::styled(format!(" {key:<18}"), Style::default().fg(CYAN)),
                Span::styled(*description, Style::default().fg(DIM)),
            ])
        })
        .collect();
    lines.push(Line::from(Span::styled(
        " any key closes",
        Style::default().fg(DIM),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_history(frame: &mut Frame, area: Rect, query: &str, selected: usize, history: &[String]) {
    let matches = matching_history(history, query);
    let shown = matches.len().min(8);
    let width = area.width.saturating_sub(8).clamp(40, 78).min(area.width);
    let height = (shown as u16 + 3).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            format!(" history · {} ", if query.is_empty() { "…" } else { query }),
            Style::default().fg(MUTED),
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<L> = Vec::new();
    if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            " no matching line",
            Style::default().fg(DIM),
        )));
    }
    let offset = selected.saturating_sub(shown.saturating_sub(1));
    for (index, line) in matches.iter().enumerate().skip(offset).take(shown) {
        let active = index == selected;
        lines.push(Line::from(vec![
            Span::styled(if active { "▶ " } else { "  " }, Style::default().fg(CYAN)),
            Span::styled(
                format::clip(line, width.saturating_sub(6) as usize),
                if active {
                    Style::default().fg(TEXT)
                } else {
                    Style::default().fg(DIM)
                },
            ),
        ]));
    }
    lines.push(Line::from(Span::styled(
        " Enter insert · Esc cancel",
        Style::default().fg(DIM),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Newest first; a fuzzy subsequence keeps readline's Ctrl+R feeling without a second index.
fn matching_history<'a>(history: &'a [String], query: &str) -> Vec<&'a String> {
    let needle = query.to_ascii_lowercase();
    history
        .iter()
        .rev()
        .filter(|line| {
            if needle.is_empty() {
                return true;
            }
            let lowered = line.to_ascii_lowercase();
            let mut chars = lowered.chars();
            needle.chars().all(|want| chars.any(|have| have == want))
        })
        .collect()
}

fn draw_login(frame: &mut Frame, area: Rect, form: &LoginForm) {
    let width = area.width.saturating_sub(8).clamp(44, 78).min(area.width);
    let height = (9 + u16::from(form.error.is_some())).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(" login ", Style::default().fg(MUTED)));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let key_text = "•".repeat(form.key.chars().count());
    let rows = [
        (
            "provider",
            provider::PRESETS[form.preset].label.to_owned(),
            true,
        ),
        ("id", form.id.clone(), false),
        ("base", form.base.clone(), false),
        ("model", form.model.clone(), false),
        ("api", form.api.clone(), true),
        ("key", key_text, false),
    ];
    let available = (inner.width as usize).saturating_sub(12);
    let mut lines: Vec<L> = rows
        .iter()
        .enumerate()
        .map(|(index, (label, value, cycle))| {
            let active = index == form.field;
            let mut spans = vec![
                Span::styled(if active { "›" } else { " " }, Style::default().fg(CYAN)),
                Span::styled(
                    format!(" {:<9}", label),
                    Style::default().fg(if active { CYAN } else { DIM }),
                ),
                Span::styled(
                    clip_text(value, available),
                    if active {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ];
            if *cycle {
                spans.push(Span::styled("  ← →", Style::default().fg(DIM)));
            }
            Line::from(spans)
        })
        .collect();
    if let Some(error) = &form.error {
        lines.push(Line::from(Span::styled(
            format!(" {error}"),
            Style::default().fg(RED),
        )));
    }
    lines.push(Line::from(Span::styled(
        " Tab next · ←→ move/cycle · Enter save · Esc cancel",
        Style::default().fg(DIM),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_model(frame: &mut Frame, area: Rect, pick: &ModelPick, store: &Store) {
    let rows = store.providers.len() + 1;
    let width = area.width.saturating_sub(8).clamp(40, 72).min(area.width);
    let height = (rows as u16 + 3).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(" model ", Style::default().fg(MUTED)));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<L> = Vec::new();
    for (index, item) in store.providers.iter().enumerate() {
        let active = index == pick.selected;
        let current = item.id == store.current;
        lines.push(Line::from(vec![
            Span::styled(if active { "▶ " } else { "  " }, Style::default().fg(CYAN)),
            Span::styled(
                format!("{}/{}", item.id, item.model),
                if active {
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                if current { "  current" } else { "" },
                Style::default().fg(DIM),
            ),
        ]));
    }
    let add = pick.selected >= store.providers.len();
    lines.push(Line::from(vec![
        Span::styled(if add { "▶ " } else { "  " }, Style::default().fg(CYAN)),
        Span::styled(
            "＋ add provider…",
            if add {
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(DIM)
            },
        ),
    ]));
    lines.push(Line::from(Span::styled(
        " Enter select · Esc cancel",
        Style::default().fg(DIM),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_login_list(frame: &mut Frame, area: Rect, selected: usize) {
    let width = area.width.saturating_sub(8).clamp(44, 72).min(area.width);
    let height = (LOGIN_METHODS.len() as u16 + 3).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(" login ", Style::default().fg(MUTED)));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<L> = Vec::new();
    for (index, (label, description)) in LOGIN_METHODS.iter().enumerate() {
        let active = index == selected;
        lines.push(Line::from(vec![
            Span::styled(if active { "▶ " } else { "  " }, Style::default().fg(CYAN)),
            Span::styled(
                format!("{label:<20}"),
                if active {
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(*description, Style::default().fg(DIM)),
        ]));
    }
    lines.push(Line::from(Span::styled(
        " Enter select · Esc cancel",
        Style::default().fg(DIM),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_oauth(frame: &mut Frame, area: Rect, state: &OAuthState) {
    let width = area.width.saturating_sub(8).clamp(48, 84).min(area.width);
    let height = 11.min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            " deepseek account ",
            Style::default().fg(MUTED),
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let room = (inner.width as usize).saturating_sub(4);
    // Wrapped, never truncated: the whole authorize URL has to stay selectable.
    let url = if state.url.is_empty() {
        "…".to_owned()
    } else {
        state.url.clone()
    };
    let paste = {
        let count = state.paste.chars().count();
        if count > room {
            let tail: String = state.paste.chars().skip(count - room).collect();
            format!("…{tail}")
        } else {
            state.paste.clone()
        }
    };
    let lines: Vec<L> = vec![
        Line::from(Span::styled(
            "Open this URL and approve (Ctrl+O opens it):",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(url, Style::default().fg(CYAN))),
        Line::from(Span::styled(
            clip_text(&state.status, room),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "or paste the callback URL:",
            Style::default().fg(DIM),
        )),
        Line::from(vec![
            Span::styled("› ", Style::default().fg(CYAN)),
            Span::styled(paste, Style::default()),
        ]),
        Line::from(Span::styled(
            " Enter finish · Esc cancel",
            Style::default().fg(DIM),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn clip_error(text: &str) -> String {
    format::clip(text, 70)
}

/// omp's working row: a braille spinner, the elapsed time, and what is being waited on.
fn draw_working(frame: &mut Frame, area: Rect, label: &str, since: Instant) {
    if area.height == 0 {
        return;
    }
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let elapsed = since.elapsed();
    // 80 ms a frame, the cadence omp's loader uses.
    let frame_index = (elapsed.as_millis() / 80) as usize % FRAMES.len();
    let seconds = elapsed.as_secs();
    let line = Line::from(vec![
        Span::styled(
            format!(" {}", FRAMES[frame_index]),
            Style::default().fg(CYAN),
        ),
        Span::styled(format!(" {seconds}s"), Style::default().fg(FAINT)),
        Span::styled(" · ", Style::default().fg(FAINT)),
        Span::styled(label.to_owned(), Style::default().fg(TIP)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// One structural line about a ticket the console opened: a stage, a report, or an end.
fn ticket_line(text: &str) -> L {
    Line::from(vec![
        Span::styled("  ◇ ", Style::default().fg(FAINT)),
        Span::styled("ticket ", Style::default().fg(FAINT)),
        Span::styled(text.to_owned(), Style::default().fg(MUTED)),
    ])
}

/// One red line for something the console refused or could not do.
fn warn_line(text: &str) -> L {
    Line::from(Span::styled(format!("! {text}"), Style::default().fg(RED)))
}

fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn insert_char(text: &mut String, cursor: usize, ch: char) {
    let byte = char_to_byte(text, cursor);
    text.insert(byte, ch);
}

fn remove_before(text: &mut String, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let byte = char_to_byte(text, cursor - 1);
    text.remove(byte);
    cursor - 1
}

fn remove_at(text: &mut String, cursor: usize) -> usize {
    if cursor >= text.chars().count() {
        return cursor;
    }
    let byte = char_to_byte(text, cursor);
    text.remove(byte);
    cursor
}

fn clip_text(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn compose_top(segments: Vec<Span<'static>>, right: Option<Span<'static>>, width: u16) -> L {
    let width = width as usize;
    let right_w = right.as_ref().map(|span| span.width()).unwrap_or(0);
    let tail = if right.is_some() { right_w + 2 } else { 0 };
    let mut spans = vec![Span::styled("╭─", Style::default().fg(CYAN))];
    let mut used = 2usize;
    let limit = width.saturating_sub(tail + 2);
    for (index, segment) in segments.into_iter().enumerate() {
        let extra = if index == 0 { 1 } else { 3 };
        if used + extra + segment.width() > limit {
            break;
        }
        if index == 0 {
            spans.push(Span::styled(" ", Style::default().fg(CYAN)));
        } else {
            spans.push(Span::styled(" · ", Style::default().fg(FAINT)));
        }
        used += extra + segment.width();
        spans.push(segment);
    }
    let trailing = usize::from(used > 2);
    let fill = width.saturating_sub(used + trailing + tail + 1);
    if trailing == 1 {
        spans.push(Span::styled(" ", Style::default()));
    }
    spans.push(Span::styled("─".repeat(fill), Style::default().fg(CYAN)));
    if let Some(right) = right {
        spans.push(Span::styled(" ", Style::default()));
        spans.push(right);
        spans.push(Span::styled(" ", Style::default()));
    }
    spans.push(Span::styled("╮", Style::default().fg(CYAN)));
    Line::from(spans)
}

fn compose_bottom(input: &str, _cursor: usize, width: u16, hint: Option<Span<'static>>) -> L {
    let width = width as usize;
    let hint_w = hint.as_ref().map(|span| span.width()).unwrap_or(0);
    let tail = if hint.is_some() { hint_w + 2 } else { 0 };
    let room = width.saturating_sub(5 + tail + 1 + 1);
    let empty = input.is_empty();
    let (text, style) = if empty {
        (
            "type an instruction, or / for commands"
                .chars()
                .take(room)
                .collect::<String>(),
            Style::default().fg(FAINT),
        )
    } else {
        let mut text = String::new();
        let mut used = 0;
        for ch in input.chars() {
            let size = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + size > room {
                break;
            }
            text.push(ch);
            used += size;
        }
        (text, Style::default().fg(TEXT))
    };
    let text_w = UnicodeWidthStr::width(text.as_str());
    let fill = width.saturating_sub(5 + text_w + 1 + tail + 1);
    let mut spans = vec![
        Span::styled("╰─ ", Style::default().fg(CYAN)),
        Span::styled("› ", Style::default().fg(CYAN)),
        Span::styled(text, style),
        Span::styled(" ", Style::default()),
        Span::styled("─".repeat(fill), Style::default().fg(CYAN)),
    ];
    if let Some(hint) = hint {
        spans.push(Span::styled(" ", Style::default()));
        spans.push(hint);
        spans.push(Span::styled(" ", Style::default()));
    }
    spans.push(Span::styled("╯", Style::default().fg(CYAN)));
    Line::from(spans)
}

fn draw_splash(
    frame: &mut Frame,
    area: Rect,
    status: &Status,
    elapsed: Duration,
    provider_missing: bool,
) {
    let mut lines: Vec<L> = Vec::new();
    if area.width >= 54 && area.height >= 14 {
        for (row, text) in LOGO.iter().enumerate() {
            lines.push(Line::from(Span::styled(
                *text,
                Style::default().fg(LOGO_COLORS[row]),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "booth",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "a terminal console for dsh-superagent",
        Style::default().fg(MUTED),
    )));
    lines.push(Line::from(Span::styled(
        status.base.clone(),
        Style::default().fg(FAINT),
    )));
    if provider_missing {
        lines.push(Line::from(Span::styled(
            "no model provider yet · /login",
            Style::default().fg(TIP),
        )));
    }
    lines.push(Line::from(""));
    let tip = TIPS[(elapsed.as_secs() / 10) as usize % TIPS.len()];
    lines.push(Line::from(vec![
        Span::styled("Tip: ", Style::default().fg(TIP)),
        Span::styled(tip, Style::default().fg(MUTED)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "type an instruction, or / for commands",
        Style::default().fg(FAINT),
    )));

    let height = (lines.len() as u16).min(area.height);
    let y = area.y + area.height.saturating_sub(height) / 3;
    let rect = Rect {
        x: area.x,
        y,
        width: area.width,
        height,
    };
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_search_is_newest_first_and_fuzzy() {
        let history = vec![
            "/status".to_owned(),
            "/keys".to_owned(),
            "/tickets --state person".to_owned(),
        ];
        let found: Vec<&str> = matching_history(&history, "ks")
            .iter()
            .map(|line| line.as_str())
            .collect();
        assert_eq!(found, vec!["/tickets --state person", "/keys"]);
        assert_eq!(matching_history(&history, "zzz").len(), 0);
        assert_eq!(matching_history(&history, "").len(), 3);
    }
}

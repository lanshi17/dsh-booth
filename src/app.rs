//! The terminal UI: one transcript, one input line, and a palette that filters as you type.

use crate::api::Api;
use crate::commands;
use crate::format::{self, L};
use crate::intake;
use crate::oauth;
use crate::provider::{self, Catalog};
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
use std::io::{self, IsTerminal, Stdout, Write};
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

/// One ticket poll: the open rows plus what a tracked ticket's detail said, the total behind the
/// open page, and the person count when the page was too short to carry every open ticket.
///
/// The page is a window (`limit=50`), so its length is not the number of open tickets. Carrying
/// the total lets the header say 120 open while the page holds 50, and a tracked ticket missing
/// from the window is looked up by id instead of being called closed.
struct TicketPoll {
    rows: Vec<crate::model::TicketRow>,
    open_total: u32,
    person: Option<u32>,
}

enum Msg {
    Lines(Vec<L>),
    Error(String),
    Status(Status),
    Replies(Vec<intake::Reply>),
    Decisions(Vec<crate::model::Decision>),
    /// The service's model catalog, fetched with every poll so the view is never stale.
    Catalog(Box<Catalog>),
    Tickets(TicketPoll),
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
    /// The name of the first health entry in error, so the alert can say which part is failing.
    first_error: Option<String>,
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
            first_error: None,
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
            first_error: state
                .health
                .iter()
                .find(|(_, health)| health.state == "error")
                .map(|(name, health)| match health.code.as_deref() {
                    Some(code) => format!("{name} ({code})"),
                    None => name.clone(),
                }),
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

/// One instruction the console sent and has not seen judged, with what it has already said.
struct Awaiting {
    sent: Instant,
    /// The 20-second notice has been shown.
    warned: bool,
    /// The 10-minute explanation has been shown.
    stale: bool,
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
    /// The service's model catalog, read from `/models`: DSH's own answer, not a booth list.
    catalog: Catalog,
    modal: Option<Modal>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Agent messages already shown in this run, so a poll cannot show one twice.
    seen: std::collections::HashSet<String>,
    /// Trigger ids of the instructions this console sent and has not seen judged yet, with when
    /// they were sent and what the console has already said about the wait.
    pending: std::collections::HashMap<String, Awaiting>,
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
    /// Where the outbox reader stopped; shared with the poll thread.
    tail: Arc<std::sync::Mutex<intake::Tail>>,
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

        let history = config.as_ref().map(intake::history).unwrap_or_default();
        let continues = config
            .as_ref()
            .and_then(intake::follow)
            .map(|(ticket, thread)| Follow { thread, ticket });
        // A restarted console starts with an empty transcript: skip past what the outbox already
        // holds instead of reading and parsing the history, so startup does not grow with the
        // deployment's age. The poll below then sees only what arrives from now on.
        let tail = Arc::new(std::sync::Mutex::new(
            config
                .as_ref()
                .map(intake::Tail::at_end)
                .unwrap_or_default(),
        ));
        Self {
            api,
            config,
            catalog: Catalog::default(),
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
            seen: std::collections::HashSet::new(),
            pending: std::collections::HashMap::new(),
            tracked: std::collections::HashMap::new(),
            continues,
            replied: std::collections::HashSet::new(),
            polled: Instant::now(),
            polling: Arc::new(AtomicBool::new(false)),
            kill: String::new(),
            unread: 0,
            tail,
            work_since: None,
            open: 0,
            person: 0,
        }
    }

    fn bootstrap(&mut self) {
        if let Some(follow) = &self.continues {
            let short = short_id(&follow.ticket);
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

    /// Drains every message a poll thread has posted.
    fn drain(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.apply(message);
        }
    }

    /// Handles one message: the whole of what a poll result does to the console.
    ///
    /// Split from `drain` so a test can feed one poll result without a terminal.
    fn apply(&mut self, message: Msg) {
        {
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
                Msg::Catalog(catalog) => self.catalog = *catalog,
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
                Msg::Tickets(poll) => self.show_tickets(poll),
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
                    // The terminal wraps a long authorize URL; keep an unwrapped copy the
                    // operator can open or copy without fighting the line breaks.
                    if let Some(config) = &self.config {
                        let path = config.state_dir.join("oauth-url.txt");
                        let _ = std::fs::create_dir_all(&config.state_dir);
                        let _ = std::fs::write(&path, &attempt.url);
                    }
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
                            // The account is the profile's own default; an override left behind
                            // would keep the agent on a provider that may no longer exist.
                            match provider::forget_default(cfg) {
                                Ok(true) => self.push(Line::from(Span::styled(
                                    "the account is the default model again",
                                    Style::default().fg(FAINT),
                                ))),
                                Ok(false) => {}
                                Err(error) => self.push(warn_line(&error)),
                            }
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
    fn show_tickets(&mut self, poll: TicketPoll) {
        let rows = poll.rows;
        // The page is a window: its length is not the number of open tickets.
        self.open = poll.open_total;
        self.person = poll.person.unwrap_or_else(|| {
            rows.iter()
                .filter(|row| row.state == "open" && row.stage == "person")
                .count()
                .try_into()
                .unwrap_or(u32::MAX)
        });
        let by_id: std::collections::HashMap<&str, &crate::model::TicketRow> =
            rows.iter().map(|row| (row.id.as_str(), row)).collect();
        let replied = self.replied.clone();
        let mut lines: Vec<L> = Vec::new();
        let mut done: Vec<String> = Vec::new();
        for (id, tracked) in self.tracked.iter_mut() {
            let short = short_id(id);
            // A ticket the poll could not resolve (a failed lookup, say) is not a closure:
            // keep following it and ask again on the next poll.
            let Some(row) = by_id.get(id.as_str()) else {
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
        let reason = format::reason(&decision.reason);
        let advice = match (decision.outcome.as_str(), reason.as_str()) {
            ("unavailable", "timeout") => {
                Some("the judgement ran past the Eval timeout · the panel's Eval block raises it")
            }
            ("unavailable", "noAdapter") => Some(
                "the session names a provider that is gone · `booth reset-sessions` moves it aside",
            ),
            ("unavailable", _) => {
                Some("a model failure, not a judgement · /status and the panel show it")
            }
            _ => None,
        };
        self.push(Line::from(vec![
            Span::styled("  ◇ ", Style::default().fg(FAINT)),
            Span::styled(label.to_owned(), Style::default().fg(FAINT)),
            Span::styled(format!("  {reason}"), Style::default().fg(MUTED)),
        ]));
        if let Some(advice) = advice {
            self.push(Line::from(Span::styled(
                format!("    {advice}"),
                Style::default().fg(FAINT),
            )));
        }
        if decision.outcome == "unavailable" {
            if let Some(trigger) = decision.trigger_ids.first() {
                let short = trigger.strip_prefix("local-").unwrap_or(trigger);
                self.push(Line::from(Span::styled(
                    format!("    /requeue {short} puts it back in the agent's queue"),
                    Style::default().fg(FAINT),
                )));
            }
        }
    }

    /// Shows one message the agent wrote back: an acknowledgement, a reply, or a notice.
    fn push_agent(&mut self, reply: intake::Reply) {
        // Only a reply or a notice reports on the work; an acknowledgement only says the agent
        // took the instruction up. Counting it as the report would hide the ticket's summary
        // whenever the worker later ends the ticket without writing one.
        if reply.kind != "acknowledgement" {
            if let Some(ticket) = &reply.ticket {
                self.replied.insert(ticket.clone());
            }
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
                    format!("  {}", short_id(ticket)),
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
                        Span::styled(short_id(&follow.ticket), Style::default().fg(CYAN)),
                        Span::styled("  · /new starts a new ticket", Style::default().fg(FAINT)),
                    ])],
                    None => {
                        // A follow-up fans out to its ticket without a decision, so only a new
                        // instruction is awaited as one.
                        self.pending.insert(
                            format!("local-{id}"),
                            Awaiting {
                                sent: Instant::now(),
                                warned: false,
                                stale: false,
                            },
                        );
                        vec![Line::from(vec![
                            Span::styled("  ↑ ", Style::default().fg(GREEN)),
                            Span::styled("sent to the agent", Style::default().fg(GREEN)),
                            Span::styled(
                                format!("  {} · picks it up within a second", short_id(&id)),
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
    ///
    /// Each line is said once. The 20-second notice and the 10-minute explanation are states of
    /// the wait, not a countdown: repeating them every poll would push everything else out of the
    /// transcript while a slow assessment runs.
    fn check_pending(&mut self) {
        let now = Instant::now();
        let mut lines = Vec::new();
        for entry in self.pending.values_mut() {
            let waited = now.duration_since(entry.sent);
            if !entry.warned && waited > Duration::from_secs(20) {
                entry.warned = true;
                lines.push(Line::from(Span::styled(
                    "  ⋯ no decision yet · /sources shows whether the source is polling",
                    Style::default().fg(FAINT),
                )));
            } else if entry.warned && !entry.stale && waited > Duration::from_secs(600) {
                // An assessment can legitimately take minutes: the Eval timeout is 120 s and the
                // model may be queued behind another turn. Keep waiting instead of declaring
                // failure, and say what a long wait usually means.
                entry.stale = true;
                lines.push(warn_line(
                    "10 minutes without a judgement · /status shows whether a model failure stopped it; \
                     a late answer still appears here",
                ));
            }
        }
        self.push_lines(lines);
    }

    fn refresh_status(&self) {
        if self.polling.swap(true, Ordering::SeqCst) {
            return;
        }
        let polling = self.polling.clone();
        let api = self.api.clone();
        let tx = self.tx.clone();
        let config = self.config.clone();
        // Keep asking for judgements while any sent instruction is unresolved.
        let pending = !self.pending.is_empty();
        let tracked: Vec<String> = self.tracked.keys().cloned().collect();
        let polled = self.api.clone();
        let tail = self.tail.clone();
        std::thread::spawn(move || {
            if pending {
                if let Ok(view) = polled.decisions() {
                    let _ = tx.send(Msg::Decisions(view.items));
                }
            }
            // Replies first: the ticket poll below then knows a report was already spoken. The
            // tail remembers where the last read stopped, so a poll costs the new bytes only.
            if let Some(config) = &config {
                if let Ok(mut tail) = tail.lock() {
                    let replies = intake::replies_from(config, &mut tail);
                    if !replies.is_empty() {
                        let _ = tx.send(Msg::Replies(replies));
                    }
                }
            }
            // Open tickets are the header's counts as well as the followed tickets' stages.
            let mut rows = Vec::new();
            let mut open_total = 0;
            let mut person = None;
            let mut fetched = false;
            if let Ok(page) = polled.tickets("open") {
                open_total = page.total;
                let complete = page.items.len() as u32 >= page.total;
                rows.extend(page.items);
                fetched = true;
                if !complete {
                    // The page carries only the first 50 of `total`; ask the overview for the
                    // exact number of tickets waiting for a person instead of counting the window.
                    if let Ok(view) = polled.overview() {
                        person = Some(view.person.len() as u32);
                    }
                }
                // A tracked ticket the page did not carry is not necessarily closed: the page is
                // a window. Ask for the ticket itself, and let only a definite answer end it.
                for id in &tracked {
                    if rows.iter().any(|row| &row.id == id) {
                        continue;
                    }
                    match polled.ticket(id) {
                        Ok(detail) => rows.push(row_from_detail(&detail)),
                        Err(crate::api::ApiError::Status(404, _)) => {
                            rows.push(crate::model::TicketRow {
                                id: id.clone(),
                                state: "ended".into(),
                                ..crate::model::TicketRow::default()
                            });
                        }
                        // A transport or protocol failure says nothing about the ticket; the
                        // next poll asks again rather than reporting a closure that did not happen.
                        Err(_) => {}
                    }
                }
            }
            if fetched {
                let _ = tx.send(Msg::Tickets(TicketPoll {
                    rows,
                    open_total,
                    person,
                }));
            }
            if let Ok(models) = polled.models() {
                let _ = tx.send(Msg::Catalog(Box::new(Catalog::parse(&models))));
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
            self.push_lines(intake::summary(&self.catalog));
            // Refresh the catalog from the service so the view is never a stale copy.
            self.refresh_status();
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
            .catalog
            .choices
            .iter()
            .position(|choice| {
                Some(&choice.provider) == self.catalog.default.as_ref().map(|(p, _)| p)
                    && Some(&choice.model) == self.catalog.default.as_ref().map(|(_, m)| m)
            })
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
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('y') {
            if !state.url.is_empty() {
                state.status = if copy_to_clipboard(&state.url) {
                    "the URL is on the clipboard".into()
                } else {
                    "could not copy; open oauth-url.txt from the state directory".into()
                };
            }
            self.modal = Some(Modal::OAuth(state));
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            if !state.url.is_empty() {
                let opened = std::process::Command::new("xdg-open")
                    .arg(&state.url)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                if opened.is_err() {
                    state.status =
                        "could not start a browser; open oauth-url.txt from the state directory"
                            .into();
                }
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
        let count = self.catalog.choices.len() + 1;
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Up => pick.selected = pick.selected.saturating_sub(1),
            KeyCode::Down => pick.selected = (pick.selected + 1).min(count.saturating_sub(1)),
            KeyCode::Enter => {
                if pick.selected >= self.catalog.choices.len() {
                    self.open_login("");
                    return;
                }
                let chosen = self.catalog.choices[pick.selected].clone();
                let Some(cfg) = self.config.clone() else {
                    return;
                };
                // The default lives in the profile's own layer, which is where the web panel
                // and DSH's own default-model plugin read it too.
                match provider::set_default(&cfg, &chosen.provider, &chosen.model) {
                    Ok(()) => {
                        self.catalog.default =
                            Some((chosen.provider.clone(), chosen.model.clone()));
                        self.push(Line::from(vec![
                            Span::styled("model ", Style::default().fg(GREEN)),
                            Span::styled(
                                format!("{}/{}", chosen.provider, chosen.model),
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
        // The profile layer registers the provider and names its key's credential ref; the key
        // itself goes into the shared credential store, where DSH's other keys live.
        match provider::add_endpoint(
            &cfg,
            &provider::Endpoint {
                id: id.clone(),
                display: id.clone(),
                base,
                api: form.api.clone(),
                model: model.clone(),
                key,
            },
        ) {
            Ok(()) => {
                self.modal = None;
                self.catalog.default = Some((id.clone(), model.clone()));
                self.push(Line::from(vec![
                    Span::styled("provider ", Style::default().fg(GREEN)),
                    Span::styled(format!("{id} · {model}"), Style::default()),
                ]));
                self.push(Line::from(Span::styled(
                    format!("key stored as {}", provider::key_ref(&id)),
                    Style::default().fg(FAINT),
                )));
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
            // `cargo install` can replace the binary under a running console; /proc/self/exe
            // then names a deleted file, so fall back to `booth` on PATH.
            let program = std::env::current_exe()
                .ok()
                .filter(|path| path.exists())
                .unwrap_or_else(|| std::path::PathBuf::from("booth"));
            let result = std::process::Command::new(program)
                .arg("restart")
                .output()
                .map_err(|error| format!("{error}; run `booth restart` yourself"));
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
        // The catalog is the profile's own answer, so this is the model the agent would run.
        if self.catalog.default.is_some() {
            self.catalog.label()
        } else if self.status.account {
            "deepseek account".into()
        } else {
            "no model".into()
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
        if let Some(entry) = &self.status.first_error {
            return Some(format!("{} is failing · /status shows it", entry));
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
                format!("↳ {}", short_id(&follow.ticket)),
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
        // Scroll by what the paragraph actually renders, not by structural lines: a long line
        // wraps into several rows, and counting it once would stop short of the newest row.
        let transcript = Paragraph::new(self.lines.clone()).wrap(Wrap { trim: false });
        let total = transcript.line_count(body.width).min(usize::from(u16::MAX)) as u16;
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
                self.catalog.default.is_none() && !self.status.account,
            );
        } else {
            frame.render_widget(transcript.scroll((self.scroll, 0)), body);
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
            draw_modal(frame, area, modal, &self.catalog, &self.history);
        }
    }
}

fn draw_modal(frame: &mut Frame, area: Rect, modal: &Modal, catalog: &Catalog, history: &[String]) {
    match modal {
        Modal::LoginList { selected } => draw_login_list(frame, area, *selected),
        Modal::Login(form) => draw_login(frame, area, form),
        Modal::OAuth(state) => draw_oauth(frame, area, state),
        Modal::Model(pick) => draw_model(frame, area, pick, catalog),
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

fn draw_model(frame: &mut Frame, area: Rect, pick: &ModelPick, catalog: &Catalog) {
    // The list is the service's own catalog; the last row is the way to add one.
    let shown = catalog.choices.len().min(9);
    let rows = shown + 1;
    let width = area.width.saturating_sub(8).clamp(40, 78).min(area.width);
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
    if catalog.choices.is_empty() {
        lines.push(Line::from(Span::styled(
            " the service lists no model — is it running?",
            Style::default().fg(DIM),
        )));
    }
    let offset = pick.selected.saturating_sub(shown.saturating_sub(1));
    for (index, choice) in catalog.choices.iter().enumerate().skip(offset).take(shown) {
        let active = index == pick.selected;
        let current =
            catalog.default.as_ref() == Some(&(choice.provider.clone(), choice.model.clone()));
        lines.push(Line::from(vec![
            Span::styled(if active { "▶ " } else { "  " }, Style::default().fg(CYAN)),
            Span::styled(
                choice.provider.clone(),
                if active {
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(MUTED)
                },
            ),
            Span::styled(" · ", Style::default().fg(FAINT)),
            Span::styled(choice.model.clone(), Style::default()),
            // The display name only adds something when it differs from the id.
            Span::styled(
                if choice.model_name == choice.model {
                    String::new()
                } else {
                    format!("  {}", choice.model_name)
                },
                Style::default().fg(DIM),
            ),
            Span::styled(
                if current { "  current" } else { "" },
                Style::default().fg(GREEN),
            ),
        ]));
    }
    let add = pick.selected >= catalog.choices.len();
    lines.push(Line::from(vec![
        Span::styled(if add { "▶ " } else { "  " }, Style::default().fg(CYAN)),
        Span::styled(
            "＋ add provider with an API key…",
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
    // Height follows the wrapped authorize URL: a fixed box cut the link's tail off.
    let inner_width = width.saturating_sub(2).max(1) as usize;
    let url_rows = wrapped_rows(&state.url, inner_width).max(1) as u16;
    let height = (8 + url_rows).min(area.height);
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
        Line::from(Span::styled(
            "the full URL is also in oauth-url.txt",
            Style::default().fg(FAINT),
        )),
        Line::from(Span::styled(
            "or paste the callback URL:",
            Style::default().fg(DIM),
        )),
        Line::from(vec![
            Span::styled("› ", Style::default().fg(CYAN)),
            Span::styled(paste, Style::default()),
        ]),
        Line::from(Span::styled(
            " Ctrl+O open · Ctrl+Y copy · Enter finish · Esc cancel",
            Style::default().fg(DIM),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// Copies through whichever clipboard tool the session has; false when none works.
fn copy_to_clipboard(text: &str) -> bool {
    let mut candidates: Vec<(&str, Vec<&str>)> = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        candidates.push(("wl-copy", Vec::new()));
    }
    if std::env::var_os("DISPLAY").is_some() {
        candidates.push(("xclip", vec!["-selection", "clipboard"]));
        candidates.push(("xsel", vec!["--clipboard", "--input"]));
    }
    // macOS: no display variable, pbcopy talks to the pasteboard directly.
    if cfg!(target_os = "macos") {
        candidates.push(("pbcopy", Vec::new()));
    }
    for (program, args) in candidates {
        let child = std::process::Command::new(program)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(stdin) = child.stdin.as_mut() {
                if stdin.write_all(text.as_bytes()).is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

/// How many terminal rows `text` needs at `width` columns, at least one.
fn wrapped_rows(text: &str, width: usize) -> usize {
    if text.is_empty() {
        return 0;
    }
    let width = width.max(1);
    text.split('\n')
        .map(|line| UnicodeWidthStr::width(line).div_ceil(width))
        .sum::<usize>()
        .max(1)
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

/// A ticket detail reduced to the row fields the poll and the transcript read. The list routes
/// omit the summary and the action of a ticket outside their page; the detail route has both.
fn row_from_detail(detail: &crate::model::TicketDetail) -> crate::model::TicketRow {
    crate::model::TicketRow {
        id: detail.id.clone(),
        title: detail.title.clone(),
        state: detail.state.clone(),
        stage: detail.stage.clone(),
        action: detail.action.clone(),
        note: detail.note.clone(),
        summary: detail.summary.clone(),
        directory: detail.directory.clone(),
        session_id: detail.session_id.clone(),
        created_at: detail.created_at.clone(),
        updated_at: detail.updated_at.clone(),
        strength: detail.strength.clone(),
        attention: detail.attention.clone(),
        actions: detail.actions.clone(),
        ..crate::model::TicketRow::default()
    }
}

/// The first eight characters of an id, for the one-line ticket markers. Counts characters,
/// not bytes: a byte slice panics when it lands inside one (SPEC defect A).
fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
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
    fn counts_wrapped_rows_for_the_oauth_box() {
        assert_eq!(wrapped_rows("", 10), 0);
        assert_eq!(wrapped_rows("short", 10), 1);
        assert_eq!(wrapped_rows(&"x".repeat(25), 10), 3);
        assert_eq!(wrapped_rows("ab\ncd", 10), 2);
    }

    #[test]
    fn a_short_id_never_panics_on_multibyte_text() {
        // SPEC scenario "非 ASCII 的 id 被截断时不 panic". The first input is longer than
        // eight characters so truncation actually happens; the second is exactly eight.
        assert_eq!(short_id("中文工单编号中文测试"), "中文工单编号中文");
        assert_eq!(short_id("中文工单编号中文"), "中文工单编号中文");
        assert_eq!(short_id("03b16192bb7f2ac0"), "03b16192");
        assert_eq!(short_id("abc"), "abc");
        assert_eq!(short_id(""), "");
    }

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

#[cfg(test)]
mod transcript_tests {
    use super::*;
    use crate::model::{Decision, TicketRow};
    use crate::testkit::FakeApi;
    use serde_json::json;

    /// A console around a temp state directory, with no terminal and no poll threads started.
    ///
    /// `App::new` reads the config from the environment, so the test sets it; nothing here draws
    /// or reads a key, which is the only part of the console that needs a tty.
    fn console(name: &str) -> (App, std::path::PathBuf, std::sync::MutexGuard<'static, ()>) {
        // The console reads its state directory from the environment.
        let guard = crate::testkit::env_lock();
        let dir =
            std::env::temp_dir().join(format!("booth-console-{name}-{}", crate::oauth::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOOTH_STATE_DIR", &dir);
        std::env::set_var("DSH_HOME", &dir);
        let api = FakeApi::start();
        (App::new(Api::new(&api.base)), dir, guard)
    }

    fn text(app: &App) -> String {
        app.lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn row(id: &str, stage: &str, action: Option<&str>) -> TicketRow {
        TicketRow {
            id: id.to_owned(),
            title: "a ticket".into(),
            state: "open".into(),
            stage: stage.to_owned(),
            action: action.map(str::to_owned),
            ..TicketRow::default()
        }
    }

    /// The poll a page holding exactly these rows stands for; the tests that drive
    /// `show_tickets` directly do not go through the service.
    fn tickets_poll(rows: Vec<TicketRow>) -> TicketPoll {
        let open_total = rows.iter().filter(|row| row.state == "open").count() as u32;
        TicketPoll {
            rows,
            open_total,
            person: None,
        }
    }

    #[test]
    fn a_ticket_stage_is_reported_once_per_change() {
        let (mut app, dir, _env) = console("stages");
        app.tracked.insert(
            "abcdef1234567890".into(),
            Tracked {
                stage: "new".into(),
            },
        );
        app.show_tickets(tickets_poll(vec![row("abcdef1234567890", "working", None)]));
        assert!(text(&app).contains("abcdef12 · working"), "{}", text(&app));

        // The same stage again says nothing new.
        let before = app.lines.len();
        app.show_tickets(tickets_poll(vec![row("abcdef1234567890", "working", None)]));
        assert_eq!(app.lines.len(), before, "a repeated stage is not repeated");

        // A stage with an action names the action.
        app.show_tickets(tickets_poll(vec![row(
            "abcdef1234567890",
            "person",
            Some("review"),
        )]));
        assert!(
            text(&app).contains("abcdef12 · needs a person · review"),
            "{}",
            text(&app)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_closed_ticket_stops_being_followed_and_releases_the_thread() {
        let (mut app, dir, _env) = console("closed");
        app.tracked.insert(
            "abcdef1234567890".into(),
            Tracked {
                stage: "working".into(),
            },
        );
        app.continues = Some(Follow {
            thread: "t-1".into(),
            ticket: "abcdef1234567890".into(),
        });
        app.replied.insert("abcdef1234567890".into());
        app.show_tickets(tickets_poll(vec![TicketRow {
            id: "abcdef1234567890".into(),
            state: "ended".into(),
            stage: "idle".into(),
            ..TicketRow::default()
        }]));
        assert!(text(&app).contains("abcdef12 · closed"), "{}", text(&app));
        // With a reply already shown, the summary is not printed twice.
        assert!(!text(&app).contains("report"), "{}", text(&app));
        assert!(app.tracked.is_empty(), "a closed ticket is not tracked");
        assert!(app.continues.is_none(), "the thread is released");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_silent_report_falls_back_to_the_summary() {
        let (mut app, dir, _env) = console("silent");
        app.tracked.insert(
            "abcdef1234567890".into(),
            Tracked {
                stage: "working".into(),
            },
        );
        app.show_tickets(tickets_poll(vec![TicketRow {
            id: "abcdef1234567890".into(),
            state: "ended".into(),
            stage: "idle".into(),
            summary: Some("the worker changed the README".into()),
            ..TicketRow::default()
        }]));
        let shown = text(&app);
        assert!(shown.contains("report"), "{shown}");
        assert!(shown.contains("the worker changed the README"), "{shown}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_acknowledgement_does_not_hide_the_silent_report() {
        // The normal accepted path: the agent acknowledges the instruction, then ends the ticket
        // with its report in the ticket but no reply. The summary is the only report shown, so the
        // acknowledgement must not stand in for it.
        let (mut app, dir, _env) = console("ack-silent");
        app.tracked.insert(
            "abcdef1234567890".into(),
            Tracked {
                stage: "working".into(),
            },
        );
        app.push_agent(intake::Reply {
            id: "a1".into(),
            kind: "acknowledgement".into(),
            text: "收到".into(),
            ticket: Some("abcdef1234567890".into()),
        });
        app.show_tickets(vec![TicketRow {
            id: "abcdef1234567890".into(),
            state: "ended".into(),
            stage: "idle".into(),
            summary: Some("the worker changed the README".into()),
            ..TicketRow::default()
        }]);
        let shown = text(&app);
        assert!(shown.contains("abcdef12 · report"), "{shown}");
        assert!(shown.contains("the worker changed the README"), "{shown}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_declined_instruction_shows_the_agents_sentence() {
        let (mut app, dir, _env) = console("declined");
        app.push_decision(&Decision {
            outcome: "ignore".into(),
            reason: json!([{ "code": "said", "values": { "text": "打招呼，没有具体改动请求" } }]),
            trigger_ids: vec!["local-1".into()],
            ..Decision::default()
        });
        let shown = text(&app);
        assert!(shown.contains("not taken"), "{shown}");
        assert!(shown.contains("打招呼，没有具体改动请求"), "{shown}");
        assert!(
            !shown.contains("said"),
            "the reason code is not shown: {shown}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unavailable_judgement_says_what_to_do_next() {
        let (mut app, dir, _env) = console("unavailable");
        app.push_decision(&Decision {
            outcome: "unavailable".into(),
            reason: json!([{ "code": "timeout" }]),
            trigger_ids: vec!["local-abc".into()],
            ..Decision::default()
        });
        let shown = text(&app);
        assert!(shown.contains("could not judge"), "{shown}");
        assert!(shown.contains("timeout"), "{shown}");
        // The advice names both the cause and the way back.
        assert!(shown.contains("Eval timeout"), "{shown}");
        assert!(shown.contains("/requeue abc"), "{shown}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_agent_reply_names_its_ticket_and_kind() {
        let (mut app, dir, _env) = console("reply");
        app.push_agent(intake::Reply {
            id: "m1".into(),
            kind: "acknowledgement".into(),
            text: "收到".into(),
            ticket: Some("abcdef1234567890".into()),
        });
        let shown = text(&app);
        assert!(shown.contains("accepted"), "{shown}");
        assert!(shown.contains("abcdef12"), "{shown}");
        assert!(shown.contains("收到"), "{shown}");
        assert!(
            !app.replied.contains("abcdef1234567890"),
            "an acknowledgement does not stand in for the report"
        );
        // A reply or a notice does report on the work, so no summary fallback follows it.
        app.push_agent(intake::Reply {
            id: "m2".into(),
            kind: "reply".into(),
            text: "done".into(),
            ticket: Some("abcdef1234567890".into()),
        });
        assert!(app.replied.contains("abcdef1234567890"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bare_text_is_written_to_the_inbox_and_a_command_is_not() {
        let (mut app, dir, _env) = console("submit");
        app.input = "把 README 的错字改掉".into();
        app.submit();
        let inbox = std::fs::read_to_string(dir.join("booth-inbox.jsonl")).unwrap();
        assert!(inbox.contains("把 README 的错字改掉"), "{inbox}");
        assert!(text(&app).contains("sent to the agent"), "{}", text(&app));

        // A slash line goes to the command grammar instead of the agent.
        app.input = "/nonexistent".into();
        app.submit();
        assert!(
            !std::fs::read_to_string(dir.join("booth-inbox.jsonl"))
                .unwrap()
                .contains("nonexistent"),
            "a command must not reach the agent"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_follow_up_continues_the_ticket_the_console_accepted() {
        let (mut app, dir, _env) = console("follow");
        app.continues = Some(Follow {
            thread: "t-1".into(),
            ticket: "abcdef1234567890".into(),
        });
        app.input = "改成另一种写法".into();
        app.submit();
        let inbox = std::fs::read_to_string(dir.join("booth-inbox.jsonl")).unwrap();
        assert!(inbox.contains("\"thread\":\"t-1\""), "{inbox}");
        assert!(
            text(&app).contains("follow-up to abcdef12"),
            "{}",
            text(&app)
        );
        // A follow-up fans out without a decision, so it is not awaited as one.
        assert!(
            app.pending.is_empty(),
            "a follow-up is not pending a judgement"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bootstrap_announces_the_ticket_it_resumes() {
        let (mut app, dir, _env) = console("bootstrap");
        app.continues = Some(Follow {
            thread: "t-1".into(),
            ticket: "abcdef1234567890".into(),
        });
        app.bootstrap();
        let shown = text(&app);
        assert!(shown.contains("continuing ticket abcdef12"), "{shown}");
        assert!(shown.contains("/new starts a new ticket"), "{shown}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn new_starts_a_new_ticket_and_forgets_the_follow_target() {
        let (mut app, dir, _env) = console("new");
        app.continues = Some(Follow {
            thread: "t-1".into(),
            ticket: "abcdef1234567890".into(),
        });
        app.input = "/new".into();
        app.submit();
        assert!(app.continues.is_none(), "/new releases the ticket");
        assert!(text(&app).contains("starts a new ticket"), "{}", text(&app));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_long_input_is_truncated_to_the_composer_width() {
        let line = compose_bottom(&"x".repeat(200), 0, 40, None);
        let width: usize = line
            .spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum();
        assert!(width <= 40, "the composer stays inside its width: {width}");
        // The rails and the prompt survive the truncation.
        let text = line.to_string();
        assert!(text.starts_with('╰') && text.ends_with('╯'), "{text}");
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    use crate::testkit::FakeApi;
    use serde_json::json;

    /// A console whose editor is driven directly; no terminal, no event loop.
    fn editor(name: &str) -> (App, std::path::PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = crate::testkit::env_lock();
        let dir =
            std::env::temp_dir().join(format!("booth-editor-{name}-{}", crate::oauth::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOOTH_STATE_DIR", &dir);
        std::env::set_var("DSH_HOME", &dir);
        let api = FakeApi::start();
        (App::new(Api::new(&api.base)), dir, guard)
    }

    /// The same console, with the fake service kept so a test can replace its routes.
    fn editor_with_api(
        name: &str,
    ) -> (
        App,
        std::path::PathBuf,
        FakeApi,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let guard = crate::testkit::env_lock();
        let dir =
            std::env::temp_dir().join(format!("booth-editor-{name}-{}", crate::oauth::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOOTH_STATE_DIR", &dir);
        std::env::set_var("DSH_HOME", &dir);
        let api = FakeApi::start();
        let app = App::new(Api::new(&api.base));
        (app, dir, api, guard)
    }

    /// Waits for the poll's ticket message, which is what these tests drive.
    fn next_tickets(app: &App) -> TicketPoll {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match app.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(Msg::Tickets(poll)) => return poll,
                Ok(_) => continue,
                Err(_) if Instant::now() < deadline => continue,
                Err(error) => panic!("the poll never sent tickets: {error}"),
            }
        }
    }

    fn rendered(app: &App) -> String {
        app.lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    fn alt(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT)
    }

    fn typed(app: &mut App, text: &str) {
        for ch in text.chars() {
            app.on_key(key(KeyCode::Char(ch)));
        }
    }

    #[test]
    fn typing_moves_the_cursor_and_editing_keeps_it_there() {
        let (mut app, dir, _env) = editor("typing");
        typed(&mut app, "hello");
        assert_eq!(app.input, "hello");
        assert_eq!(app.cursor, 5);

        app.on_key(key(KeyCode::Left));
        app.on_key(key(KeyCode::Left));
        app.on_key(key(KeyCode::Backspace));
        assert_eq!(app.input, "helo", "backspace deletes before the cursor");
        assert_eq!(app.cursor, 2);

        // Delete removes the character under the cursor: "helo" with the cursor on 'l'.
        app.on_key(key(KeyCode::Delete));
        assert_eq!(app.input, "heo");
        app.on_key(key(KeyCode::End));
        app.on_key(key(KeyCode::Delete));
        assert_eq!(app.input, "heo", "delete at the end changes nothing");

        app.on_key(key(KeyCode::Home));
        assert_eq!(app.cursor, 0);
        app.on_key(key(KeyCode::End));
        assert_eq!(app.cursor, app.input.chars().count());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_multibyte_line_edits_by_character() {
        let (mut app, dir, _env) = editor("multibyte");
        typed(&mut app, "工单");
        assert_eq!(app.cursor, 2, "the cursor counts characters");
        app.on_key(key(KeyCode::Backspace));
        assert_eq!(
            app.input, "工",
            "backspace removes one character, not one byte"
        );
        app.on_key(key(KeyCode::End));
        typed(&mut app, "作");
        assert_eq!(app.input, "工作");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn readline_motion_and_the_kill_ring() {
        let (mut app, dir, _env) = editor("readline");
        typed(&mut app, "one two three");

        app.on_key(ctrl('w'));
        assert_eq!(app.input, "one two ", "Ctrl+W deletes the word before");
        app.on_key(alt('b'));
        assert_eq!(app.cursor, 4, "Alt+B lands at the start of 'two'");
        app.on_key(alt('f'));
        assert_eq!(app.cursor, 8, "Alt+F lands after the word");
        app.on_key(ctrl('a'));
        assert_eq!(app.cursor, 0);
        app.on_key(ctrl('e'));
        assert_eq!(app.cursor, 8);

        // Ctrl+U deletes to the start and keeps it for Ctrl+Y.
        app.on_key(ctrl('u'));
        assert_eq!(app.input, "");
        app.on_key(ctrl('y'));
        assert_eq!(app.input, "one two ");

        app.on_key(ctrl('a'));
        app.on_key(ctrl('k'));
        assert_eq!(app.input, "", "Ctrl+K deletes to the end");
        app.on_key(ctrl('y'));
        assert_eq!(app.input, "one two ");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_palette_filters_selects_and_completes() {
        let (mut app, dir, _env) = editor("palette");
        typed(&mut app, "/tick");
        let matches = app.palette().expect("a slash line opens the palette");
        assert!(matches.iter().any(|(name, _)| name.starts_with("/ticket")));

        // The selection moves within the list, and Tab completes it.
        app.on_key(key(KeyCode::Down));
        assert_eq!(app.selection, 1);
        app.on_key(key(KeyCode::Up));
        assert_eq!(app.selection, 0);
        app.on_key(key(KeyCode::Tab));
        assert!(
            app.input.ends_with(' '),
            "completion leaves a space: {:?}",
            app.input
        );
        assert!(
            app.input.trim().len() > "/tick".len(),
            "the typed prefix is completed, not kept: {:?}",
            app.input
        );
        assert_eq!(app.cursor, app.input.chars().count());

        // Esc empties the line and closes the palette.
        app.on_key(key(KeyCode::Esc));
        assert!(app.input.is_empty());
        assert!(app.palette().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_line_without_a_slash_never_opens_the_palette() {
        let (mut app, dir, _env) = editor("palette-closed");
        typed(&mut app, "hi");
        assert!(app.palette().is_none());
        // Ctrl+P opens it only on an empty line, so a draft survives.
        app.on_key(ctrl('p'));
        assert_eq!(app.input, "hi");
        app.input.clear();
        app.cursor = 0;
        app.on_key(ctrl('p'));
        assert_eq!(app.input, "/");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn history_walks_backwards_and_forwards() {
        let (mut app, dir, _env) = editor("history");
        app.history = vec!["first".into(), "second".into()];
        app.on_key(key(KeyCode::Up));
        assert_eq!(app.input, "second", "the newest line comes first");
        app.on_key(key(KeyCode::Up));
        assert_eq!(app.input, "first");
        app.on_key(key(KeyCode::Up));
        assert_eq!(app.input, "first", "the oldest line is the end");
        app.on_key(key(KeyCode::Down));
        assert_eq!(app.input, "second");
        app.on_key(key(KeyCode::Down));
        assert_eq!(app.input, "", "past the newest is an empty line");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn new_output_counts_as_unread_only_while_scrolled_up() {
        let (mut app, dir, _env) = editor("unread");
        app.follow = true;
        app.push(Line::from("one"));
        assert_eq!(app.unread, 0, "following the bottom, nothing is unread");

        app.follow = false;
        app.push(Line::from("two"));
        app.push(Line::from("three"));
        assert_eq!(app.unread, 2);
        assert!(
            app.alert_text(80).unwrap().contains("2 new"),
            "{:?}",
            app.alert_text(80)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_transcript_is_capped_and_older_lines_fall_away() {
        let (mut app, dir, _env) = editor("cap");
        for index in 0..4200 {
            app.push(Line::from(format!("line {index}")));
        }
        assert_eq!(
            app.lines.len(),
            4000,
            "the transcript keeps the newest 4000 lines"
        );
        assert!(
            app.lines[0].to_string().contains("line 200"),
            "{}",
            app.lines[0]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn following_the_bottom_shows_the_newest_wrapped_rows() {
        use ratatui::backend::TestBackend;

        let (mut app, dir, _env) = editor("wrapped-scroll");
        app.follow = true;
        // Each logical line wraps across several terminal rows, so a scroll offset counted in
        // logical lines stops short of the true bottom and hides what just arrived.
        for index in 0..6 {
            app.push(Line::from(format!("line {index} {}", "x".repeat(120))));
        }
        app.push(Line::from("the newest line ends here TAIL"));
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let shown: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            shown.contains("TAIL"),
            "following the bottom keeps the newest wrapped rows on screen:\n{shown}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_status_reports_what_the_service_answered() {
        // The poll runs on its own thread and answers with a message; a mutant that makes the
        // method do nothing leaves the console on "connecting" forever.
        let (mut app, dir, _env) = editor("refresh");
        let base = app.api.base().to_owned();
        app.status = Status::connecting(&base);
        app.refresh_status();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match app.rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(Msg::Status(status)) => {
                    app.status = status;
                    break;
                }
                Ok(_) => continue,
                Err(_) if std::time::Instant::now() < deadline => continue,
                Err(error) => panic!("the poll never answered: {error}"),
            }
        }
        assert_eq!(app.status.runtime, "ready");
        assert_eq!(app.status.workers, 3, "the load comes from the service");
        assert!(
            app.status.repository.as_deref() == Some("o/r"),
            "the account names the repository: {:?}",
            app.status.repository
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_status_delivers_an_agent_reply_from_the_outbox() {
        let (app, dir, _env) = editor("refresh-reply");
        std::fs::write(
            dir.join("booth-outbox.jsonl"),
            "{\"id\":\"m1\",\"kind\":\"reply\",\"text\":\"done\",\"ticket\":\"t-1\"}\n",
        )
        .unwrap();
        app.refresh_status();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut delivered: Option<Vec<intake::Reply>> = None;
        while std::time::Instant::now() < deadline {
            match app.rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(Msg::Replies(replies)) => {
                    delivered = Some(replies);
                    break;
                }
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
        let replies = delivered.expect("the poll delivers what the agent wrote");
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].text, "done");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_slow_judgement_is_reported_once_at_each_threshold() {
        let (mut app, dir, _env) = editor("pending-slow");
        let rendered = |app: &App| {
            app.lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        app.pending.insert(
            "local-1".into(),
            Awaiting {
                sent: Instant::now() - Duration::from_secs(21),
                warned: false,
                stale: false,
            },
        );
        app.check_pending();
        assert!(rendered(&app).contains("no decision yet"));
        let after_warning = rendered(&app);
        app.check_pending();
        assert_eq!(
            rendered(&app),
            after_warning,
            "the 20-second line is said once"
        );

        // A judgement behind another turn may take minutes; the 10-minute line is the next and
        // last notice instead of the 20-second one starting over.
        if let Some(entry) = app.pending.get_mut("local-1") {
            entry.sent = Instant::now() - Duration::from_secs(601);
        }
        app.check_pending();
        assert!(rendered(&app).contains("10 minutes without a judgement"));
        let after_stale = rendered(&app);
        app.check_pending();
        app.check_pending();
        assert_eq!(
            rendered(&app),
            after_stale,
            "the 10-minute line is said once, not on every poll"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restarted_console_skips_the_outbox_it_found() {
        let _env = crate::testkit::env_lock();
        let dir = std::env::temp_dir().join(format!("booth-restart-{}", crate::oauth::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOOTH_STATE_DIR", &dir);
        std::env::set_var("DSH_HOME", &dir);
        // A transcript's worth of history the console did not show, because it was not running.
        let history: String = (0..20)
            .map(|index| {
                format!("{{\"id\":\"old-{index}\",\"kind\":\"reply\",\"text\":\"old {index}\"}}\n")
            })
            .collect();
        std::fs::write(dir.join("booth-outbox.jsonl"), history).unwrap();
        let api = FakeApi::start();
        let app = App::new(Api::new(&api.base));
        assert!(
            app.lines.is_empty(),
            "a restart starts with an empty transcript"
        );
        // The tail the polls share sits at the end of what was already there, so the first poll
        // has nothing to deliver; parsing the history would only cost startup time.
        let config = app
            .config
            .clone()
            .expect("the test set the state directory");
        let mut tail = app.tail.lock().unwrap();
        assert!(
            intake::replies_from(&config, &mut tail).is_empty(),
            "the history is not replayed"
        );
        // A message that arrives after the restart is still delivered.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join("booth-outbox.jsonl"))
            .unwrap();
        file.write_all(b"{\"id\":\"new-1\",\"kind\":\"reply\",\"text\":\"new\"}\n")
            .unwrap();
        let fresh = intake::replies_from(&config, &mut tail);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].id, "new-1");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_model_picker_lists_the_catalog_and_an_add_row() {
        let (mut app, dir, _env) = editor("model");
        // The list is whatever the service reported: no booth-side store to keep in step.
        app.catalog = Catalog::parse(&serde_json::json!({
            "default": { "provider": "a", "model": "m-a" },
            "groups": [
                { "id": "a", "name": "A", "models": [{ "id": "m-a" }] },
                { "id": "b", "name": "B", "models": [{ "id": "m-b" }] }
            ]
        }));
        app.modal = Some(Modal::Model(ModelPick { selected: 0 }));

        // Down past the last choice lands on the add row, which opens the login form.
        app.modal_key(key(KeyCode::Down));
        app.modal_key(key(KeyCode::Down));
        app.modal_key(key(KeyCode::Enter));
        assert!(
            matches!(app.modal, Some(Modal::Login(_))),
            "the add row opens the login form"
        );

        // The header shows the catalog's default, not a remembered one.
        app.modal = None;
        assert_eq!(app.model_label(), "a/m-a");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_login_form_value_follows_the_focused_field() {
        // `value()` is what every editor key writes into, so an arm that returns the wrong
        // field would edit the base URL while the operator edits the model.
        let mut form = LoginForm::new(0);
        // Each field's own text, copied out so the loop can borrow the form mutably.
        let id = form.id.clone();
        let base = form.base.clone();
        let model = form.model.clone();
        let expected = [
            (0usize, String::new()), // the preset row is a chooser, not a text field
            (1, id),
            (2, base),
            (3, model),
            (4, String::new()), // the api row is a chooser
            (5, String::new()), // the key starts empty
        ];
        for (field, want) in expected {
            form.focus(field);
            assert_eq!(form.value(), want, "field {field}");
        }
        assert_eq!(form.value(), "", "the key field starts empty");

        // The key field is masked on screen, but the value is the key itself.
        form.key = "sk-test".into();
        form.focus(5);
        assert_eq!(form.value(), "sk-test");
    }

    #[test]
    fn tab_walks_the_login_form_in_both_directions() {
        let (mut app, dir, _env) = editor("login-tab");
        app.modal = Some(Modal::Login(LoginForm::new(0)));
        let field = |app: &App| match &app.modal {
            Some(Modal::Login(form)) => form.field,
            _ => usize::MAX,
        };
        assert_eq!(field(&app), 1, "the id field is focused first");
        app.modal_key(key(KeyCode::Tab));
        assert_eq!(field(&app), 2);
        app.modal_key(key(KeyCode::BackTab));
        assert_eq!(field(&app), 1);
        // BackTab walks to the preset row, and from there wraps to the last field.
        app.modal_key(key(KeyCode::BackTab));
        assert_eq!(field(&app), 0, "the preset row comes before the id");
        app.modal_key(key(KeyCode::BackTab));
        assert_eq!(field(&app), 5, "below the first row is the last");
        app.modal_key(key(KeyCode::Tab));
        assert_eq!(field(&app), 0, "the last field wraps to the preset");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_login_form_edits_one_field_at_a_time() {
        let (mut app, dir, _env) = editor("login-form");
        app.modal = Some(Modal::Login(LoginForm::new(0)));
        // The id field is focused after the preset, so typing replaces nothing and appends.
        for ch in "-new".chars() {
            app.modal_key(key(KeyCode::Char(ch)));
        }
        match &app.modal {
            Some(Modal::Login(form)) => {
                // The form opens with the cursor at the start of the id field, so typing
                // inserts there rather than appending.
                assert!(form.id.starts_with("-new"), "{}", form.id);
                assert_eq!(form.cursor, 4);
                // Tab walks to the next field rather than inserting a tab.
                assert!(!form.id.contains('\t'));
            }
            _ => panic!("the login form is not open"),
        }
        app.modal_key(key(KeyCode::Esc));
        assert!(app.modal.is_none(), "Esc closes the form");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use crate::testkit::FakeApi;

    /// A console whose DSH home is a temp directory, so a write lands somewhere inspectable.
    fn console() -> (App, std::path::PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = crate::testkit::env_lock();
        let root = std::env::temp_dir().join(format!("booth-app-prov-{}", crate::oauth::uuid_v4()));
        std::fs::create_dir_all(root.join("profiles").join("superagent")).unwrap();
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::env::set_var("DSH_HOME", &root);
        std::env::set_var("BOOTH_STATE_DIR", root.join("state"));
        let api = FakeApi::start();
        (App::new(Api::new(&api.base)), root, guard)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn text(app: &App) -> String {
        app.lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn catalog() -> Catalog {
        Catalog::parse(&serde_json::json!({
            "default": { "provider": "a", "model": "m-a" },
            "groups": [
                { "id": "a", "name": "A", "models": [{ "id": "m-a" }, { "id": "m-a2" }] },
                { "id": "b", "name": "B", "models": [{ "id": "m-b" }] }
            ]
        }))
    }

    #[test]
    fn the_catalog_message_replaces_what_the_console_shows() {
        let (mut app, root, _env) = console();
        assert!(app.catalog.default.is_none());
        app.apply(Msg::Catalog(Box::new(catalog())));
        assert_eq!(app.model_label(), "a/m-a");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn choosing_a_model_writes_the_profiles_own_layer() {
        let (mut app, root, _env) = console();
        app.catalog = catalog();
        app.modal = Some(Modal::Model(ModelPick { selected: 2 }));

        app.modal_key(key(KeyCode::Enter));

        let patch = crate::service::Patch::read(&app.config.clone().unwrap()).unwrap();
        let entry = patch
            .get("agent-default-model")
            .expect("the default is written");
        assert_eq!(entry["config"]["provider"], "b");
        assert_eq!(entry["config"]["model"], "m-b");
        assert!(text(&app).contains("model b/m-b"), "{}", text(&app));
        // The console reflects the change without waiting for a restart.
        assert_eq!(app.model_label(), "b/m-b");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_api_key_login_writes_the_profile_and_a_credential_ref() {
        let (mut app, root, _env) = console();
        let mut form = LoginForm::new(0);
        form.id = "openrouter".into();
        form.base = "https://openrouter.ai/api/v1".into();
        form.model = "x/y".into();
        form.key = "sk-test".into();

        app.commit_login(form);

        let cfg = app.config.clone().unwrap();
        let patch = crate::service::Patch::read(&cfg).unwrap();
        let provider = &patch.get("llm-pi-ai").unwrap()["config"]["providers"]["openrouter"];
        assert_eq!(provider["apiKeyEnv"], "BOOTH_OPENROUTER_KEY");
        assert_eq!(
            patch.get("agent-default-model").unwrap()["config"]["provider"],
            "openrouter"
        );
        let credentials = std::fs::read_to_string(root.join(".credentials.yaml")).unwrap();
        assert!(credentials.contains("sk-test"), "{credentials}");
        // The key never reaches the transcript.
        assert!(!text(&app).contains("sk-test"), "{}", text(&app));
        assert!(
            text(&app).contains("BOOTH_OPENROUTER_KEY"),
            "{}",
            text(&app)
        );
        assert!(app.modal.is_none(), "a saved form closes");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_incomplete_login_form_stays_open_with_a_reason() {
        let (mut app, root, _env) = console();
        let mut form = LoginForm::new(0);
        form.key = "sk-test".into();
        form.base = "not-a-url".into();
        app.commit_login(form);
        match &app.modal {
            Some(Modal::Login(form)) => {
                assert!(form.error.as_deref().unwrap_or_default().contains("http"));
            }
            _ => panic!("the form stays open on a bad field"),
        }
        assert!(
            !root.join(".credentials.yaml").exists(),
            "nothing is written for an invalid form"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signing_in_drops_booths_model_override() {
        let (app, root, _env) = console();
        let cfg = app.config.clone().unwrap();
        crate::provider::set_default(&cfg, "a", "m-a").unwrap();
        assert!(crate::service::Patch::read(&cfg)
            .unwrap()
            .get("agent-default-model")
            .is_some());

        // The same call the sign-in path makes.
        assert!(crate::provider::forget_default(&cfg).unwrap());
        assert!(crate::service::Patch::read(&cfg)
            .unwrap()
            .get("agent-default-model")
            .is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod provider_view_tests {
    use super::*;
    use crate::testkit::FakeApi;

    fn console() -> (App, std::path::PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = crate::testkit::env_lock();
        let root = std::env::temp_dir().join(format!("booth-view-{}", crate::oauth::uuid_v4()));
        std::fs::create_dir_all(root.join("profiles").join("superagent")).unwrap();
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::env::set_var("DSH_HOME", &root);
        std::env::set_var("BOOTH_STATE_DIR", root.join("state"));
        let api = FakeApi::start();
        (App::new(Api::new(&api.base)), root, guard)
    }

    fn text(app: &App) -> String {
        app.lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_model_picker_opens_on_the_current_choice() {
        let (mut app, root, _env) = console();
        // Both providers offer `m-shared`: matching on the model alone would land on the first
        // row, so the provider is what has to decide.
        app.catalog = Catalog::parse(&serde_json::json!({
            "default": { "provider": "b", "model": "m-shared" },
            "groups": [
                { "id": "a", "name": "A", "models": [{ "id": "m-shared" }] },
                { "id": "b", "name": "B", "models": [{ "id": "m-shared" }] }
            ]
        }));
        app.open_model();
        match &app.modal {
            Some(Modal::Model(pick)) => {
                assert_eq!(pick.selected, 1, "the current model is selected")
            }
            _ => panic!("the picker opens"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_and_model_read_from_the_service() {
        let (mut app, root, _env) = console();
        // `/provider` asks the service rather than showing a remembered copy.
        app.spawn("/provider".to_owned());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline && !text(&app).contains("DSH profile") {
            app.drain();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let shown = text(&app);
        assert!(
            shown.contains("deepseek-official · deepseek-flash"),
            "{shown}"
        );
        assert!(shown.contains("no booth-side file"), "{shown}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_picker_moves_both_ways_and_stops_at_the_ends() {
        let (mut app, root, _env) = console();
        app.catalog = Catalog::parse(&serde_json::json!({
            "default": { "provider": "a", "model": "m-a" },
            "groups": [
                { "id": "a", "name": "A", "models": [{ "id": "m-a" }] },
                { "id": "b", "name": "B", "models": [{ "id": "m-b" }] }
            ]
        }));
        app.modal = Some(Modal::Model(ModelPick { selected: 1 }));
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        app.modal_key(key(KeyCode::Up));
        match &app.modal {
            Some(Modal::Model(pick)) => assert_eq!(pick.selected, 0),
            _ => panic!("the picker stays open"),
        }
        // Up at the first row stays there rather than wrapping below zero.
        app.modal_key(key(KeyCode::Up));
        match &app.modal {
            Some(Modal::Model(pick)) => assert_eq!(pick.selected, 0),
            _ => panic!("the picker stays open"),
        }
        // Esc closes without writing anything.
        app.modal_key(key(KeyCode::Esc));
        assert!(app.modal.is_none());
        assert!(
            crate::service::Patch::read(&app.config.clone().unwrap())
                .unwrap()
                .get("agent-default-model")
                .is_none(),
            "cancelling writes nothing"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn typing_provider_prints_the_shared_view() {
        let (mut app, root, _env) = console();
        app.catalog = Catalog::parse(&serde_json::json!({
            "default": { "provider": "deepseek-official", "model": "deepseek-flash" },
            "groups": [{ "id": "deepseek-official", "name": "DeepSeek",
                         "models": [{ "id": "deepseek-flash", "name": "DeepSeek-V4.1-Flash" }] }]
        }));
        app.input = "/provider".into();
        app.submit();
        let shown = text(&app);
        assert!(
            shown.contains("deepseek-official · deepseek-flash"),
            "{shown}"
        );
        assert!(shown.contains("no booth-side file"), "{shown}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_header_says_no_model_when_the_catalog_is_empty() {
        let (app, root, _env) = console();
        assert_eq!(app.model_label(), "no model");
        assert!(!app.status.account);
        std::fs::remove_dir_all(root).unwrap();
    }
}

//! The terminal UI: one transcript, one input line, and a palette that filters as you type.

use crate::api::Api;
use crate::commands;
use crate::format::{self, L};
use crate::provider::{self, Provider, Store};
use crate::service;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io::{self, IsTerminal, Stdout};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

const CYAN: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const RED: Color = Color::Red;
const GREEN: Color = Color::Green;

enum Msg {
    Lines(Vec<L>),
    Error(String),
    Header(Line<'static>),
}

enum Modal {
    Login(LoginForm),
    Model(ModelPick),
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
    header: Line<'static>,
    config: Option<service::Config>,
    store: Store,
    modal: Option<Modal>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
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
        let header = Line::from(vec![
            Span::styled(
                "booth",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {} · connecting…", api.base()),
                Style::default().fg(DIM),
            ),
        ]);
        let config = service::Config::from_env().ok();
        let store = config.as_ref().map(Store::load).unwrap_or_default();
        Self {
            api,
            config,
            store,
            modal: None,
            lines: Vec::new(),
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            hist: None,
            selection: 0,
            busy: false,
            follow: true,
            scroll: 0,
            quit: false,
            header,
            tx,
            rx,
        }
    }

    fn bootstrap(&mut self) {
        self.push(Line::from(vec![
            Span::styled(
                "booth",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " — a terminal console for dsh-superagent",
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        self.push(format::muted(
            "type / for commands, or a word to search. Enter runs, Tab completes.",
        ));
        self.push(format::blank());
        self.refresh_header();
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        loop {
            self.drain();
            terminal.draw(|frame| self.draw(frame))?;
            if self.quit {
                return Ok(());
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
                    self.refresh_header();
                }
                Msg::Error(error) => {
                    self.busy = false;
                    self.push(Line::from(Span::styled(
                        format!("! {error}"),
                        Style::default().fg(RED),
                    )));
                }
                Msg::Header(line) => self.header = line,
            }
        }
    }

    fn push(&mut self, line: L) {
        self.push_lines(vec![line]);
    }

    fn push_lines(&mut self, lines: Vec<L>) {
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

    fn refresh_header(&self) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let line = match api.state() {
                Ok(state) => header_line(&api, &state),
                Err(error) => {
                    let text = error.to_string();
                    let mut spans = vec![
                        Span::styled(
                            "booth",
                            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!(" · {} ", api.base()), Style::default().fg(DIM)),
                        Span::styled(clip_error(&text), Style::default().fg(RED)),
                    ];
                    if text.contains("cannot reach") {
                        spans.push(Span::styled(" · booth up", Style::default().fg(DIM)));
                    }
                    Line::from(spans)
                }
            };
            let _ = tx.send(Msg::Header(line));
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
                KeyCode::Char('c') => {
                    self.quit = true;
                    return;
                }
                KeyCode::Char('l') => {
                    self.lines.clear();
                    self.scroll = 0;
                    return;
                }
                _ => {}
            }
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
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        if line == "/model" {
            self.open_model();
            return;
        }
        if line == "/login" || line.starts_with("/login ") {
            self.open_login(line.trim_start_matches("/login").trim());
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
                self.refresh_header();
                return;
            }
            _ => {}
        }
        self.push(Line::from(vec![
            Span::styled("> ", Style::default().fg(CYAN)),
            Span::styled(line.clone(), Style::default().add_modifier(Modifier::BOLD)),
        ]));
        if line == "/help" || line == "/?" {
            self.push_lines(format::help());
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

    fn modal_key(&mut self, key: KeyEvent) {
        match self.modal.take() {
            Some(Modal::Login(form)) => self.login_key(key, form),
            Some(Modal::Model(pick)) => self.model_key(key, pick),
            None => {}
        }
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

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let [header, body, input, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        frame.render_widget(Paragraph::new(self.header.clone()), header);

        let height = body.height.max(1);
        let total = self.lines.len() as u16;
        let max_scroll = total.saturating_sub(height);
        if self.follow {
            self.scroll = max_scroll;
        } else {
            self.scroll = self.scroll.min(max_scroll);
        }
        let paragraph = Paragraph::new(self.lines.clone())
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));
        frame.render_widget(paragraph, body);

        let prompt = "booth> ";
        let text = Line::from(vec![
            Span::styled(prompt, Style::default().fg(CYAN)),
            Span::raw(self.input.clone()),
        ]);
        frame.render_widget(Paragraph::new(text), input);
        let before = &self.input[..char_to_byte(&self.input, self.cursor)];
        let x =
            input.x + UnicodeWidthStr::width(prompt) as u16 + UnicodeWidthStr::width(before) as u16;
        frame.set_cursor_position(Position::new(x.min(area.width.saturating_sub(1)), input.y));

        if let Some(matches) = self.palette() {
            let shown = matches.len().min(8);
            let width = area.width.saturating_sub(8).clamp(20, 70).min(area.width);
            let height = shown as u16 + 2;
            let rect = Rect {
                x: area.x + 4,
                y: input.y.saturating_sub(height),
                width,
                height,
            };
            frame.render_widget(Clear, rect);
            let block = Block::bordered().title(" commands ");
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let selected = self.selection.min(shown.saturating_sub(1));
            let lines: Vec<L> = matches
                .iter()
                .take(shown)
                .enumerate()
                .map(|(index, (name, description))| {
                    let active = index == selected;
                    Line::from(vec![
                        Span::styled(if active { "▶ " } else { "  " }, Style::default().fg(CYAN)),
                        Span::styled(
                            format!("{name:<20}"),
                            if active {
                                Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(CYAN)
                            },
                        ),
                        Span::styled(*description, Style::default().fg(DIM)),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), inner);
        }

        if let Some(modal) = &self.modal {
            draw_modal(frame, area, modal, &self.store);
        }

        let footer = if self.busy {
            Line::from(Span::styled("working…", Style::default().fg(Color::Yellow)))
        } else {
            Line::from(Span::styled(
                "Enter run · Tab complete · ↑↓ pick/history · PgUp/PgDn scroll · Esc clear · Ctrl+C quit",
                Style::default().fg(DIM),
            ))
        };
        frame.render_widget(Paragraph::new(footer), footer_area);
    }
}

fn header_line(api: &Api, state: &crate::model::StateView) -> Line<'static> {
    let runtime = state
        .health
        .get("runtime")
        .map(|health| health.state.as_str())
        .unwrap_or("?");
    let runtime_color = match runtime {
        "ready" => GREEN,
        "paused" => Color::Yellow,
        _ => RED,
    };
    let mut spans = vec![
        Span::styled(
            "booth",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" · {} · ", api.base()), Style::default().fg(DIM)),
        Span::styled(runtime.to_string(), Style::default().fg(runtime_color)),
        Span::styled(
            if state.load.max_workers > 0 {
                format!(
                    " · {}/{} running · {} queued",
                    state.load.running, state.load.max_workers, state.load.queued
                )
            } else {
                format!(
                    " · {} running · {} queued",
                    state.load.running, state.load.queued
                )
            },
            Style::default().fg(DIM),
        ),
    ];
    if state.paused {
        spans.push(Span::styled(
            " · paused",
            Style::default().fg(Color::Yellow),
        ));
    }
    let errors = state
        .health
        .values()
        .filter(|health| health.state == "error")
        .count();
    if errors > 0 {
        spans.push(Span::styled(
            format!(" · {errors} health error(s); /status"),
            Style::default().fg(RED),
        ));
    }
    if state.setup.problem.is_some() {
        spans.push(Span::styled(
            " · setup pending — /status",
            Style::default().fg(RED),
        ));
    }
    Line::from(spans)
}

fn clip_error(text: &str) -> String {
    format::clip(text, 70)
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

fn draw_modal(frame: &mut Frame, area: Rect, modal: &Modal, store: &Store) {
    match modal {
        Modal::Login(form) => draw_login(frame, area, form),
        Modal::Model(pick) => draw_model(frame, area, pick, store),
    }
}

fn draw_login(frame: &mut Frame, area: Rect, form: &LoginForm) {
    let width = area.width.saturating_sub(8).clamp(44, 78).min(area.width);
    let height = (10 + u16::from(form.error.is_some())).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered().title(" login ");
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
    let height = (rows as u16 + 4).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered().title(" model ");
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

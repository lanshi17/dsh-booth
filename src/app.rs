//! The terminal UI: one transcript, one input line, and a palette that filters as you type.

use crate::api::Api;
use crate::commands;
use crate::format::{self, L};
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
        Self {
            api,
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

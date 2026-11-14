//! `booth` — a command-palette console for a running dsh-superagent.
//!
//! The binary is intentionally thin: the TUI lives in `app`, the command grammar in
//! `commands`, and every HTTP call in `api`. `booth status --json` and friends make the same
//! calls without a terminal, for scripts and the hourly check the deployment guide asks for.

mod api;
mod app;
mod commands;
mod decider;
mod feishu;
mod format;
mod gh;
mod intake;
mod model;
mod oauth;
mod provider;
mod service;
#[cfg(test)]
mod testkit;
mod wechat;
mod worker;

use api::Api;
use clap::{Parser, Subcommand};
use ratatui::text::Line as TuiLine;
use serde::Serialize;
use std::io::{self, Write};

const DEFAULT_URL: &str = "http://127.0.0.1:3082";

#[derive(Parser)]
#[command(
    name = "booth",
    version,
    about = "A terminal console for dsh-superagent",
    long_about = "Runs the bundled superagent service and reads its loopback management API. \
                  `booth init` prepares the profile, `booth up` runs the service, `booth` with no \
                  command opens the console."
)]
struct Cli {
    /// Base URL of the dsh-superagent management API.
    #[arg(
        long,
        global = true,
        env = "DSH_SUPERAGENT_URL",
        default_value = DEFAULT_URL
    )]
    url: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Prepare the DSH profile and build the bundled superagent.
    Init,
    /// Connect the WeChat iLink Bot or allow a sender to contact the portal.
    Wechat {
        #[command(subcommand)]
        command: WechatCommand,
    },
    /// Connect a Feishu bot, allow a sender, or receive a group chat.
    Feishu {
        #[command(subcommand)]
        command: FeishuCommand,
    },
    /// Start the service in the background.
    Up,
    /// Stop the service booth started.
    Down,
    /// Restart the service.
    Restart,
    /// Print the service log.
    Logs {
        #[arg(long, default_value_t = 40)]
        lines: usize,
    },
    /// Show the model provider booth starts the service with.
    Provider {
        /// Print the service's catalog as served, like the other read commands.
        #[arg(long)]
        json: bool,
    },
    /// Move the DSH sessions aside so the next turn uses the configured model.
    ResetSessions,
    /// Choose among the options a portal or a sub-agent submits on stdin.
    Decide {
        /// Profile provider that serves the decision model.
        #[arg(long)]
        provider: Option<String>,
        /// Decision model. The default is typesafe/jev.
        #[arg(long)]
        model: Option<String>,
        /// Remember this provider and model for later decisions.
        #[arg(long)]
        save: bool,
    },
    /// Hand one instruction to a worker. The default worker is dsh.
    Send {
        /// Who does the work: dsh, omp, claude, codex, or cursor-agent.
        #[arg(long, default_value = "dsh")]
        agent: String,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Report the GitHub CLI the GitHub Source would use, or install a newer one.
    Gh {
        /// Download the pinned release when the system gh is too old.
        #[arg(long)]
        install: bool,
    },
    /// Name the repository and the people who may ask for work.
    Setup {
        /// owner/name; inferred from the checkout when omitted.
        repository: Option<String>,
        /// Directory workers work in.
        directory: Option<String>,
    },
    /// Runtime health, pause state, setup problem and accounts.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// How work is routed, run in parallel and remembered.
    Agents {
        #[arg(long)]
        json: bool,
    },
    /// Problems, tickets waiting for a person, executor load, 24h flow.
    Overview {
        #[arg(long)]
        json: bool,
    },
    /// Portal conversations and delegated worker tasks.
    Portal {
        /// Skip the latest conversations when reading a later page.
        #[arg(default_value_t = 0)]
        offset: u32,
        #[arg(long)]
        json: bool,
    },
    /// List tickets by stage.
    Tickets {
        #[arg(long, default_value = "open")]
        state: String,
        #[arg(long)]
        json: bool,
    },
    /// One ticket: mandate, assignments, inputs and threads.
    Ticket {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// List threads.
    Threads {
        #[arg(long, default_value = "recent")]
        filter: String,
        #[arg(long)]
        json: bool,
    },
    /// One thread: its timeline and decisions.
    Thread {
        key: String,
        #[arg(long)]
        json: bool,
    },
    /// Recent decisions and the reason recorded for each.
    Decisions {
        #[arg(long)]
        json: bool,
    },
    /// Each Source's state and problems.
    Sources {
        #[arg(long)]
        json: bool,
    },
    /// Search tickets, threads and messages.
    Search {
        query: String,
        #[arg(long)]
        json: bool,
    },
    /// Withdraw a ticket's mandate.
    Stop { id: String },
    /// Start a fresh assignment for a stalled ticket.
    Retry { id: String },
    /// Read or change what may start work without a person.
    Eval {
        /// off | observe | auto; omitted prints the current mode.
        mode: Option<String>,
    },
    /// Pause the executor.
    Pause,
    /// Resume the executor.
    Resume,
}

#[derive(Subcommand)]
enum WechatCommand {
    /// Print a fresh iLink login link, wait for confirmation, and save credentials.
    Login,
    /// Allow a WeChat sender ID to message the portal.
    Allow { user_id: String },
}

#[derive(Subcommand)]
enum FeishuCommand {
    /// Configure the Feishu app, the bot-only lark-cli and the Feishu Source.
    Login {
        /// The app's App ID (cli_…).
        #[arg(long, env = "FEISHU_APP_ID")]
        app_id: String,
        /// The app's App Secret; prefer the environment over the command line.
        #[arg(long, env = "FEISHU_APP_SECRET", hide_env_values = true)]
        app_secret: String,
        /// A Feishu open_id to allow to contact the portal; repeatable.
        #[arg(long = "allow", value_name = "OPEN_ID")]
        allow: Vec<String>,
    },
    /// Allow a Feishu sender to message the portal.
    Allow { open_id: String },
    /// Receive messages in a group chat, in addition to direct messages.
    Chat {
        chat_id: String,
        /// Receive every message, not only those that mention the bot.
        #[arg(long)]
        all: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let api = Api::new(&cli.url);
    let result = match cli.command {
        None => {
            // The interactive frontend is OMP itself. Booth only owns lifecycle and the portal
            // adapter; this keeps composer, palette, history, transcript and shortcuts native.
            let startup = service::Config::from_env().and_then(|cfg| {
                if api.state().is_err() {
                    service::start(&cfg)
                } else {
                    Ok(())
                }
            });
            startup.and_then(|_| launch_omp())
        }
        Some(command) => run(api, command),
    };
    if let Err(error) = result {
        eprintln!("booth: {error}");
        if error.contains("cannot reach") {
            eprintln!("hint: `booth up` (and `booth init` the first time)");
        }
        std::process::exit(1);
    }
}

fn launch_omp() -> Result<(), String> {
    let extension = std::env::var_os("BOOTH_OMP_EXTENSION")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("omp-portal/extension.ts"));
    if !extension.is_file() {
        return Err(format!("OMP portal extension is missing: {}", extension.display()));
    }
    let status = std::process::Command::new("omp")
        .arg("--extension")
        .arg(extension)
        .status()
        .map_err(|error| format!("cannot launch OMP (`omp`): {error}"))?;
    if status.success() { Ok(()) } else { Err(format!("OMP exited with {status}")) }
}

/// What `booth provider` prints: the service's own catalog, never a secret.
fn provider_lines(catalog: &provider::Catalog) -> Vec<String> {
    let mut lines = Vec::new();
    match &catalog.default {
        Some((provider, model)) => lines.push(format!("model      {provider} · {model}")),
        None => lines.push("model      none — /login in the console".into()),
    }
    for (id, name) in catalog.providers() {
        lines.push(format!("provider   {id} ({name})"));
    }
    if !catalog.routable.is_empty() {
        lines.push(format!("routable   {}", catalog.routable.join(", ")));
    }
    lines.push("source     the DSH profile's own config; no booth-side file".into());
    lines
}

/// The four routes `/provider` configures, in the same order as the screen.
fn duty_lines(cfg: Option<&service::Config>, catalog: &provider::Catalog) -> Vec<String> {
    provider::DUTIES
        .iter()
        .map(|duty| {
            format!(
                "{:<10} {}",
                duty.id,
                provider::current_route(cfg, catalog, duty)
            )
        })
        .collect()
}

/// The `/models` view `booth provider` reads, as the service served it.
///
/// The human output is a summary; a script that needs the whole catalog (models, reasoning
/// efforts, routable providers) asks for the same view the web panel reads instead of parsing
/// those lines or hand-rolling the same-origin request.
fn provider_view(api: &Api) -> Result<serde_json::Value, String> {
    api.models().map_err(String::from)
}

fn show_provider(api: &Api) -> Result<(), String> {
    let catalog = provider::Catalog::parse(&provider_view(api)?);
    for line in provider_lines(&catalog) {
        emit_line(&line);
    }
    let cfg = service::Config::from_env().ok();
    for line in duty_lines(cfg.as_ref(), &catalog) {
        emit_line(&line);
    }
    Ok(())
}

/// The ticket states `booth tickets` can list, in the service's spelling.
///
/// The service parses `state` with `z.enum(TICKET_FILTERS).catch('open')`, so a value outside
/// this list still answers 200 with the *open* rows while the human heading repeats the value
/// the person typed. The console grammar rejects that in `commands`; the one-shot flags are
/// defined here, so the same contract is applied before the request.
const TICKET_STATES: [&str; 5] = ["open", "person", "working", "idle", "ended"];

/// The thread lists `booth threads` can ask for. The service coerces an unknown `filter` to
/// `subscribed`, which would contradict the printed heading in the same way.
const THREAD_FILTERS: [&str; 4] = ["recent", "subscribed", "evaluating", "all"];

/// One of `allowed`, with `default` for an omitted value; matching ignores case so a
/// capitalised filter reaches the service in the spelling it expects.
fn list_choice(
    argument: &str,
    default: &'static str,
    allowed: &[&'static str],
    what: &str,
) -> Result<&'static str, String> {
    let argument = argument.trim();
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

fn run(api: Api, command: Command) -> Result<(), String> {
    match command {
        Command::Init => service::init(&service::Config::from_env()?),
        Command::Wechat { command } => {
            let cfg = service::Config::from_env()?;
            match command {
                WechatCommand::Login => wechat::login(&cfg, &api),
                WechatCommand::Allow { user_id } => wechat::allow(&api, &user_id),
            }
        }
        Command::Feishu { command } => {
            let cfg = service::Config::from_env()?;
            match command {
                FeishuCommand::Login {
                    app_id,
                    app_secret,
                    allow,
                } => feishu::login(&cfg, &api, &app_id, &app_secret, &allow),
                FeishuCommand::Allow { open_id } => feishu::allow(&api, &open_id),
                FeishuCommand::Chat { chat_id, all } => feishu::chat(&api, &chat_id, all),
            }
        }
        Command::Up => service::start(&service::Config::from_env()?),
        Command::Down => service::stop(&service::Config::from_env()?),
        Command::Restart => service::restart(&service::Config::from_env()?),
        Command::Logs { lines } => service::logs(&service::Config::from_env()?, lines),
        Command::Provider { json } => {
            if json {
                print_json(&provider_view(&api)?)
            } else {
                show_provider(&api)
            }
        }
        Command::ResetSessions => {
            let cfg = service::Config::from_env()?;
            let work = service::execution_root(&api, &cfg);
            match service::reset_sessions(&cfg, &work)? {
                Some(path) => println!("sessions moved to {}", path.display()),
                None => println!("no sessions to move"),
            }
            Ok(())
        }
        Command::Decide {
            provider,
            model,
            save,
        } => {
            let cfg = service::Config::from_env()?;
            if save {
                decider::save_route(
                    &cfg,
                    provider.as_deref().unwrap_or(""),
                    model.as_deref().unwrap_or(""),
                )
            } else {
                decider::decide(
                    &cfg,
                    &mut io::stdin(),
                    provider.as_deref(),
                    model.as_deref(),
                )
            }
        }
        Command::Send { agent, text } => {
            send_instruction(&service::Config::from_env()?, &agent, text)
        }
        Command::Gh { install } => {
            let cfg = service::Config::from_env()?;
            let system = gh::version(std::path::Path::new("gh")).unwrap_or_else(|| "?".into());
            println!(
                "system      gh {system} · {}",
                if gh::system_capable() {
                    "supports GitHub reads"
                } else {
                    "too old for GitHub reads"
                }
            );
            let local = cfg.state_dir.join("bin").join("gh");
            if local.exists() {
                println!(
                    "installed   gh {} · {}",
                    gh::version(&local).unwrap_or_else(|| "?".into()),
                    local.display()
                );
            }
            if install {
                match gh::ensure(&cfg, true)? {
                    Some(path) => println!("selected    {path}"),
                    None => println!("selected    gh (system)"),
                }
            } else if gh::system_capable() {
                println!("selected    gh (system)");
            } else if local.exists() && gh::capable(&local) {
                println!("selected    {}", local.display());
            } else {
                println!(
                    "hint        booth gh --install downloads gh {} (13 MB)",
                    gh::VERSION
                );
            }
            Ok(())
        }
        Command::Setup {
            repository,
            directory,
        } => {
            let cfg = service::Config::from_env()?;
            let argument = match (repository, directory) {
                (Some(repository), Some(directory)) => format!("{repository} {directory}"),
                (Some(repository), None) => repository,
                (None, Some(_)) => {
                    return Err(
                        "give the repository first: booth setup owner/name [directory]".into(),
                    )
                }
                (None, None) => String::new(),
            };
            let lines = intake::setup(&api, &cfg, &argument)?;
            print_lines(lines);
            Ok(())
        }
        Command::Status { json } => show(
            &api,
            json,
            api.state().map_err(String::from)?,
            format::status,
        ),
        Command::Agents { json } => {
            let cfg = service::Config::from_env()?;
            let view = api.overview().map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                let work = service::execution_root(&api, &cfg);
                print_lines(format::agents(
                    &view,
                    api.base(),
                    service::session_count(&cfg, &work),
                    &service::session_root(&cfg).display().to_string(),
                ));
                Ok(())
            }
        }
        Command::Overview { json } => show(
            &api,
            json,
            api.overview().map_err(String::from)?,
            format::overview,
        ),
        Command::Portal { offset, json } => {
            let view = api.portal(offset).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::portal(&view));
                Ok(())
            }
        }
        Command::Tickets { state, json } => {
            let state = list_choice(&state, "open", &TICKET_STATES, "ticket state")?;
            let view = api.tickets(state).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::ticket_list(&view, state));
                Ok(())
            }
        }
        Command::Ticket { id, json } => {
            let view = api.ticket(&id).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::ticket_detail(&view));
                Ok(())
            }
        }
        Command::Threads { filter, json } => {
            let filter = list_choice(&filter, "recent", &THREAD_FILTERS, "thread filter")?;
            let view = api.threads(filter).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::thread_list(&view, filter));
                Ok(())
            }
        }
        Command::Thread { key, json } => {
            let view = api.thread(&key).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::thread_detail(&view));
                Ok(())
            }
        }
        Command::Decisions { json } => {
            let view = api.decisions().map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::decisions(&view));
                Ok(())
            }
        }
        Command::Sources { json } => {
            let view = api.sources().map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::sources(&view));
                Ok(())
            }
        }
        Command::Search { query, json } => {
            let view = api.search(&query).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::search(&view));
                Ok(())
            }
        }
        Command::Stop { id } => {
            api.stop(&id).map_err(String::from)?;
            println!("stopped {id}");
            Ok(())
        }
        Command::Retry { id } => {
            api.retry(&id).map_err(String::from)?;
            println!("retried {id}");
            Ok(())
        }
        Command::Eval { mode } => {
            print_lines(intake::eval(&api, mode.as_deref().unwrap_or_default())?);
            Ok(())
        }
        Command::Pause => {
            api.pause(true).map_err(String::from)?;
            println!("executor paused");
            Ok(())
        }
        Command::Resume => {
            api.pause(false).map_err(String::from)?;
            println!("executor resumed");
            Ok(())
        }
    }
}

/// Send a shell instruction to the same portal conversation as the console.
fn send_instruction(cfg: &service::Config, agent: &str, text: Vec<String>) -> Result<(), String> {
    let who = worker::Who::parse(agent)?;
    let text = text.join(" ");
    if who != worker::Who::Dsh {
        emit_line(&worker::run(who, &text)?);
        return Ok(());
    }
    let thread = intake::portal_thread(cfg)?;
    let id = intake::send(cfg, &text, Some(&thread))?;
    emit_line(&format!("sent {id} to the portal"));
    Ok(())
}

fn show<T, F>(api: &Api, json: bool, view: T, render: F) -> Result<(), String>
where
    T: Serialize,
    F: Fn(&T, &str) -> Vec<TuiLine<'static>>,
{
    if json {
        print_json(&view)
    } else {
        print_lines(render(&view, api.base()));
        Ok(())
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    emit_line(&text);
    Ok(())
}

fn print_lines(lines: Vec<TuiLine<'static>>) {
    for line in lines {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        emit_line(&text);
    }
}

/// Writes one line to stdout without making a closed reader a failure.
///
/// The one-shot commands are meant for scripts, and a reader that stops early — `booth decisions
/// | head -1` or `booth tickets --json | head -c 200` is common — closes the pipe while the
/// command is still writing. `println!` panics on that `EPIPE`, so the command died with a Rust
/// panic and exit 101 even though its work had succeeded. The reader went away, so there is
/// nothing left to say; a write failure of another kind is reported on stderr instead.
fn emit_line(text: &str) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = write_line(&mut out, text).map_err(report_write);
}

fn write_line(out: &mut impl Write, text: &str) -> io::Result<()> {
    writeln!(out, "{text}")
}

fn report_write(error: io::Error) {
    let _ = report_write_to(&mut io::stderr(), error);
}

fn report_write_to(out: &mut impl Write, error: io::Error) -> io::Result<()> {
    if error.kind() == io::ErrorKind::BrokenPipe {
        return Ok(());
    }
    writeln!(out, "booth: cannot write to stdout: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_is_a_one_shot_command_with_offset_and_json_options() {
        let cli = Cli::try_parse_from(["booth", "portal", "20", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Portal {
                offset: 20,
                json: true
            })
        ));
        let default = Cli::try_parse_from(["booth", "portal"]).unwrap();
        assert!(matches!(
            default.command,
            Some(Command::Portal {
                offset: 0,
                json: false
            })
        ));
    }

    #[test]
    fn wechat_login_and_allow_are_terminal_subcommands() {
        assert!(matches!(
            Cli::try_parse_from(["booth", "wechat", "login"])
                .unwrap()
                .command,
            Some(Command::Wechat {
                command: WechatCommand::Login
            })
        ));
        assert!(
            matches!(Cli::try_parse_from(["booth", "wechat", "allow", "wxid-user"]).unwrap().command,
            Some(Command::Wechat { command: WechatCommand::Allow { user_id } }) if user_id == "wxid-user")
        );
    }

    #[test]
    fn feishu_login_allow_and_chat_are_terminal_subcommands() {
        let cli = Cli::try_parse_from([
            "booth",
            "feishu",
            "login",
            "--app-id",
            "cli_abc123",
            "--app-secret",
            "secret",
            "--allow",
            "ou_one",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Feishu { command: FeishuCommand::Login { ref app_id, ref app_secret, ref allow } })
                if app_id == "cli_abc123" && app_secret == "secret" && allow == &["ou_one".to_owned()]
        ));
        assert!(matches!(
            Cli::try_parse_from(["booth", "feishu", "allow", "ou_user"]).unwrap().command,
            Some(Command::Feishu { command: FeishuCommand::Allow { open_id } }) if open_id == "ou_user"
        ));
        assert!(matches!(
            Cli::try_parse_from(["booth", "feishu", "chat", "oc_chat", "--all"]).unwrap().command,
            Some(Command::Feishu { command: FeishuCommand::Chat { chat_id, all: true } }) if chat_id == "oc_chat"
        ));
    }

    #[test]
    fn the_provider_command_says_when_there_is_no_model() {
        // An empty catalog is a real state (a profile with no model configured). The test's name
        // is the claim, so it checks the line a person reads, not only the parse below it.
        let empty = crate::provider::Catalog::parse(&serde_json::json!({}));
        assert_eq!(empty.default, None);
        let lines = provider_lines(&empty);
        assert!(lines[0].contains("none"), "{lines:?}");
    }

    #[test]
    fn the_provider_lines_name_the_catalog_and_never_a_secret() {
        let catalog = crate::provider::Catalog::parse(&serde_json::json!({
            "default": { "provider": "openrouter", "model": "x/y" },
            "routableProviders": ["openrouter"],
            "groups": [{ "id": "openrouter", "name": "OpenRouter",
                         "models": [{ "id": "x/y", "name": "X/Y" }] }]
        }));
        let lines = provider_lines(&catalog);
        assert!(lines[0].contains("openrouter · x/y"), "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|line| line.contains("provider   openrouter")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("routable   openrouter")),
            "{lines:?}"
        );
        assert!(
            lines.last().unwrap().contains("no booth-side file"),
            "{lines:?}"
        );
        // No key value can reach this output: the catalog carries none.
        assert!(!lines.iter().any(|line| line.contains("sk-")), "{lines:?}");
        let duties = duty_lines(None, &catalog);
        assert!(
            duties
                .iter()
                .any(|line| line.starts_with("typesafe") && line.contains("typesafe/jev")),
            "{duties:?}"
        );
        assert!(
            duties
                .iter()
                .any(|line| line.starts_with("search") && line.contains("not set")),
            "{duties:?}"
        );

        // An empty catalog says so instead of printing nothing.
        let empty = provider_lines(&crate::provider::Catalog::default());
        assert!(empty[0].contains("none"), "{empty:?}");
    }

    #[test]
    fn the_provider_command_takes_json_like_the_other_api_views() {
        // Every management-API view takes `--json`; `provider` reads /models but rejected it.
        // A script needs the catalog the panel reads, not the human summary.
        let cli = Cli::try_parse_from(["booth", "provider", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Provider { json: true })
        ));
    }

    /// A writer that fails every write with the given kind, such as a closed pipe.
    struct Failing(io::ErrorKind);

    impl Write for Failing {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(self.0, "no reader"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_reader_is_the_end_of_the_output_not_a_panic() {
        // `booth status | head -1` closes stdout while the view is still being written. The line
        // helper must hand back the write error instead of panicking, and the report must not
        // turn that reader's choice into an error; another write failure is still reported.
        let mut sink = Vec::new();
        write_line(&mut sink, "one").unwrap();
        assert_eq!(sink, b"one\n");
        Failing(io::ErrorKind::BrokenPipe).flush().unwrap();

        let error = write_line(&mut Failing(io::ErrorKind::BrokenPipe), "two").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        report_write(error);
        let mut report = Vec::new();
        report_write_to(
            &mut report,
            write_line(&mut Failing(io::ErrorKind::BrokenPipe), "three").unwrap_err(),
        )
        .unwrap();
        assert!(report.is_empty(), "{report:?}");

        let error = write_line(&mut Failing(io::ErrorKind::PermissionDenied), "four").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        report_write_to(&mut report, error).unwrap();
        let text = String::from_utf8(report).unwrap();
        assert!(text.contains("no reader"), "{text}");
    }

    #[test]
    fn the_rendered_views_write_through_the_pipe_safe_line() {
        // The list and JSON views share the two helpers; exercising one of each proves they go
        // through `emit_line` rather than `println!`.
        let fake = crate::testkit::FakeApi::start();
        let api = Api::new(&fake.base);
        run(api.clone(), Command::Overview { json: false }).unwrap();
        run(api, Command::Provider { json: true }).unwrap();
    }

    /// Runs one test in a child of this test binary, so the test can read what it wrote to
    /// stdout: the harness captures `print!`, not direct `io::stdout()` writes.
    fn output_of_test(name: &str, flag: &str) -> std::process::Output {
        let exe = std::env::current_exe().expect("the test executable");
        std::process::Command::new(exe)
            .args(["--exact", name, "--nocapture"])
            .env(flag, "1")
            .output()
            .expect("run the test in a child")
    }

    #[test]
    fn a_rendered_view_writes_every_line() {
        const FLAG: &str = "BOOTH_TEST_PRINT_LINES";
        if std::env::var_os(FLAG).is_some() {
            print_lines(vec![TuiLine::from("alpha"), TuiLine::from("beta")]);
            return;
        }
        let out = output_of_test("tests::a_rendered_view_writes_every_line", FLAG);
        assert!(out.status.success(), "{out:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("alpha\n"), "{text}");
        assert!(text.contains("beta\n"), "{text}");
    }

    #[test]
    fn the_provider_view_writes_the_catalog() {
        const FLAG: &str = "BOOTH_TEST_SHOW_PROVIDER";
        if std::env::var_os(FLAG).is_some() {
            let fake = crate::testkit::FakeApi::start();
            show_provider(&Api::new(&fake.base)).unwrap();
            return;
        }
        let out = output_of_test("tests::the_provider_view_writes_the_catalog", FLAG);
        assert!(out.status.success(), "{out:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("provider   deepseek-official"), "{text}");
        assert!(
            text.contains("model      deepseek-official · deepseek-flash"),
            "{text}"
        );
    }

    #[test]
    fn a_json_view_writes_the_value() {
        const FLAG: &str = "BOOTH_TEST_PRINT_JSON";
        if std::env::var_os(FLAG).is_some() {
            print_json(&serde_json::json!({ "answer": 42 })).unwrap();
            return;
        }
        let out = output_of_test("tests::a_json_view_writes_the_value", FLAG);
        assert!(out.status.success(), "{out:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("\"answer\": 42"), "{text}");
    }

    #[test]
    fn another_write_failure_reaches_stderr() {
        const FLAG: &str = "BOOTH_TEST_REPORT_WRITE";
        if std::env::var_os(FLAG).is_some() {
            report_write(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
            return;
        }
        let out = output_of_test("tests::another_write_failure_reaches_stderr", FLAG);
        assert!(out.status.success(), "{out:?}");
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(text.contains("cannot write to stdout: denied"), "{text}");
    }

    #[test]
    fn the_provider_json_is_the_catalog_as_served() {
        let fake = crate::testkit::FakeApi::start();
        let view = provider_view(&Api::new(&fake.base)).unwrap();
        assert_eq!(view["default"]["model"], "deepseek-flash");
        assert!(view["groups"].is_array(), "{view:?}");
    }

    #[test]
    fn the_provider_command_prints_the_service_catalog() {
        // The fake serves /models, so this exercises the real request and rendering path.
        let api = crate::testkit::FakeApi::start();
        let api = Api::new(&api.base);
        let catalog = provider::Catalog::parse(&api.models().unwrap());
        assert_eq!(
            catalog.default,
            Some(("deepseek-official".to_owned(), "deepseek-flash".to_owned()))
        );
        assert!(show_provider(&api).is_ok());
    }

    #[test]
    fn a_list_argument_is_canonical_or_rejected() {
        // The service coerces an unknown `state` to `open` and an unknown `filter` to
        // `subscribed`; the heading would still name the value that was ignored.
        assert_eq!(
            list_choice("PERSON", "open", &TICKET_STATES, "ticket state").unwrap(),
            "person"
        );
        assert_eq!(
            list_choice("", "open", &TICKET_STATES, "ticket state").unwrap(),
            "open"
        );
        let error = list_choice("bogus", "open", &TICKET_STATES, "ticket state").unwrap_err();
        assert!(error.contains("bogus"), "{error}");
        assert!(error.contains("working"), "{error}");

        assert_eq!(
            list_choice("Subscribed", "recent", &THREAD_FILTERS, "thread filter").unwrap(),
            "subscribed"
        );
        let error = list_choice("lately", "recent", &THREAD_FILTERS, "thread filter").unwrap_err();
        assert!(error.contains("lately"), "{error}");
        assert!(error.contains("evaluating"), "{error}");
    }

    #[test]
    fn a_bad_list_flag_fails_before_the_request() {
        // A dead port makes the order observable: if the request ran first, the error would be
        // the transport failure instead of the rejected filter.
        let api = Api::new("http://127.0.0.1:9");
        for json in [false, true] {
            let error = run(
                api.clone(),
                Command::Tickets {
                    state: "bogus".into(),
                    json,
                },
            )
            .unwrap_err();
            assert!(error.contains("unknown ticket state"), "{error}");
        }
        let error = run(
            api,
            Command::Threads {
                filter: "lately".into(),
                json: false,
            },
        )
        .unwrap_err();
        assert!(error.contains("unknown thread filter"), "{error}");
    }

    /// Sets an environment variable for one test and restores it when the test ends, so the
    /// `Send` command's real `Config::from_env` path can be exercised without leaking state.
    struct EnvVar(&'static str, Option<std::ffi::OsString>);

    impl EnvVar {
        fn set(name: &'static str, value: &std::path::Path) -> Self {
            let previous = std::env::var_os(name);
            std::env::set_var(name, value);
            Self(name, previous)
        }
    }

    impl Drop for EnvVar {
        fn drop(&mut self) {
            match &self.1 {
                Some(value) => std::env::set_var(self.0, value),
                None => std::env::remove_var(self.0),
            }
        }
    }

    #[test]
    fn a_shell_instruction_uses_the_portal_conversation() {
        // The console records the accepted ticket it follows in booth-follow.json. Typing the
        // same text there sends it on that thread; the shell command used to ignore the record
        // and start a new ticket, so a person answering a blocked ticket got no answer.
        let _env = crate::testkit::env_lock();
        let dir = std::env::temp_dir().join(format!("booth-send-{}", crate::oauth::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let _state = EnvVar::set("BOOTH_STATE_DIR", &dir);
        let _home = EnvVar::set("DSH_HOME", &dir);
        let _bundle = EnvVar::set("BOOTH_SUPERAGENT_DIR", &dir);
        std::fs::write(
            dir.join("booth-follow.json"),
            serde_json::json!({ "ticket": "abcdef1234567890", "thread": "t-followed" }).to_string(),
        )
        .unwrap();

        let api = Api::new("http://127.0.0.1:9");
        run(
            api.clone(),
            Command::Send {
                agent: "dsh".into(),
                text: vec!["revise".into(), "it".into()],
            },
        )
        .unwrap();
        let inbox = std::fs::read_to_string(dir.join("booth-inbox.jsonl")).unwrap();
        let line: serde_json::Value = serde_json::from_str(inbox.trim()).unwrap();
        assert_eq!(line["text"], "revise it");
        let thread = line["thread"].as_str().unwrap().to_owned();
        assert!(thread.starts_with("portal-"), "{line}");

        // Without a recorded follow the instruction still starts a new ticket, as before.
        std::fs::remove_file(dir.join("booth-follow.json")).unwrap();
        run(
            api,
            Command::Send {
                agent: "dsh".into(),
                text: vec!["start".into(), "over".into()],
            },
        )
        .unwrap();
        let inbox = std::fs::read_to_string(dir.join("booth-inbox.jsonl")).unwrap();
        let line: serde_json::Value = serde_json::from_str(inbox.lines().last().unwrap()).unwrap();
        assert_eq!(line["text"], "start over");
        assert_eq!(line["thread"], thread, "{line}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

//! `booth` — a command-palette console for a running dsh-superagent.
//!
//! The binary is intentionally thin: the TUI lives in `app`, the command grammar in
//! `commands`, and every HTTP call in `api`. `booth status --json` and friends make the same
//! calls without a terminal, for scripts and the hourly check the deployment guide asks for.

mod api;
mod app;
mod commands;
mod format;
mod gh;
mod intake;
mod model;
mod oauth;
mod provider;
mod service;
#[cfg(test)]
mod testkit;

use api::Api;
use clap::{Parser, Subcommand};
use ratatui::text::Line as TuiLine;
use serde::Serialize;

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
    Provider,
    /// Move the DSH sessions aside so the next turn uses the configured model.
    ResetSessions,
    /// Hand one instruction to the agent, as typing it in the console does.
    Send {
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
    /// Search tickets, threads and triggers.
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

fn main() {
    let cli = Cli::parse();
    let api = Api::new(&cli.url);
    let result = match cli.command {
        None => app::run(api),
        Some(command) => run(api, command),
    };
    if let Err(error) = result {
        eprintln!("booth: {error}");
        if error.contains("cannot reach") {
            eprintln!("hint: `booth start` (and `booth init` the first time)");
        }
        std::process::exit(1);
    }
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

fn show_provider(api: &Api) -> Result<(), String> {
    let catalog = provider::Catalog::parse(&api.models().map_err(String::from)?);
    for line in provider_lines(&catalog) {
        println!("{line}");
    }
    Ok(())
}

fn run(api: Api, command: Command) -> Result<(), String> {
    match command {
        Command::Init => service::init(&service::Config::from_env()?),
        Command::Up => service::start(&service::Config::from_env()?),
        Command::Down => service::stop(&service::Config::from_env()?),
        Command::Restart => service::restart(&service::Config::from_env()?),
        Command::Logs { lines } => service::logs(&service::Config::from_env()?, lines),
        Command::Provider => show_provider(&api),
        Command::ResetSessions => {
            let cfg = service::Config::from_env()?;
            match service::reset_sessions(&cfg)? {
                Some(path) => println!("sessions moved to {}", path.display()),
                None => println!("no sessions to move"),
            }
            Ok(())
        }
        Command::Send { text } => {
            let cfg = service::Config::from_env()?;
            let id = intake::send(&cfg, &text.join(" "), None)?;
            println!("sent {id} — the agent picks it up within a second");
            Ok(())
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
                print_lines(format::agents(
                    &view,
                    api.base(),
                    service::session_count(&cfg),
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
        Command::Tickets { state, json } => {
            let view = api.tickets(&state).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::ticket_list(&view, &state));
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
            let view = api.threads(&filter).map_err(String::from)?;
            if json {
                print_json(&view)
            } else {
                print_lines(format::thread_list(&view, &filter));
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
    println!("{text}");
    Ok(())
}

fn print_lines(lines: Vec<TuiLine<'static>>) {
    for line in lines {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        println!("{text}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provider_command_says_when_there_is_no_model() {
        // An empty catalog is a real state (a profile with no model configured).
        let api = crate::testkit::FakeApi::start();
        let empty = crate::provider::Catalog::parse(&serde_json::json!({}));
        assert_eq!(empty.default, None);
        assert!(cur_or_absent(&api.base));
    }

    /// The `None` arm, which a live service always fills in.
    fn cur_or_absent(base: &str) -> bool {
        let api = Api::new(base);
        show_provider(&api).is_ok()
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

        // An empty catalog says so instead of printing nothing.
        let empty = provider_lines(&crate::provider::Catalog::default());
        assert!(empty[0].contains("none"), "{empty:?}");
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
}

//! `booth` — a command-palette console for a running dsh-superagent.
//!
//! The binary is intentionally thin: the TUI lives in `app`, the command grammar in
//! `commands`, and every HTTP call in `api`. `booth status --json` and friends make the same
//! calls without a terminal, for scripts and the hourly check the deployment guide asks for.

mod api;
mod app;
mod commands;
mod format;
mod model;

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
    long_about = "Reads the bundle's loopback management API. Run `booth` with no command for the \
                  console; run `booth status` for one shot. The service must be started with \
                  `scripts/dsh-superagent start`."
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
    /// Runtime health, pause state, setup problem and accounts.
    Status {
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
            eprintln!("hint: start it with `scripts/dsh-superagent start`, then run `sa status`");
        }
        std::process::exit(1);
    }
}

fn run(api: Api, command: Command) -> Result<(), String> {
    match command {
        Command::Status { json } => show(
            &api,
            json,
            api.state().map_err(String::from)?,
            format::status,
        ),
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

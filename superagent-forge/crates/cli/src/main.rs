use clap::Parser;
use clap::Subcommand;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "forge",
    version,
    about = "Superagent Forge control plane (MCP stdio; optional OIDC HS256)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate configuration. Does not create directories, open a database or start workers.
    Check {
        #[arg(short, long, default_value = "config/default.toml")]
        config: PathBuf,
    },
    /// Print MCP tool contracts.
    Protocol,
    /// Serve MCP JSON-RPC on stdio using a trusted token map from the environment.
    ///
    /// Requires `FORGE_MCP_DB` and `FORGE_MCP_TOKENS` (`token=principal:create|cancel,...`).
    /// Optional OIDC HS256: `FORGE_OIDC_SECRET`, `FORGE_OIDC_ISSUER`, `FORGE_OIDC_AUDIENCE`,
    /// and optional `FORGE_OIDC_SUBJECTS` (`sub=principal:create|cancel,...`).
    McpStdio,
}

fn parse_identity_flags(flags: &str) -> (bool, bool) {
    let flags = flags.to_ascii_lowercase();
    (flags.contains("create"), flags.contains("cancel"))
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::Check { config } => {
            let config = forge_config::Config::parse(&std::fs::read_to_string(config)?)?;
            println!(
                "configuration valid; concurrency={}; network_allowed={}; persistence, MCP stdio, signed merge auth and remote push helpers are available",
                config.max_concurrency, config.network_allowed
            );
        }
        Command::Protocol => println!(
            "{}",
            serde_json::to_string_pretty(&forge_mcp_server::planned_tools())?
        ),
        Command::McpStdio => {
            let db = std::env::var("FORGE_MCP_DB")
                .map_err(|_| "FORGE_MCP_DB is required for mcp-stdio")?;
            let tokens = std::env::var("FORGE_MCP_TOKENS")
                .map_err(|_| "FORGE_MCP_TOKENS is required for mcp-stdio")?;
            let mut identities = forge_mcp_server::TrustedIdentityMap::new();
            for entry in tokens.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let (token, rest) = entry
                    .split_once('=')
                    .ok_or("FORGE_MCP_TOKENS entries must be token=principal:flags")?;
                let (principal, flags) = rest.split_once(':').unwrap_or((rest, "create,cancel"));
                let (can_create_tasks, can_cancel) = parse_identity_flags(flags);
                identities.insert(
                    token,
                    forge_mcp_server::TrustedIdentity {
                        principal: forge_domain::PrincipalId(principal.to_owned()),
                        can_create_tasks,
                        can_cancel,
                    },
                );
            }
            let policy = forge_mcp_server::policy_from_identities(&identities);
            let repo = forge_persistence::TursoTaskRepository::open(db)?;
            let secret = std::env::var("FORGE_OIDC_SECRET").ok();
            let issuer = std::env::var("FORGE_OIDC_ISSUER").ok();
            let audience = std::env::var("FORGE_OIDC_AUDIENCE").ok();
            let server = match (secret, issuer, audience) {
                (Some(secret), Some(issuer), Some(audience)) => {
                    let mut oidc = forge_identity::OidcHs256Validator::new(
                        secret.into_bytes(),
                        issuer,
                        audience,
                    );
                    if let Ok(subjects) = std::env::var("FORGE_OIDC_SUBJECTS") {
                        for entry in subjects.split(',') {
                            let entry = entry.trim();
                            if entry.is_empty() {
                                continue;
                            }
                            let (sub, rest) = entry
                                .split_once('=')
                                .ok_or("FORGE_OIDC_SUBJECTS entries must be sub=principal:flags")?;
                            let (principal, flags) =
                                rest.split_once(':').unwrap_or((rest, "create,cancel"));
                            let (can_create_tasks, can_cancel) = parse_identity_flags(flags);
                            oidc.allow_subject(
                                sub,
                                forge_identity::TrustedIdentity {
                                    principal: forge_domain::PrincipalId(principal.to_owned()),
                                    can_create_tasks,
                                    can_cancel,
                                },
                            );
                        }
                    }
                    forge_mcp_server::McpServer::with_oidc(identities, oidc, policy, repo)
                }
                (None, None, None) => forge_mcp_server::McpServer::new(identities, policy, repo),
                _ => {
                    return Err(
                        "FORGE_OIDC_SECRET, FORGE_OIDC_ISSUER and FORGE_OIDC_AUDIENCE must be set together"
                            .into(),
                    );
                }
            };
            let stdin = io::stdin();
            let mut input = stdin.lock();
            let mut output = io::stdout();
            forge_mcp_server::serve_stdio(&server, &mut input, &mut output)?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("forge: {error}");
            ExitCode::FAILURE
        }
    }
}

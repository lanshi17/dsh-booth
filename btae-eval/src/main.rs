use btae_eval::Budget;
use btae_eval::ClusterPair;
use btae_eval::Manifest;
use btae_eval::Observation;
use clap::Parser;
use clap::Subcommand;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    version,
    about = "BTAE preparation, sandbox diagnostics and explicit engineering process collection"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run an isolated synthetic worker, not a real study or model.
    #[cfg(target_os = "linux")]
    Diagnose(Box<DiagnosticArgs>),
    /// Collect one bounded external headless process (engineering smoke only).
    #[cfg(target_os = "linux")]
    Collect {
        #[arg(long)]
        executable: PathBuf,
        #[arg(long = "arg", allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long)]
        prompt: Option<PathBuf>,
        #[arg(long = "pass-env")]
        environment: Vec<String>,
        #[arg(long)]
        cancel_file: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 300_000)]
        wall_ms: u64,
    },
    /// Export and collect one isolated first-round task (engineering only).
    #[cfg(target_os = "linux")]
    CollectTask(Box<TaskArgs>),
    /// Verify sealed task artifacts against the frozen plan and public input.
    #[cfg(target_os = "linux")]
    VerifyTask {
        directory: PathBuf,
        manifest: PathBuf,
    },
    /// Check a diagnostic's file hashes and its binding to a frozen run plan.
    #[cfg(target_os = "linux")]
    VerifyDiagnostic {
        directory: PathBuf,
        manifest: PathBuf,
    },
    /// Validate draft inputs, allowing unresolved launch blockers.
    Validate { manifest: PathBuf },
    /// Check launch prerequisites. Draft/unreviewed studies return nonzero.
    CheckReady { manifest: PathBuf },
    /// Emit a deterministic randomized run plan; never starts workers.
    Plan { manifest: PathBuf },
    /// Score evaluator-collected input. Worker reports are not accepted as evidence.
    Score { input: PathBuf },
    /// Exploratory repository-level paired bootstrap on external aggregate inputs.
    Bootstrap {
        input: PathBuf,
        #[arg(long, default_value_t = 20261102)]
        seed: u64,
        #[arg(long, default_value_t = 10000)]
        resamples: usize,
    },
}

#[cfg(target_os = "linux")]
#[derive(clap::Args)]
struct DiagnosticArgs {
    #[arg(long, default_value = "datasets/pilot.json")]
    manifest: PathBuf,
    #[arg(long)]
    fixture_worker: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "queue-r01")]
    task: String,
    #[arg(long, default_value = "A")]
    arm: String,
    #[arg(long, default_value_t = 1)]
    repeat: u32,
    #[arg(long, value_enum, default_value = "success")]
    scenario: btae_eval::runner::Scenario,
    #[arg(long, default_value_t = 5000)]
    wall_ms: u64,
    #[arg(long, default_value_t = 32)]
    tokens: u64,
    #[arg(long)]
    cancel_file: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
#[derive(clap::Args)]
struct TaskArgs {
    #[arg(long, default_value = "datasets/pilot.json")]
    manifest: PathBuf,
    #[arg(long)]
    executable: PathBuf,
    #[arg(long = "arg", allow_hyphen_values = true)]
    args: Vec<String>,
    #[arg(long)]
    bundle: Option<PathBuf>,
    #[arg(long)]
    profile: Option<PathBuf>,
    #[arg(long = "pass-env")]
    environment: Vec<String>,
    #[arg(long)]
    share_network: bool,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "queue-r01")]
    task: String,
    #[arg(long, default_value = "A")]
    arm: String,
    #[arg(long, default_value_t = 1)]
    repeat: u32,
    #[arg(long, default_value_t = 300_000)]
    wall_ms: u64,
    #[arg(long)]
    cancel_file: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoreInput {
    expected_impact: BTreeSet<String>,
    budget: Budget,
    observation: Observation,
}

fn read<T: DeserializeOwned>(p: PathBuf) -> Result<T, Box<dyn Error + Send + Sync>> {
    Ok(serde_json::from_slice(&std::fs::read(p)?)?)
}
fn print(value: &impl serde::Serialize) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn run(cli: Cli) -> Result<(), Box<dyn Error + Send + Sync>> {
    match cli.command {
        #[cfg(target_os = "linux")]
        Command::CollectTask(args) => {
            let mut environment = BTreeMap::new();
            for name in args.environment {
                if environment.contains_key(&name) {
                    return Err("duplicate environment variable name".into());
                }
                let value = std::env::var(&name)
                    .map_err(|_| "requested environment variable missing or not UTF-8")?;
                environment.insert(name, value);
            }
            let record =
                btae_eval::task_collect::collect_task(btae_eval::task_collect::TaskRequest {
                    manifest: args.manifest,
                    task_id: args.task,
                    arm: args.arm,
                    repeat: args.repeat,
                    executable: args.executable,
                    args: args.args,
                    bundle: args.bundle,
                    profile: args.profile,
                    environment,
                    share_network: args.share_network,
                    output: args.output,
                    cancel_file: args.cancel_file,
                    limits: btae_eval::dsh_live::LiveLimits {
                        wall_ms: args.wall_ms,
                        ..Default::default()
                    },
                })?;
            print(&record)?;
            if record.outcome != "completed" {
                return Err(
                    "engineering task process did not complete; see sealed task receipt".into(),
                );
            }
        }
        #[cfg(target_os = "linux")]
        Command::VerifyTask {
            directory,
            manifest,
        } => {
            print(&btae_eval::task_collect::verify_task(
                &directory, &manifest,
            )?)?;
        }
        #[cfg(target_os = "linux")]
        Command::Collect {
            executable,
            args,
            prompt,
            environment,
            cancel_file,
            wall_ms,
            output,
        } => {
            let mut prompt_bytes = Vec::new();
            let reader: Box<dyn std::io::Read> = match prompt {
                Some(path) => Box::new(std::fs::File::open(path)?),
                None => Box::new(std::io::stdin()),
            };
            reader
                .take((btae_eval::dsh_live::MAX_PROMPT_BYTES + 1) as u64)
                .read_to_end(&mut prompt_bytes)?;
            let mut child_env = BTreeMap::new();
            for name in environment {
                if name.is_empty()
                    || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    || child_env.contains_key(&name)
                {
                    return Err("invalid or duplicate environment variable name".into());
                }
                let value = std::env::var(&name)
                    .map_err(|_| "requested environment variable missing or not UTF-8")?;
                child_env.insert(name, value);
            }
            let args = if args.is_empty() {
                vec![
                    "--profile".into(),
                    "headless".into(),
                    "--json".into(),
                    "-".into(),
                ]
            } else {
                args
            };
            let record = btae_eval::dsh_live::run_live(btae_eval::dsh_live::LiveRequest {
                executable,
                args,
                prompt: prompt_bytes,
                cwd: None,
                environment: child_env,
                cancel_file,
                output,
                limits: btae_eval::dsh_live::LiveLimits {
                    wall_ms,
                    ..Default::default()
                },
            })?;
            print(&record)?;
            if record.outcome != btae_eval::dsh_live::LiveOutcome::Completed {
                return Err("engineering process did not complete; see collected record".into());
            }
        }
        #[cfg(target_os = "linux")]
        Command::Diagnose(args) => {
            let request = btae_eval::runner::DiagnosticRequest {
                manifest: args.manifest,
                task_id: args.task,
                arm: args.arm,
                repeat: args.repeat,
                fixture_worker: args.fixture_worker,
                output: args.output,
                scenario: args.scenario,
                limits: btae_eval::runner::Limits {
                    wall_ms: args.wall_ms,
                    tokens: args.tokens,
                    ..Default::default()
                },
                host_probe: None,
                host_address: None,
                cancel_file: args.cancel_file,
            };
            print(&btae_eval::runner::run_diagnostic(request)?)?;
        }
        #[cfg(target_os = "linux")]
        Command::VerifyDiagnostic {
            directory,
            manifest,
        } => {
            let record = btae_eval::runner::verify_diagnostic(&directory, &manifest)?;
            print(
                &serde_json::json!({"valid_diagnostic":true,"run_id":record.run.run_id,
                "data_kind":record.data_kind,"outcome":record.outcome,"empirical_task_success":null}),
            )?;
        }
        Command::Validate { manifest } => {
            let m: Manifest = read(manifest)?;
            m.validate()?;
            print(
                &serde_json::json!({"input_valid":true,"declarations_complete":m.blockers().is_empty(),"blockers":m.blockers(),"note":"Validation does not launch a study or verify host enforcement."}),
            )?;
        }
        Command::CheckReady { manifest } => {
            let m: Manifest = read(manifest)?;
            let blockers = m.blockers();
            print(
                &serde_json::json!({"declarations_complete":blockers.is_empty(),"blockers":blockers,
                "note":"This checks declarations only; host-enforced isolation and evidence must be audited separately."}),
            )?;
            if !blockers.is_empty() {
                return Err("study is not ready for empirical execution".into());
            }
        }
        Command::Plan { manifest } => {
            let bytes = std::fs::read(manifest)?;
            let m = serde_json::from_slice(&bytes)?;
            print(&btae_eval::plan(&m, &bytes)?)?;
        }
        Command::Score { input } => {
            let input: ScoreInput = read(input)?;
            print(&btae_eval::score(
                &input.observation,
                &input.expected_impact,
                &input.budget,
            )?)?;
        }
        Command::Bootstrap {
            input,
            seed,
            resamples,
        } => {
            let pairs: Vec<ClusterPair> = read(input)?;
            print(&btae_eval::cluster_bootstrap(&pairs, seed, resamples)?)?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("btae-eval: {e}");
            ExitCode::FAILURE
        }
    }
}

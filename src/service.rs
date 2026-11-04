//! Service lifecycle: the same binary that talks to the API also owns the service behind it.
//!
//! `booth init` prepares a DSH profile from the pinned `superagent/` checkout; `booth start`
//! runs `dsh --profile <name> --no-open` detached and waits for the management API; `booth stop`
//! ends it. No tmux, no shell script: the pid and the log live under the state directory.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::api::Api;

const DEFAULT_PORT: u16 = 3082;
const READY_TIMEOUT: Duration = Duration::from_secs(90);
const STOP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct Config {
    pub profile: String,
    pub port: u16,
    pub dsh_home: PathBuf,
    pub state_dir: PathBuf,
    pub bundle: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let profile = std::env::var("BOOTH_PROFILE").unwrap_or_else(|_| "superagent".to_owned());
        let port = std::env::var("BOOTH_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        let dsh_home = match std::env::var_os("DSH_HOME") {
            Some(value) => PathBuf::from(value),
            None => PathBuf::from(
                std::env::var_os("HOME").ok_or("HOME is not set; set DSH_HOME instead")?,
            )
            .join(".dsh"),
        };
        let state_dir = match std::env::var_os("BOOTH_STATE_DIR") {
            Some(value) => PathBuf::from(value),
            None => dsh_home.join("state").join(&profile),
        };
        Ok(Self {
            bundle: bundle_dir()?,
            profile,
            port,
            dsh_home,
            state_dir,
        })
    }

    pub fn profile_dir(&self) -> PathBuf {
        self.dsh_home.join("profiles").join(&self.profile)
    }

    fn patch(&self) -> PathBuf {
        self.profile_dir().join("cordis.patch.yml")
    }

    fn log(&self) -> PathBuf {
        self.state_dir.join("booth.log")
    }

    fn pid(&self) -> PathBuf {
        self.state_dir.join("booth.pid")
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

fn bundle_dir() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("BOOTH_SUPERAGENT_DIR") {
        return Ok(PathBuf::from(value));
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("superagent");
    if manifest.join("package.json").is_file() {
        return Ok(manifest);
    }
    let cwd = std::env::current_dir()
        .map_err(|error| format!("cannot read the working directory: {error}"))?
        .join("superagent");
    if cwd.join("package.json").is_file() {
        return Ok(cwd);
    }
    Err("cannot find superagent/; run booth from the repository or set BOOTH_SUPERAGENT_DIR".into())
}

pub fn init(cfg: &Config) -> Result<(), String> {
    println!("profile  {}", cfg.profile_dir().display());
    println!("bundle   {}", cfg.bundle.display());
    println!("state    {}", cfg.state_dir.display());

    tool("dsh", "--version", "install DSH first")?;
    tool("pnpm", "--version", "install pnpm first")?;

    if !cfg.profile_dir().exists() {
        println!("\ncreating profile `{}` from the web template", cfg.profile);
        run(
            "dsh",
            &[
                cfg.profile.as_str(),
                "--from-default-profile",
                "web",
                "--dump-config",
            ],
            &cfg.dsh_home,
        )?;
    }

    write_patch(cfg)?;
    build(cfg)?;
    install(cfg)?;

    fs::create_dir_all(&cfg.state_dir)
        .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
    println!("\nready — `booth up`, then `/setup owner/repo` in the console or in the panel");
    Ok(())
}

pub fn start(cfg: &Config) -> Result<(), String> {
    if let Some(pid) = read_pid(cfg) {
        if alive(pid) {
            println!("already running (pid {pid})");
            return report(cfg);
        }
        let _ = fs::remove_file(cfg.pid());
    }
    if Api::new(&cfg.url()).state().is_ok() {
        println!(
            "a service already listens on {} — not started by booth",
            cfg.url()
        );
        return report(cfg);
    }
    if !cfg.bundle.join("lib/index.js").is_file() {
        return Err("superagent is not built yet; run `booth init` first".into());
    }
    if !cfg.patch().is_file() {
        return Err("the profile has no booth config yet; run `booth init` first".into());
    }
    if !linked(cfg, "dsh-booth-local") {
        println!("the booth console Source is not linked into the profile; run `booth init` to install it");
    }

    fs::create_dir_all(&cfg.state_dir)
        .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(cfg.log())
        .map_err(|error| format!("cannot open {}: {error}", cfg.log().display()))?;
    let errors = log
        .try_clone()
        .map_err(|error| format!("cannot open the log twice: {error}"))?;

    let store = crate::provider::Store::load(cfg);
    // A session of its own: the service must outlive the terminal that started it, including
    // a terminal that closes while `booth restart` is still running inside it.
    let mut command = if which("setsid") {
        let mut command = Command::new("setsid");
        command.arg("dsh");
        command
    } else {
        let mut command = Command::new("nohup");
        command.arg("dsh");
        command
    };
    command.args(["--profile", cfg.profile.as_str()]);
    if !store.providers.is_empty() && crate::provider::overlay(cfg).is_file() {
        command.arg("--patch").arg(crate::provider::overlay(cfg));
    }
    command
        .arg("--no-open")
        .current_dir(&cfg.bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(errors));
    for (name, value) in store.environment() {
        command.env(name, value);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("cannot start dsh: {error}"))?;
    let pid = child.id();
    fs::write(cfg.pid(), pid.to_string())
        .map_err(|error| format!("cannot write {}: {error}", cfg.pid().display()))?;
    println!("starting dsh (pid {pid}); log {}", cfg.log().display());

    let api = Api::new(&cfg.url());
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if api.state().is_ok() {
            return report(cfg);
        }
        if !alive(pid) {
            tail(cfg, 12);
            return Err(format!(
                "dsh exited while starting; see {}",
                cfg.log().display()
            ));
        }
        if Instant::now() > deadline {
            tail(cfg, 12);
            return Err(format!(
                "no answer on {} after {}s; see {}",
                cfg.url(),
                READY_TIMEOUT.as_secs(),
                cfg.log().display()
            ));
        }
        sleep(Duration::from_millis(400));
    }
}

pub fn stop(cfg: &Config) -> Result<(), String> {
    let Some(pid) = read_pid(cfg) else {
        return Err("no pid file — the service was not started by booth".into());
    };
    if !alive(pid) {
        let _ = fs::remove_file(cfg.pid());
        println!("not running (stale pid {pid})");
        return Ok(());
    }
    signal(pid, "TERM")?;
    let deadline = Instant::now() + STOP_TIMEOUT;
    while alive(pid) {
        if Instant::now() > deadline {
            signal(pid, "KILL")?;
            break;
        }
        sleep(Duration::from_millis(200));
    }
    let _ = fs::remove_file(cfg.pid());
    println!("stopped (pid {pid})");
    Ok(())
}

pub fn restart(cfg: &Config) -> Result<(), String> {
    if cfg.pid().exists() {
        stop(cfg)?;
    }
    start(cfg)
}

pub fn logs(cfg: &Config, lines: usize) -> Result<(), String> {
    tail(cfg, lines);
    Ok(())
}

fn report(cfg: &Config) -> Result<(), String> {
    let state = Api::new(&cfg.url()).state().map_err(String::from)?;
    let runtime = state
        .health
        .get("runtime")
        .map(|health| health.state.as_str())
        .unwrap_or("?");
    println!(
        "up · {} · runtime {} · {}/{} running · {} queued",
        cfg.url(),
        runtime,
        state.load.running,
        state.load.max_workers,
        state.load.queued
    );
    if let Some(problem) = &state.setup.problem {
        println!("setup pending: {}", problem.detail);
        println!("hint: /setup owner/repo in the console, or `booth setup owner/repo`");
    }
    Ok(())
}

fn write_patch(cfg: &Config) -> Result<(), String> {
    let path = cfg.patch();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let console = format!(
        "- id: dsh-booth-local\n  config:\n    stateDir: {}\n",
        cfg.state_dir.display()
    );
    if path.exists() {
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        if text.contains("dsh-superagent") {
            // A profile from an earlier booth lacks the console Source; add only that entry.
            if !text.contains("dsh-booth-local") {
                let separator = if text.ends_with('\n') { "" } else { "\n" };
                let mut next = format!("{text}{separator}{console}");
                if !next.ends_with('\n') {
                    next.push('\n');
                }
                fs::write(&path, next)
                    .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            }
            return Ok(());
        }
        // A fresh profile ships a comment and an empty array; keep the comment, replace the array.
        let kept: Vec<&str> = text
            .lines()
            .filter(|line| line.trim() != "[]" && !line.trim().is_empty())
            .collect();
        let custom = kept.iter().any(|line| !line.trim_start().starts_with('#'));
        if custom {
            return Err(format!(
                "{} already holds custom entries; add the webserver, dsh-superagent and dsh-booth-local entries yourself",
                path.display()
            ));
        }
        let header = if kept.is_empty() {
            String::new()
        } else {
            format!("{}\n", kept.join("\n"))
        };
        let yaml = format!(
            "{header}- id: webserver\n  config:\n    host: 127.0.0.1\n    port: {}\n- id: dsh-superagent\n  config:\n    stateDir: {}\n{console}",
            cfg.port,
            cfg.state_dir.display()
        );
        return fs::write(&path, yaml)
            .map_err(|error| format!("cannot write {}: {error}", path.display()));
    }
    let yaml = format!(
        "- id: webserver\n  config:\n    host: 127.0.0.1\n    port: {}\n- id: dsh-superagent\n  config:\n    stateDir: {}\n{console}",
        cfg.port,
        cfg.state_dir.display()
    );
    fs::write(&path, yaml).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn build(cfg: &Config) -> Result<(), String> {
    if !cfg.bundle.join("node_modules").is_dir() {
        println!("\ninstalling superagent dependencies");
        run("pnpm", &["install"], &cfg.bundle)?;
    }
    println!("\nbuilding superagent");
    run("pnpm", &["build"], &cfg.bundle)
}

fn install(cfg: &Config) -> Result<(), String> {
    link(cfg, "dsh-superagent", &cfg.bundle)?;
    let console = cfg
        .bundle
        .parent()
        .map(|repo| repo.join("local-source"))
        .ok_or("cannot find local-source/ beside superagent/")?;
    if !console.join("index.js").is_file() {
        return Err(format!(
            "cannot find the console Source at {}; it ships beside superagent/",
            console.display()
        ));
    }
    link(cfg, "dsh-booth-local", &console)?;
    ensure_bundle(cfg, "dsh-booth-local")
}

/// Adds `package` to the profile's bundles so DSH applies its cordis patch layer.
fn ensure_bundle(cfg: &Config, package: &str) -> Result<(), String> {
    let manifest = cfg.profile_dir().join("package.json");
    let text = fs::read_to_string(&manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("cannot parse {}: {error}", manifest.display()))?;
    let bundles = value
        .get_mut("dsh")
        .and_then(|dsh| dsh.get_mut("profile"))
        .and_then(|profile| profile.get_mut("bundles"))
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| format!("{} has no dsh.profile.bundles", manifest.display()))?;
    if bundles.iter().any(|item| item == package) {
        return Ok(());
    }
    bundles.push(serde_json::Value::String(package.to_owned()));
    let mut next = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("cannot write {}: {error}", manifest.display()))?;
    next.push('\n');
    fs::write(&manifest, next)
        .map_err(|error| format!("cannot write {}: {error}", manifest.display()))
}

/// Whether the profile lists `package`, so its bundle patch applies.
fn linked(cfg: &Config, package: &str) -> bool {
    fs::read_to_string(cfg.profile_dir().join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| {
            value
                .get("dependencies")
                .and_then(|deps| deps.get(package))
                .map(|_| true)
        })
        .unwrap_or(false)
}

/// Links one package into the profile with `dsh plugin add`, which runs pnpm in the profile.
fn link(cfg: &Config, package: &str, path: &Path) -> Result<(), String> {
    let manifest = cfg.profile_dir().join("package.json");
    let text = fs::read_to_string(&manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    let package_json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("cannot parse {}: {error}", manifest.display()))?;
    let want = format!("link:{}", path.display());
    let current = package_json
        .get("dependencies")
        .and_then(|deps| deps.get(package))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if current == want {
        return Ok(());
    }
    println!("\nlinking {package} into profile `{}`", cfg.profile);
    run(
        "dsh",
        &["plugin", "--profile", cfg.profile.as_str(), "add", &want],
        &cfg.profile_dir(),
    )
}

fn run(program: &str, args: &[&str], cwd: &Path) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|error| format!("cannot run {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {} failed ({status})", args.join(" ")))
    }
}

fn tool(program: &str, arg: &str, hint: &str) -> Result<(), String> {
    let status = Command::new(program)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| format!("{program} is not installed; {hint}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {arg}` failed; {hint}"))
    }
}

fn signal(pid: u32, name: &str) -> Result<(), String> {
    let status = Command::new("kill")
        .args([format!("-{name}"), pid.to_string()])
        .status()
        .map_err(|error| format!("cannot run kill: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill -{name} {pid} failed ({status})"))
    }
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn read_pid(cfg: &Config) -> Option<u32> {
    fs::read_to_string(cfg.pid())
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

fn alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

fn tail(cfg: &Config, lines: usize) {
    match fs::read_to_string(cfg.log()) {
        Ok(text) => {
            let all: Vec<&str> = text.lines().collect();
            for line in &all[all.len().saturating_sub(lines)..] {
                println!("{line}");
            }
        }
        Err(error) => println!("no log at {} ({error})", cfg.log().display()),
    }
}

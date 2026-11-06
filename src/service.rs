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

    pub fn patch(&self) -> PathBuf {
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
        if alive(pid) && is_service(pid, &cfg.profile) {
            println!("already running (pid {pid})");
            return report(cfg);
        }
        // A stale or reused pid must not stop a start.
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
    // An earlier booth kept providers in its own file; move them into the profile on the way up.
    if let Some(note) = migration_note(crate::provider::migrate(cfg)) {
        println!("{note}");
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

    // No `--patch` and no injected keys: the profile's own layer is where models live, so the
    // service starts with exactly the configuration the web panel and the profile share.
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
    command
        .arg("--no-open")
        .current_dir(&cfg.bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(errors));
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
    if !is_service(pid, &cfg.profile) {
        return Err(format!(
            "pid {pid} is not the `{}` service — the pid file is stale; check /proc/{pid}/cmdline \
             and remove {} yourself",
            cfg.profile,
            cfg.pid().display()
        ));
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

/// The DSH session tree for this state directory, such as
/// `~/.dsh/sessions/--home-me-.dsh-state-superagent--`.
pub fn session_root(cfg: &Config) -> PathBuf {
    let encoded = format!(
        "--{}--",
        cfg.state_dir
            .display()
            .to_string()
            .trim_start_matches('/')
            .replace('/', "-")
    );
    cfg.dsh_home.join("sessions").join(encoded)
}

/// Moves the sessions aside so the next ones are created with the current default model.
///
/// A DSH session keeps the provider and model it was created with. After a sign-in or a model
/// switch that provider can disappear, and every turn then fails with `no adapter registered for
/// provider …` — an error a person cannot act on. The sessions are moved, not deleted, so the
/// transcripts can still be read.
pub fn reset_sessions(cfg: &Config) -> Result<Option<PathBuf>, String> {
    let root = session_root(cfg);
    if !root.is_dir()
        || fs::read_dir(&root)
            .map(|mut it| it.next().is_none())
            .unwrap_or(true)
    {
        return Ok(None);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or_default();
    let target = cfg.state_dir.join(format!("sessions-{stamp}"));
    fs::create_dir_all(&target)
        .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("cannot read {}: {error}", root.display()))?
        .flatten()
    {
        let destination = target.join(entry.file_name());
        fs::rename(entry.path(), destination)
            .map_err(|error| format!("cannot move {} aside: {error}", entry.path().display()))?;
    }
    Ok(Some(target))
}

/// How many sessions the state directory holds, router and workers together.
pub fn session_count(cfg: &Config) -> usize {
    fs::read_dir(session_root(cfg))
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

/// The profile's own patch layer, which DSH composes without any `--patch` flag.
pub struct Patch {
    /// Leading comments, kept verbatim across a rewrite.
    header: String,
    entries: Vec<serde_json::Value>,
}

impl Patch {
    /// Reads the layer, refusing anything a YAML parser cannot reproduce.
    ///
    /// Refusing matters: the write below re-serializes the whole list, so a layer holding
    /// something this cannot parse — a `!!js` expression, say — would lose it.
    pub fn read(cfg: &Config) -> Result<Self, String> {
        let path = cfg.patch();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    header: String::new(),
                    entries: Vec::new(),
                })
            }
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        };
        let (header, body) = split_header(&text);
        // A tag expression cannot survive re-serialization, so the layer is left alone rather
        // than quietly rewritten without it.
        if body.contains("!!") {
            return Err(format!(
                "{} holds a YAML tag expression, which a rewrite would drop; edit it by hand",
                path.display()
            ));
        }
        if body.trim().is_empty() {
            return Ok(Self {
                header,
                entries: Vec::new(),
            });
        }
        let entries: Vec<serde_json::Value> = serde_norway::from_str(&body)
            .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
        Ok(Self { header, entries })
    }

    pub fn write(&self, cfg: &Config) -> Result<(), String> {
        let path = cfg.patch();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        let body = serde_norway::to_string(&self.entries)
            .map_err(|error| format!("cannot render {}: {error}", path.display()))?;
        let text = if self.header.is_empty() {
            body
        } else {
            format!("{}{body}", self.header)
        };
        fs::write(&path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
    }

    pub fn get(&self, id: &str) -> Option<&serde_json::Value> {
        self.entries
            .iter()
            .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(id))
    }

    /// Sets one entry's `config`, merging into whatever it already holds.
    pub fn set_config(&mut self, id: &str, config: serde_json::Value) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(id))
        {
            entry["config"] = config;
            return;
        }
        self.entries.push(serde_json::json!({
            "id": id,
            "config": config,
        }));
    }

    /// Sets one value inside an entry's config, creating the entry or the key as needed.
    pub fn set_value(&mut self, id: &str, path: &[&str], value: serde_json::Value) {
        let config = self
            .get(id)
            .and_then(|entry| entry.get("config"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut config = if config.is_object() {
            config
        } else {
            serde_json::json!({})
        };
        let mut cursor = &mut config;
        for part in &path[..path.len() - 1] {
            if !cursor[*part].is_object() {
                cursor[*part] = serde_json::json!({});
            }
            cursor = &mut cursor[*part];
        }
        if let Some(last) = path.last() {
            cursor[*last] = value;
        }
        self.set_config(id, config);
    }

    /// Removes one entry by id; returns whether it was there.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.get("id").and_then(|value| value.as_str()) != Some(id));
        before != self.entries.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| entry.get("id").and_then(|value| value.as_str()))
            .map(str::to_owned)
            .collect()
    }
}

/// Splits leading comment lines from the YAML document that follows them.
fn split_header(text: &str) -> (String, String) {
    let mut header = String::new();
    let mut body = String::new();
    let mut in_header = true;
    for line in text.lines() {
        if in_header && (line.trim_start().starts_with('#') || line.trim().is_empty()) {
            if !line.trim().is_empty() {
                header.push_str(line);
                header.push('\n');
            }
            continue;
        }
        in_header = false;
        body.push_str(line);
        body.push('\n');
    }
    (header, body)
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

/// What to say about the one-time provider migration, if anything.
fn migration_note(result: Result<Vec<String>, String>) -> Option<String> {
    match result {
        Ok(moved) if !moved.is_empty() => Some(format!(
            "moved {} provider(s) into the profile: {}",
            moved.len(),
            moved.join(", ")
        )),
        Ok(_) => None,
        Err(error) => Some(format!("could not migrate the old provider file: {error}")),
    }
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
        // Name both the signal and the process: the OS message alone does not.
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

/// Whether a NUL-separated command line is this profile's DSH service.
///
/// Split out from `/proc` so the argument matching is testable without spawning processes.
/// `--profile <name>` and `--profile=<name>` both count; a positional argument such as `plugin`
/// makes it a one-shot command, not the service.
fn serves_profile(cmdline: &[u8], profile: &str) -> bool {
    let words: Vec<&str> = cmdline
        .split(|byte| *byte == 0)
        .filter(|word| !word.is_empty())
        .map(|word| std::str::from_utf8(word).unwrap_or_default())
        .collect();
    let Some(program) = words.first() else {
        return false;
    };
    let name = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name != "dsh" && name != "node" {
        return false;
    }
    // `node <script> --profile X` puts the script where a native binary has nothing. A positional
    // word is a subcommand only while no option has been seen yet; after that, a positional is
    // that option's value (`--patch <file>`), which is not a subcommand. The real service runs
    // `node <script> --profile superagent --patch <file> --no-open`, so this is not cosmetic:
    // without it `booth stop` refused the service it had started itself.
    let mut rest = &words[1..];
    if name == "node" {
        if let Some(script) = rest.first() {
            if script.contains('/') || script.ends_with(".js") || script.ends_with(".mjs") {
                rest = &rest[1..];
            }
        }
    }
    let mut matched = false;
    let mut subcommand = false;
    let mut following = false;
    let mut seen_option = false;
    for word in rest {
        if following {
            matched = *word == profile;
            following = false;
            continue;
        }
        if let Some(value) = word.strip_prefix("--profile=") {
            matched = value == profile;
            seen_option = true;
            continue;
        }
        if *word == "--profile" {
            following = true;
            seen_option = true;
            continue;
        }
        if word.starts_with('-') {
            seen_option = true;
            continue;
        }
        // A positional before the first option is the subcommand; after one it is an option's
        // value, such as the file that follows `--patch`.
        if !seen_option {
            subcommand = true;
        }
    }
    matched && !subcommand
}

/// Whether `pid` runs this profile's DSH service.
///
/// A pid file outlives a crash and the kernel reuses pids, so the pid alone says nothing about
/// what is running now (SPEC defect B).
fn is_service(pid: u32, profile: &str) -> bool {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|raw| serves_profile(&raw, profile))
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str) -> Config {
        let root = std::env::temp_dir().join(format!("booth-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("profiles").join("superagent")).unwrap();
        Config {
            profile: "superagent".into(),
            port: 3082,
            dsh_home: root.join("profiles").parent().unwrap().to_path_buf(),
            state_dir: root.join("state"),
            bundle: root.join("superagent"),
        }
    }

    fn cmdline(words: &[&str]) -> Vec<u8> {
        let mut raw = Vec::new();
        for word in words {
            raw.extend_from_slice(word.as_bytes());
            raw.push(0);
        }
        raw
    }

    #[test]
    fn start_defers_to_a_service_it_did_not_start() {
        // The port already answers: booth reports that service instead of launching a second.
        let mut cfg = config("start-foreign");
        let api = crate::testkit::FakeApi::start();
        cfg.port = api.base.rsplit(':').next().unwrap().parse().unwrap();
        fs::create_dir_all(&cfg.state_dir).unwrap();
        // A stale pid file must not stop the check either.
        fs::write(cfg.pid(), std::process::id().to_string()).unwrap();
        assert!(start(&cfg).is_ok());
        // The stale pid file is cleared first, then the answering service is reported.
        assert!(!cfg.pid().exists(), "a stale pid file is removed");
        std::fs::remove_dir_all(&cfg.state_dir).unwrap();
    }

    #[test]
    fn a_failed_signal_names_the_pid_and_the_signal() {
        // SPEC scenario "目标进程已消失": the OS message alone does not say which process.
        let error = signal(0x7fff_fffe, "TERM").unwrap_err();
        assert!(error.contains("TERM"), "{error}");
        assert!(error.contains("2147483646"), "{error}");
    }

    #[test]
    fn matches_only_this_profiles_service_command_line() {
        // SPEC scenario "pid 文件指向一个不是 dsh 的进程", at the level the argument match lives.
        assert!(serves_profile(
            &cmdline(&[
                "/home/me/.bun/bin/dsh",
                "--profile",
                "superagent",
                "--no-open"
            ]),
            "superagent"
        ));
        assert!(serves_profile(
            &cmdline(&["node", "--profile=superagent", "--no-open"]),
            "superagent"
        ));
        // A node script named without a path is still a script, not a subcommand: either half of
        // the "looks like a script" test must be enough on its own.
        assert!(serves_profile(
            &cmdline(&["node", "dsh.js", "--profile", "superagent"]),
            "superagent"
        ));
        // The shape the real service runs under, captured from /proc: node, its script, then the
        // flags. The script is not a subcommand.
        assert!(serves_profile(
            &cmdline(&[
                "node",
                "/home/me/.bun/bin/dsh",
                "--profile",
                "superagent",
                "--no-open",
            ]),
            "superagent"
        ));
        // A hand-written `--patch` overlay does not change what the process is.
        assert!(serves_profile(
            &cmdline(&[
                "dsh",
                "--profile",
                "superagent",
                "--patch",
                "/tmp/extra.yml",
                "--no-open",
            ]),
            "superagent"
        ));
        // Another profile is another service.
        assert!(!serves_profile(
            &cmdline(&["dsh", "--profile", "other"]),
            "superagent"
        ));
        // A one-shot subcommand is not the long-running service.
        assert!(!serves_profile(
            &cmdline(&["dsh", "plugin", "--profile", "superagent", "add", "x"]),
            "superagent"
        ));
        // An unrelated program that happens to mention the profile.
        assert!(!serves_profile(
            &cmdline(&["/bin/bash", "-c", "dsh --profile superagent"]),
            "superagent"
        ));
        // An unrelated program carrying exactly the service's arguments: only the program name
        // separates them, so this line is what makes the name check load-bearing.
        assert!(!serves_profile(
            &cmdline(&["/usr/bin/python3", "--profile", "superagent", "--no-open"]),
            "superagent"
        ));
        // Nothing to read.
        assert!(!serves_profile(b"", "superagent"));
    }

    #[test]
    fn is_service_reads_a_live_command_line() {
        // A positive case needs a real /proc entry, so the test runs an idle node process whose
        // arguments are the service's shape: `node <script> --profile <name>`. node is already
        // required to run booth at all.
        let dir = config("is-service-live").state_dir.join("probe");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("idle.js");
        fs::write(&script, "setTimeout(() => {}, 30000)").unwrap();
        let mut child = match std::process::Command::new("node")
            .arg(&script)
            .args(["--profile", "superagent"])
            .spawn()
        {
            Ok(child) => child,
            Err(_) => {
                eprintln!("node is not available; the positive /proc case did not run");
                let _ = fs::remove_dir_all(&dir);
                return;
            }
        };
        let pid = child.id();
        for _ in 0..50 {
            if fs::read(format!("/proc/{pid}/cmdline"))
                .map(|raw| !raw.is_empty())
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let found = is_service(pid, "superagent");
        let other = is_service(pid, "another-profile");
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&dir);
        assert!(found, "a node process carrying this profile is the service");
        assert!(!other, "another profile's process is not");
    }

    #[test]
    fn is_service_recognizes_its_own_dsh_process_only() {
        // SPEC scenario "pid 文件指向一个不是 dsh 的进程": an unrelated live process must be
        // refused before any signal, so a stale or reused pid cannot be killed.
        let me = std::process::id();
        assert!(
            !is_service(me, "superagent"),
            "the test process is not a dsh service"
        );
        let cmdline = fs::read(format!("/proc/{me}/cmdline")).unwrap_or_default();
        assert!(
            !cmdline.is_empty(),
            "the test needs a readable /proc/self/cmdline"
        );
    }

    #[test]
    fn stop_refuses_a_pid_file_pointing_at_another_program() {
        let cfg = config("stop-foreign");
        fs::create_dir_all(&cfg.state_dir).unwrap();
        let me = std::process::id();
        fs::write(cfg.pid(), me.to_string()).unwrap();
        let error = stop(&cfg).unwrap_err();
        assert!(error.contains(&me.to_string()), "{error}");
        assert!(error.contains("not"), "{error}");
        // The process is still there and the pid file is left for the operator to inspect.
        assert!(alive(me));
        assert!(cfg.pid().exists());
    }

    #[test]
    fn finds_the_session_tree_of_the_state_directory() {
        let cfg = config("sessions-path");
        let root = session_root(&cfg);
        // `/home/me/.dsh/state/superagent` becomes `--home-me-.dsh-state-superagent--`.
        let encoded = cfg
            .state_dir
            .display()
            .to_string()
            .trim_start_matches('/')
            .replace('/', "-");
        assert_eq!(
            root.file_name().unwrap().to_string_lossy(),
            format!("--{encoded}--")
        );
        assert!(root.parent().unwrap().ends_with("sessions"));
    }

    #[test]
    fn moves_sessions_aside_and_tolerates_none() {
        let cfg = config("sessions-move");
        assert_eq!(reset_sessions(&cfg).unwrap(), None);
        let root = session_root(&cfg);
        fs::create_dir_all(root.join("orchestrator-x")).unwrap();
        fs::write(root.join("orchestrator-x").join("session.json"), "{}").unwrap();
        let moved = reset_sessions(&cfg).unwrap().expect("a move");
        assert!(moved.join("orchestrator-x").join("session.json").is_file());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    #[test]
    fn writes_the_three_entries_into_a_fresh_patch() {
        let cfg = config("patch-fresh");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        write_patch(&cfg).unwrap();
        let text = fs::read_to_string(cfg.patch()).unwrap();
        for entry in ["webserver", "dsh-superagent", "dsh-booth-local"] {
            assert!(text.contains(entry), "{entry} missing from {text}");
        }
        assert!(text.contains(&cfg.state_dir.display().to_string()));
    }

    #[test]
    fn keeps_the_profile_comment_and_drops_the_empty_array() {
        let cfg = config("patch-comment");
        let path = cfg.patch();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "# profile layer\n[]\n").unwrap();
        write_patch(&cfg).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# profile layer\n"), "{text}");
        assert!(!text.lines().any(|line| line.trim() == "[]"), "{text}");
        assert!(text.contains("dsh-booth-local"), "{text}");
    }

    #[test]
    fn appends_the_console_source_to_an_older_booth_patch() {
        let cfg = config("patch-older");
        let path = cfg.patch();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "- id: webserver\n  config:\n    port: 3082\n- id: dsh-superagent\n  config:\n    stateDir: /tmp/s\n",
        )
        .unwrap();
        write_patch(&cfg).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("dsh-booth-local").count(), 1, "{text}");
        // A second run adds nothing.
        write_patch(&cfg).unwrap();
        let again = fs::read_to_string(&path).unwrap();
        assert_eq!(again.matches("dsh-booth-local").count(), 1, "{again}");
    }

    #[test]
    fn refuses_to_rewrite_a_patch_with_custom_entries() {
        let cfg = config("patch-custom");
        let path = cfg.patch();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "- id: something-else\n  config: {}\n").unwrap();
        assert!(write_patch(&cfg).is_err());
    }
}

#[cfg(test)]
mod patch_tests {
    use super::*;

    fn config(name: &str) -> Config {
        let root = std::env::temp_dir().join(format!("booth-patch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("profiles").join("superagent")).unwrap();
        Config {
            profile: "superagent".into(),
            port: 3082,
            dsh_home: root.join("profiles").parent().unwrap().to_path_buf(),
            state_dir: root.join("state"),
            bundle: root.join("superagent"),
        }
    }

    #[test]
    fn sets_and_keeps_entries_a_profile_already_holds() {
        let cfg = config("set");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        fs::write(
            cfg.patch(),
            "# written by hand\n- id: webserver\n  config:\n    port: 3082\n",
        )
        .unwrap();

        let mut patch = Patch::read(&cfg).unwrap();
        patch.set_value(
            "agent-default-model",
            &["provider"],
            serde_json::json!("openrouter"),
        );
        patch.set_value("agent-default-model", &["model"], serde_json::json!("x/y"));
        patch.write(&cfg).unwrap();

        let text = fs::read_to_string(cfg.patch()).unwrap();
        assert!(text.starts_with("# written by hand\n"), "{text}");
        let entries: Vec<serde_json::Value> = serde_norway::from_str(&text).unwrap();
        assert_eq!(entries.len(), 2, "{text}");
        let reloaded = Patch::read(&cfg).unwrap();
        assert_eq!(
            reloaded.get("agent-default-model").unwrap()["config"]["provider"],
            "openrouter"
        );
        // The entry that was already there is untouched.
        assert_eq!(reloaded.get("webserver").unwrap()["config"]["port"], 3082);
    }

    #[test]
    fn merges_into_an_existing_config_instead_of_replacing_it() {
        let cfg = config("merge");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        fs::write(
            cfg.patch(),
            "- id: llm-pi-ai\n  config:\n    providers:\n      a:\n        api: openai-completions\n",
        )
        .unwrap();
        let mut patch = Patch::read(&cfg).unwrap();
        patch.set_value(
            "llm-pi-ai",
            &["providers", "a", "baseURL"],
            serde_json::json!("https://a"),
        );
        patch.write(&cfg).unwrap();
        let reloaded = Patch::read(&cfg).unwrap();
        let config = &reloaded.get("llm-pi-ai").unwrap()["config"];
        assert_eq!(
            config["providers"]["a"]["api"], "openai-completions",
            "the old key survives"
        );
        assert_eq!(config["providers"]["a"]["baseURL"], "https://a");
    }

    #[test]
    fn removes_only_the_named_entry() {
        let cfg = config("remove");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        fs::write(
            cfg.patch(),
            "- id: agent-default-model\n  config:\n    provider: a\n    model: b\n- id: webserver\n  config:\n    port: 3082\n",
        )
        .unwrap();
        let mut patch = Patch::read(&cfg).unwrap();
        assert!(patch.remove("agent-default-model"));
        assert!(
            !patch.remove("agent-default-model"),
            "removing twice changes nothing"
        );
        patch.write(&cfg).unwrap();
        assert_eq!(
            Patch::read(&cfg).unwrap().ids(),
            vec!["webserver".to_owned()]
        );
    }

    #[test]
    fn refuses_a_layer_it_cannot_reproduce() {
        // A `!!js` expression would be lost by a rewrite, so reading refuses rather than
        // silently dropping the operator's own entry.
        let cfg = config("tags");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        fs::write(
            cfg.patch(),
            "- id: hmr\n  config:\n    root: !!js process.env.X\n",
        )
        .unwrap();
        match Patch::read(&cfg) {
            Ok(patch) => panic!("a layer with !!js must not be rewritten: {:?}", patch.ids()),
            Err(error) => assert!(error.contains("tag expression"), "{error}"),
        }
    }

    #[test]
    fn the_migration_note_is_quiet_when_there_was_nothing_to_move() {
        assert_eq!(migration_note(Ok(Vec::new())), None);
        let moved = migration_note(Ok(vec!["a".into(), "b".into()])).unwrap();
        assert!(moved.contains("2 provider(s)"), "{moved}");
        assert!(moved.contains("a, b"), "{moved}");
        let failed = migration_note(Err("bad file".into())).unwrap();
        assert!(failed.contains("bad file"), "{failed}");
    }

    #[test]
    fn a_comment_only_layer_keeps_its_comment() {
        let cfg = config("comment-only");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        fs::write(cfg.patch(), "# nothing configured yet\n").unwrap();
        let patch = Patch::read(&cfg).unwrap();
        assert!(patch.ids().is_empty());
        patch.write(&cfg).unwrap();
        let text = fs::read_to_string(cfg.patch()).unwrap();
        assert!(text.starts_with("# nothing configured yet"), "{text}");
        assert!(text.contains("[]"), "the empty list is written: {text}");
    }

    #[test]
    fn a_config_that_is_not_a_map_is_replaced_by_one() {
        // A hand-written `config: something` would otherwise make the setter panic.
        let cfg = config("odd-config");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        fs::write(cfg.patch(), "- id: llm-pi-ai\n  config: not-a-map\n").unwrap();
        let mut patch = Patch::read(&cfg).unwrap();
        patch.set_value("llm-pi-ai", &["providers"], serde_json::json!({}));
        patch.write(&cfg).unwrap();
        let reloaded = Patch::read(&cfg).unwrap();
        assert!(reloaded.get("llm-pi-ai").unwrap()["config"].is_object());
    }

    #[test]
    fn an_unreadable_layer_is_reported_rather_than_ignored() {
        let cfg = config("unreadable");
        fs::create_dir_all(cfg.patch()).unwrap(); // a directory where the file belongs
        match Patch::read(&cfg) {
            Ok(patch) => panic!(
                "a directory must not read as an empty layer: {:?}",
                patch.ids()
            ),
            Err(error) => assert!(error.contains("cannot read"), "{error}"),
        }
    }

    #[test]
    fn a_missing_layer_is_empty_not_an_error() {
        let cfg = config("missing");
        let patch = Patch::read(&cfg).unwrap();
        assert!(patch.ids().is_empty());
    }
}

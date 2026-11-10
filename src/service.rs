//! Service lifecycle: the same binary that talks to the API also owns the service behind it.
//!
//! `booth init` prepares a DSH profile from the pinned `superagent/` checkout; `booth up`
//! runs `dsh --profile <name> --no-open` detached and waits for the management API; `booth stop`
//! ends it. No tmux, no shell script: the pid and the log live under the state directory.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
        let dsh_home = dsh_home()?;
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

/// DSH's `$DSH_HOME` precedence and spelling: a blank override is unset, and a leading `~` names
/// the OS home.
///
/// Every path this module prepares — the profile layer, the service's state, the session trees —
/// is derived from this directory and then handed to `dsh`, which resolves `DSH_HOME` by these
/// rules. Reading the raw value instead splits the deployment: `DSH_HOME=~/dsh` would make booth
/// write `./~/dsh/profiles/…` while dsh creates `$HOME/dsh/profiles/…`, and a blank override
/// would point booth at `./profiles/…` while dsh keeps using `$HOME/.dsh`. The command line that
/// starts the service inherits the same environment, so the two must agree here.
///
/// `provider` and `oauth` resolve the credentials store this way for the same reason; the
/// duplication is deliberate because that resolver is private to those modules.
fn dsh_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("DSH_HOME") {
        if let Some(text) = value.to_str() {
            if !text.trim().is_empty() {
                return expand_home(text);
            }
        } else {
            // A non-UTF-8 override cannot carry a `~` prefix; use the bytes as they are.
            return Ok(PathBuf::from(value));
        }
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".dsh"))
        .ok_or_else(|| "HOME is not set; set DSH_HOME instead".to_owned())
}

/// Expands the `~` prefixes DSH's home resolver accepts. A `~` that cannot be expanded is an
/// error, not a literal directory beside the working directory.
fn expand_home(text: &str) -> Result<PathBuf, String> {
    let rest = if text == "~" {
        ""
    } else if let Some(rest) = text.strip_prefix("~/") {
        rest
    } else if let Some(rest) = text.strip_prefix("~\\") {
        rest
    } else {
        return Ok(PathBuf::from(text));
    };
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "cannot expand `~` in DSH_HOME: HOME is not set".to_owned())?;
    Ok(home.join(rest))
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

    // DSH creates the profile under its home, but the command below runs *from* that home. On a
    // machine that never ran DSH — or with a fresh `DSH_HOME` — the directory does not exist yet,
    // and the spawn would fail with a bare "No such file or directory" that blames `dsh` instead
    // of the missing home. Create it first; the state directory below lives under it too.
    fs::create_dir_all(&cfg.dsh_home).map_err(|error| {
        format!(
            "cannot create the DSH home {}: {error}",
            cfg.dsh_home.display()
        )
    })?;

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
    // The management API is not the only sign that the port is taken. DSH's web server binds
    // before dsh-superagent registers its routes, and another program can own the port, so a
    // listener with no management API is still a port a second dsh cannot bind. Launching one
    // anyway would fail after `booth up` wrote its pid, leaving the pid file pointing at a dead
    // process while the service that does listen can no longer be stopped through booth.
    if port_in_use(cfg.port) {
        return Err(format!(
            "{} is already in use — a service is still starting, or another program owns the \
             port; booth did not start a second dsh",
            cfg.url()
        ));
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

    create_private_state_dir(cfg)?;
    let (log, errors) = open_log(cfg)?;

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

/// Creates the state directory, and keeps it private.
///
/// The directory holds the database, the message files and the service log; the log carries the
/// web panel's access token (see `open_log`). `create_dir_all` uses the umask, so a new directory
/// is normally world-traversable, and a directory created by an earlier booth keeps its mode.
fn create_private_state_dir(cfg: &Config) -> Result<(), String> {
    fs::create_dir_all(&cfg.state_dir)
        .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cfg.state_dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot keep {} private: {error}", cfg.state_dir.display()))?;
    }
    Ok(())
}

/// Opens the service log, and the second handle dsh's stderr is redirected to.
///
/// The service prints its web panel URL, including the access token, while it starts; the log
/// therefore holds a credential and must stay private. The file is created by this call, but an
/// earlier booth or a hand-made log keeps whatever mode it had, so it is tightened every time.
fn open_log(cfg: &Config) -> Result<(fs::File, fs::File), String> {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(cfg.log())
        .map_err(|error| format!("cannot open {}: {error}", cfg.log().display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        log.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("cannot keep {} private: {error}", cfg.log().display()))?;
    }
    let errors = log
        .try_clone()
        .map_err(|error| format!("cannot open the log twice: {error}"))?;
    Ok((log, errors))
}

pub fn stop(cfg: &Config) -> Result<(), String> {
    stop_with_timeout(cfg, STOP_TIMEOUT)
}

/// Stops the service, giving it `timeout` to exit after SIGTERM before forcing it.
///
/// The timeout is a parameter so a test can reproduce the pid-reuse window below in a fraction of
/// a second instead of paying the whole `STOP_TIMEOUT`.
fn stop_with_timeout(cfg: &Config, timeout: Duration) -> Result<(), String> {
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
    let deadline = Instant::now() + timeout;
    loop {
        // The pid is not the identity: while this waits, the service can exit and the kernel can
        // hand the pid to an unrelated program. Only a process that still carries this profile's
        // command line may be signalled — the check keeps the SIGKILL below off a reused pid.
        if !is_service(pid, &cfg.profile) {
            break;
        }
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

/// The root DSH keeps its session stores under: one project tree per working directory.
fn sessions_root(cfg: &Config) -> PathBuf {
    cfg.dsh_home.join("sessions")
}

/// DSH's name for a session's project directory, given the session's working directory.
///
/// Reproduces `projectKey` in DSH's JSONL session store: separators (`/`, `\`, `:`) collapse
/// into one `-`, every other unsafe UTF-16 code unit becomes `~XXXX` (uppercase), the leading
/// dash run is dropped, and the result is wrapped in `--` and cut to 251 characters. The
/// mapping is lossy on purpose: it is the name DSH created, not a path that can be reversed.
pub fn project_key(path: &str) -> String {
    let mut readable = String::new();
    let mut separator = false;
    for unit in path.encode_utf16() {
        match char::from_u32(unit as u32) {
            Some('/') | Some('\\') | Some(':') => {
                if !separator {
                    readable.push('-');
                }
                separator = true;
            }
            Some(ch)
                if ch == '~'
                    || !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-') =>
            {
                readable.push_str(&format!("~{unit:04X}"));
                separator = false;
            }
            Some(ch) => {
                readable.push(ch);
                separator = false;
            }
            None => {
                // A lone surrogate cannot occur in Rust text, but DSH escapes one all the same.
                readable.push_str(&format!("~{unit:04X}"));
                separator = false;
            }
        }
    }
    let readable = readable.trim_start_matches('-');
    let readable = if readable.is_empty() {
        "root"
    } else {
        readable
    };
    let readable: String = readable.chars().take(251).collect();
    format!("--{readable}--")
}

/// The path DSH keys a session tree by: the workspace's real path.
///
/// DSH canonicalizes a workspace with `fs.realpath` — trailing separators, `.`/`..` segments
/// and symlinks all resolved — records that path in the session header, and keys the session
/// tree by it. The configured spelling is not that path: a symlinked `DSH_HOME`, a `..` in
/// `BOOTH_STATE_DIR`, or a trailing separator all encode differently. A path that does not
/// exist cannot hold a session yet, so its configured spelling stands in for it. (Not applied
/// off Unix: Rust's `fs::canonicalize` returns a `\\?\` verbatim path there, and this
/// module's process checks are already `/proc`-only.)
fn real_path(path: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }
    #[cfg(not(unix))]
    {
        path.to_path_buf()
    }
}

/// The key of this deployment's orchestrator tree, as DSH made it.
fn router_key(cfg: &Config) -> String {
    project_key(&real_path(&cfg.state_dir).display().to_string())
}

/// The orchestrator Session's project tree, such as
/// `~/.dsh/sessions/--home-me-.dsh-state-superagent--`.
pub fn session_root(cfg: &Config) -> PathBuf {
    sessions_root(cfg).join(router_key(cfg))
}

/// Where the service puts a ticket's worktree, and with it that ticket's worker Session.
///
/// The profile's `executor.directory` wins; an unset directory or an unreachable service means
/// the bundle's own default, the state directory's `work/`.
pub fn execution_root(api: &Api, cfg: &Config) -> PathBuf {
    api.block("executor")
        .ok()
        .and_then(|(value, _)| {
            value
                .get("directory")
                .and_then(|directory| directory.as_str())
                .map(str::to_owned)
        })
        .filter(|directory| !directory.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| cfg.state_dir.join("work"))
}

/// This deployment's project trees: the orchestrator's, plus one per worker under the
/// execution root.
///
/// A worker's tree is named after its ticket directory, `<root>/<worktree>`, so the encoded
/// root is the prefix to match. DSH's naming is lossy (a separator and a literal `-` produce
/// the same character), which is why the router tree is matched by its own exact key.
fn project_dirs(cfg: &Config, execution_root: &Path) -> Vec<PathBuf> {
    let router = router_key(cfg);
    // A worker's tree is keyed by the real path of `<root>/<worktree>`; canonicalizing the root
    // covers a symlinked or otherwise non-canonical execution directory, which the profile may
    // name, while the worktree name is only a segment joined under it.
    let root = real_path(execution_root).display().to_string();
    // `Path` keeps a trailing separator, which would encode as an extra dash and break the
    // prefix; the service joins child names without one.
    let trimmed = root.trim_end_matches(['/', '\\']);
    let trimmed = if trimmed.is_empty() {
        root.as_str()
    } else {
        trimmed
    };
    let worker = project_key(trimmed);
    let prefix = format!("{}-", worker.strip_suffix("--").unwrap_or(&worker));
    let Ok(entries) = fs::read_dir(sessions_root(cfg)) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .map(|name| {
                    let name = name.to_string_lossy();
                    name == router || name.starts_with(&prefix)
                })
                .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    dirs
}

/// How many sessions this deployment holds: the router's and every worker's.
///
/// Sessions of ended tickets are still on disk, so they count: the number reports how much
/// conversation the deployment can still read back.
pub fn session_count(cfg: &Config, execution_root: &Path) -> usize {
    project_dirs(cfg, execution_root)
        .iter()
        .map(|dir| {
            fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|entry| entry.path().is_dir())
                        .count()
                })
                .unwrap_or(0)
        })
        .sum()
}

/// Moves this deployment's sessions aside so the next ones are created with the current
/// default model.
///
/// A DSH session keeps the provider and model it was created with. After a sign-in or a model
/// switch that provider can disappear, and every turn then fails with `no adapter registered for
/// provider …` — an error a person cannot act on. The orchestrator's tree and every worker's
/// are moved, not deleted, so the transcripts can still be read. A worker keeps the conversation
/// its ticket resumes, so moving only the orchestrator's tree would leave that failure in place.
pub fn reset_sessions(cfg: &Config, execution_root: &Path) -> Result<Option<PathBuf>, String> {
    let mut sessions = Vec::new();
    for project in project_dirs(cfg, execution_root) {
        let Ok(entries) = fs::read_dir(&project) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                sessions.push(entry.path());
            }
        }
    }
    if sessions.is_empty() {
        return Ok(None);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or_default();
    let target = cfg.state_dir.join(format!("sessions-{stamp}"));
    fs::create_dir_all(&target)
        .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
    for session in sessions {
        let destination = target.join(
            session
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("session")),
        );
        fs::rename(&session, destination)
            .map_err(|error| format!("cannot move {} aside: {error}", session.display()))?;
    }
    Ok(Some(target))
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
        let body = serde_norway::to_string(&self.entries)
            .map_err(|error| format!("cannot render {}: {error}", path.display()))?;
        let text = if self.header.is_empty() {
            body
        } else {
            format!("{}{body}", self.header)
        };
        write_staged(&path, &text)
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

/// Names two staged writes in this process differently even within the same nanosecond.
static STAGED_NONCE: AtomicU64 = AtomicU64::new(0);

/// Replaces `path` with `text` in one step.
///
/// `fs::write` truncates the target before the new bytes are written, so a crash, a full disk or a
/// reader that looks at the wrong moment sees a half-written or empty file. The profile layer is
/// not regenerable by hand: it carries the webserver port, both state directories and every
/// provider and model, so losing it leaves the service booting with defaults and the console
/// pointing at a service that is not there. The whole text is written to a sibling and renamed
/// over `path`, which commits in one step on one filesystem; a write that cannot complete leaves
/// the old file exactly as it was and removes the staged name.
fn write_staged(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_nanos())
        .unwrap_or_default();
    let staged = path.with_file_name(format!(
        "{}.{}.{stamp}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        STAGED_NONCE.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staged)?;
        // The staged name is a sibling and briefly visible: keep the layer's own mode so an
        // exclusive create does not widen a file the operator narrowed.
        #[cfg(unix)]
        {
            if let Ok(metadata) = fs::metadata(path) {
                file.set_permissions(metadata.permissions())?;
            }
        }
        file.write_all(text.as_bytes())
    })();
    let installed = written.and_then(|()| fs::rename(&staged, path));
    // On success the rename consumed the staged name and this is the harmless ENOENT; otherwise
    // it clears the half-written sibling so no partial layer outlives the failure.
    let _ = fs::remove_file(&staged);
    installed.map_err(|error| format!("cannot write {}: {error}", path.display()))
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
                write_staged(&path, &next)?;
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
        return write_staged(&path, &yaml);
    }
    let yaml = format!(
        "- id: webserver\n  config:\n    host: 127.0.0.1\n    port: {}\n- id: dsh-superagent\n  config:\n    stateDir: {}\n{console}",
        cfg.port,
        cfg.state_dir.display()
    );
    write_staged(&path, &yaml)
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

/// Whether something already listens on this deployment's loopback port.
///
/// A connect, not a bind: a bind probe would race the listener it describes, while a completed
/// connect proves one owns the port (the connection is dropped without a request). The timeout is
/// short because the check runs before a start and must not wait on a port that is not there.
fn port_in_use(port: u16) -> bool {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
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
    // A child that exited but was never reaped is still listed under /proc. It cannot answer, so
    // starting must not wait for it (the READY_TIMEOUT would pass while the console blames the
    // port rather than the dead service).
    let dir = Path::new("/proc").join(pid.to_string());
    if !dir.exists() {
        return false;
    }
    match fs::read_to_string(dir.join("status")) {
        Ok(status) => !status
            .lines()
            .any(|line| line.starts_with("State:") && line.contains('Z')),
        // Listed but not readable: the pid exists, which is all this check used to ask.
        Err(_) => true,
    }
}

/// The last `count` lines of `path`, read from the end instead of loading the whole file.
///
/// `booth.log` only grows; `booth logs` and a failed start must stay cheap on a long one.
fn tail_lines(path: &Path, count: usize) -> std::io::Result<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut window = 8 * 1024u64;
    loop {
        let start = size.saturating_sub(window);
        let mut raw = vec![0u8; (size - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut raw)?;
        let text = String::from_utf8_lossy(&raw);
        let mut lines: Vec<&str> = text.lines().collect();
        if start > 0 {
            // A window that begins inside a line leaves a fragment at the front. If the byte just
            // before the window is the newline, the window begins on a whole line and nothing is
            // dropped.
            let mut lead = [0u8; 1];
            file.seek(SeekFrom::Start(start - 1))?;
            file.read_exact(&mut lead)?;
            if lead[0] != b'\n' && !lines.is_empty() {
                lines.remove(0);
            }
        }
        if start == 0 || lines.len() >= count {
            let take = lines.len().saturating_sub(count);
            return Ok(lines[take..].iter().map(|line| line.to_string()).collect());
        }
        window = window.saturating_mul(2);
    }
}

fn tail(cfg: &Config, lines: usize) {
    match tail_lines(&cfg.log(), lines) {
        Ok(lines) => {
            for line in lines {
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

    /// Saves the variables these tests replace and puts them back, so a temp home cannot leak
    /// into the tests that spawn programs by name.
    struct HomeEnv {
        dsh_home: Option<std::ffi::OsString>,
        home: Option<std::ffi::OsString>,
        state_dir: Option<std::ffi::OsString>,
        bundle: Option<std::ffi::OsString>,
    }

    impl HomeEnv {
        fn capture() -> Self {
            Self {
                dsh_home: std::env::var_os("DSH_HOME"),
                home: std::env::var_os("HOME"),
                state_dir: std::env::var_os("BOOTH_STATE_DIR"),
                bundle: std::env::var_os("BOOTH_SUPERAGENT_DIR"),
            }
        }
    }

    impl Drop for HomeEnv {
        fn drop(&mut self) {
            for (name, value) in [
                ("DSH_HOME", self.dsh_home.take()),
                ("HOME", self.home.take()),
                ("BOOTH_STATE_DIR", self.state_dir.take()),
                ("BOOTH_SUPERAGENT_DIR", self.bundle.take()),
            ] {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    /// Every path `init` and `up` prepare comes from this directory and is then handed to `dsh`,
    /// which resolves `DSH_HOME` by its own rules: a blank override is unset and a leading `~`
    /// names the OS home. Reading the raw value splits the deployment instead.
    #[test]
    fn resolves_dsh_home_the_way_dsh_does() {
        let _env = crate::testkit::env_lock();
        let env = HomeEnv::capture();
        let home = std::env::temp_dir().join(format!("booth-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        for (value, expected) in [
            ("~", home.clone()),
            ("~/dsh", home.join("dsh")),
            ("~\\dsh", home.join("dsh")),
        ] {
            std::env::set_var("DSH_HOME", value);
            assert_eq!(dsh_home().unwrap(), expected, "DSH_HOME={value:?}");
        }

        // A blank override is unset, the way DSH treats it.
        for value in ["", "   "] {
            std::env::set_var("DSH_HOME", value);
            assert_eq!(dsh_home().unwrap(), home.join(".dsh"), "DSH_HOME={value:?}");
        }

        // A literal path is used as given; a word that merely begins with `~` is not a prefix.
        std::env::set_var("DSH_HOME", "/srv/dsh");
        assert_eq!(dsh_home().unwrap(), PathBuf::from("/srv/dsh"));
        std::env::set_var("DSH_HOME", "~other/dsh");
        assert_eq!(dsh_home().unwrap(), PathBuf::from("~other/dsh"));

        // With no override at all, DSH's default is `$HOME/.dsh`.
        std::env::remove_var("DSH_HOME");
        assert_eq!(dsh_home().unwrap(), home.join(".dsh"));

        let _ = fs::remove_dir_all(&home);
        drop(env);
    }

    #[test]
    fn the_service_paths_follow_the_home_dsh_will_use() {
        let _env = crate::testkit::env_lock();
        let env = HomeEnv::capture();
        let home = std::env::temp_dir().join(format!("booth-env-home-{}", std::process::id()));
        let bundle = std::env::temp_dir().join(format!("booth-env-bundle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);
        std::env::set_var("BOOTH_SUPERAGENT_DIR", &bundle);
        std::env::remove_var("BOOTH_STATE_DIR");

        // The empty override is unset for DSH; booth must not turn every path relative.
        std::env::set_var("DSH_HOME", "");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.dsh_home, home.join(".dsh"));
        assert!(
            cfg.state_dir.is_absolute(),
            "the state directory is not relative to some working directory: {}",
            cfg.state_dir.display()
        );
        assert_eq!(cfg.state_dir, cfg.dsh_home.join("state").join(&cfg.profile));
        assert_eq!(
            cfg.patch(),
            cfg.dsh_home
                .join("profiles")
                .join(&cfg.profile)
                .join("cordis.patch.yml")
        );

        // A `~` prefix names the same home DSH expands it to.
        std::env::set_var("DSH_HOME", "~/dsh");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.dsh_home, home.join("dsh"));
        assert_eq!(
            cfg.state_dir,
            home.join("dsh").join("state").join(&cfg.profile)
        );

        let _ = fs::remove_dir_all(&home);
        drop(env);
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

    /// A listener that answers every request the way DSH's web server does before
    /// dsh-superagent registers its routes: an empty 404. It keeps accepting so a retry sees the
    /// same "starting" state, and stops when the returned flag is set.
    fn starting_service() -> (
        u16,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = [0u8; 4096];
                        let _ = stream.read(&mut request);
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        (port, stop, handle)
    }

    #[test]
    fn a_port_is_in_use_only_while_a_listener_owns_it() {
        let (port, stop, listener) = starting_service();
        assert!(port_in_use(port), "a bound port reads as in use");
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        listener.join().unwrap();
        assert!(
            !port_in_use(port),
            "the port is free once its listener is gone"
        );
    }

    #[cfg(unix)]
    #[test]
    fn start_does_not_launch_a_second_dsh_onto_a_starting_services_port() {
        // DSH's web server binds before its management API registers. `start` asked only whether
        // the API answered, so a service in that window looked absent: booth launched a second
        // dsh (which cannot bind), wrote *its* pid over the live service's, and could then report
        // the first service as up while the pid file pointed at a dead process.
        use std::os::unix::fs::PermissionsExt;

        let _env = crate::testkit::env_lock();
        let mut cfg = config("start-starting");
        fs::create_dir_all(cfg.bundle.join("lib")).unwrap();
        fs::write(cfg.bundle.join("lib/index.js"), "").unwrap();
        fs::write(
            cfg.patch(),
            "- id: webserver\n  config:\n    port: 3082\n- id: dsh-superagent\n  config:\n    stateDir: /tmp/s\n",
        )
        .unwrap();
        let (port, stop, listener) = starting_service();
        cfg.port = port;

        // A fake `setsid` records an attempted launch; starting the real service from a test
        // would bind a real port and outlive the suite.
        let bin = cfg.state_dir.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let marker = bin.join("spawned");
        let setsid = bin.join("setsid");
        fs::write(
            &setsid,
            format!(
                "#!/bin/sh\nprintf started > '{}'\nexit 0\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&setsid, fs::Permissions::from_mode(0o755)).unwrap();
        let saved = std::env::var_os("PATH");
        let mut paths = vec![bin];
        if let Some(saved) = &saved {
            paths.extend(std::env::split_paths(saved));
        }
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());

        let outcome = start(&cfg);

        match saved {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        listener.join().unwrap();
        let spawned = marker.exists();
        let error = outcome.err().unwrap_or_default();
        let _ = fs::remove_dir_all(&cfg.dsh_home);
        assert!(
            !spawned,
            "a second dsh was launched onto a port a service is still starting on: {error}"
        );
        assert!(
            error.contains("in use"),
            "the refusal names the occupied port: {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn init_creates_a_missing_dsh_home_before_running_dsh() {
        use std::os::unix::fs::PermissionsExt;

        let _env = crate::testkit::env_lock();
        let root = std::env::temp_dir().join(format!("booth-init-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("superagent")).unwrap();
        fs::create_dir_all(root.join("local-source")).unwrap();
        fs::write(root.join("local-source").join("index.js"), "").unwrap();
        let cfg = Config {
            profile: "superagent".into(),
            port: 3082,
            dsh_home: root.join("home"), // a machine that never ran DSH
            state_dir: root.join("state"),
            bundle: root.join("superagent"),
        };
        // What `dsh --from-default-profile web --dump-config` leaves behind: the manifest the
        // rest of init reads when it links the two bundles.
        let manifest = root.join("manifest.json");
        fs::write(&manifest, r#"{"dsh":{"profile":{"bundles":[]}}}"#).unwrap();

        // A fake DSH records the directory it was run from and creates the profile there. The
        // command below cannot even be spawned unless init created the home first, so the record
        // is the assertion: on a missing home the process never starts.
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let ran = root.join("dsh-ran");
        let dsh = bin.join("dsh");
        fs::write(
            &dsh,
            format!(
                "#!/bin/sh\n\
                 case \"$1\" in\n\
                   --version) exit 0 ;;\n\
                 esac\n\
                 if [ \"$2\" = \"--from-default-profile\" ]; then\n\
                   here=$(pwd -P)\n\
                   printf '%s' \"$here\" > '{ran}'\n\
                   mkdir -p \"$here/profiles/{profile}\"\n\
                   cp '{manifest}' \"$here/profiles/{profile}/package.json\"\n\
                 fi\n\
                 exit 0\n",
                ran = ran.display(),
                manifest = manifest.display(),
                profile = cfg.profile,
            ),
        )
        .unwrap();
        let pnpm = bin.join("pnpm");
        fs::write(&pnpm, "#!/bin/sh\nexit 0\n").unwrap();
        for tool in [&dsh, &pnpm] {
            fs::set_permissions(tool, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let saved = std::env::var_os("PATH");
        let mut paths = vec![bin];
        if let Some(saved) = &saved {
            paths.extend(std::env::split_paths(saved));
        }
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());

        let outcome = init(&cfg);

        // Restore the machine's PATH before asserting: a failed expectation must not leak into
        // the tests that spawn programs by name while this one runs.
        match saved {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            fs::canonicalize(fs::read_to_string(&ran).unwrap()).unwrap(),
            fs::canonicalize(&cfg.dsh_home).unwrap(),
            "dsh runs from a home that now exists"
        );
        assert!(cfg.patch().is_file(), "the profile layer is written");
        let layers = fs::read_to_string(cfg.patch()).unwrap();
        assert!(layers.contains("dsh-superagent"), "{layers}");
        let manifest = fs::read_to_string(cfg.profile_dir().join("package.json")).unwrap();
        assert!(manifest.contains("dsh-booth-local"), "{manifest}");
        assert!(cfg.state_dir.is_dir(), "the state directory is created");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn the_state_directory_is_private_even_when_it_already_existed() {
        use std::os::unix::fs::PermissionsExt;

        let cfg = config("state-private");
        fs::create_dir_all(&cfg.state_dir).unwrap();
        fs::set_permissions(&cfg.state_dir, fs::Permissions::from_mode(0o755)).unwrap();
        create_private_state_dir(&cfg).unwrap();
        let mode = fs::metadata(&cfg.state_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "the state directory holds the database and the messages"
        );

        // A directory this run creates is private from the start, whatever the umask says.
        fs::remove_dir_all(&cfg.state_dir).unwrap();
        create_private_state_dir(&cfg).unwrap();
        let mode = fs::metadata(&cfg.state_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "a fresh state directory is private");
        std::fs::remove_dir_all(&cfg.state_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_service_log_is_private_even_when_it_already_existed() {
        use std::os::unix::fs::PermissionsExt;

        let cfg = config("log-private");
        fs::create_dir_all(&cfg.state_dir).unwrap();
        // A log written by an earlier booth (or by hand) is group/world-readable; opening it for
        // the next start must not leave the token-bearing file that way.
        fs::write(cfg.log(), "old line\n").unwrap();
        fs::set_permissions(cfg.log(), fs::Permissions::from_mode(0o644)).unwrap();

        drop(open_log(&cfg).unwrap());
        let mode = fs::metadata(cfg.log()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the service log holds the web token");

        // A log this run creates is private from the start, whatever the umask says.
        fs::remove_file(cfg.log()).unwrap();
        drop(open_log(&cfg).unwrap());
        let mode = fs::metadata(cfg.log()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a fresh service log is private");
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

    /// Runs `body` in a process whose command line looks like this profile's service, and touches
    /// `ready` once `body` has installed its trap. The body can then replace the image, which is
    /// how a pid is handed to another program without the kernel reusing it.
    fn fake_service(cfg: &Config, name: &str, body: &str) -> (std::process::Child, PathBuf) {
        let dir = cfg.state_dir.join("fake-service");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join(name);
        let ready = dir.join("ready");
        fs::write(
            &script,
            format!(
                "#!/bin/bash\n{body}\ntouch '{}'\nwhile :; do sleep 0.05; done\n",
                ready.display()
            ),
        )
        .unwrap();
        let child = Command::new("bash")
            .args([
                "-c",
                &format!(
                    "exec -a node /bin/bash '{}' --profile superagent",
                    script.display()
                ),
            ])
            .spawn()
            .expect("bash");
        for _ in 0..100 {
            if ready.exists() {
                break;
            }
            sleep(Duration::from_millis(20));
        }
        assert!(ready.exists(), "the fake service did not become ready");
        (child, dir)
    }

    /// Waits up to two seconds for a killed child to be reaped, then reports whether it is gone.
    fn reaped(child: &mut std::process::Child, pid: u32) -> bool {
        for _ in 0..100 {
            if child.try_wait().unwrap().is_some() {
                return !alive(pid);
            }
            sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        false
    }

    #[test]
    fn stop_leaves_a_pid_the_service_handed_to_another_program() {
        // The service exits on TERM and its pid starts naming an unrelated program before the
        // timeout. Only a process still carrying the service command line may be forced, so the
        // program that inherited the pid must survive.
        let cfg = config("stop-reused-pid");
        let (mut child, dir) = fake_service(
            &cfg,
            "reused.sh",
            "trap 'exec -a booth-innocent /bin/sleep 30' TERM",
        );
        let pid = child.id();
        fs::write(cfg.pid(), pid.to_string()).unwrap();
        let outcome = stop_with_timeout(&cfg, Duration::from_millis(400));
        // SIGKILL takes effect asynchronously, so give a killed child time to become the zombie
        // that `try_wait` reports. The helper sleeps longer than this on its own.
        sleep(Duration::from_millis(200));
        let survived = child.try_wait().unwrap().is_none();
        let removed = !cfg.pid().exists();
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&dir);
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(survived, "the program that reused the pid was killed");
        assert!(removed, "the pid file is cleared once the service is gone");
    }

    #[test]
    fn stop_forces_a_service_that_ignores_term() {
        // The identity check must not turn into a refusal to stop: a service that stays put past
        // the timeout is still the same process and is killed.
        let cfg = config("stop-force");
        let (mut child, dir) = fake_service(&cfg, "ignore.sh", "trap '' TERM");
        let pid = child.id();
        fs::write(cfg.pid(), pid.to_string()).unwrap();
        let outcome = stop_with_timeout(&cfg, Duration::from_millis(300));
        let gone = reaped(&mut child, pid);
        let removed = !cfg.pid().exists();
        let _ = fs::remove_dir_all(&dir);
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(
            gone,
            "a service that ignores TERM is killed at the deadline"
        );
        assert!(removed);
    }

    #[test]
    fn names_the_session_tree_the_way_dsh_does() {
        // The store's convention, from `projectKey` in its JSONL session backend.
        assert_eq!(
            project_key("/home/me/.dsh/state/superagent"),
            "--home-me-.dsh-state-superagent--"
        );
        // Unsafe code units are escaped rather than passed through.
        assert_eq!(project_key("/home/me/my work"), "--home-me-my~0020work--");
        // Separator runs collapse, and the leading run is dropped.
        assert_eq!(project_key("//home//me/"), "--home-me---");
        assert_eq!(project_key("C:\\Users\\me"), "--C-Users-me--");
        // A non-ASCII path is escaped per UTF-16 code unit.
        assert_eq!(project_key("ünïcode"), "--~00FCn~00EFcode--");

        let cfg = config("sessions-path");
        let root = session_root(&cfg);
        assert_eq!(
            root.file_name().unwrap().to_string_lossy(),
            project_key(&cfg.state_dir.display().to_string())
        );
        assert!(root.parent().unwrap().ends_with("sessions"));
    }

    #[cfg(unix)]
    #[test]
    fn finds_the_orchestrator_tree_through_a_symlinked_state_directory() {
        use std::os::unix::fs::symlink;

        let cfg = config("sessions-realpath-router");
        let real = cfg.state_dir.with_file_name("real-state");
        fs::create_dir_all(&real).unwrap();
        symlink(&real, cfg.state_dir.clone()).unwrap();
        // DSH records the real path in the session's header, so that is the spelling it keys
        // the project tree by; the profile's configured spelling is only where the service was
        // pointed.
        let canonical = fs::canonicalize(&real).unwrap();
        let key = project_key(&canonical.display().to_string());
        fs::create_dir_all(sessions_root(&cfg).join(&key).join("orchestrator-x")).unwrap();

        assert_eq!(
            session_root(&cfg).file_name().unwrap().to_string_lossy(),
            key,
            "the tree is looked up by the path DSH keyed it with"
        );
        assert_eq!(session_count(&cfg, &cfg.state_dir.join("work")), 1);
        let _ = fs::remove_dir_all(&cfg.dsh_home);
    }

    #[cfg(unix)]
    #[test]
    fn finds_worker_trees_under_a_symlinked_execution_root() {
        use std::os::unix::fs::symlink;

        let cfg = config("sessions-realpath-worker");
        fs::create_dir_all(&cfg.state_dir).unwrap();
        let real = cfg.state_dir.join("real-work");
        let configured = cfg.state_dir.join("work");
        fs::create_dir_all(&real).unwrap();
        symlink(&real, &configured).unwrap();
        // The service joins the worktree name under the configured root, and DSH keys that
        // directory by its real path.
        let canonical = fs::canonicalize(&real).unwrap();
        let worker = sessions_root(&cfg).join(project_key(
            &canonical.join("fix-parser").display().to_string(),
        ));
        fs::create_dir_all(worker.join("worker-t-3")).unwrap();

        assert_eq!(session_count(&cfg, &configured), 1);
        // The profile may name the root with a trailing separator, which encodes as an extra
        // dash unless it is resolved the way DSH resolves it.
        assert_eq!(
            session_count(&cfg, &PathBuf::from(format!("{}/", configured.display()))),
            1
        );
        let _ = fs::remove_dir_all(&cfg.dsh_home);
    }

    #[test]
    fn counts_the_orchestrator_and_every_worker_session() {
        let cfg = config("sessions-count");
        let root = sessions_root(&cfg);
        let work = cfg.state_dir.join("work");
        let router = session_root(&cfg);
        let worker_one = root.join(project_key(&work.join("t-1").display().to_string()));
        let worker_two = root.join(project_key(&work.join("t-2").display().to_string()));
        fs::create_dir_all(router.join("orchestrator-x")).unwrap();
        fs::create_dir_all(worker_one.join("worker-t-1")).unwrap();
        fs::create_dir_all(worker_two.join("worker-t-2")).unwrap();
        // A project tree another deployment owns must not be counted.
        fs::create_dir_all(root.join("--elsewhere--").join("orchestrator-y")).unwrap();
        assert_eq!(session_count(&cfg, &work), 3);
        // A file inside a session is not a second session.
        fs::write(worker_one.join("worker-t-1").join("session.json"), "{}").unwrap();
        assert_eq!(session_count(&cfg, &work), 3);
    }

    #[test]
    fn moves_the_orchestrators_and_every_workers_sessions_aside() {
        let cfg = config("sessions-move");
        let work = cfg.state_dir.join("work");
        assert_eq!(reset_sessions(&cfg, &work).unwrap(), None);
        let root = sessions_root(&cfg);
        let router = session_root(&cfg);
        fs::create_dir_all(router.join("orchestrator-x")).unwrap();
        fs::write(router.join("orchestrator-x").join("session.json"), "{}").unwrap();
        let worker = root.join(project_key(&work.join("fix-parser").display().to_string()));
        fs::create_dir_all(worker.join("worker-t-9")).unwrap();
        fs::write(worker.join("worker-t-9").join("session.json"), "{}").unwrap();
        let foreign = root.join("--elsewhere--");
        fs::create_dir_all(foreign.join("orchestrator-y")).unwrap();

        let moved = reset_sessions(&cfg, &work).unwrap().expect("a move");
        assert!(moved.join("orchestrator-x").join("session.json").is_file());
        assert!(
            moved.join("worker-t-9").join("session.json").is_file(),
            "a worker keeps the conversation its ticket resumes"
        );
        assert_eq!(fs::read_dir(&router).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&worker).unwrap().count(), 0);
        assert!(
            foreign.join("orchestrator-y").is_dir(),
            "another deployment's tree is untouched"
        );
        assert_eq!(session_count(&cfg, &work), 0);
    }

    #[test]
    fn follows_a_configured_execution_directory_with_or_without_a_trailing_separator() {
        let cfg = config("sessions-custom");
        let work = PathBuf::from("/srv/booth/work");
        let worker =
            sessions_root(&cfg).join(project_key(&work.join("fix-parser").display().to_string()));
        fs::create_dir_all(worker.join("worker-t-3")).unwrap();
        assert_eq!(session_count(&cfg, &work), 1);
        // The profile may name the directory with a trailing separator; the service joins
        // child names without one, so the prefix must come out the same.
        assert_eq!(session_count(&cfg, &PathBuf::from("/srv/booth/work/")), 1);
        let moved = reset_sessions(&cfg, &PathBuf::from("/srv/booth/work/"))
            .unwrap()
            .expect("a move");
        assert!(moved.join("worker-t-3").is_dir());
    }

    #[test]
    fn reads_the_execution_directory_from_the_profile() {
        let fake = crate::testkit::FakeApi::start();
        let cfg = config("sessions-root");
        let api = Api::new(&fake.base);
        // The default is the bundle's own `<state>/work`.
        assert_eq!(execution_root(&api, &cfg), cfg.state_dir.join("work"));
        fake.settings.lock().unwrap()["blocks"]["executor"]["value"]["directory"] =
            serde_json::json!("/srv/booth/work");
        assert_eq!(execution_root(&api, &cfg), PathBuf::from("/srv/booth/work"));
        // A service that is down must not hide the default.
        let down = Api::new("http://127.0.0.1:1");
        assert_eq!(execution_root(&down, &cfg), cfg.state_dir.join("work"));
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

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The layer is the one file where the webserver port, both state directories and every
    /// provider meet. Truncating it before the new bytes are written means a crash, a full disk
    /// or a reader that looks at the wrong moment can lose all of it. A write that cannot be
    /// staged must refuse while the old bytes are still there.
    #[cfg(unix)]
    #[test]
    fn a_layer_it_cannot_stage_is_left_intact() {
        use std::os::unix::fs::PermissionsExt;

        let cfg = config("staged");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        let original = "# kept by hand\n- id: webserver\n  config:\n    port: 3082\n";
        fs::write(cfg.patch(), original).unwrap();
        let mut patch = Patch::read(&cfg).unwrap();
        patch.set_value("agent-default-model", &["provider"], serde_json::json!("x"));

        // The existing file stays writable by its owner; only the directory cannot take the
        // sibling the replacement is staged in. An in-place write would truncate and succeed.
        fs::set_permissions(cfg.profile_dir(), fs::Permissions::from_mode(0o500)).unwrap();
        let refused = patch.write(&cfg);
        fs::set_permissions(cfg.profile_dir(), fs::Permissions::from_mode(0o700)).unwrap();

        assert!(
            refused.is_err(),
            "a write that cannot stage must refuse: {refused:?}"
        );
        assert_eq!(
            fs::read_to_string(cfg.patch()).unwrap(),
            original,
            "the old layer is still there"
        );
        assert_eq!(
            names(&cfg.profile_dir()),
            ["cordis.patch.yml"],
            "a staged sibling survived"
        );

        // Once the directory takes a sibling again the same patch replaces the layer in one
        // step, and a mode the operator narrowed stays narrow.
        fs::set_permissions(cfg.patch(), fs::Permissions::from_mode(0o600)).unwrap();
        patch.write(&cfg).unwrap();
        let text = fs::read_to_string(cfg.patch()).unwrap();
        assert!(text.contains("agent-default-model"), "{text}");
        let mode = fs::metadata(cfg.patch()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the replacement keeps the layer's mode");
        assert_eq!(
            names(&cfg.profile_dir()),
            ["cordis.patch.yml"],
            "a staged sibling survived"
        );
        fs::remove_dir_all(&cfg.dsh_home).ok();
    }

    /// `booth init` appends the console entry to an older layer; an interrupted or impossible
    /// write must not leave that layer truncated, because it is the only copy of the profile's
    /// configuration.
    #[cfg(unix)]
    #[test]
    fn an_init_that_cannot_stage_leaves_the_old_patch_alone() {
        use std::os::unix::fs::PermissionsExt;

        let cfg = config("staged-init");
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        let original = "- id: webserver\n  config:\n    port: 3082\n- id: dsh-superagent\n  config:\n    stateDir: /tmp/s\n";
        fs::write(cfg.patch(), original).unwrap();

        fs::set_permissions(cfg.profile_dir(), fs::Permissions::from_mode(0o500)).unwrap();
        let refused = write_patch(&cfg);
        fs::set_permissions(cfg.profile_dir(), fs::Permissions::from_mode(0o700)).unwrap();

        assert!(
            refused.is_err(),
            "a write that cannot stage must refuse: {refused:?}"
        );
        assert_eq!(
            fs::read_to_string(cfg.patch()).unwrap(),
            original,
            "the old layer is still there"
        );
        assert_eq!(
            names(&cfg.profile_dir()),
            ["cordis.patch.yml"],
            "a staged sibling survived"
        );
        fs::remove_dir_all(&cfg.dsh_home).ok();
    }

    #[test]
    fn tail_lines_reads_only_the_end_of_a_long_log() {
        let cfg = config("tail-lines");
        fs::create_dir_all(&cfg.state_dir).unwrap();
        let path = cfg.log();
        let body: String = (0..100_000).map(|line| format!("line {line}\n")).collect();
        fs::write(&path, body).unwrap();
        assert_eq!(
            tail_lines(&path, 3).unwrap(),
            vec!["line 99997", "line 99998", "line 99999"]
        );

        // Fewer lines than asked for is the whole file, and one line is still exact.
        let small = cfg.state_dir.join("small.log");
        fs::write(&small, "a\nb\n").unwrap();
        assert_eq!(tail_lines(&small, 5).unwrap(), vec!["a", "b"]);
        assert_eq!(tail_lines(&small, 1).unwrap(), vec!["b"]);

        // A record longer than the first window is found by growing the window, not by reading
        // a truncated line.
        let long = cfg.state_dir.join("long.log");
        let wide = "x".repeat(20_000);
        fs::write(&long, format!("first\n{wide}\n")).unwrap();
        assert_eq!(tail_lines(&long, 1).unwrap(), vec![wide]);
        std::fs::remove_dir_all(cfg.state_dir).unwrap();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn a_zombie_is_not_a_running_service() {
        // `start` never waits for the child it spawns, so a dsh that exits at once stays listed
        // in /proc as a zombie; waiting for it to answer would burn the whole READY_TIMEOUT.
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut zombie = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            zombie = stat
                .rsplit_once(')')
                .map(|(_, rest)| rest.trim_start().starts_with('Z'))
                .unwrap_or(false);
            if zombie {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(zombie, "the child should be a zombie until it is reaped");
        assert!(!alive(pid), "a zombie cannot serve");
        child.wait().unwrap();
        assert!(!alive(pid), "a reaped pid is gone");
    }

    #[test]
    fn a_missing_layer_is_empty_not_an_error() {
        let cfg = config("missing");
        let patch = Patch::read(&cfg).unwrap();
        assert!(patch.ids().is_empty());
    }
}

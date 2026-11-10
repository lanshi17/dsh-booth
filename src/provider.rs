//! Model providers, shared with the DSH profile rather than kept beside it.
//!
//! There is no booth-side provider file. What the agent can run with comes from the service's
//! own catalog (`GET /models`, the same answer the web panel shows), and a change is written
//! into the profile layer DSH already composes — `agent-default-model` for the default and
//! `llm-pi-ai.providers` for an API-key provider. A key goes into the credential refs in
//! `~/.dsh/.credentials.yaml`, which is what `apiKeyEnv` resolves against.
//!
//! Nothing is passed with `--patch`, so a provider configured in the web panel and one
//! configured here are the same provider.

use crate::service::{Config, Patch};
use serde_json::{json, Value};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A provider/model pair the profile offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub provider: String,
    pub provider_name: String,
    pub model: String,
    pub model_name: String,
}

/// The service's catalog, read from `GET /models`.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub default: Option<(String, String)>,
    pub choices: Vec<Choice>,
    /// Providers the profile can route to, in the order the service lists them.
    pub routable: Vec<String>,
}

impl Catalog {
    pub fn parse(value: &Value) -> Self {
        let default = value.get("default").and_then(|entry| {
            let provider = entry.get("provider").and_then(Value::as_str)?;
            let model = entry.get("model").and_then(Value::as_str)?;
            Some((provider.to_owned(), model.to_owned()))
        });
        let mut choices = Vec::new();
        for group in value
            .get("groups")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let provider = group
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let provider_name = group
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&provider)
                .to_owned();
            for model in group
                .get("models")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let id = model.get("id").and_then(Value::as_str).unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                choices.push(Choice {
                    provider: provider.clone(),
                    provider_name: provider_name.clone(),
                    model: id.to_owned(),
                    model_name: model
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(id)
                        .to_owned(),
                });
            }
        }
        let routable = value
            .get("routableProviders")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            default,
            choices,
            routable,
        }
    }

    /// The choice matching the default, when the catalog lists it.
    pub fn current(&self) -> Option<&Choice> {
        let (provider, model) = self.default.as_ref()?;
        self.choices
            .iter()
            .find(|choice| &choice.provider == provider && &choice.model == model)
    }

    /// `provider/model`, or a plain word when the catalog names no model.
    pub fn label(&self) -> String {
        match &self.default {
            Some((provider, model)) => format!("{provider}/{model}"),
            None => "no model".into(),
        }
    }

    /// The providers, deduplicated in catalog order.
    pub fn providers(&self) -> Vec<(String, String)> {
        let mut seen: Vec<(String, String)> = Vec::new();
        for choice in &self.choices {
            if !seen.iter().any(|(id, _)| id == &choice.provider) {
                seen.push((choice.provider.clone(), choice.provider_name.clone()));
            }
        }
        seen
    }
}

/// Points the profile's default model at `provider`/`model`.
pub fn set_default(cfg: &Config, provider: &str, model: &str) -> Result<(), String> {
    let mut patch = Patch::read(cfg)?;
    point_default(&mut patch, provider, model);
    patch.write(cfg)
}

/// Selects the default model without discarding the rest of the entry.
///
/// `agent-default-model` is the profile's own config, and it carries an optional
/// `reasoningEffort` beside the pair booth writes. Rewriting the whole `config` object would
/// delete that field even when the selection does not change — which is exactly what choosing
/// the current model again, or logging in for the provider already selected, does. Only a
/// changed selection replaces the entry, so the new model's own default effort applies.
fn point_default(patch: &mut Patch, provider: &str, model: &str) {
    let unchanged = patch
        .get("agent-default-model")
        .and_then(|entry| entry.get("config"))
        .is_some_and(|config| {
            config.get("provider").and_then(Value::as_str) == Some(provider)
                && config.get("model").and_then(Value::as_str) == Some(model)
        });
    if unchanged {
        return;
    }
    patch.set_config(
        "agent-default-model",
        json!({ "provider": provider, "model": model }),
    );
}

/// Drops booth's default-model override, so the profile's own default applies again.
///
/// Used after signing in: the account is the profile's default, and an override left behind
/// would keep the agent on a provider that may no longer exist.
pub fn forget_default(cfg: &Config) -> Result<bool, String> {
    let mut patch = Patch::read(cfg)?;
    let removed = patch.remove("agent-default-model");
    if removed {
        patch.write(cfg)?;
    }
    Ok(removed)
}

/// One API-key provider, as `/login` collects it.
pub struct Endpoint {
    pub id: String,
    pub display: String,
    pub base: String,
    pub api: String,
    pub model: String,
    pub key: String,
}

/// Registers an API-key provider in the profile and stores its key as a credential ref.
///
/// The provider entry names the ref (`apiKeyEnv`), never the secret, so the profile layer stays
/// safe to read and the key lives where DSH's other keys live.
pub fn add_endpoint(cfg: &Config, endpoint: &Endpoint) -> Result<(), String> {
    if endpoint.id.trim().is_empty() || endpoint.model.trim().is_empty() {
        return Err("a provider needs an id and a model".into());
    }
    let reference = key_ref(&endpoint.id);
    let mut patch = Patch::read(cfg)?;
    for (key, value) in [
        ("apiKeyEnv", json!(reference)),
        ("displayName", json!(endpoint.display)),
        ("baseURL", json!(endpoint.base)),
        ("api", json!(endpoint.api)),
    ] {
        patch.set_value("llm-pi-ai", &["providers", &endpoint.id, key], value);
    }
    // The profile is shared with the web panel, so an entry may already list more models
    // than this form knows about, with per-model settings such as its context window. The
    // login names one model; it must not delete the rest of the provider's catalog.
    let mut models = models_of(&patch, &endpoint.id);
    if !models
        .iter()
        .any(|model| model.get("id").and_then(Value::as_str) == Some(endpoint.model.as_str()))
    {
        models.push(json!({ "id": endpoint.model }));
    }
    patch.set_value(
        "llm-pi-ai",
        &["providers", &endpoint.id, "models"],
        json!(models),
    );
    write_ref(&reference, &endpoint.key)?;
    point_default(&mut patch, &endpoint.id, &endpoint.model);
    patch.write(cfg)
}

/// The model list an existing provider entry carries, so re-registering keeps it.
fn models_of(patch: &Patch, id: &str) -> Vec<Value> {
    patch
        .get("llm-pi-ai")
        .and_then(|entry| entry.get("config"))
        .and_then(|config| config.get("providers"))
        .and_then(|providers| providers.get(id))
        .and_then(|provider| provider.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The credential ref a provider's key is stored under, such as `BOOTH_DEEPSEEK_KEY`.
pub fn key_ref(id: &str) -> String {
    let mut name = String::from("BOOTH_");
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_uppercase());
        } else {
            name.push('_');
        }
    }
    name.push_str("_KEY");
    name
}

/// Whether the credential file already holds `name`.
pub fn has_ref(name: &str) -> bool {
    credentials_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_norway::from_str::<Value>(&text).ok())
        .map(|root| root.get("refs").and_then(|refs| refs.get(name)).is_some())
        .unwrap_or(false)
}

fn credentials_path() -> Result<PathBuf, String> {
    Ok(harness_home()?.join(".credentials.yaml"))
}

/// DSH's `$DSH_HOME` precedence and spelling: a blank override is unset, and a leading `~` names
/// the OS home. Resolving the raw value instead points a provider's key at a different store than
/// the one DSH reads — for an empty override, `.credentials.yaml` in the working directory, so the
/// key would never authenticate the provider. `oauth.rs` resolves the same store this way.
fn harness_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("DSH_HOME") {
        if let Some(text) = value.to_str() {
            if !text.trim().is_empty() {
                return expand_home(text);
            }
        } else {
            return Ok(PathBuf::from(value));
        }
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".dsh"))
        .ok_or_else(|| "HOME is not set".to_owned())
}

/// Expand the `~` prefixes DSH's home resolver accepts. A `~` that cannot be expanded is an
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

/// Stores one secret as `refs.<name>`, keeping every other record and ref.
///
/// The store is shared with DSH, so the replacement is staged beside it and renamed into place
/// rather than truncating the existing file: a failure must leave the previous store whole.
///
/// The atomic rename only protects readers. This is still a read-modify-write cycle, and DSH and
/// the other booth commands write the same store: a record committed between the read and the
/// rename would be replaced by the bytes this cycle started from. Take the store's `<file>.lock`
/// sibling before reading, as DSH does, and hold it until the replacement is committed.
fn write_ref(name: &str, secret: &str) -> Result<(), String> {
    let path = credentials_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    with_store_lock(&path, || {
        let mut root: Value = match fs::read_to_string(&path) {
            Ok(text) => serde_norway::from_str(&text)
                .map_err(|error| format!("cannot parse {}: {error}", path.display()))?,
            // Only an absent file is a fresh store. Any other read failure — an unreadable mode, a
            // truncated write, an I/O error — must not read as "empty": the write below would then
            // replace the shared file and take every other ref and DSH's records with it.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                json!({ "version": 1, "refs": {}, "records": {} })
            }
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        };
        if !root.is_object() {
            root = json!({ "version": 1, "refs": {}, "records": {} });
        }
        root["version"] = json!(1);
        if !root.get("refs").map(Value::is_object).unwrap_or(false) {
            root["refs"] = json!({});
        }
        root["refs"][name] = json!(secret);
        let text = serde_norway::to_string(&root)
            .map_err(|error| format!("cannot encode credentials: {error}"))?;
        replace_store(&path, &text)
    })
}

/// Replace the shared store in one step, so a crash, a full disk or a concurrent reader never
/// sees a half-written file. The whole text is staged as a sibling with exclusive create and
/// the credential mode, then renamed over the store; a failed install leaves the old store as
/// it was and removes the staged name.
fn replace_store(path: &Path, text: &str) -> Result<(), String> {
    let staged = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        crate::oauth::uuid_v4()
    ));
    let installed = write_staged(&staged, text).and_then(|()| fs::rename(&staged, path));
    // On success the rename consumed the staged name and this is the harmless ENOENT; otherwise
    // it clears the half-written sibling so no partial store outlives the failure.
    let _ = fs::remove_file(&staged);
    installed.map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Write the complete replacement to a fresh inode. An exclusive create refuses a file already
/// at the staged name, and the credential mode is set before the rename, so the replacement is
/// never wider than the store it replaces.
fn write_staged(staged: &Path, text: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(staged)?;
    file.write_all(text.as_bytes())?;
    fs::set_permissions(staged, fs::Permissions::from_mode(0o600))
}

/// Hold the shared store's cross-process writer lock around one read-modify-rename cycle. The
/// lock is a `<store>.lock` sibling recording the writer's pid, the same file and format DSH's
/// own credentials store uses, so the two writers serialize against each other. Readers stay
/// lock-free: the rename commit is atomic.
fn with_store_lock<T>(
    store: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let lock = acquire_store_lock(store)?;
    let result = operation();
    drop(lock);
    result
}

/// A held writer lock; dropping it releases the lock file, on the error path too.
struct StoreLock {
    path: PathBuf,
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// How long a contender waits for the writer lock before reporting it. DSH sizes its own wait at
/// 30 s because a record mutation can hold the lock across a network round trip (a token refresh);
/// a shorter wait here would fail a login that raced one.
const STORE_LOCK_WAIT: Duration = Duration::from_secs(30);

fn acquire_store_lock(store: &Path) -> Result<StoreLock, String> {
    let path = lock_path(store);
    let deadline = Instant::now() + STORE_LOCK_WAIT;
    let mut delay = Duration::from_millis(20);
    loop {
        match create_lock(&path) {
            Ok(()) => return Ok(StoreLock { path }),
            // A writer that died while holding the lock must not block a login forever: take
            // over a lock only when its recorded pid is proven gone, and wait for every other
            // holder, including one whose record cannot be read.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if take_over_exited_lock(&path) {
                    continue;
                }
            }
            Err(error) => return Err(format!("cannot write {}: {error}", store.display())),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "cannot write {}: timed out waiting for the writer lock at {}",
                store.display(),
                path.display()
            ));
        }
        std::thread::sleep(delay);
        delay = (delay * 2).min(Duration::from_millis(200));
    }
}

fn lock_path(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

fn create_lock(path: &Path) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    if let Err(error) = writeln!(file, "{}", std::process::id()) {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

/// Remove a lock whose recorded holder is gone. The claim file serializes contenders, and the
/// lock is re-read and re-probed under it, so a removal never deletes a lock acquired after the
/// dead holder's — including one whose holder reused the pid.
fn take_over_exited_lock(lock: &Path) -> bool {
    let Ok(record) = fs::read_to_string(lock) else {
        return false;
    };
    if !holder_exited(&record) {
        return false;
    }
    let mut claim_name = lock.as_os_str().to_os_string();
    claim_name.push(format!(".takeover-{}", record.trim()));
    let claim = PathBuf::from(claim_name);
    let Ok(mut file) = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&claim)
    else {
        return false;
    };
    let _ = writeln!(file, "{}", std::process::id());
    let removed = fs::read_to_string(lock)
        .map(|current| current == record)
        .unwrap_or(false)
        && holder_exited(&record)
        && fs::remove_file(lock).is_ok();
    let _ = fs::remove_file(&claim);
    removed
}

/// Whether a `<pid>\n` lock record names a process that is proven gone. A malformed record, this
/// process, or a probe that cannot decide all count as "not gone": the lock is then waited for,
/// never stolen.
fn holder_exited(record: &str) -> bool {
    let Some(pid) = record
        .strip_suffix('\n')
        .and_then(|text| text.parse::<i32>().ok())
    else {
        return false;
    };
    if pid <= 0 || pid as u32 == std::process::id() {
        return false;
    }
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| !status.success())
        .unwrap_or(false)
}

/// The endpoint presets `/login` offers before "custom". Base URLs and model ids are starting
/// points; every field in the form stays editable, and what is saved is the profile entry.
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub base: &'static str,
    pub api: &'static str,
    pub model: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "deepseek",
        label: "DeepSeek API key",
        base: "https://api.deepseek.com",
        api: "openai-completions",
        model: "deepseek-chat",
    },
    Preset {
        id: "openai",
        label: "OpenAI",
        base: "https://api.openai.com/v1",
        api: "openai-responses",
        model: "gpt-5",
    },
    Preset {
        id: "openrouter",
        label: "OpenRouter",
        base: "https://openrouter.ai/api/v1",
        api: "openai-completions",
        model: "deepseek/deepseek-chat",
    },
    Preset {
        id: "custom",
        label: "Custom endpoint",
        base: "",
        api: "openai-completions",
        model: "",
    },
];

/// The protocols `llm-pi-ai` accepts, in the order the form cycles them.
pub const APIS: &[&str] = &["openai-completions", "openai-responses"];

/// Converts the file an earlier booth kept into the profile's own configuration.
///
/// Providers used to live in `<state>/provider.json` with their keys, and the service got them
/// through a `--patch` overlay. Both are gone; this moves what the file still holds into the
/// profile layer and the credential refs, keeps the provider it marked `current`, then renames it
/// so the migration runs once.
pub fn migrate(cfg: &Config) -> Result<Vec<String>, String> {
    let legacy = cfg.state_dir.join("provider.json");
    let text = match fs::read_to_string(&legacy) {
        Ok(text) => text,
        Err(_) => return Ok(Vec::new()),
    };
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("cannot parse {}: {error}", legacy.display()))?;
    let mut moved = Vec::new();
    let current = value
        .get("current")
        .and_then(Value::as_str)
        .map(str::to_owned);
    // Registering the entries one by one moves the default; keep the profile's config for the
    // pair it already selected, so restoring that pair also restores fields this module does
    // not manage (the old file may name the same provider/model the panel already had).
    let default_before = Patch::read(cfg)
        .ok()
        .and_then(|patch| patch.get("agent-default-model").cloned())
        .and_then(|entry| entry.get("config").cloned());
    let mut chosen = None;
    for entry in value
        .get("providers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        let key = entry.get("key").and_then(Value::as_str).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let model = entry
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        add_endpoint(
            cfg,
            &Endpoint {
                id: id.to_owned(),
                display: entry
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned(),
                base: entry
                    .get("base")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                api: entry
                    .get("api")
                    .and_then(Value::as_str)
                    .unwrap_or("openai-completions")
                    .to_owned(),
                model: model.clone(),
                key: key.to_owned(),
            },
        )?;
        if current.as_deref() == Some(id) {
            chosen = Some((id.to_owned(), model));
        }
        moved.push(id.to_owned());
    }
    // `add_endpoint` makes each provider the default as it registers it, so the old file's
    // `current` has to be restored after the loop or the last entry silently wins.
    if let Some((id, model)) = chosen {
        let kept = default_before.filter(|config| {
            config.get("provider").and_then(Value::as_str) == Some(id.as_str())
                && config.get("model").and_then(Value::as_str) == Some(model.as_str())
        });
        match kept {
            // The profile already selected this pair: write its config back whole, not just the
            // pair, so its reasoning effort (and anything else the panel added) survives.
            Some(config) => {
                let mut patch = Patch::read(cfg)?;
                patch.set_config("agent-default-model", config);
                patch.write(cfg)?;
            }
            None => set_default(cfg, &id, &model)?,
        }
    }
    // The overlay a `booth up` from an earlier version passed is no longer read.
    let _ = fs::remove_file(cfg.state_dir.join("provider.yml"));
    let _ = fs::rename(&legacy, cfg.state_dir.join("provider.json.migrated"));
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODELS: &str = r#"{
        "default": { "provider": "deepseek-official", "model": "deepseek-flash" },
        "routableProviders": ["deepseek-official", "deepseek-account"],
        "groups": [
            {
                "id": "deepseek-official",
                "name": "DeepSeek",
                "models": [
                    { "id": "deepseek-flash", "name": "DeepSeek-V4.1-Flash" },
                    { "id": "deepseek-v4-pro", "name": "DeepSeek-V4.1-Pro" }
                ]
            },
            { "id": "openrouter", "name": "OpenRouter", "models": [{ "id": "x/y" }] }
        ]
    }"#;

    fn catalog() -> Catalog {
        Catalog::parse(&serde_json::from_str(MODELS).unwrap())
    }

    fn config() -> Config {
        let root = std::env::temp_dir().join(format!("booth-prov-{}", crate::oauth::uuid_v4()));
        fs::create_dir_all(root.join("profiles").join("superagent")).unwrap();
        Config {
            profile: "superagent".into(),
            port: 3082,
            dsh_home: root.clone(),
            state_dir: root.join("state"),
            bundle: root.join("superagent"),
        }
    }

    #[test]
    fn reads_the_default_and_every_choice() {
        let catalog = catalog();
        assert_eq!(
            catalog.default,
            Some(("deepseek-official".to_owned(), "deepseek-flash".to_owned()))
        );
        assert_eq!(catalog.choices.len(), 3);
        assert_eq!(catalog.choices[0].provider_name, "DeepSeek");
        assert_eq!(catalog.choices[0].model_name, "DeepSeek-V4.1-Flash");
        assert_eq!(catalog.choices[2].model_name, "x/y");
        assert_eq!(catalog.current().unwrap().model, "deepseek-flash");
        assert_eq!(catalog.label(), "deepseek-official/deepseek-flash");
        assert_eq!(
            catalog.providers(),
            vec![
                ("deepseek-official".to_owned(), "DeepSeek".to_owned()),
                ("openrouter".to_owned(), "OpenRouter".to_owned())
            ]
        );
    }

    #[test]
    fn the_current_choice_needs_both_the_provider_and_the_model() {
        // A shared model id must not make another provider look current.
        let catalog = Catalog::parse(&serde_json::json!({
            "default": { "provider": "b", "model": "m-shared" },
            "groups": [
                { "id": "a", "name": "A", "models": [{ "id": "m-shared" }] },
                { "id": "b", "name": "B", "models": [{ "id": "m-shared" }] }
            ]
        }));
        assert_eq!(catalog.current().unwrap().provider, "b");

        // A default the catalog does not list has no current choice.
        let missing = Catalog::parse(&serde_json::json!({
            "default": { "provider": "a", "model": "nope" },
            "groups": [{ "id": "a", "name": "A", "models": [{ "id": "m-a" }] }]
        }));
        assert!(missing.current().is_none());
    }

    #[test]
    fn an_empty_catalog_says_so_instead_of_panicking() {
        let catalog = Catalog::parse(&serde_json::json!({}));
        assert_eq!(catalog.default, None);
        assert!(catalog.choices.is_empty());
        assert!(catalog.routable.is_empty());
        assert_eq!(catalog.label(), "no model");
        assert!(catalog.current().is_none());
        assert!(catalog.providers().is_empty());
    }

    #[test]
    fn an_entry_without_a_model_id_is_skipped() {
        // A malformed catalog must not put an empty row in the picker.
        let catalog = Catalog::parse(&serde_json::json!({
            "groups": [{ "id": "a", "name": "A",
                         "models": [{ "name": "no id" }, { "id": "m-a" }] }]
        }));
        assert_eq!(catalog.choices.len(), 1, "{:?}", catalog.choices);
        assert_eq!(catalog.choices[0].model, "m-a");
    }

    #[test]
    fn the_credentials_path_falls_back_to_home() {
        let _env = crate::testkit::env_lock();
        let previous = (std::env::var_os("DSH_HOME"), std::env::var_os("HOME"));
        std::env::remove_var("DSH_HOME");
        std::env::set_var("HOME", "/tmp/booth-home-probe");
        let path = credentials_path().unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/booth-home-probe/.dsh/.credentials.yaml")
        );

        std::env::remove_var("HOME");
        let error = credentials_path().unwrap_err();
        assert!(error.contains("HOME"), "{error}");

        // Another test's environment is restored, not left as this one found it.
        if let Some(value) = previous.0 {
            std::env::set_var("DSH_HOME", value);
        }
        if let Some(value) = previous.1 {
            std::env::set_var("HOME", value);
        }
    }

    /// The environment the store resolver reads, restored even when an assertion fails.
    struct Env {
        dsh_home: Option<std::ffi::OsString>,
        home: Option<std::ffi::OsString>,
    }

    impl Env {
        fn capture() -> Self {
            Self {
                dsh_home: std::env::var_os("DSH_HOME"),
                home: std::env::var_os("HOME"),
            }
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            restore("DSH_HOME", self.dsh_home.take());
            restore("HOME", self.home.take());
        }
    }

    fn restore(name: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    /// A variable that was absent before the test is removed again, not left pointing at
    /// whatever the test set: the next test must not inherit a temp home.
    #[test]
    fn an_absent_variable_is_restored_as_absent() {
        let _env = crate::testkit::env_lock();
        std::env::set_var("BOOTH_PROVIDER_PROBE", "set by this test");
        restore("BOOTH_PROVIDER_PROBE", None);
        assert!(std::env::var_os("BOOTH_PROVIDER_PROBE").is_none());
    }

    fn temp_home(name: &str) -> PathBuf {
        let home =
            std::env::temp_dir().join(format!("booth-provider-{name}-{}", crate::oauth::uuid_v4()));
        fs::create_dir_all(&home).unwrap();
        home
    }

    /// DSH treats an empty or whitespace-only `$DSH_HOME` as unset, so a credential key saved
    /// through `/login` must land in `$HOME/.dsh/.credentials.yaml` — not `.credentials.yaml` in
    /// whatever directory booth started from, which is not a store DSH reads.
    #[test]
    fn a_blank_dsh_home_is_the_default_store() {
        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        let home = temp_home("blank-home");
        std::env::set_var("HOME", &home);

        for value in ["", " ", "\t\n"] {
            std::env::set_var("DSH_HOME", value);
            assert_eq!(
                credentials_path().unwrap(),
                home.join(".dsh/.credentials.yaml"),
                "DSH_HOME={value:?}"
            );
        }

        drop(env);
        let _ = fs::remove_dir_all(&home);
    }

    /// DSH expands `~`, `~/…` and `~\…` against the OS home before joining the store. A literal
    /// `~` directory beside the working directory is not the store DSH reads, so a key saved
    /// there would never authenticate the provider.
    #[test]
    fn a_tilde_dsh_home_is_expanded_like_dsh() {
        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        let home = temp_home("tilde-home");
        std::env::set_var("HOME", &home);

        for (value, expected) in [
            ("~", home.join(".credentials.yaml")),
            ("~/state", home.join("state/.credentials.yaml")),
            ("~\\state", home.join("state/.credentials.yaml")),
            ("/srv/booth", PathBuf::from("/srv/booth/.credentials.yaml")),
        ] {
            std::env::set_var("DSH_HOME", value);
            assert_eq!(credentials_path().unwrap(), expected, "DSH_HOME={value:?}");
        }

        drop(env);
        let _ = fs::remove_dir_all(&home);
    }

    /// The key really lands in the resolved store, and `has_ref` — which reads the same path —
    /// sees it, so the saved key and the check that reports it cannot disagree.
    #[test]
    fn a_key_saved_under_a_tilde_home_reaches_the_dsh_store() {
        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        let home = temp_home("tilde-key");
        std::env::set_var("HOME", &home);
        std::env::set_var("DSH_HOME", "~/state");

        write_ref("BOOTH_DEMO_KEY", "sk-demo").unwrap();
        let store = home.join("state/.credentials.yaml");
        let text = fs::read_to_string(&store).unwrap();
        let root: Value = serde_norway::from_str(&text).unwrap();
        assert_eq!(root["refs"]["BOOTH_DEMO_KEY"], "sk-demo");
        assert!(has_ref("BOOTH_DEMO_KEY"));

        drop(env);
        let _ = fs::remove_dir_all(&home);
    }

    /// No home resolver can invent a home: with `$HOME` gone a blank or tilde override is an
    /// error, not a path under the working directory.
    #[test]
    fn a_home_it_cannot_resolve_is_an_error() {
        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        std::env::remove_var("HOME");

        std::env::set_var("DSH_HOME", "~");
        let error = credentials_path().unwrap_err();
        assert!(error.contains("HOME"), "{error}");

        std::env::set_var("DSH_HOME", "");
        let error = credentials_path().unwrap_err();
        assert!(error.contains("HOME"), "{error}");

        drop(env);
    }

    /// A `$DSH_HOME` that is not UTF-8 is still one path; decoding it lossily would rename it.
    #[test]
    fn a_non_utf8_dsh_home_keeps_its_bytes() {
        use std::os::unix::ffi::OsStringExt;

        let _env = crate::testkit::env_lock();
        let env = Env::capture();
        let raw = std::ffi::OsString::from_vec(b"/srv/booth-\xff".to_vec());
        std::env::set_var("DSH_HOME", &raw);
        assert_eq!(
            credentials_path().unwrap(),
            PathBuf::from(&raw).join(".credentials.yaml")
        );

        drop(env);
    }

    #[test]
    fn a_credentials_file_that_is_not_a_map_is_replaced() {
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        fs::write(cfg.dsh_home.join(".credentials.yaml"), "just a string\n").unwrap();
        write_ref("A_KEY", "secret").unwrap();
        let text = fs::read_to_string(cfg.dsh_home.join(".credentials.yaml")).unwrap();
        let root: Value = serde_norway::from_str(&text).unwrap();
        assert_eq!(root["refs"]["A_KEY"], "secret");
        assert_eq!(root["version"], 1);
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn a_credentials_file_it_cannot_read_is_not_replaced() {
        // The shared store holds DSH's own refs and records, including the account grant. A
        // read failure is not an empty store: rewriting it would drop all of that.
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        let path = cfg.dsh_home.join(".credentials.yaml");
        fs::write(
            &path,
            "version: 1\nrefs:\n  KEEP_ME: keep\nrecords:\n  deepseek/grant:\n    kind: grant\n    payload:\n      token: t\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        // Readable by no one, writable by its owner: reading fails, rewriting would succeed.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();
        let error = write_ref("NEW_KEY", "n").unwrap_err();
        assert!(error.contains("cannot read"), "{error}");

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let root: Value = serde_norway::from_str(&text).unwrap();
        assert_eq!(root["refs"]["KEEP_ME"], "keep", "the other refs survive");
        assert_eq!(root["records"]["deepseek/grant"]["payload"]["token"], "t");
        assert!(root["refs"].get("NEW_KEY").is_none(), "no key was written");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn a_migration_skips_entries_it_cannot_use() {
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        fs::create_dir_all(&cfg.state_dir).unwrap();
        fs::write(
            cfg.state_dir.join("provider.json"),
            r#"{"providers":[{"base":"https://x","model":"m","key":"k"},{"id":"nokey","model":"m"},{"id":"ok","base":"https://o","model":"m","key":"k2"}]}"#,
        )
        .unwrap();
        // The first entry has no id and the second no key: only the third is carried over.
        assert_eq!(migrate(&cfg).unwrap(), vec!["ok".to_owned()]);
        let patch = Patch::read(&cfg).unwrap();
        assert!(patch.get("llm-pi-ai").unwrap()["config"]["providers"]["ok"].is_object());
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn a_credential_ref_is_derived_from_the_provider_id() {
        assert_eq!(key_ref("deepseek"), "BOOTH_DEEPSEEK_KEY");
        assert_eq!(key_ref("my-endpoint.east"), "BOOTH_MY_ENDPOINT_EAST_KEY");
    }

    #[test]
    fn writes_a_provider_and_its_key_where_dsh_reads_them() {
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        add_endpoint(
            &cfg,
            &Endpoint {
                id: "openrouter".into(),
                display: "OpenRouter".into(),
                base: "https://openrouter.ai/api/v1".into(),
                api: "openai-completions".into(),
                model: "x/y".into(),
                key: "sk-secret".into(),
            },
        )
        .unwrap();

        let patch = Patch::read(&cfg).unwrap();
        let provider = &patch.get("llm-pi-ai").unwrap()["config"]["providers"]["openrouter"];
        assert_eq!(provider["apiKeyEnv"], "BOOTH_OPENROUTER_KEY");
        assert_eq!(provider["baseURL"], "https://openrouter.ai/api/v1");
        assert_eq!(provider["models"][0]["id"], "x/y");
        let text = fs::read_to_string(cfg.patch()).unwrap();
        assert!(!text.contains("sk-secret"), "the key is not in the patch");
        assert_eq!(
            patch.get("agent-default-model").unwrap()["config"]["provider"],
            "openrouter"
        );

        let credentials = fs::read_to_string(cfg.dsh_home.join(".credentials.yaml")).unwrap();
        let root: Value = serde_norway::from_str(&credentials).unwrap();
        assert_eq!(root["refs"]["BOOTH_OPENROUTER_KEY"], "sk-secret");
        assert!(has_ref("BOOTH_OPENROUTER_KEY"));
        assert!(!has_ref("BOOTH_OTHER_KEY"));
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn re_registering_a_provider_keeps_the_models_it_already_had() {
        // The profile is shared with the web panel, so the entry may list more models than
        // this one-model form knows, with per-model settings. Logging in must not delete them.
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        let entries = json!([{
            "id": "llm-pi-ai",
            "config": { "providers": { "openrouter": {
                "apiKeyEnv": "PANEL_REF",
                "baseURL": "https://proxy.example/v1",
                "api": "openai-completions",
                "models": [
                    { "id": "a/b", "contextWindow": 1_000_000 },
                    { "id": "c/d" }
                ]
            }}}
        }]);
        fs::write(cfg.patch(), serde_json::to_string(&entries).unwrap()).unwrap();

        let endpoint = |model: &str, key: &str| Endpoint {
            id: "openrouter".into(),
            display: "OpenRouter".into(),
            base: "https://openrouter.ai/api/v1".into(),
            api: "openai-completions".into(),
            model: model.into(),
            key: key.into(),
        };
        // A model the panel did not list is added beside its models.
        add_endpoint(&cfg, &endpoint("e/f", "sk-first")).unwrap();
        // Logging in again for a model that is already listed does not duplicate it.
        add_endpoint(&cfg, &endpoint("a/b", "sk-second")).unwrap();

        let patch = Patch::read(&cfg).unwrap();
        let provider = &patch.get("llm-pi-ai").unwrap()["config"]["providers"]["openrouter"];
        let models = provider["models"].as_array().unwrap();
        let ids: Vec<&str> = models
            .iter()
            .filter_map(|model| model.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["a/b", "c/d", "e/f"], "{models:?}");
        assert_eq!(
            models[0]["contextWindow"], 1_000_000,
            "per-model settings survive"
        );

        let credentials = fs::read_to_string(cfg.dsh_home.join(".credentials.yaml")).unwrap();
        let root: Value = serde_norway::from_str(&credentials).unwrap();
        assert_eq!(root["refs"]["BOOTH_OPENROUTER_KEY"], "sk-second");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn storing_a_ref_leaves_other_records_alone_and_stays_private() {
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        fs::write(
            cfg.dsh_home.join(".credentials.yaml"),
            "version: 1\nrefs:\n  KEEP_ME: keep\nrecords:\n  a/b:\n    kind: grant\n    payload:\n      token: t\n",
        )
        .unwrap();
        write_ref("NEW_KEY", "n").unwrap();
        let text = fs::read_to_string(cfg.dsh_home.join(".credentials.yaml")).unwrap();
        let root: Value = serde_norway::from_str(&text).unwrap();
        assert_eq!(root["refs"]["KEEP_ME"], "keep");
        assert_eq!(root["refs"]["NEW_KEY"], "n");
        assert_eq!(root["records"]["a/b"]["payload"]["token"], "t");
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(cfg.dsh_home.join(".credentials.yaml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "a secret file stays private");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn a_store_it_cannot_stage_is_left_intact() {
        // The store is shared with DSH and the other booth commands, so a rewrite must not
        // truncate it in place. A directory that cannot take the staged sibling still lets its
        // existing file be rewritten, so the refusal has to happen before the old bytes change.
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        let path = cfg.dsh_home.join(".credentials.yaml");
        fs::write(
            &path,
            "version: 1\nrefs:\n  KEEP_ME: keep\nrecords:\n  deepseek/grant:\n    kind: grant\n    payload:\n      token: t\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        // The file stays writable by its owner; only its directory cannot take a sibling.
        fs::set_permissions(&cfg.dsh_home, fs::Permissions::from_mode(0o500)).unwrap();
        let error = write_ref("NEW_KEY", "n").unwrap_err();
        assert!(error.contains("cannot write"), "{error}");

        fs::set_permissions(&cfg.dsh_home, fs::Permissions::from_mode(0o700)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let root: Value = serde_norway::from_str(&text).unwrap();
        assert_eq!(root["refs"]["KEEP_ME"], "keep", "the other refs survive");
        assert_eq!(root["records"]["deepseek/grant"]["payload"]["token"], "t");
        assert!(root["refs"].get("NEW_KEY").is_none(), "no key was written");
        let staged: Vec<String> = fs::read_dir(&cfg.dsh_home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".credentials.yaml.") && name.ends_with(".tmp"))
            .collect();
        assert!(staged.is_empty(), "a staged sibling survived: {staged:?}");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    /// The store is read-modify-written by DSH and the other booth commands. The atomic rename
    /// keeps readers safe, but two writers that both read the old bytes still race: the later
    /// rename drops the earlier writer's record. The store's writer lock is what serializes that
    /// cycle, so a key write must wait for a writer that already holds it.
    #[test]
    fn a_key_write_waits_for_the_shared_writer_lock() {
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        let path = cfg.dsh_home.join(".credentials.yaml");
        let lock = cfg.dsh_home.join(".credentials.yaml.lock");
        let old = "version: 1\nrefs:\n  KEEP_ME: keep\nrecords:\n  deepseek/grant:\n    kind: grant\n    payload:\n      token: t\n";
        fs::write(&path, old).unwrap();
        // A live writer holds the lock. Its record names this process, so a contender that
        // honours the lock waits instead of taking it over.
        fs::write(&lock, format!("{}\n", std::process::id())).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let write = std::thread::spawn(move || {
            tx.send(write_ref("NEW_KEY", "n")).unwrap();
        });

        // The lock holder is still writing; the key write must not have touched the store.
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "the key was written while another writer held the store's lock"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), old);

        // The other writer finishes its read-modify-rename while still holding the lock.
        fs::write(
            &path,
            format!("{old}  other/writer:\n    kind: grant\n    payload:\n      token: kept\n"),
        )
        .unwrap();
        fs::remove_file(&lock).unwrap();

        rx.recv_timeout(Duration::from_secs(5))
            .expect("the key write never took the writer lock")
            .unwrap();
        write.join().unwrap();

        let root: Value = serde_norway::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["refs"]["NEW_KEY"], "n");
        assert_eq!(root["refs"]["KEEP_ME"], "keep");
        assert_eq!(
            root["records"]["other/writer"]["payload"]["token"], "kept",
            "the key write overwrote the record the other writer committed"
        );
        assert!(!lock.exists(), "the key write left its lock behind");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    /// A writer killed while holding the lock leaves the lock file behind. Its record names a pid
    /// that no longer exists, so the next key write takes it over instead of waiting out the
    /// timeout, and leaves neither the stale lock nor a takeover claim behind.
    #[test]
    fn a_lock_from_a_dead_writer_is_taken_over() {
        let _env = crate::testkit::env_lock();
        let home = temp_home("stale-lock");
        std::env::set_var("DSH_HOME", &home);
        let lock = home.join(".credentials.yaml.lock");
        // Above every host's pid_max, so no live process can carry it.
        fs::write(&lock, "2147483646\n").unwrap();

        write_ref("NEW_KEY", "n").unwrap();

        let path = home.join(".credentials.yaml");
        let root: Value = serde_norway::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["refs"]["NEW_KEY"], "n");
        let siblings: Vec<String> = fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            siblings,
            [".credentials.yaml"],
            "a lock or takeover claim survived"
        );
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn choosing_the_model_that_is_already_current_keeps_its_reasoning_effort() {
        // `agent-default-model` carries an optional `reasoningEffort` beside the pair this
        // module writes. Picking the current model again is not a reason to drop it: the
        // console has no field for effort and must not edit what it does not show.
        let cfg = config();
        fs::create_dir_all(cfg.profile_dir()).unwrap();
        fs::write(
            cfg.patch(),
            "- id: agent-default-model\n  config:\n    provider: a\n    model: m\n    reasoningEffort: high\n",
        )
        .unwrap();
        set_default(&cfg, "a", "m").unwrap();
        let patch = Patch::read(&cfg).unwrap();
        let config = &patch.get("agent-default-model").unwrap()["config"];
        assert_eq!(config["reasoningEffort"], "high", "{config}");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn re_registering_the_current_provider_keeps_its_reasoning_effort() {
        // A re-login for the provider/model already selected is also a no-op selection: the
        // shared profile's effort field survives even though the form does not render it.
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        let entries = json!([
            {
                "id": "llm-pi-ai",
                "config": { "providers": { "openrouter": {
                    "apiKeyEnv": "PANEL_REF",
                    "baseURL": "https://openrouter.ai/api/v1",
                    "api": "openai-completions",
                    "models": [{ "id": "x/y" }]
                }}}
            },
            {
                "id": "agent-default-model",
                "config": { "provider": "openrouter", "model": "x/y", "reasoningEffort": "high" }
            }
        ]);
        fs::write(cfg.patch(), serde_json::to_string(&entries).unwrap()).unwrap();
        add_endpoint(
            &cfg,
            &Endpoint {
                id: "openrouter".into(),
                display: "OpenRouter".into(),
                base: "https://openrouter.ai/api/v1".into(),
                api: "openai-completions".into(),
                model: "x/y".into(),
                key: "sk-secret".into(),
            },
        )
        .unwrap();
        let patch = Patch::read(&cfg).unwrap();
        let config = &patch.get("agent-default-model").unwrap()["config"];
        assert_eq!(config["reasoningEffort"], "high", "{config}");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn forgetting_the_default_removes_only_that_entry() {
        let cfg = config();
        fs::write(
            cfg.patch(),
            "- id: agent-default-model\n  config:\n    provider: a\n    model: b\n- id: webserver\n  config:\n    port: 3082\n",
        )
        .unwrap();
        assert!(forget_default(&cfg).unwrap());
        assert!(
            !forget_default(&cfg).unwrap(),
            "the second call finds nothing"
        );
        assert_eq!(
            Patch::read(&cfg).unwrap().ids(),
            vec!["webserver".to_owned()]
        );
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn a_provider_needs_an_id_and_a_model() {
        let cfg = config();
        let error = add_endpoint(
            &cfg,
            &Endpoint {
                id: String::new(),
                display: String::new(),
                base: String::new(),
                api: String::new(),
                model: "x".into(),
                key: "k".into(),
            },
        )
        .unwrap_err();
        assert!(error.contains("id"), "{error}");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    fn config() -> Config {
        let root = std::env::temp_dir().join(format!("booth-migrate-{}", crate::oauth::uuid_v4()));
        fs::create_dir_all(root.join("profiles").join("superagent")).unwrap();
        fs::create_dir_all(root.join("state")).unwrap();
        Config {
            profile: "superagent".into(),
            port: 3082,
            dsh_home: root.clone(),
            state_dir: root.join("state"),
            bundle: root.join("superagent"),
        }
    }

    #[test]
    fn moves_an_old_provider_file_into_the_profile_once() {
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        fs::create_dir_all(&cfg.state_dir).unwrap();
        fs::write(
            cfg.state_dir.join("provider.json"),
            r#"{"current":"deepseek","providers":[{"id":"deepseek","base":"https://api.deepseek.com","api":"openai-completions","model":"deepseek-chat","key":"sk-old"}]}"#,
        )
        .unwrap();
        // A stale overlay from the version that passed --patch.
        fs::write(cfg.state_dir.join("provider.yml"), "- id: llm-pi-ai\n").unwrap();

        let moved = migrate(&cfg).unwrap();
        assert_eq!(moved, vec!["deepseek".to_owned()]);

        // The provider is in the profile, the key is a ref, and the old files are gone.
        let patch = Patch::read(&cfg).unwrap();
        let provider = &patch.get("llm-pi-ai").unwrap()["config"]["providers"]["deepseek"];
        assert_eq!(provider["baseURL"], "https://api.deepseek.com");
        assert_eq!(provider["apiKeyEnv"], "BOOTH_DEEPSEEK_KEY");
        let credentials = fs::read_to_string(cfg.dsh_home.join(".credentials.yaml")).unwrap();
        assert!(credentials.contains("sk-old"), "{credentials}");
        assert!(!cfg.state_dir.join("provider.yml").exists());
        assert!(cfg.state_dir.join("provider.json.migrated").exists());

        // Running again is a no-op: the original name is gone.
        assert!(migrate(&cfg).unwrap().is_empty());
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn a_machine_without_the_old_file_migrates_nothing() {
        let cfg = config();
        assert!(migrate(&cfg).unwrap().is_empty());
        assert!(
            !cfg.patch().exists(),
            "nothing is written when there is nothing to move"
        );
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn migration_keeps_the_provider_that_was_current() {
        // The old file named the provider the service ran with; moving every entry must not
        // quietly leave the last one as the default.
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        fs::create_dir_all(&cfg.state_dir).unwrap();
        fs::write(
            cfg.state_dir.join("provider.json"),
            r#"{"current":"first","providers":[
                {"id":"first","base":"https://first.example","api":"openai-completions","model":"m-first","key":"k1"},
                {"id":"second","base":"https://second.example","api":"openai-completions","model":"m-second","key":"k2"}
            ]}"#,
        )
        .unwrap();

        let moved = migrate(&cfg).unwrap();
        assert_eq!(moved, vec!["first".to_owned(), "second".to_owned()]);

        let patch = Patch::read(&cfg).unwrap();
        let default = &patch.get("agent-default-model").unwrap()["config"];
        assert_eq!(default["provider"], "first", "the old current wins");
        assert_eq!(default["model"], "m-first");
        // Both providers are still registered.
        let providers = &patch.get("llm-pi-ai").unwrap()["config"]["providers"];
        assert!(providers.get("first").is_some(), "{providers}");
        assert!(providers.get("second").is_some(), "{providers}");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn migration_keeps_the_reasoning_effort_of_the_default_it_keeps() {
        // Registering the entries one by one moves the default away from the old `current`
        // and back. The move back must not rewrite the profile's own `reasoningEffort` for
        // the pair it is keeping.
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        fs::create_dir_all(&cfg.state_dir).unwrap();
        let panel = json!([{
            "id": "agent-default-model",
            "config": { "provider": "current", "model": "m-current", "reasoningEffort": "high" }
        }]);
        fs::write(cfg.patch(), serde_json::to_string(&panel).unwrap()).unwrap();
        fs::write(
            cfg.state_dir.join("provider.json"),
            r#"{"current":"current","providers":[
                {"id":"other","base":"https://other.example","api":"openai-completions","model":"m-other","key":"k1"},
                {"id":"current","base":"https://current.example","api":"openai-completions","model":"m-current","key":"k2"}
            ]}"#,
        )
        .unwrap();

        assert_eq!(
            migrate(&cfg).unwrap(),
            vec!["other".to_owned(), "current".to_owned()]
        );

        let patch = Patch::read(&cfg).unwrap();
        let config = &patch.get("agent-default-model").unwrap()["config"];
        assert_eq!(config["provider"], "current", "{config}");
        assert_eq!(config["model"], "m-current", "{config}");
        assert_eq!(config["reasoningEffort"], "high", "{config}");
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }

    #[test]
    fn migration_keeps_models_the_profile_already_lists() {
        // The old booth kept its providers beside the profile, so the same route may already
        // be configured in the panel with more models and per-model settings. Moving the old
        // provider in must not shrink the catalog the panel serves.
        let _env = crate::testkit::env_lock();
        let cfg = config();
        std::env::set_var("DSH_HOME", &cfg.dsh_home);
        fs::create_dir_all(&cfg.state_dir).unwrap();
        let panel = json!([{
            "id": "llm-pi-ai",
            "config": { "providers": { "openrouter": {
                "apiKeyEnv": "PANEL_REF",
                "baseURL": "https://openrouter.ai/api/v1",
                "api": "openai-completions",
                "models": [
                    { "id": "a/b", "contextWindow": 1_000_000 },
                    { "id": "c/d" }
                ]
            }}}
        }]);
        fs::write(cfg.patch(), serde_json::to_string(&panel).unwrap()).unwrap();
        fs::write(
            cfg.state_dir.join("provider.json"),
            r#"{"current":"openrouter","providers":[{"id":"openrouter","base":"https://openrouter.ai/api/v1","api":"openai-completions","model":"e/f","key":"sk-old"}]}"#,
        )
        .unwrap();

        assert_eq!(migrate(&cfg).unwrap(), vec!["openrouter".to_owned()]);

        let patch = Patch::read(&cfg).unwrap();
        let provider = &patch.get("llm-pi-ai").unwrap()["config"]["providers"]["openrouter"];
        let models = provider["models"].as_array().unwrap();
        let ids: Vec<&str> = models
            .iter()
            .filter_map(|model| model.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["a/b", "c/d", "e/f"], "{models:?}");
        assert_eq!(models[0]["contextWindow"], 1_000_000);
        fs::remove_dir_all(&cfg.dsh_home).unwrap();
    }
}

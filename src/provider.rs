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
use std::path::PathBuf;

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
    patch.set_config(
        "agent-default-model",
        json!({ "provider": provider, "model": model }),
    );
    patch.write(cfg)
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
    patch.set_value(
        "llm-pi-ai",
        &["providers", &endpoint.id, "models"],
        json!([{ "id": endpoint.model }]),
    );
    write_ref(&reference, &endpoint.key)?;
    patch.set_config(
        "agent-default-model",
        json!({ "provider": endpoint.id, "model": endpoint.model }),
    );
    patch.write(cfg)
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
    match std::env::var_os("DSH_HOME") {
        Some(value) => Ok(PathBuf::from(value).join(".credentials.yaml")),
        None => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".dsh/.credentials.yaml"))
            .ok_or_else(|| "HOME is not set".to_owned()),
    }
}

/// Stores one secret as `refs.<name>`, keeping every other record and ref.
fn write_ref(name: &str, secret: &str) -> Result<(), String> {
    let path = credentials_path()?;
    let mut root: Value = match fs::read_to_string(&path) {
        Ok(text) => serde_norway::from_str(&text)
            .map_err(|error| format!("cannot parse {}: {error}", path.display()))?,
        Err(_) => json!({ "version": 1, "refs": {}, "records": {} }),
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(&path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    Ok(())
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
        set_default(cfg, &id, &model)?;
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
}

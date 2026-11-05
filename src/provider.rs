//! Model providers, in the shape Pi uses: `/login` stores a key, `/model` picks the model.
//!
//! The service is started by `booth up`, so this module also renders the DSH patch overlay
//! (`llm-pi-ai` + `agent-default-model`) that `up` passes with `--patch`, and the environment
//! variables that carry the keys. Keys live in `provider.json` (mode 0600) under the state
//! directory; the overlay only names the variables.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use crate::service::Config;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub id: String,
    pub base: String,
    pub api: String,
    pub model: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub current: String,
    pub providers: Vec<Provider>,
}

pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub base: &'static str,
    pub api: &'static str,
    pub model: &'static str,
}

/// The presets `/login` offers before "custom endpoint". Base URLs and model ids are only
/// starting points: the form lets every field be edited.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "deepseek",
        label: "DeepSeek",
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

pub const APIS: &[&str] = &["openai-completions", "openai-responses"];

impl Store {
    pub fn load(cfg: &Config) -> Self {
        fs::read_to_string(path(cfg))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, cfg: &Config) -> Result<(), String> {
        fs::create_dir_all(&cfg.state_dir)
            .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| format!("cannot encode provider.json: {error}"))?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path(cfg))
            .map_err(|error| format!("cannot write {}: {error}", path(cfg).display()))?;
        file.write_all(text.as_bytes())
            .map_err(|error| format!("cannot write {}: {error}", path(cfg).display()))?;
        self.write_overlay(cfg)
    }

    pub fn current(&self) -> Option<&Provider> {
        self.providers.iter().find(|item| item.id == self.current)
    }

    pub fn upsert(&mut self, provider: Provider) {
        match self
            .providers
            .iter_mut()
            .find(|item| item.id == provider.id)
        {
            Some(slot) => *slot = provider.clone(),
            None => self.providers.push(provider.clone()),
        }
        self.current = provider.id;
    }

    /// The `(name, value)` pairs `booth up` hands to the service process.
    pub fn environment(&self) -> Vec<(String, String)> {
        self.providers
            .iter()
            .map(|provider| (key_env(&provider.id), provider.key.clone()))
            .collect()
    }

    /// Registers the API-key providers without choosing a default model, so the signed-in
    /// DeepSeek account stays the default while sessions that name `deepseek` keep their adapter.
    /// Rewriting the overlay this way is what keeps a sign-in from invalidating old sessions.
    pub fn write_providers_only(&self, cfg: &Config) -> Result<(), String> {
        self.write(cfg, false)
    }

    fn write_overlay(&self, cfg: &Config) -> Result<(), String> {
        self.write(cfg, true)
    }

    fn write(&self, cfg: &Config, choose_default: bool) -> Result<(), String> {
        let mut providers = serde_json::Map::new();
        for provider in &self.providers {
            providers.insert(
                provider.id.clone(),
                serde_json::json!({
                    "apiKeyEnv": key_env(&provider.id),
                    "baseURL": provider.base,
                    "api": provider.api,
                    "models": [{ "id": provider.model }],
                }),
            );
        }
        let mut entries = vec![serde_json::json!({
            "id": "llm-pi-ai",
            "name": "@deepseek-ai/dsh-llm-pi-ai",
            "config": { "providers": providers },
        })];
        if choose_default {
            if let Some(current) = self.current() {
                entries.push(serde_json::json!({
                    "id": "agent-default-model",
                    "config": { "provider": current.id, "model": current.model },
                }));
            }
        }
        let text = serde_norway::to_string(&entries)
            .map_err(|error| format!("cannot render the provider overlay: {error}"))?;
        fs::write(overlay(cfg), text)
            .map_err(|error| format!("cannot write {}: {error}", overlay(cfg).display()))
    }
}

pub fn path(cfg: &Config) -> PathBuf {
    cfg.state_dir.join("provider.json")
}

pub fn overlay(cfg: &Config) -> PathBuf {
    cfg.state_dir.join("provider.yml")
}

/// The environment variable that carries one provider's key.
pub fn key_env(id: &str) -> String {
    let mut name = String::from("BOOTH_KEY_");
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_uppercase());
        } else {
            name.push('_');
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::Config;
    use std::path::PathBuf;

    fn config(name: &str) -> Config {
        let root =
            std::env::temp_dir().join(format!("booth-provider-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Config {
            profile: "test".into(),
            port: 0,
            dsh_home: root.clone(),
            state_dir: root,
            bundle: PathBuf::from("/tmp"),
        }
    }

    fn store() -> Store {
        Store {
            current: "deepseek".into(),
            providers: vec![Provider {
                id: "deepseek".into(),
                base: "https://api.deepseek.com".into(),
                api: "openai-completions".into(),
                model: "deepseek-chat".into(),
                key: "sk-test".into(),
            }],
        }
    }

    #[test]
    fn the_full_overlay_names_a_default_model() {
        let cfg = config("full");
        store().save(&cfg).unwrap();
        let text = fs::read_to_string(overlay(&cfg)).unwrap();
        assert!(text.contains("agent-default-model"), "{text}");
        assert!(text.contains("deepseek"), "{text}");
    }

    #[test]
    fn the_account_overlay_keeps_every_provider_but_no_default() {
        let cfg = config("account");
        store().write_providers_only(&cfg).unwrap();
        let text = fs::read_to_string(overlay(&cfg)).unwrap();
        // Sessions that still name `deepseek` keep their adapter after a sign-in.
        assert!(text.contains("BOOTH_KEY_DEEPSEEK"), "{text}");
        assert!(!text.contains("agent-default-model"), "{text}");
    }
}

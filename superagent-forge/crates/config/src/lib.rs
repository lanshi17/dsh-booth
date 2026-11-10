use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub workspace_root: PathBuf,
    pub database_path: PathBuf,
    pub max_concurrency: usize,
    pub timeout_seconds: u64,
    pub network_allowed: bool,
    pub environment_allowlist: BTreeSet<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
    #[error(
        "paths must be absolute; workspaces cannot use /; limits must be positive; environment names must be valid"
    )]
    Invalid,
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text)?;
        let valid_name = |s: &str| {
            !s.is_empty()
                && !s.as_bytes()[0].is_ascii_digit()
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        };
        if !config.workspace_root.is_absolute()
            || config.workspace_root.parent().is_none()
            || !config.database_path.is_absolute()
            || config.max_concurrency == 0
            || config.timeout_seconds == 0
            || !config.environment_allowlist.iter().all(|s| valid_name(s))
        {
            return Err(ConfigError::Invalid);
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CONFIG: &str = include_str!("../../../config/default.toml");
    #[test]
    fn rejects_unknown_permissions_and_zero_limits() {
        assert!(!Config::parse(CONFIG).unwrap().network_allowed);
        assert!(
            Config::parse(&CONFIG.replace("max_concurrency = 2", "max_concurrency = 0")).is_err()
        );
        assert!(Config::parse(&format!("{CONFIG}\nallow_merge = true\n")).is_err());
    }
}

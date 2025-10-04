use crate::algo::Algorithm;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub default_sign_algo: Option<Algorithm>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_file_path()?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file {config_path:?}"))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file {config_path:?}"))?;

        Ok(config)
    }

    fn config_file_path() -> anyhow::Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::format_err!("Could not determine home directory"))?;

        Ok(home_dir.join(".config").join("ratify.toml"))
    }
}

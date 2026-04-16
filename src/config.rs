use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::{fs, path::Path};

const SAMPLE_CONFIG: &str = include_str!("../config.sample.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: Server,
    pub matrix: Matrix,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Matrix {
    #[serde(default)]
    pub homeservers: Vec<String>,
    #[serde(default)]
    pub allow_any_server: bool,
    #[serde(default)]
    pub admin_bot: String,
    #[serde(default)]
    pub admin_room_alias: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub device_display_name: String,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let body = fs::read_to_string(path)
            .with_context(|| format!("reading config from {}", path.display()))?;
        toml::from_str(&body).context("parsing config TOML")
    }
}

/// Write the bundled sample config to `path`. Errors if the file already exists.
pub fn generate_sample(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        return Err(anyhow!("config file already exists: {}", path.display()));
    }
    fs::write(path, SAMPLE_CONFIG)
        .with_context(|| format!("writing config to {}", path.display()))?;
    Ok(())
}

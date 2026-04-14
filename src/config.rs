use anyhow::{Context, Result};
use serde::Deserialize;
use std::{fs, path::Path};

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
    pub homeserver: String,
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

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub listen: SocketAddr,
    pub hosts: Vec<HostConfig>,
}

#[derive(Debug, Deserialize)]
pub struct HostConfig {
    pub domain: String,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub backend: SocketAddr,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        let config: Config = toml::from_str(&text)
            .with_context(|| format!("parsing config file {}", path.display()))?;
        anyhow::ensure!(
            !config.hosts.is_empty(),
            "config must define at least one [[hosts]] entry"
        );
        Ok(config)
    }
}

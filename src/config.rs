use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const DEFAULT_RELAYS: &[(&str, bool, bool)] = &[
    ("wss://relay.damus.io", true, true),
    ("wss://nos.lol", true, true),
    ("wss://relay.primal.net", true, true),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountMode {
    #[default]
    ReadOnly,
    Local,
    Bunker,
}

impl AccountMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Local => "local key",
            Self::Bunker => "bunker",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::ReadOnly => Self::Local,
            Self::Local => Self::Bunker,
            Self::Bunker => Self::ReadOnly,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub npub: String,
    pub mode: AccountMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub url: String,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub account: Option<AccountConfig>,
    #[serde(default = "default_relays")]
    pub relays: Vec<RelayConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            account: None,
            relays: default_relays(),
        }
    }
}

fn default_relays() -> Vec<RelayConfig> {
    DEFAULT_RELAYS
        .iter()
        .map(|(url, read, write)| RelayConfig {
            url: (*url).to_string(),
            read: *read,
            write: *write,
        })
        .collect()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut config: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config {}", path.display()))?;

        if config.relays.is_empty() {
            config.relays = default_relays();
        }

        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let raw = toml::to_string_pretty(self)?;
        fs::write(path, raw).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn set_relay(&mut self, url: &str, read: bool, write: bool) {
        if let Some(relay) = self.relays.iter_mut().find(|relay| relay.url == url) {
            relay.read = read;
            relay.write = write;
        } else {
            self.relays.push(RelayConfig {
                url: url.to_string(),
                read,
                write,
            });
        }
    }

    pub fn remove_relay(&mut self, url: &str) {
        self.relays.retain(|relay| relay.url != url);
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub database: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn discover(
        config_override: Option<PathBuf>,
        data_override: Option<PathBuf>,
    ) -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "rastrich")
            .context("could not determine XDG project directories")?;

        let config = config_override.unwrap_or_else(|| dirs.config_dir().join("config.toml"));
        let data_dir = data_override.unwrap_or_else(|| dirs.data_dir().to_path_buf());
        let database = data_dir.join("events.sqlite3");
        let log_dir = dirs.state_dir().unwrap_or(dirs.data_dir()).join("logs");

        fs::create_dir_all(&data_dir)
            .with_context(|| format!("failed to create {}", data_dir.display()))?;
        if let Some(parent) = config.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::create_dir_all(&log_dir)
            .with_context(|| format!("failed to create {}", log_dir.display()))?;

        Ok(Self {
            config,
            database,
            log_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_usable() {
        let config = Config::default();
        assert!(config.account.is_none());
        assert!(!config.relays.is_empty());
        assert!(config.relays.iter().all(|relay| relay.read || relay.write));
    }

    #[test]
    fn relay_mutations_are_deduplicated() {
        let mut config = Config::default();
        let url = "wss://example.com";
        config.set_relay(url, true, false);
        config.set_relay(url, false, true);
        assert_eq!(
            config
                .relays
                .iter()
                .filter(|relay| relay.url == url)
                .count(),
            1
        );
        let relay = config.relays.iter().find(|relay| relay.url == url).unwrap();
        assert!(!relay.read);
        assert!(relay.write);
        config.remove_relay(url);
        assert!(!config.relays.iter().any(|relay| relay.url == url));
    }

    #[test]
    fn toml_round_trip() {
        let config = Config {
            account: Some(AccountConfig {
                npub: "npub1test".to_string(),
                mode: AccountMode::ReadOnly,
            }),
            relays: default_relays(),
        };
        let raw = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&raw).unwrap();
        assert_eq!(parsed.account.unwrap().npub, "npub1test");
        assert_eq!(parsed.relays.len(), config.relays.len());
    }
}

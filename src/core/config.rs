//! Persistent configuration, stored as JSON under Application Support.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    /// Folders indexed for file search.
    pub search_folders: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            search_folders: home_dir().into_iter().collect(),
        }
    }
}

impl Config {
    /// Loads the config, falling back to a default that searches `$HOME`. The
    /// default is written to disk on first run so the settings window shows a
    /// populated list.
    pub fn load() -> Self {
        if let Ok(text) = std::fs::read_to_string(config_path()) {
            if let Ok(config) = serde_json::from_str(&text) {
                return config;
            }
        }
        let config = Self::default();
        let _ = config.save();
        config
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text)
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// `$HOME/Library/Application Support/Glance/config.json`.
pub fn config_path() -> PathBuf {
    support_dir().join("config.json")
}

/// `$HOME/Library/Application Support/Glance/plugins` — where user Lua plugins live.
pub fn plugins_dir() -> PathBuf {
    support_dir().join("plugins")
}

fn support_dir() -> PathBuf {
    let mut path = home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("Library/Application Support/Glance");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let config = Config {
            search_folders: vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")],
        };
        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.search_folders, config.search_folders);
    }
}

//! Persistent configuration, stored as JSON under Application Support.

use std::path::PathBuf;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use serde::{Deserialize, Serialize};

/// A web search shortcut: typing `<keyword> terms` opens `url` with `{}`
/// replaced by the (encoded) terms.
#[derive(Clone, Serialize, Deserialize)]
pub struct WebLink {
    pub keyword: String,
    pub name: String,
    pub url: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    /// Folders indexed for file search.
    #[serde(default = "default_folders")]
    pub search_folders: Vec<PathBuf>,
    /// Summon shortcut, stored as a `global_hotkey` string (e.g. "super+Space").
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Maximum results shown in the list.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// Web search quick-links.
    #[serde(default = "default_web_links")]
    pub web_links: Vec<WebLink>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            search_folders: default_folders(),
            hotkey: default_hotkey(),
            max_results: default_max_results(),
            web_links: default_web_links(),
        }
    }
}

impl Config {
    /// Loads the config, falling back to defaults. The default is written to
    /// disk on first run so the settings window shows populated values.
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

fn default_folders() -> Vec<PathBuf> {
    home_dir().into_iter().collect()
}

fn default_hotkey() -> String {
    HotKey::new(Some(Modifiers::SUPER), Code::Space).to_string()
}

fn default_max_results() -> usize {
    8
}

fn default_web_links() -> Vec<WebLink> {
    let link = |keyword: &str, name: &str, url: &str| WebLink {
        keyword: keyword.to_string(),
        name: name.to_string(),
        url: url.to_string(),
    };
    vec![
        link("s", "DuckDuckGo", "https://duckduckgo.com/?q={}"),
        link("g", "Google", "https://www.google.com/search?q={}"),
        link("yt", "YouTube", "https://www.youtube.com/results?search_query={}"),
        link("gh", "GitHub", "https://github.com/search?q={}"),
        link("w", "Wikipedia", "https://en.wikipedia.org/w/index.php?search={}"),
    ]
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
    use std::str::FromStr;

    #[test]
    fn round_trips_through_json() {
        let config = Config::default();
        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.search_folders, config.search_folders);
        assert_eq!(back.hotkey, config.hotkey);
        assert_eq!(back.max_results, config.max_results);
        assert_eq!(back.web_links.len(), config.web_links.len());
    }

    #[test]
    fn old_config_still_loads() {
        // A pre-existing config with only search_folders must fill the rest.
        let old = r#"{ "search_folders": ["/tmp/x"] }"#;
        let config: Config = serde_json::from_str(old).unwrap();
        assert_eq!(config.search_folders, vec![PathBuf::from("/tmp/x")]);
        assert_eq!(config.max_results, 8);
        assert!(!config.web_links.is_empty());
        assert!(HotKey::from_str(&config.hotkey).is_ok());
    }
}

//! Orchestrates the plugins: runs each against the query, merges their scored
//! results into one ranked list.

use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config as NucleoConfig, Matcher};

use crate::core::config::Config;
use crate::core::file_index::FileIndex;
use crate::core::item::SearchItem;
use crate::plugins::app_launcher::AppLauncher;
use crate::plugins::calculator::Calculator;
use crate::plugins::calendar::Calendar;
use crate::plugins::file_search::FileSearch;
use crate::plugins::lua_host::ScriptHost;
use crate::plugins::system::System;
use crate::plugins::web_search::WebSearch;
use crate::plugins::{Plugin, PluginCx};

pub struct SearchEngine {
    plugins: Vec<Box<dyn Plugin>>,
    matcher: Matcher,
    files: FileIndex,
}

impl SearchEngine {
    pub fn new() -> Self {
        let files = FileIndex::new();
        files.reindex(Config::load().search_folders);

        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(Calculator::new()),
            Box::new(WebSearch::new()),
            Box::new(Calendar::new()),
            Box::new(AppLauncher::new()),
            Box::new(FileSearch::new(files.clone())),
            Box::new(System::new()),
            Box::new(ScriptHost::new()),
        ];

        Self {
            plugins,
            matcher: Matcher::new(NucleoConfig::DEFAULT),
            files,
        }
    }

    /// A shared handle to the file index, so the settings UI can re-index when
    /// the folder list changes.
    pub fn file_index(&self) -> FileIndex {
        self.files.clone()
    }

    /// Re-scans/reloads plugins (the Lua host picks up new or edited scripts).
    pub fn reload_plugins(&mut self) {
        for plugin in &mut self.plugins {
            plugin.reload();
        }
    }

    /// Returns up to `limit` results ranked across all plugins. An empty query
    /// yields nothing.
    pub fn search(&mut self, query: &str, limit: usize) -> Vec<SearchItem> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut cx = PluginCx {
            pattern: &pattern,
            matcher: &mut self.matcher,
            limit,
        };

        let mut scored = Vec::new();
        for plugin in &mut self.plugins {
            scored.extend(plugin.search(query, &mut cx));
        }

        scored.sort_by_key(|s| std::cmp::Reverse(s.score));
        scored.truncate(limit);
        scored.into_iter().map(|s| s.item).collect()
    }
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::item::Action;

    #[test]
    fn surfaces_the_settings_command() {
        let mut engine = SearchEngine::new();
        let has_settings = |q: &str, engine: &mut SearchEngine| {
            engine
                .search(q, 8)
                .iter()
                .any(|item| matches!(item.action, Action::OpenSettings))
        };

        // Prefixes of its keywords surface it.
        assert!(has_settings("settings", &mut engine));
        assert!(has_settings("set", &mut engine));
        assert!(has_settings("glan", &mut engine));

        // Unrelated queries must not.
        assert!(!has_settings("lo", &mut engine));
        assert!(!has_settings("spo", &mut engine));
    }

    #[test]
    fn respects_the_limit() {
        let mut engine = SearchEngine::new();
        assert!(engine.search("e", 5).len() <= 5);
        assert!(engine.search("", 8).is_empty());
    }
}

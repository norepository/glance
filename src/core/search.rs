//! Orchestrates the plugins: runs each against the query, merges their scored
//! results into one ranked list.

use std::cell::RefCell;
use std::rc::Rc;

use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config as NucleoConfig, Matcher};

use crate::core::config::WebLink;
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
}

impl SearchEngine {
    pub fn new(file_index: FileIndex, web_links: Rc<RefCell<Vec<WebLink>>>) -> Self {
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(Calculator::new()),
            Box::new(WebSearch::new(web_links)),
            Box::new(Calendar::new()),
            Box::new(AppLauncher::new()),
            Box::new(FileSearch::new(file_index)),
            Box::new(System::new()),
            Box::new(ScriptHost::new()),
        ];

        Self {
            plugins,
            matcher: Matcher::new(NucleoConfig::DEFAULT),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::item::Action;

    fn engine() -> SearchEngine {
        SearchEngine::new(FileIndex::new(), Rc::new(RefCell::new(Vec::new())))
    }

    #[test]
    fn surfaces_the_settings_command() {
        let mut engine = engine();
        let has_settings = |q: &str, engine: &mut SearchEngine| {
            engine
                .search(q, 8)
                .iter()
                .any(|item| matches!(item.action, Action::OpenSettings))
        };

        assert!(has_settings("settings", &mut engine));
        assert!(has_settings("set", &mut engine));
        assert!(!has_settings("lo", &mut engine));
    }

    #[test]
    fn respects_the_limit() {
        let mut engine = engine();
        assert!(engine.search("e", 5).len() <= 5);
        assert!(engine.search("", 8).is_empty());
    }
}

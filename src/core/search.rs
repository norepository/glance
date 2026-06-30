//! Fuzzy-ranks apps, files, and built-in commands against a query, merging them
//! into one ranked list of [`SearchItem`]s.

use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config as NucleoConfig, Matcher};

use crate::core::app_index::{index_apps, AppEntry};
use crate::core::config::Config;
use crate::core::file_index::FileIndex;
use crate::core::item::{Action, SearchItem};

/// Files are only searched once the query is at least this long, to avoid
/// ranking the entire index on one or two characters.
const MIN_FILE_QUERY_LEN: usize = 3;
/// Score bonuses so apps and commands outrank files at equal match quality.
const APP_BONUS: u32 = 100;
const CMD_BONUS: u32 = 150;

/// A built-in command surfaced by keyword (e.g. "settings").
struct Command {
    keywords: String,
    title: &'static str,
    subtitle: &'static str,
    action: Action,
}

impl AsRef<str> for Command {
    fn as_ref(&self) -> &str {
        &self.keywords
    }
}

pub struct SearchEngine {
    apps: Vec<AppEntry>,
    files: FileIndex,
    commands: Vec<Command>,
    matcher: Matcher,
}

impl SearchEngine {
    pub fn new() -> Self {
        let files = FileIndex::new();
        files.reindex(Config::load().search_folders);

        Self {
            apps: index_apps(),
            files,
            commands: builtin_commands(),
            matcher: Matcher::new(NucleoConfig::DEFAULT),
        }
    }

    /// A shared handle to the file index, so the settings UI can re-index when
    /// the folder list changes.
    pub fn file_index(&self) -> FileIndex {
        self.files.clone()
    }

    /// Returns up to `limit` results ranked across apps, files, and commands.
    /// An empty query yields nothing.
    pub fn search(&mut self, query: &str, limit: usize) -> Vec<SearchItem> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut scored: Vec<(SearchItem, u32)> = Vec::new();

        for (app, score) in pattern.match_list(self.apps.iter(), &mut self.matcher) {
            scored.push((app_item(app), score + APP_BONUS));
        }

        for (cmd, score) in pattern.match_list(self.commands.iter(), &mut self.matcher) {
            scored.push((command_item(cmd), score + CMD_BONUS));
        }

        if query.len() >= MIN_FILE_QUERY_LEN {
            let files = self.files.read();
            // match_list returns matches sorted best-first; only the top `limit`
            // can survive the final truncate, so avoid allocating the rest.
            for (file, score) in pattern
                .match_list(files.iter(), &mut self.matcher)
                .into_iter()
                .take(limit)
            {
                scored.push((file_item(&file.name, &file.path), score));
            }
        }

        scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
        scored.truncate(limit);
        scored.into_iter().map(|(item, _)| item).collect()
    }
}

fn builtin_commands() -> Vec<Command> {
    vec![Command {
        keywords: "settings preferences glance config options".to_string(),
        title: "Glance Settings",
        subtitle: "Preferences",
        action: Action::OpenSettings,
    }]
}

fn app_item(app: &AppEntry) -> SearchItem {
    SearchItem {
        title: app.name.clone(),
        subtitle: None,
        icon_path: Some(app.path.clone()),
        action: Action::Open(app.path.clone()),
    }
}

fn file_item(name: &str, path: &std::path::Path) -> SearchItem {
    SearchItem {
        title: name.to_string(),
        subtitle: Some(path.to_string_lossy().into_owned()),
        icon_path: Some(path.to_path_buf()),
        action: Action::Open(path.to_path_buf()),
    }
}

fn command_item(cmd: &Command) -> SearchItem {
    SearchItem {
        title: cmd.title.to_string(),
        subtitle: Some(cmd.subtitle.to_string()),
        icon_path: None,
        action: cmd.action.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_known_app() {
        let mut engine = SearchEngine::new();
        assert!(!engine.apps.is_empty(), "expected a non-empty app index");
        assert!(engine.search("", 8).is_empty());

        let target = engine.apps[0].name.clone();
        let results = engine.search(&target, 8);
        assert!(
            results.iter().any(|item| item.title == target),
            "expected to find {target:?} in results"
        );
        assert!(results.len() <= 8, "limit must be respected");
    }

    #[test]
    fn surfaces_the_settings_command() {
        let mut engine = SearchEngine::new();
        let results = engine.search("settings", 8);
        assert!(
            results
                .iter()
                .any(|item| matches!(item.action, Action::OpenSettings)),
            "expected the settings command"
        );
    }
}

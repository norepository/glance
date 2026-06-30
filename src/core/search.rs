//! Fuzzy-ranks the app index against a query using nucleo.

use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher};

use crate::core::app_index::{index_apps, AppEntry};

pub struct SearchEngine {
    index: Vec<AppEntry>,
    matcher: Matcher,
}

impl SearchEngine {
    pub fn new() -> Self {
        Self {
            index: index_apps(),
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Returns up to `limit` apps ranked by fuzzy match score (best first). An
    /// empty query yields no results.
    pub fn search(&mut self, query: &str, limit: usize) -> Vec<AppEntry> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut matches = pattern.match_list(self.index.iter(), &mut self.matcher);
        matches.truncate(limit);
        matches.into_iter().map(|(entry, _score)| entry.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_apps_and_finds_a_known_one() {
        let mut engine = SearchEngine::new();
        // Every macOS install has apps in /System/Applications.
        assert!(!engine.index.is_empty(), "expected a non-empty app index");

        // An empty query returns nothing.
        assert!(engine.search("", 8).is_empty());

        // Searching for an indexed app's own name surfaces it.
        let target = engine.index[0].name.clone();
        let results = engine.search(&target, 8);
        assert!(
            results.iter().any(|e| e.name == target),
            "expected to find {target:?} in results"
        );
        assert!(results.len() <= 8, "limit must be respected");
    }
}

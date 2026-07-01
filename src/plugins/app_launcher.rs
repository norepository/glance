//! Built-in plugin: fuzzy-matches installed applications.

use crate::core::app_index::{index_apps, AppEntry};
use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

/// Bonus so apps outrank files at equal match quality.
const APP_BONUS: u32 = 100;

pub struct AppLauncher {
    apps: Vec<AppEntry>,
}

impl AppLauncher {
    pub fn new() -> Self {
        Self { apps: index_apps() }
    }
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for AppLauncher {
    fn id(&self) -> &'static str {
        "apps"
    }

    fn search(&mut self, _query: &str, cx: &mut PluginCx) -> Vec<Scored> {
        cx.pattern
            .match_list(self.apps.iter(), cx.matcher)
            .into_iter()
            .map(|(app, score)| Scored {
                item: app_item(app),
                score: score + APP_BONUS,
            })
            .collect()
    }
}

fn app_item(app: &AppEntry) -> SearchItem {
    SearchItem {
        title: app.name.clone(),
        subtitle: None,
        icon_path: Some(app.path.clone()),
        action: Action::Open(app.path.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nucleo::pattern::{CaseMatching, Normalization, Pattern};
    use nucleo::{Config, Matcher};

    #[test]
    fn finds_a_known_app() {
        let mut plugin = AppLauncher::new();
        assert!(!plugin.apps.is_empty(), "expected a non-empty app index");

        let name = plugin.apps[0].name.clone();
        let pattern = Pattern::parse(&name, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut cx = PluginCx {
            pattern: &pattern,
            matcher: &mut matcher,
            limit: 8,
        };
        let results = plugin.search(&name, &mut cx);
        assert!(results.iter().any(|s| s.item.title == name));
    }
}

//! Built-in plugin: fuzzy-matches installed applications.

use crate::core::app_index::{app_dirs, index_apps, AppEntry};
use crate::core::item::{Action, SearchItem};
use crate::core::live_index::LiveIndex;

use super::{Plugin, PluginCx, Scored};

/// Bonus so apps outrank files at equal match quality.
const APP_BONUS: u32 = 100;

pub struct AppLauncher {
    apps: LiveIndex<AppEntry>,
}

impl AppLauncher {
    pub fn new() -> Self {
        let apps = LiveIndex::new();
        // Build now and re-index automatically when apps are installed/removed.
        apps.refresh(app_dirs(), index_apps);
        Self { apps }
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
        let apps = self.apps.read();
        cx.pattern
            .match_list(apps.iter(), cx.matcher)
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


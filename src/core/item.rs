//! A source-agnostic search result: apps, files, and built-in commands all
//! become a [`SearchItem`] so the result list and launch path are uniform.

use std::path::PathBuf;

/// What happens when a result is activated (Return / click).
#[derive(Clone)]
pub enum Action {
    /// Open a file or app bundle with its default handler.
    Open(PathBuf),
    /// Open a web URL in the default browser.
    OpenUrl(String),
    /// Open the Glance settings window.
    OpenSettings,
    /// Copy text to the clipboard.
    Copy(String),
    /// Run a command detached (used by Lua plugins).
    Run { program: String, args: Vec<String> },
    /// Re-scan and reload user Lua plugins.
    ReloadPlugins,
    /// Start a countdown timer.
    StartTimer { seconds: u64, label: Option<String> },
    /// Drill into a timer's management actions.
    ShowTimer(u64),
    /// Pause or resume a timer.
    ToggleTimer(u64),
    /// Restart a timer from its full duration.
    RestartTimer(u64),
    /// Cancel and remove a timer.
    CancelTimer(u64),
    /// Return to the active-timer list.
    ShowTimerList,
}

#[derive(Clone)]
pub struct SearchItem {
    pub title: String,
    /// Secondary line, e.g. the file path for file results.
    pub subtitle: Option<String>,
    /// Source for the row icon via `NSWorkspace::iconForFile`; `None` = no icon.
    pub icon_path: Option<PathBuf>,
    pub action: Action,
}

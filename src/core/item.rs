//! A source-agnostic search result: apps, files, and built-in commands all
//! become a [`SearchItem`] so the result list and launch path are uniform.

use std::path::PathBuf;

/// What happens when a result is activated (Return / click).
#[derive(Clone)]
pub enum Action {
    /// Open a file or app bundle with its default handler.
    Open(PathBuf),
    /// Open the Glance settings window.
    OpenSettings,
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

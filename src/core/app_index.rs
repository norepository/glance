//! Indexes installed applications by scanning the standard app folders.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// A single indexed application.
#[derive(Clone)]
pub struct AppEntry {
    pub name: String,
    pub path: PathBuf,
}

// Lets nucleo rank `AppEntry`s directly by their display name.
impl AsRef<str> for AppEntry {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

/// Scans the standard application locations for `.app` bundles. Each root is
/// scanned at its top level plus one level into subfolders (e.g. Utilities).
pub fn index_apps() -> Vec<AppEntry> {
    let mut roots: Vec<PathBuf> = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }

    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for root in &roots {
        scan_dir(root, true, &mut apps, &mut seen);
    }
    apps
}

fn scan_dir(dir: &Path, descend: bool, apps: &mut Vec<AppEntry>, seen: &mut HashSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("app") {
            add_app(path, apps, seen);
        } else if descend && path.is_dir() {
            // One level deep only, to catch /Applications/Utilities etc.
            scan_dir(&path, false, apps, seen);
        }
    }
}

fn add_app(path: PathBuf, apps: &mut Vec<AppEntry>, seen: &mut HashSet<PathBuf>) {
    let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if !seen.insert(canonical) {
        return;
    }
    let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return;
    };
    if name.is_empty() {
        return;
    }
    apps.push(AppEntry { name, path });
}

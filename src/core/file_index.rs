//! Background file index: walks the configured folders (skipping hidden files,
//! `.gitignore`d paths, and heavy directories) into an in-memory list that the
//! search engine fuzzy-matches.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockReadGuard};

use ignore::WalkBuilder;

/// Safety cap on indexed files, to bound memory/time for large trees.
const FILE_CAP: usize = 200_000;
/// Directory names skipped wholesale (in addition to hidden/.gitignore).
const SKIP_DIRS: &[&str] = &["Library", "node_modules"];

pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
}

// Lets nucleo rank `FileEntry`s directly by their file name.
impl AsRef<str> for FileEntry {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

/// A handle to the shared file index. Cloning shares the same underlying store,
/// so the search engine and the settings UI can both reach it.
#[derive(Clone)]
pub struct FileIndex {
    entries: Arc<RwLock<Vec<FileEntry>>>,
}

impl FileIndex {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Rebuilds the index from `folders` on a background thread, swapping in the
    /// result when done. The launcher stays responsive in the meantime.
    pub fn reindex(&self, folders: Vec<PathBuf>) {
        let entries = Arc::clone(&self.entries);
        std::thread::spawn(move || {
            let indexed = index_files(&folders);
            if let Ok(mut guard) = entries.write() {
                *guard = indexed;
            }
        });
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Vec<FileEntry>> {
        self.entries.read().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for FileIndex {
    fn default() -> Self {
        Self::new()
    }
}

fn index_files(folders: &[PathBuf]) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    for folder in folders {
        let walker = WalkBuilder::new(folder)
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .filter_entry(|entry| {
                // Skip heavy directories by name at any level.
                !entry
                    .file_name()
                    .to_str()
                    .map(|name| SKIP_DIRS.contains(&name))
                    .unwrap_or(false)
            })
            .build();

        for result in walker {
            if entries.len() >= FILE_CAP {
                return entries;
            }
            let Ok(entry) = result else { continue };
            if entry.file_type().is_some_and(|t| t.is_file()) {
                if let Some(file) = to_entry(entry.path()) {
                    entries.push(file);
                }
            }
        }
    }
    entries
}

fn to_entry(path: &Path) -> Option<FileEntry> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(FileEntry {
        name,
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_known_file() {
        let dir = std::env::temp_dir().join(format!("glance-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("needle.txt");
        std::fs::write(&file, b"hi").unwrap();

        let found = index_files(&[dir.clone()]);
        assert!(
            found.iter().any(|e| e.name == "needle.txt"),
            "expected to index needle.txt"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

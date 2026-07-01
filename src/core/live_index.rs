//! A background-refreshed index. Rebuilds its contents when the watched folders
//! change on disk (via FSEvents through `notify`), debounced, so results stay
//! fresh without polling — event-driven and low-overhead.

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, RwLock, RwLockReadGuard};
use std::thread;
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Quiet period after a change before rebuilding (coalesces bursts).
const DEBOUNCE: Duration = Duration::from_millis(800);

pub struct LiveIndex<T> {
    entries: Arc<RwLock<Vec<T>>>,
    // Kept alive so watching continues; dropped/replaced on each `refresh`.
    watcher: Arc<Mutex<Option<RecommendedWatcher>>>,
}

impl<T> Clone for LiveIndex<T> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            watcher: self.watcher.clone(),
        }
    }
}

impl<T> Default for LiveIndex<T> {
    fn default() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            watcher: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T: Send + Sync + 'static> LiveIndex<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Vec<T>> {
        self.entries.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Rebuilds now (in the background) and watches `dirs`, rebuilding again —
    /// debounced — whenever a structural change happens under them. Replaces any
    /// previous watch (so it's safe to call again when `dirs` change).
    pub fn refresh<F>(&self, dirs: Vec<PathBuf>, build: F)
    where
        F: Fn() -> Vec<T> + Send + 'static,
    {
        let entries = self.entries.clone();
        let (tx, rx) = mpsc::channel::<()>();

        // Worker: initial build, then debounced rebuilds until the channel closes
        // — which happens when this watcher is dropped/replaced.
        thread::spawn(move || {
            let store = |value| {
                if let Ok(mut guard) = entries.write() {
                    *guard = value;
                }
            };
            store(build());
            while rx.recv().is_ok() {
                // Drain the burst, then rebuild once.
                while rx.recv_timeout(DEBOUNCE).is_ok() {}
                store(build());
            }
        });

        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if is_structural(&event.kind) && event.paths.iter().any(|p| interesting(p)) {
                    let _ = tx.send(());
                }
            }
        });

        match watcher {
            Ok(mut watcher) => {
                for dir in &dirs {
                    let _ = watcher.watch(dir, RecursiveMode::Recursive);
                }
                // Dropping the previous watcher closes its channel, ending its worker.
                *self.watcher.lock().unwrap_or_else(|e| e.into_inner()) = Some(watcher);
            }
            Err(err) => {
                eprintln!("glance: file watcher unavailable ({err}); index won't auto-update.");
                // `tx` was moved into the failed handler and dropped, so the worker
                // does its initial build and then exits.
            }
        }
    }
}

/// Only structural changes (create/remove/rename) affect an index of names and
/// paths — content edits don't — so we ignore the latter to avoid needless
/// rebuilds (e.g. rebuilding on every file save).
fn is_structural(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Any
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Modify(ModifyKind::Any)
    )
}

/// Skips locations we never index anyway, so their churn (e.g. `~/Library`
/// caches) doesn't trigger rebuilds.
fn interesting(path: &Path) -> bool {
    !path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "Library" || name == "node_modules" || (name.starts_with('.') && name.len() > 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_dir(dir: PathBuf) -> impl Fn() -> Vec<String> + Send + 'static {
        move || {
            let mut names = Vec::new();
            if let Ok(read) = std::fs::read_dir(&dir) {
                for entry in read.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        names.push(name);
                    }
                }
            }
            names
        }
    }

    #[test]
    fn rebuilds_when_a_watched_folder_changes() {
        let dir = std::env::temp_dir().join(format!("glance-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let index: LiveIndex<String> = LiveIndex::new();
        index.refresh(vec![dir.clone()], list_dir(dir.clone()));

        // Change the folder after the watch is established.
        thread::sleep(Duration::from_millis(300));
        std::fs::write(dir.join("appeared.txt"), b"x").unwrap();

        // FSEvents latency + debounce; poll for the update.
        let mut found = false;
        for _ in 0..50 {
            if index.read().iter().any(|n| n == "appeared.txt") {
                found = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        std::fs::remove_dir_all(&dir).ok();
        assert!(found, "index should auto-refresh when the folder changes");
    }
}

//! Built-in plugin: fuzzy-matches files from the background index.

use std::path::Path;

use crate::core::file_index::FileIndex;
use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

/// Files are only searched once the query is at least this long, to avoid
/// ranking the entire index on one or two characters.
const MIN_FILE_QUERY_LEN: usize = 3;

pub struct FileSearch {
    files: FileIndex,
}

impl FileSearch {
    pub fn new(files: FileIndex) -> Self {
        Self { files }
    }
}

impl Plugin for FileSearch {
    fn id(&self) -> &'static str {
        "files"
    }

    fn search(&mut self, query: &str, cx: &mut PluginCx) -> Vec<Scored> {
        if query.len() < MIN_FILE_QUERY_LEN {
            return Vec::new();
        }
        let files = self.files.read();
        // match_list is sorted best-first; only the top `limit` can survive the
        // engine's final truncate, so avoid allocating the rest.
        cx.pattern
            .match_list(files.iter(), cx.matcher)
            .into_iter()
            .take(cx.limit)
            .map(|(file, score)| Scored {
                item: file_item(&file.name, &file.path),
                score,
            })
            .collect()
    }
}

fn file_item(name: &str, path: &Path) -> SearchItem {
    SearchItem {
        title: name.to_string(),
        subtitle: Some(path.to_string_lossy().into_owned()),
        icon_path: Some(path.to_path_buf()),
        action: Action::Open(path.to_path_buf()),
    }
}

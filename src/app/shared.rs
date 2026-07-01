//! `Shared` — cheaply-clonable handles to live runtime state, so the settings
//! window can mutate the engine, hotkey, result limit, and web links in place.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::app::hotkey::Hotkey;
use crate::core::config::WebLink;
use crate::core::file_index::FileIndex;
use crate::core::search::SearchEngine;

#[derive(Clone)]
pub struct Shared {
    pub engine: Rc<RefCell<SearchEngine>>,
    pub file_index: FileIndex,
    pub hotkey: Rc<RefCell<Hotkey>>,
    pub max_results: Rc<Cell<usize>>,
    pub web_links: Rc<RefCell<Vec<WebLink>>>,
}

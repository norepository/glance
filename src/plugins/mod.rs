//! Plugin architecture. Built-in result sources implement [`Plugin`] directly
//! (native, in-process); user-authored Lua plugins are hosted behind the same
//! trait by [`lua_host`].

pub mod app_launcher;
pub mod calculator;
pub mod calendar;
pub mod file_search;
pub mod lua_host;
pub mod system;
pub mod web_search;

use nucleo::pattern::Pattern;
use nucleo::Matcher;

use crate::core::item::SearchItem;

/// A result paired with its relevance score. The engine merges `Scored`s from
/// every plugin, sorts by score (desc), and truncates.
pub struct Scored {
    pub item: SearchItem,
    pub score: u32,
}

/// Shared services handed to plugins for one query.
pub struct PluginCx<'a> {
    /// The parsed query pattern, for fuzzy plugins (`pattern.match_list(...)`).
    pub pattern: &'a Pattern,
    /// The shared, reused fuzzy matcher.
    pub matcher: &'a mut Matcher,
    /// Maximum results the engine will keep — plugins may cap their own output.
    pub limit: usize,
}

pub trait Plugin {
    /// Stable identifier, used in logs (e.g. when a Lua plugin errors).
    #[allow(dead_code)]
    fn id(&self) -> &'static str;

    /// Produce scored results for `query` (already trimmed). Keyword- or
    /// parse-gated plugins return an empty vec when they don't apply.
    fn search(&mut self, query: &str, cx: &mut PluginCx) -> Vec<Scored>;

    /// Re-load any external state (the Lua host re-scans its plugin dir).
    /// No-op for native plugins.
    fn reload(&mut self) {}
}

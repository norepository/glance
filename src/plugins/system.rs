//! Built-in plugin: Glance's own commands (e.g. open Settings), surfaced when
//! the query is a prefix of one of a command's keywords.

use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

/// Commands need at least this many characters so single letters don't surface
/// them.
const MIN_CMD_QUERY_LEN: usize = 2;
const CMD_BONUS: u32 = 100;

struct Command {
    keywords: &'static str,
    title: &'static str,
    subtitle: &'static str,
    action: Action,
}

impl Command {
    /// The first keyword the (lower-cased) query is a prefix of, if any.
    fn prefix_token(&self, query_lower: &str) -> Option<&'static str> {
        let keywords: &'static str = self.keywords;
        keywords
            .split_whitespace()
            .find(|token| token.starts_with(query_lower))
    }
}

pub struct System {
    commands: Vec<Command>,
}

impl System {
    pub fn new() -> Self {
        Self {
            commands: vec![
                Command {
                    keywords: "settings preferences glance",
                    title: "Glance Settings",
                    subtitle: "Preferences",
                    action: Action::OpenSettings,
                },
                Command {
                    keywords: "reload plugins",
                    title: "Reload Plugins",
                    subtitle: "Re-scan Lua plugins",
                    action: Action::ReloadPlugins,
                },
            ],
        }
    }
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for System {
    fn id(&self) -> &'static str {
        "system"
    }

    fn search(&mut self, query: &str, cx: &mut PluginCx) -> Vec<Scored> {
        if query.len() < MIN_CMD_QUERY_LEN {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut out = Vec::new();
        for cmd in &self.commands {
            // Gate on a prefix match, then score the matched keyword with nucleo
            // so the command ranks on the same scale as apps.
            if let Some(token) = cmd.prefix_token(&query_lower) {
                if let Some((_, score)) = cx
                    .pattern
                    .match_list(std::iter::once(token), cx.matcher)
                    .into_iter()
                    .next()
                {
                    out.push(Scored {
                        item: command_item(cmd),
                        score: score + CMD_BONUS,
                    });
                }
            }
        }
        out
    }
}

fn command_item(cmd: &Command) -> SearchItem {
    SearchItem {
        title: cmd.title.to_string(),
        subtitle: Some(cmd.subtitle.to_string()),
        icon_path: None,
        action: cmd.action.clone(),
    }
}

//! Hosts user-authored Lua plugins from
//! `~/Library/Application Support/Glance/plugins/<name>/plugin.lua`.
//!
//! Each plugin script returns a table `{ name, keyword?, search = fn(query) }`
//! where `search` returns a list of `{ title, subtitle?, icon?, action? }`.
//! Actions are declarative: `{ type = "open"|"url"|"copy"|"run", ... }`.

use std::path::{Path, PathBuf};

use mlua::{Function, Lua, Table};

use crate::core::config::plugins_dir;
use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

/// Base score for script results (keyword-triggered, so ranking high is fine);
/// the plugin's own ordering is preserved by decrementing per item.
const SCRIPT_BASE: u32 = 8_000;

struct LuaPlugin {
    name: String,
    keyword: Option<String>,
    // `search` is declared before `lua` so it drops first; both are reference
    // counted by mlua, so order is not strictly required.
    search: Function,
    #[allow(dead_code)]
    lua: Lua,
}

impl LuaPlugin {
    fn load(dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let script = std::fs::read_to_string(dir.join("plugin.lua"))?;
        let lua = Lua::new();
        install_api(&lua)?;
        let table: Table = lua.load(&script).set_name(dir.to_string_lossy()).eval()?;

        let name = table
            .get::<Option<String>>("name")
            .unwrap_or(None)
            .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "plugin".to_string());
        let keyword = table.get::<Option<String>>("keyword").unwrap_or(None);
        let search: Function = table.get("search")?;

        Ok(LuaPlugin {
            name,
            keyword,
            search,
            lua,
        })
    }

    fn run(&self, query: &str) -> Result<Vec<SearchItem>, mlua::Error> {
        let ret: Table = self.search.call(query.to_string())?;
        let mut items = Vec::new();
        for value in ret.sequence_values::<Table>() {
            if let Some(item) = parse_item(value?) {
                items.push(item);
            }
        }
        Ok(items)
    }
}

/// Installs the `glance` helper table available to every plugin.
fn install_api(lua: &Lua) -> mlua::Result<()> {
    let glance = lua.create_table()?;
    // glance.run(cmd) -> stdout: run a shell command and capture its output.
    glance.set(
        "run",
        lua.create_function(|_, cmd: String| {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .map_err(mlua::Error::external)?;
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        })?,
    )?;
    lua.globals().set("glance", glance)?;
    Ok(())
}

fn parse_item(t: Table) -> Option<SearchItem> {
    let title: String = t.get("title").ok()?;
    let subtitle = t.get::<Option<String>>("subtitle").unwrap_or(None);
    let icon_path = t
        .get::<Option<String>>("icon")
        .unwrap_or(None)
        .map(PathBuf::from);
    let action = t
        .get::<Option<Table>>("action")
        .ok()
        .flatten()
        .and_then(|a| parse_action(&a))
        .unwrap_or_else(|| Action::Copy(title.clone()));
    Some(SearchItem {
        title,
        subtitle,
        icon_path,
        action,
    })
}

fn parse_action(t: &Table) -> Option<Action> {
    let kind: String = t.get("type").ok()?;
    match kind.as_str() {
        "open" => Some(Action::Open(PathBuf::from(t.get::<String>("value").ok()?))),
        "url" => Some(Action::OpenUrl(t.get::<String>("value").ok()?)),
        "copy" => Some(Action::Copy(t.get::<String>("value").ok()?)),
        "run" => Some(Action::Run {
            program: t.get::<String>("program").ok()?,
            args: t.get::<Vec<String>>("args").unwrap_or_default(),
        }),
        _ => None,
    }
}

pub struct ScriptHost {
    plugins: Vec<LuaPlugin>,
}

impl ScriptHost {
    pub fn new() -> Self {
        let mut host = ScriptHost {
            plugins: Vec::new(),
        };
        host.load_all();
        host
    }

    fn load_all(&mut self) {
        self.plugins.clear();
        let dir = plugins_dir();
        scaffold_if_missing(&dir);

        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("plugin.lua").is_file() {
                match LuaPlugin::load(&path) {
                    Ok(plugin) => self.plugins.push(plugin),
                    Err(err) => {
                        eprintln!("glance: failed to load plugin {}: {err}", path.display())
                    }
                }
            }
        }
    }
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for ScriptHost {
    fn id(&self) -> &'static str {
        "lua"
    }

    fn reload(&mut self) {
        self.load_all();
    }

    fn search(&mut self, query: &str, _cx: &mut PluginCx) -> Vec<Scored> {
        let mut out = Vec::new();
        for plugin in &self.plugins {
            // Keyword plugins only run on "<kw>" or "<kw> <rest>".
            let sub = match &plugin.keyword {
                Some(kw) if query == kw => "",
                Some(kw) => match query.strip_prefix(kw.as_str()).and_then(|r| r.strip_prefix(' '))
                {
                    Some(rest) => rest,
                    None => continue,
                },
                None => query,
            };

            match plugin.run(sub) {
                Ok(items) => {
                    for (i, item) in items.into_iter().enumerate() {
                        out.push(Scored {
                            item,
                            score: SCRIPT_BASE.saturating_sub(i as u32),
                        });
                    }
                }
                Err(err) => eprintln!("glance: plugin '{}' error: {err}", plugin.name),
            }
        }
        out
    }
}

/// On first run, create the plugins dir with a small example so users have a
/// template. Never overwrites an existing dir.
fn scaffold_if_missing(dir: &Path) {
    if dir.exists() {
        return;
    }
    if std::fs::create_dir_all(dir.join("hello")).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join("hello/plugin.lua"), SAMPLE_PLUGIN);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_runs_a_plugin() {
        let dir = std::env::temp_dir().join(format!("glance-lua-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.lua"),
            r#"
return {
  name = "T", keyword = "t",
  search = function(q)
    return { { title = "got " .. q, action = { type = "copy", value = q } } }
  end,
}
"#,
        )
        .unwrap();

        let plugin = LuaPlugin::load(&dir).unwrap();
        assert_eq!(plugin.keyword.as_deref(), Some("t"));

        let items = plugin.run("hi").unwrap();
        assert_eq!(items[0].title, "got hi");
        assert!(matches!(&items[0].action, Action::Copy(v) if v == "hi"));

        std::fs::remove_dir_all(&dir).ok();
    }
}

const SAMPLE_PLUGIN: &str = r#"-- Glance sample plugin. Type "hello <text>" in the launcher.
return {
  name = "Hello",
  keyword = "hello",
  search = function(query)
    if query == "" then
      return { { title = "Hello from Lua!", subtitle = "type something after 'hello'" } }
    end
    return {
      {
        title = "Copy: " .. query,
        subtitle = "press return to copy",
        action = { type = "copy", value = query },
      },
      {
        title = "Search Google for " .. query,
        action = { type = "url", value = "https://www.google.com/search?q=" .. query },
      },
    }
  end,
}
"#;

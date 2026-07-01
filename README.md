# 🧿 Glance

A blazing fast, native and minimal launcher for macOS 

![preview](./assets/default.png)

## Why

We've all been there: you hit Spotlight, type a query, and get not what you were looking for. This tool fixes that — and it's faster too.

## Features

- **Global hotkey** — summon/dismiss a blurred, rounded panel with **⌘+Space** or custom hotkey.
- **App launcher** — fuzzy-matches installed apps and launches on Return.
- **File search** — fuzzy-matches files in configured folders (default `$HOME`), indexed in
  the background.
- **Calculator + units** — `2+2`, `sqrt(2)`, `5 km in miles`; Return copies the result.
- **Web search / quick links** — `g rust traits`, `yt lofi`, `gh …`, or bare URLs.
- **Settings window** — manage search folders and more
- **User plugins in Lua** — drop a script in the plugins folder to add your own commands
  (see [Writing plugins](#writing-plugins)).
- **No Dock icon** — runs as an accessory agent; quit from the settings window.

## Requirements

- macOS 
- Rust 
- Xcode **Command Line Tools**

## Running

```sh
cargo run
```

Press **⌘+Space** to summon/dismiss the panel (configurable in Settings). Running via
`cargo run` is the quickest way to develop, but it launches the bare binary 

> **Note:** ⌘+Space is macOS's default Spotlight shortcut. If the panel doesn't appear, free
> it up in **System Settings → Keyboard → Keyboard Shortcuts → Spotlight**

## Building the app

Bundle a proper `Glance.app`

```sh
./scripts/make_icon.sh 🧿      # any emoji; omit for the default
```

```sh
./scripts/bundle.sh            # → target/release/bundle/Glance.app
./scripts/bundle.sh --install  # also copies it to /Applications
```

Edit the `NSColor(...)` line in `scripts/make_icon.sh` to change the background card color,
or remove the background fill for a transparent, glyph-only icon. macOS caches icons
aggressively — if the old one lingers after reinstalling, run `killall Dock Finder`.

## Writing plugins

User plugins are Lua scripts in `~/Library/Application Support/Glance/plugins/`. Each plugin
is a folder containing a `plugin.lua` that returns a table:

```lua
return {
  name = "Weather",
  keyword = "wt",  -- optional; if set, the plugin only runs on "wt" or "wt <rest>"
  search = function(query)
    local report = glance.run("curl -s wttr.in/" .. query .. "?format=3")
    return {
      { title = report, subtitle = "press return to copy",
        action = { type = "copy", value = report } },
    }
  end,
}
```

`search(query)` returns a list of result items. Each item has `title`, an optional
`subtitle` and `icon` (a file path), and an `action`:

| Action | Effect |
| --- | --- |
| `{ type = "open", value = "/path" }` | Open a file/app with its default handler |
| `{ type = "url", value = "https://…" }` | Open a URL in the browser |
| `{ type = "copy", value = "text" }` | Copy text to the clipboard |
| `{ type = "run", program = "…", args = {…} }` | Run a command detached |

If `action` is omitted, Return copies the title. The `glance.run(cmd)` helper runs a shell
command and returns its stdout. Plugins run as your own code with the full Lua standard
library. After adding or editing a plugin, run **"Reload Plugins"** from the launcher (a
`hello` example is created on first launch).

## To Fix/Do
- User plugins buggy
- Sometimes buggy when opening new applications
- Calendar events when opening Glance

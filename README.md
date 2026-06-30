# Glance

A fast, native, minimal app launcher for macOS — Spotlight-style, written in Rust on
AppKit. No Electron, no webview, no runtime. Just a compiled binary that summons a
floating search panel the instant you press a key.

> **Status:** Milestone 1 — the window mechanics work. A borderless panel toggles on a
> global hotkey, takes focus, and dismisses cleanly. Search, app indexing, and plugins
> are next.

## Why

Built as a reaction to Electron-based launchers feeling sluggish. The goal is a
keystroke-to-result budget of **under 16 ms** by talking to AppKit directly instead of
shipping a browser.

## Features so far

- **Global hotkey** — summon/dismiss the panel with **⌘+Space** from anywhere.
- **Borderless floating panel** — a centered `NSPanel` that takes keyboard focus and
  floats above other windows.
- **Quick dismissal** — **Esc** hides it; clicking away (losing focus) hides it.
- **No Dock icon** — runs as an accessory agent.
- **No special permissions** — the hotkey uses Carbon's `RegisterEventHotKey`, so there's
  no Accessibility prompt.

## Requirements

- macOS (developed against macOS 26)
- Rust (developed against 1.96)
- Xcode **Command Line Tools** — no full Xcode / `.xcodeproj` needed

## Run

```sh
cargo run
```

Press **⌘+Space** to toggle the panel.

> **Note:** ⌘+Space is macOS's default Spotlight shortcut. If the panel doesn't appear,
> free up the key in **System Settings → Keyboard → Keyboard Shortcuts → Spotlight**
> (uncheck "Show Spotlight search"), then relaunch. The terminal prints a clear message if
> the hotkey couldn't be registered.

## Architecture

```
src/
├── main.rs            # NSApplication setup, accessory policy, install delegate, run loop
├── app/
│   └── delegate.rs    # AppDelegate: owns the panel + hotkey, polls events, toggles
└── ui/
    └── panel.rs       # GlancePanel (borderless NSPanel) + search field
```

Built on [`objc2`](https://crates.io/crates/objc2) for AppKit bindings,
[`global-hotkey`](https://crates.io/crates/global-hotkey) for the system shortcut.

A few implementation notes:

- **`GlancePanel` subclasses `NSPanel`** and overrides `canBecomeKeyWindow` — a borderless
  `NSWindow` refuses key status, so its text field couldn't accept input. An `NSPanel`
  can.
- **Hotkey events are drained from the run loop** by a repeating `NSTimer`.
  `global-hotkey` delivers events over a channel rather than pushing them, so we poll it
  (~50 ms) from the main thread where AppKit lives.
- **Dismissal** is handled in `NSResponder`/`NSWindow` overrides: `cancelOperation:` for
  Esc, `resignKeyWindow` for click-away.

## Roadmap

- [x] **M1** — Borderless panel toggling on a global hotkey
- [ ] **M2** — App indexing + fuzzy search + result list
- [ ] **M3** — Plugin trait + Calculator plugin alongside the app launcher
- [ ] **M4** — Vibrancy/blur, icon caching, performance pass, `.app` bundling + signing

## License

TBD.

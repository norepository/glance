//! `SettingsController` — the settings window: launch-at-login, summon shortcut,
//! max results, web quick-links, search folders, and utility buttons. Every
//! change is persisted (full config) and applied to live runtime state.

use std::cell::RefCell;
use std::path::PathBuf;
use std::ptr::NonNull;

use block2::RcBlock;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSColor, NSControl, NSControlStateValueOff,
    NSControlStateValueOn, NSEvent, NSEventMask, NSEventModifierFlags, NSFont, NSModalResponseOK,
    NSOpenPanel, NSStepper, NSTextField, NSView, NSWindow, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString, NSURL};

use crate::app::login_item;
use crate::app::shared::Shared;
use crate::core::config::{config_path, plugins_dir, Config, WebLink};

const WIN_W: f64 = 600.0;
const WIN_H: f64 = 660.0;
const PAD: f64 = 20.0;
const GAP: f64 = 14.0;
const ROW_H: f64 = 30.0;
const LIST_H: f64 = 120.0;

pub struct SettingsControllerIvars {
    window: Retained<NSWindow>,
    web_list: Retained<NSView>,
    folder_list: Retained<NSView>,
    login_checkbox: Retained<NSButton>,
    max_label: Retained<NSTextField>,
    record_button: Retained<NSButton>,
    keyword_field: Retained<NSTextField>,
    url_field: Retained<NSTextField>,
    hotkey_monitor: RefCell<Option<Retained<AnyObject>>>,
    folders: RefCell<Vec<PathBuf>>,
    shared: Shared,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlanceSettingsController"]
    #[ivars = SettingsControllerIvars]
    pub struct SettingsController;

    unsafe impl NSObjectProtocol for SettingsController {}

    impl SettingsController {
        #[unsafe(method(addFolder:))]
        fn add_folder(&self, _sender: Option<&AnyObject>) {
            let panel = NSOpenPanel::openPanel(self.mtm());
            panel.setCanChooseDirectories(true);
            panel.setCanChooseFiles(false);
            panel.setAllowsMultipleSelection(true);
            if panel.runModal() == NSModalResponseOK {
                let mut added = false;
                for url in panel.URLs().to_vec() {
                    if let Some(path) = url.path() {
                        let path = PathBuf::from(path.to_string());
                        let mut folders = self.ivars().folders.borrow_mut();
                        if !folders.contains(&path) {
                            folders.push(path);
                            added = true;
                        }
                    }
                }
                if added {
                    self.persist();
                    self.reindex_files();
                    self.rebuild_folders();
                }
            }
        }

        #[unsafe(method(removeFolder:))]
        fn remove_folder(&self, sender: &NSControl) {
            let index = sender.tag() as usize;
            {
                let mut folders = self.ivars().folders.borrow_mut();
                if index >= folders.len() {
                    return;
                }
                folders.remove(index);
            }
            self.persist();
            self.reindex_files();
            self.rebuild_folders();
        }

        #[unsafe(method(addWebLink:))]
        fn add_web_link(&self, _sender: Option<&AnyObject>) {
            let iv = self.ivars();
            let keyword = iv.keyword_field.stringValue().to_string().trim().to_string();
            let url = iv.url_field.stringValue().to_string().trim().to_string();
            if keyword.is_empty() || !url.contains("{}") {
                return;
            }
            iv.web_links().push(WebLink {
                keyword,
                name: iv.web_name_from_url(&url),
                url,
            });
            iv.keyword_field.setStringValue(&NSString::from_str(""));
            iv.url_field.setStringValue(&NSString::from_str(""));
            self.persist();
            self.rebuild_web_links();
        }

        #[unsafe(method(removeWebLink:))]
        fn remove_web_link(&self, sender: &NSControl) {
            let index = sender.tag() as usize;
            {
                let mut links = self.ivars().shared.web_links.borrow_mut();
                if index >= links.len() {
                    return;
                }
                links.remove(index);
            }
            self.persist();
            self.rebuild_web_links();
        }

        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, _sender: Option<&AnyObject>) {
            let checkbox = &self.ivars().login_checkbox;
            let want_on = checkbox.state() == NSControlStateValueOn;
            if let Err(err) = login_item::set_enabled(want_on) {
                eprintln!("glance: launch-at-login change failed: {err}");
                // Revert the checkbox to the real state.
                self.set_login_state();
            }
        }

        #[unsafe(method(changeMaxResults:))]
        fn change_max_results(&self, sender: &NSControl) {
            let value = sender.integerValue().max(1) as usize;
            self.ivars().shared.max_results.set(value);
            self.ivars()
                .max_label
                .setStringValue(&NSString::from_str(&value.to_string()));
            self.persist();
        }

        #[unsafe(method(recordHotkey:))]
        fn record_hotkey(&self, _sender: Option<&AnyObject>) {
            self.start_recording();
        }

        #[unsafe(method(openPluginsFolder:))]
        fn open_plugins_folder(&self, _sender: Option<&AnyObject>) {
            let dir = plugins_dir();
            let _ = std::fs::create_dir_all(&dir);
            open_path(&dir);
        }

        #[unsafe(method(openConfigFile:))]
        fn open_config_file(&self, _sender: Option<&AnyObject>) {
            open_path(&config_path());
        }

        #[unsafe(method(reloadPlugins:))]
        fn reload_plugins(&self, _sender: Option<&AnyObject>) {
            self.ivars().shared.engine.borrow_mut().reload_plugins();
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl SettingsControllerIvars {
    fn web_links(&self) -> std::cell::RefMut<'_, Vec<WebLink>> {
        self.shared.web_links.borrow_mut()
    }

    /// A display name derived from a URL's host (e.g. "google" from google.com).
    fn web_name_from_url(&self, url: &str) -> String {
        url.split("://")
            .nth(1)
            .unwrap_or(url)
            .split('/')
            .next()
            .unwrap_or(url)
            .trim_start_matches("www.")
            .split('.')
            .next()
            .unwrap_or(url)
            .to_string()
    }
}

impl SettingsController {
    pub fn new(mtm: MainThreadMarker, shared: Shared) -> Retained<Self> {
        let folders = Config::load().search_folders;
        let zero = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));

        let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIN_W, WIN_H));
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;
        let window: Retained<NSWindow> = unsafe {
            msg_send![
                NSWindow::alloc(mtm),
                initWithContentRect: content_rect,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        };
        window.setTitle(&NSString::from_str("Glance Settings"));
        unsafe { window.setReleasedWhenClosed(false) };

        // Controls are created now (targets wired after `self` exists).
        let web_list = NSView::initWithFrame(NSView::alloc(mtm), zero);
        let folder_list = NSView::initWithFrame(NSView::alloc(mtm), zero);
        let login_checkbox = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Launch at login"),
                None,
                None,
                mtm,
            )
        };
        let max_label = plain_label(mtm, "8", 13.0, false);
        let record_button = unsafe {
            NSButton::buttonWithTitle_target_action(&NSString::from_str("Record…"), None, None, mtm)
        };
        let keyword_field = input_field(mtm, "keyword");
        let url_field = input_field(mtm, "https://example.com/?q={}");

        let this = Self::alloc(mtm).set_ivars(SettingsControllerIvars {
            window: window.clone(),
            web_list: web_list.clone(),
            folder_list: folder_list.clone(),
            login_checkbox: login_checkbox.clone(),
            max_label: max_label.clone(),
            record_button: record_button.clone(),
            keyword_field: keyword_field.clone(),
            url_field: url_field.clone(),
            hotkey_monitor: RefCell::new(None),
            folders: RefCell::new(folders),
            shared,
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };

        this.build_chrome(mtm);
        this.rebuild_web_links();
        this.rebuild_folders();
        this
    }

    pub fn show(&self) {
        NSApplication::sharedApplication(self.mtm()).activate();
        let window = &self.ivars().window;
        self.set_login_state();
        self.update_shortcut_label();
        window.center();
        window.makeKeyAndOrderFront(None);
    }

    fn build_chrome(&self, mtm: MainThreadMarker) {
        let iv = self.ivars();
        let Some(content) = iv.window.contentView() else {
            return;
        };
        let target: &AnyObject = self;
        let inner_w = WIN_W - 2.0 * PAD;
        let mut y = WIN_H - PAD;
        // Places a row of `height`, returns its origin y and advances the cursor.
        let mut row = |height: f64| -> f64 {
            y -= height;
            let origin = y;
            y -= GAP;
            origin
        };

        // Title.
        let title = plain_label(mtm, "Glance Settings", 16.0, true);
        title.setFrame(NSRect::new(NSPoint::new(PAD, row(26.0)), NSSize::new(inner_w, 24.0)));
        content.addSubview(&title);

        // Launch at login.
        let ly = row(22.0);
        iv.login_checkbox
            .setFrame(NSRect::new(NSPoint::new(PAD, ly), NSSize::new(inner_w, 22.0)));
        wire(&iv.login_checkbox, target, sel!(toggleLogin:));
        content.addSubview(&iv.login_checkbox);

        // Max results: label + stepper + value.
        let my = row(ROW_H);
        let max_cap = plain_label(mtm, "Max results:", 13.0, false);
        max_cap.setFrame(NSRect::new(NSPoint::new(PAD, my + 4.0), NSSize::new(110.0, 22.0)));
        content.addSubview(&max_cap);
        let stepper = NSStepper::initWithFrame(
            NSStepper::alloc(mtm),
            NSRect::new(NSPoint::new(PAD + 120.0, my), NSSize::new(20.0, ROW_H)),
        );
        stepper.setMinValue(1.0);
        stepper.setMaxValue(12.0);
        stepper.setIntegerValue(iv.shared.max_results.get() as isize);
        stepper.setValueWraps(false);
        wire(&stepper, target, sel!(changeMaxResults:));
        content.addSubview(&stepper);
        iv.max_label.setStringValue(&NSString::from_str(
            &iv.shared.max_results.get().to_string(),
        ));
        iv.max_label
            .setFrame(NSRect::new(NSPoint::new(PAD + 150.0, my + 4.0), NSSize::new(60.0, 22.0)));
        content.addSubview(&iv.max_label);

        // Shortcut: label + record button.
        let sy = row(ROW_H);
        let sc_cap = plain_label(mtm, "Shortcut:", 13.0, false);
        sc_cap.setFrame(NSRect::new(NSPoint::new(PAD, sy + 4.0), NSSize::new(90.0, 22.0)));
        content.addSubview(&sc_cap);
        iv.record_button
            .setFrame(NSRect::new(NSPoint::new(PAD + 100.0, sy), NSSize::new(220.0, ROW_H)));
        wire(&iv.record_button, target, sel!(recordHotkey:));
        content.addSubview(&iv.record_button);

        // Web quick-links heading + list + add row.
        let wh = row(22.0);
        let web_head = plain_label(mtm, "Web quick-links", 14.0, true);
        web_head.setFrame(NSRect::new(NSPoint::new(PAD, wh), NSSize::new(inner_w, 22.0)));
        content.addSubview(&web_head);
        let wl = row(LIST_H);
        iv.web_list
            .setFrame(NSRect::new(NSPoint::new(PAD, wl), NSSize::new(inner_w, LIST_H)));
        content.addSubview(&iv.web_list);
        let wa = row(ROW_H);
        iv.keyword_field
            .setFrame(NSRect::new(NSPoint::new(PAD, wa), NSSize::new(80.0, ROW_H - 4.0)));
        content.addSubview(&iv.keyword_field);
        iv.url_field.setFrame(NSRect::new(
            NSPoint::new(PAD + 90.0, wa),
            NSSize::new(inner_w - 90.0 - 90.0, ROW_H - 4.0),
        ));
        content.addSubview(&iv.url_field);
        let add_web = button(mtm, "Add", self, sel!(addWebLink:));
        add_web.setFrame(NSRect::new(
            NSPoint::new(WIN_W - PAD - 80.0, wa),
            NSSize::new(80.0, ROW_H),
        ));
        content.addSubview(&add_web);

        // Folders heading + list + add.
        let fh = row(22.0);
        let fold_head = plain_label(mtm, "Folders to search", 14.0, true);
        fold_head.setFrame(NSRect::new(NSPoint::new(PAD, fh), NSSize::new(inner_w, 22.0)));
        content.addSubview(&fold_head);
        let fl = row(LIST_H);
        iv.folder_list
            .setFrame(NSRect::new(NSPoint::new(PAD, fl), NSSize::new(inner_w, LIST_H)));
        content.addSubview(&iv.folder_list);
        let fa = row(ROW_H);
        let add_folder = button(mtm, "Add Folder…", self, sel!(addFolder:));
        add_folder.setFrame(NSRect::new(NSPoint::new(PAD, fa), NSSize::new(140.0, ROW_H)));
        content.addSubview(&add_folder);

        // Footer utility buttons (absolute, bottom row).
        let footer = [
            ("Open Plugins Folder", sel!(openPluginsFolder:)),
            ("Open Config File", sel!(openConfigFile:)),
            ("Reload Plugins", sel!(reloadPlugins:)),
            ("Quit Glance", sel!(quit:)),
        ];
        let bw = (inner_w - 3.0 * 8.0) / 4.0;
        for (i, (label, action)) in footer.into_iter().enumerate() {
            let b = button(mtm, label, self, action);
            b.setFrame(NSRect::new(
                NSPoint::new(PAD + i as f64 * (bw + 8.0), PAD),
                NSSize::new(bw, ROW_H),
            ));
            content.addSubview(&b);
        }
    }

    fn rebuild_web_links(&self) {
        let mtm = self.mtm();
        let iv = self.ivars();
        for view in iv.web_list.subviews().to_vec() {
            view.removeFromSuperview();
        }
        let height = iv.web_list.frame().size.height;
        let width = iv.web_list.frame().size.width;
        let links = iv.shared.web_links.borrow();
        for (i, link) in links.iter().enumerate() {
            let ry = height - (i as f64 + 1.0) * 28.0;
            let label = plain_label(mtm, &format!("{}  →  {}", link.keyword, link.url), 12.0, false);
            label.setFrame(NSRect::new(
                NSPoint::new(0.0, ry + 4.0),
                NSSize::new(width - 90.0, 20.0),
            ));
            iv.web_list.addSubview(&label);
            let remove = button(mtm, "Remove", self, sel!(removeWebLink:));
            remove.setTag(i as isize);
            remove.setFrame(NSRect::new(NSPoint::new(width - 80.0, ry), NSSize::new(80.0, 26.0)));
            iv.web_list.addSubview(&remove);
        }
    }

    fn rebuild_folders(&self) {
        let mtm = self.mtm();
        let iv = self.ivars();
        for view in iv.folder_list.subviews().to_vec() {
            view.removeFromSuperview();
        }
        let height = iv.folder_list.frame().size.height;
        let width = iv.folder_list.frame().size.width;
        let folders = iv.folders.borrow();
        for (i, folder) in folders.iter().enumerate() {
            let ry = height - (i as f64 + 1.0) * 28.0;
            let label = plain_label(mtm, &folder.to_string_lossy(), 12.0, false);
            label.setFrame(NSRect::new(
                NSPoint::new(0.0, ry + 4.0),
                NSSize::new(width - 90.0, 20.0),
            ));
            iv.folder_list.addSubview(&label);
            let remove = button(mtm, "Remove", self, sel!(removeFolder:));
            remove.setTag(i as isize);
            remove.setFrame(NSRect::new(NSPoint::new(width - 80.0, ry), NSSize::new(80.0, 26.0)));
            iv.folder_list.addSubview(&remove);
        }
    }

    fn set_login_state(&self) {
        let on = if login_item::is_enabled() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        };
        self.ivars().login_checkbox.setState(on);
    }

    fn update_shortcut_label(&self) {
        let title = self.ivars().shared.hotkey.borrow().current().to_string();
        self.ivars()
            .record_button
            .setTitle(&NSString::from_str(&title));
    }

    /// Arms a key-event monitor to capture the next shortcut. The next KeyDown
    /// with a modifier is turned into the new hotkey; Esc cancels.
    fn start_recording(&self) {
        if self.ivars().hotkey_monitor.borrow().is_some() {
            return;
        }
        self.ivars()
            .record_button
            .setTitle(&NSString::from_str("Press keys…"));

        let this = unsafe { Retained::retain(self as *const Self as *mut Self) }
            .expect("retain settings controller");
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event = unsafe { event.as_ref() };
            // Escape (keyCode 53) cancels recording.
            if event.keyCode() == 53 {
                this.finish_recording();
                return std::ptr::null_mut();
            }
            let chars = event
                .charactersIgnoringModifiers()
                .map(|s| s.to_string())
                .unwrap_or_default();
            if let Some(hotkey) = hotkey_from(event.modifierFlags(), &chars) {
                this.apply_hotkey(hotkey);
                this.finish_recording();
            }
            // Swallow the key so it doesn't type into anything.
            std::ptr::null_mut()
        });

        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
        };
        *self.ivars().hotkey_monitor.borrow_mut() = monitor;
    }

    fn finish_recording(&self) {
        if let Some(token) = self.ivars().hotkey_monitor.borrow_mut().take() {
            unsafe { NSEvent::removeMonitor(&token) };
        }
        self.update_shortcut_label();
    }

    fn apply_hotkey(&self, hotkey: HotKey) {
        let result = self.ivars().shared.hotkey.borrow_mut().set(hotkey);
        match result {
            Ok(()) => self.persist(),
            Err(err) => eprintln!("glance: could not set shortcut {hotkey} ({err})."),
        }
    }

    fn reindex_files(&self) {
        let folders = self.ivars().folders.borrow().clone();
        self.ivars().shared.file_index.reindex(folders);
    }

    /// Writes the full config from current live state, so no field is clobbered.
    fn persist(&self) {
        let iv = self.ivars();
        let config = Config {
            search_folders: iv.folders.borrow().clone(),
            hotkey: iv.shared.hotkey.borrow().current().to_string(),
            max_results: iv.shared.max_results.get(),
            web_links: iv.shared.web_links.borrow().clone(),
        };
        let _ = config.save();
    }
}

/// Builds a `HotKey` from captured modifier flags + the base character. Requires
/// at least one modifier; returns `None` for unmapped keys.
fn hotkey_from(flags: NSEventModifierFlags, chars: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    if flags.contains(NSEventModifierFlags::Command) {
        mods |= Modifiers::SUPER;
    }
    if flags.contains(NSEventModifierFlags::Option) {
        mods |= Modifiers::ALT;
    }
    if flags.contains(NSEventModifierFlags::Control) {
        mods |= Modifiers::CONTROL;
    }
    if flags.contains(NSEventModifierFlags::Shift) {
        mods |= Modifiers::SHIFT;
    }
    if mods.is_empty() {
        return None;
    }
    let code = char_to_code(chars.chars().next()?)?;
    Some(HotKey::new(Some(mods), code))
}

fn char_to_code(c: char) -> Option<Code> {
    use Code::*;
    Some(match c.to_ascii_lowercase() {
        'a' => KeyA, 'b' => KeyB, 'c' => KeyC, 'd' => KeyD, 'e' => KeyE, 'f' => KeyF,
        'g' => KeyG, 'h' => KeyH, 'i' => KeyI, 'j' => KeyJ, 'k' => KeyK, 'l' => KeyL,
        'm' => KeyM, 'n' => KeyN, 'o' => KeyO, 'p' => KeyP, 'q' => KeyQ, 'r' => KeyR,
        's' => KeyS, 't' => KeyT, 'u' => KeyU, 'v' => KeyV, 'w' => KeyW, 'x' => KeyX,
        'y' => KeyY, 'z' => KeyZ,
        '0' => Digit0, '1' => Digit1, '2' => Digit2, '3' => Digit3, '4' => Digit4,
        '5' => Digit5, '6' => Digit6, '7' => Digit7, '8' => Digit8, '9' => Digit9,
        ' ' => Space,
        '-' => Minus, '=' => Equal, '[' => BracketLeft, ']' => BracketRight,
        ';' => Semicolon, '\'' => Quote, ',' => Comma, '.' => Period, '/' => Slash,
        '`' => Backquote, '\\' => Backslash,
        _ => return None,
    })
}

fn wire(control: &NSControl, target: &AnyObject, action: Sel) {
    unsafe {
        control.setTarget(Some(target));
        control.setAction(Some(action));
    }
}

fn open_path(path: &std::path::Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let _ = NSWorkspace::sharedWorkspace().openURL(&url);
}

fn plain_label(
    mtm: MainThreadMarker,
    text: &str,
    size: f64,
    bold: bool,
) -> Retained<NSTextField> {
    let label = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 20.0)),
    );
    label.setStringValue(&NSString::from_str(text));
    label.setBezeled(false);
    label.setBordered(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);
    let font = if bold {
        NSFont::boldSystemFontOfSize(size)
    } else {
        NSFont::systemFontOfSize(size)
    };
    label.setFont(Some(&font));
    label.setTextColor(Some(&NSColor::labelColor()));
    label
}

fn input_field(mtm: MainThreadMarker, placeholder: &str) -> Retained<NSTextField> {
    let field = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 22.0)),
    );
    field.setEditable(true);
    field.setBezeled(true);
    field.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field
}

fn button(
    mtm: MainThreadMarker,
    title: &str,
    target: &SettingsController,
    action: Sel,
) -> Retained<NSButton> {
    let target: &AnyObject = target;
    unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(target), Some(action), mtm)
    }
}

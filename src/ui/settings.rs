//! `SettingsController` — a titled window for managing the folders indexed for
//! file search, plus a Quit button. Changes are persisted and trigger a
//! background re-index.

use std::cell::RefCell;
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSColor, NSControl, NSFont, NSModalResponseOK,
    NSOpenPanel, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString};

use crate::core::config::Config;
use crate::core::file_index::FileIndex;

const WIN_W: f64 = 520.0;
const WIN_H: f64 = 380.0;
const PAD: f64 = 20.0;
const ROW_H: f64 = 32.0;
const BTN_H: f64 = 30.0;

pub struct SettingsControllerIvars {
    window: Retained<NSWindow>,
    list: Retained<NSView>,
    file_index: FileIndex,
    folders: RefCell<Vec<PathBuf>>,
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
            let mtm = self.mtm();
            let panel = NSOpenPanel::openPanel(mtm);
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
                    self.rebuild_list();
                }
            }
        }

        #[unsafe(method(removeFolder:))]
        fn remove_folder(&self, sender: &NSControl) {
            let index = sender.tag() as usize;
            {
                let mut folders = self.ivars().folders.borrow_mut();
                if index < folders.len() {
                    folders.remove(index);
                } else {
                    return;
                }
            }
            self.persist();
            self.rebuild_list();
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl SettingsController {
    pub fn new(mtm: MainThreadMarker, file_index: FileIndex) -> Retained<Self> {
        let folders = Config::load().search_folders;

        let content_rect = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WIN_W, WIN_H),
        );
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

        // Container for the folder rows; rebuilt whenever the list changes.
        let list_rect = NSRect::new(
            NSPoint::new(PAD, PAD + BTN_H + PAD),
            NSSize::new(WIN_W - 2.0 * PAD, WIN_H - (PAD + BTN_H + PAD) - 56.0),
        );
        let list = NSView::initWithFrame(NSView::alloc(mtm), list_rect);

        let this = Self::alloc(mtm).set_ivars(SettingsControllerIvars {
            window: window.clone(),
            list: list.clone(),
            file_index,
            folders: RefCell::new(folders),
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };

        this.build_chrome(mtm);
        this.rebuild_list();
        this
    }

    pub fn show(&self) {
        let mtm = self.mtm();
        NSApplication::sharedApplication(mtm).activate();
        let window = &self.ivars().window;
        window.center();
        window.makeKeyAndOrderFront(None);
    }

    /// Builds the static chrome: heading, the list container, and the bottom
    /// Add / Quit buttons.
    fn build_chrome(&self, mtm: MainThreadMarker) {
        let iv = self.ivars();
        let Some(content) = iv.window.contentView() else {
            return;
        };

        // Heading.
        let heading = NSTextField::initWithFrame(
            NSTextField::alloc(mtm),
            NSRect::new(
                NSPoint::new(PAD, WIN_H - 40.0),
                NSSize::new(WIN_W - 2.0 * PAD, 22.0),
            ),
        );
        heading.setStringValue(&NSString::from_str("Folders to search"));
        heading.setBezeled(false);
        heading.setBordered(false);
        heading.setDrawsBackground(false);
        heading.setEditable(false);
        heading.setSelectable(false);
        heading.setFont(Some(&NSFont::boldSystemFontOfSize(15.0)));
        heading.setTextColor(Some(&NSColor::labelColor()));
        content.addSubview(&heading);

        content.addSubview(&iv.list);

        // Add Folder… (bottom-left).
        let add = button(mtm, "Add Folder…", self, sel!(addFolder:));
        add.setFrame(NSRect::new(
            NSPoint::new(PAD, PAD),
            NSSize::new(140.0, BTN_H),
        ));
        content.addSubview(&add);

        // Quit Glance (bottom-right).
        let quit = button(mtm, "Quit Glance", self, sel!(quit:));
        quit.setFrame(NSRect::new(
            NSPoint::new(WIN_W - PAD - 130.0, PAD),
            NSSize::new(130.0, BTN_H),
        ));
        content.addSubview(&quit);
    }

    /// Rebuilds one row per folder: path label + Remove button (tag = index).
    fn rebuild_list(&self) {
        let mtm = self.mtm();
        let iv = self.ivars();

        for view in iv.list.subviews().to_vec() {
            view.removeFromSuperview();
        }

        let list_height = iv.list.frame().size.height;
        let list_width = iv.list.frame().size.width;
        let folders = iv.folders.borrow();

        for (i, folder) in folders.iter().enumerate() {
            let y = list_height - (i as f64 + 1.0) * ROW_H;

            let label = NSTextField::initWithFrame(
                NSTextField::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, y + 4.0),
                    NSSize::new(list_width - 100.0, 22.0),
                ),
            );
            label.setStringValue(&NSString::from_str(&folder.to_string_lossy()));
            label.setBezeled(false);
            label.setBordered(false);
            label.setDrawsBackground(false);
            label.setEditable(false);
            label.setSelectable(false);
            label.setFont(Some(&NSFont::systemFontOfSize(13.0)));
            label.setTextColor(Some(&NSColor::labelColor()));
            iv.list.addSubview(&label);

            let remove = button(mtm, "Remove", self, sel!(removeFolder:));
            remove.setTag(i as isize);
            remove.setFrame(NSRect::new(
                NSPoint::new(list_width - 90.0, y),
                NSSize::new(90.0, BTN_H),
            ));
            iv.list.addSubview(&remove);
        }
    }

    /// Persists the folder list and kicks off a background re-index.
    fn persist(&self) {
        let folders = self.ivars().folders.borrow().clone();
        let config = Config {
            search_folders: folders.clone(),
        };
        let _ = config.save();
        self.ivars().file_index.reindex(folders);
    }
}

fn button(
    mtm: MainThreadMarker,
    title: &str,
    target: &SettingsController,
    action: objc2::runtime::Sel,
) -> Retained<NSButton> {
    let target: &AnyObject = target;
    unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(target),
            Some(action),
            mtm,
        )
    }
}

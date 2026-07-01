//! `AppDelegate` — owns the panel + global hotkey, polls hotkey events from a
//! repeating timer, and toggles the panel.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::str::FromStr;

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSResponder, NSTextField, NSTextFieldDelegate, NSWorkspace,
};
use objc2_foundation::{NSNotification, NSObject, NSTimer};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};

use crate::app::hotkey::Hotkey;
use crate::app::shared::Shared;
use crate::core::config::Config;
use crate::core::file_index::FileIndex;
use crate::core::search::SearchEngine;
use crate::ui::controller::SearchController;
use crate::ui::panel::{build_panel, GlancePanel};

/// How often we drain the global-hotkey channel from the run loop (seconds).
const HOTKEY_POLL_INTERVAL: f64 = 0.05;

#[derive(Default)]
pub struct AppDelegateIvars {
    panel: OnceCell<Retained<GlancePanel>>,
    field: OnceCell<Retained<NSTextField>>,
    controller: OnceCell<Retained<SearchController>>,
    hotkey: OnceCell<Rc<RefCell<Hotkey>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlanceAppDelegate"]
    #[ivars = AppDelegateIvars]
    pub struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            self.setup();
        }
    }

    impl AppDelegate {
        #[unsafe(method(pollHotkeys:))]
        fn poll_hotkeys(&self, _timer: &NSTimer) {
            let Some(hotkey) = self.ivars().hotkey.get() else {
                return;
            };
            let id = hotkey.borrow().id();
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id == id && event.state == HotKeyState::Pressed {
                    self.toggle();
                }
            }
        }
    }
);

impl AppDelegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppDelegateIvars::default());
        unsafe { msg_send![super(this), init] }
    }

    fn setup(&self) {
        let mtm = self.mtm();

        let config = Config::load();

        // Build the panel + search field + results list.
        let (panel, field, results_view) = build_panel(mtm);

        // Shared, live-mutable runtime state.
        let file_index = FileIndex::new();
        file_index.reindex(config.search_folders.clone());
        let web_links = Rc::new(RefCell::new(config.web_links.clone()));
        let max_results = Rc::new(Cell::new(config.max_results));
        let engine = Rc::new(RefCell::new(SearchEngine::new(
            file_index.clone(),
            web_links.clone(),
        )));

        // Summon shortcut from config (⌘+Space default). Carbon RegisterEventHotKey
        // under the hood — no Accessibility permission needed.
        let initial = HotKey::from_str(&config.hotkey)
            .unwrap_or_else(|_| HotKey::new(Some(Modifiers::SUPER), Code::Space));
        let hotkey = Rc::new(RefCell::new(
            Hotkey::new(initial).expect("failed to create hotkey manager"),
        ));

        let shared = Shared {
            engine,
            file_index,
            hotkey: hotkey.clone(),
            max_results,
            web_links,
        };

        // Wire the search controller as the field's delegate (live search + nav).
        let controller =
            SearchController::new(mtm, panel.clone(), field.clone(), results_view, shared);
        let delegate = ProtocolObject::<dyn NSTextFieldDelegate>::from_ref(&*controller);
        unsafe { field.setDelegate(Some(delegate)) };

        let _ = self.ivars().panel.set(panel);
        let _ = self.ivars().field.set(field);
        let _ = self.ivars().controller.set(controller);
        let _ = self.ivars().hotkey.set(hotkey);

        // Drain the hotkey channel from the AppKit run loop.
        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                HOTKEY_POLL_INTERVAL,
                self,
                sel!(pollHotkeys:),
                None,
                true,
            );
        }
    }

    fn toggle(&self) {
        let Some(panel) = self.ivars().panel.get() else {
            return;
        };

        if panel.isVisible() {
            panel.orderOut(None);
            panel.restore_previous_app();
        } else {
            // Remember who was frontmost (unless it's us) so we can hand focus
            // back on dismiss.
            let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication();
            let current_pid = std::process::id() as i32;
            let previous = frontmost.filter(|app| app.processIdentifier() != current_pid);
            panel.set_previous_app(previous);

            let app = NSApplication::sharedApplication(self.mtm());
            app.activate();
            panel.makeKeyAndOrderFront(None);
            if let Some(field) = self.ivars().field.get() {
                let responder: &NSResponder = field;
                panel.makeFirstResponder(Some(responder));
            }
            // Start each summon clean: empty field, no results, base size.
            if let Some(controller) = self.ivars().controller.get() {
                controller.reset();
            }
        }
    }
}

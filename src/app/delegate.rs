//! `AppDelegate` — owns the panel + global hotkey, polls hotkey events from a
//! repeating timer, and toggles the panel.

use std::cell::{Cell, OnceCell};

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSResponder, NSTextField, NSTextFieldDelegate,
};
use objc2_foundation::{NSNotification, NSObject, NSTimer};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

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
    hotkey_manager: OnceCell<GlobalHotKeyManager>,
    hotkey_id: Cell<u32>,
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
            let id = self.ivars().hotkey_id.get();
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

        // Build the panel + search field + results list, and wire up the
        // search controller as the field's delegate (live search + nav).
        let (panel, field, results_view) = build_panel(mtm);
        let controller = SearchController::new(
            mtm,
            panel.clone(),
            field.clone(),
            results_view,
            SearchEngine::new(),
        );
        let delegate = ProtocolObject::<dyn NSTextFieldDelegate>::from_ref(&*controller);
        unsafe { field.setDelegate(Some(delegate)) };

        let _ = self.ivars().panel.set(panel);
        let _ = self.ivars().field.set(field);
        let _ = self.ivars().controller.set(controller);

        // Register ⌘+Space. Uses Carbon RegisterEventHotKey under the hood, so
        // no Accessibility permission is required. Note: ⌘+Space is macOS's
        // default Spotlight shortcut — disable it in System Settings if Glance
        // doesn't receive the key.
        let manager = GlobalHotKeyManager::new().expect("failed to create hotkey manager");
        let hotkey = HotKey::new(Some(Modifiers::SUPER), Code::Space);
        if let Err(err) = manager.register(hotkey) {
            eprintln!(
                "glance: failed to register ⌘+Space ({err}). \
                 Disable Spotlight's ⌘+Space in System Settings → Keyboard → \
                 Keyboard Shortcuts → Spotlight, then relaunch."
            );
        }
        self.ivars().hotkey_id.set(hotkey.id());
        let _ = self.ivars().hotkey_manager.set(manager);

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
        } else {
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

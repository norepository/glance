//! Glance — a Spotlight-style macOS app launcher.
//!
//! Milestone 1: a borderless `NSPanel` that toggles visibility on a global
//! hotkey (⌥+Space), centered on screen, containing a basic search field.

mod app;
mod core;
mod plugins;
mod ui;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate};

use crate::app::AppDelegate;

fn main() {
    // AppKit must be driven from the main thread.
    let mtm = MainThreadMarker::new().expect("must run on the main thread");

    let app = NSApplication::sharedApplication(mtm);
    // Accessory: no Dock icon, no app menu. Runtime equivalent of LSUIElement
    // (a real Info.plist comes with bundling in milestone 4).
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // The delegate owns the panel + hotkey for the lifetime of the program.
    let delegate: Retained<AppDelegate> = AppDelegate::new(mtm);
    let object = ProtocolObject::<dyn NSApplicationDelegate>::from_ref(&*delegate);
    app.setDelegate(Some(object));

    app.run();
}

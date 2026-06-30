//! The borderless floating `NSPanel` and its search field.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSPanel, NSTextField, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

const PANEL_WIDTH: f64 = 640.0;
const PANEL_HEIGHT: f64 = 60.0;
const FONT_SIZE: f64 = 28.0;
/// Above ordinary windows (NSStatusWindowLevel). Keeps the launcher on top.
const PANEL_LEVEL: isize = 25;

define_class!(
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlancePanel"]
    pub struct GlancePanel;

    impl GlancePanel {
        // A borderless NSWindow refuses key status; override so the search
        // field can accept keyboard input.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }

        // Esc in the search field travels up the responder chain as
        // `cancelOperation:` — hide the panel.
        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            self.orderOut(None);
        }

        // Click-away: when the panel loses key status (user clicks another
        // window/app), dismiss it.
        #[unsafe(method(resignKeyWindow))]
        fn resign_key_window(&self) {
            unsafe { msg_send![super(self), resignKeyWindow] }
            self.orderOut(None);
        }
    }
);

/// Builds the floating panel and its search field. The field is returned so the
/// delegate can make it first responder when showing the panel.
pub fn build_panel(mtm: MainThreadMarker) -> (Retained<GlancePanel>, Retained<NSTextField>) {
    let content_rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(PANEL_WIDTH, PANEL_HEIGHT),
    );

    let panel: Retained<GlancePanel> = unsafe {
        msg_send![
            GlancePanel::alloc(mtm),
            initWithContentRect: content_rect,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };

    unsafe {
        panel.setFloatingPanel(true);
        panel.setLevel(PANEL_LEVEL);
        panel.setHidesOnDeactivate(false);
        panel.setReleasedWhenClosed(false);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
    }

    // Search field fills the content view.
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), content_rect);
    field.setBezeled(false);
    field.setBordered(false);
    field.setDrawsBackground(false);
    field.setEditable(true);
    field.setSelectable(true);
    field.setFont(Some(&NSFont::systemFontOfSize(FONT_SIZE)));
    field.setTextColor(Some(&NSColor::labelColor()));
    field.setPlaceholderString(Some(&NSString::from_str("Search…")));

    if let Some(content_view) = panel.contentView() {
        content_view.addSubview(&field);
    }

    panel.center();

    (panel, field)
}

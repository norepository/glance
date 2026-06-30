//! The borderless floating `NSPanel` and its search field.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSPanel, NSTextField, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

pub const PANEL_WIDTH: f64 = 640.0;
/// Height of the search field — the panel's height with no results showing.
pub const BASE_HEIGHT: f64 = 60.0;
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

/// Builds the floating panel, its search field, and the (initially empty)
/// results container below the field. All three are returned so the controller
/// can drive live search and resize the panel.
pub fn build_panel(
    mtm: MainThreadMarker,
) -> (Retained<GlancePanel>, Retained<NSTextField>, Retained<NSView>) {
    let content_rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(PANEL_WIDTH, BASE_HEIGHT),
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

    // Search field sits at the top of the content view (full content for now,
    // re-framed to the top strip once results appear).
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), content_rect);
    field.setBezeled(false);
    field.setBordered(false);
    field.setDrawsBackground(false);
    field.setEditable(true);
    field.setSelectable(true);
    field.setFont(Some(&NSFont::systemFontOfSize(FONT_SIZE)));
    field.setTextColor(Some(&NSColor::labelColor()));
    field.setPlaceholderString(Some(&NSString::from_str("Search…")));

    // Results container, below the field. Starts zero-height (no results).
    let results_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, 0.0));
    let results_view = NSView::initWithFrame(NSView::alloc(mtm), results_rect);

    if let Some(content_view) = panel.contentView() {
        content_view.addSubview(&field);
        content_view.addSubview(&results_view);
    }

    panel.center();

    (panel, field, results_view)
}

/// Resizes the panel to fit `results_height` of results below the search field,
/// keeping the panel's top edge fixed (its origin is bottom-left), and re-frames
/// the field (top strip) and results container (below) to match.
pub fn resize(
    panel: &GlancePanel,
    field: &NSTextField,
    results_view: &NSView,
    results_height: f64,
) {
    let total = BASE_HEIGHT + results_height;

    let mut frame = panel.frame();
    let top = frame.origin.y + frame.size.height;
    frame.size.height = total;
    frame.origin.y = top - total;
    panel.setFrame_display(frame, true);

    let width = frame.size.width;
    field.setFrame(NSRect::new(
        NSPoint::new(0.0, results_height),
        NSSize::new(width, BASE_HEIGHT),
    ));
    results_view.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(width, results_height),
    ));
}

//! The borderless floating `NSPanel` and its search field.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBackingStoreType, NSColor, NSFocusRingType, NSFont, NSPanel,
    NSRunningApplication, NSTextField, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

pub const PANEL_WIDTH: f64 = 640.0;
/// Height of the search field — the panel's height with no results showing.
pub const BASE_HEIGHT: f64 = 60.0;
const FONT_SIZE: f64 = 28.0;
/// Above ordinary windows (NSStatusWindowLevel). Keeps the launcher on top.
const PANEL_LEVEL: isize = 25;
/// Corner radius for the rounded panel.
const CORNER_RADIUS: f64 = 14.0;
/// Insets for the search field so its text clears the rounded corners.
const FIELD_TOP_PAD: f64 = 12.0;
const FIELD_X: f64 = 16.0;

/// Remembers the app that was frontmost when Glance opened, so focus can be
/// handed back on dismiss.
#[derive(Default)]
pub struct GlancePanelIvars {
    previous_app: RefCell<Option<Retained<NSRunningApplication>>>,
}

define_class!(
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlancePanel"]
    #[ivars = GlancePanelIvars]
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
        // `cancelOperation:` — hide the panel and return focus.
        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            self.orderOut(None);
            self.restore_previous_app();
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

impl GlancePanel {
    /// Records the app that was frontmost before Glance opened.
    pub fn set_previous_app(&self, app: Option<Retained<NSRunningApplication>>) {
        *self.ivars().previous_app.borrow_mut() = app;
    }

    /// Reactivates the previously-frontmost app, restoring its window's focus
    /// (e.g. the text field the user was typing in). Consumes the stored app.
    pub fn restore_previous_app(&self) {
        if let Some(app) = self.ivars().previous_app.borrow_mut().take() {
            app.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }
}

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

    let this = GlancePanel::alloc(mtm).set_ivars(GlancePanelIvars::default());
    let panel: Retained<GlancePanel> = unsafe {
        msg_send![
            super(this),
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
        // Transparent window + drop shadow so the rounded blur stands out.
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(true);
    }

    // Blurred, rounded background gives the bar contrast against the desktop.
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_rect);
    effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setWantsLayer(true);
    if let Some(layer) = effect.layer() {
        layer.setCornerRadius(CORNER_RADIUS);
        layer.setMasksToBounds(true);
        layer.setBorderWidth(1.0);
        layer.setBorderColor(Some(&NSColor::separatorColor().CGColor()));
    }
    let effect_view: &NSView = &effect;
    panel.setContentView(Some(effect_view));

    // Search field sits at the top of the content view (full content for now,
    // re-framed to the top strip once results appear).
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), content_rect);
    field.setBezeled(false);
    field.setBordered(false);
    field.setDrawsBackground(false);
    field.setEditable(true);
    field.setSelectable(true);
    // No blue focus ring around the search field.
    field.setFocusRingType(NSFocusRingType::None);
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
    // Lower the field a touch (text is top-aligned) and inset it horizontally so
    // it doesn't sit under the rounded corners.
    field.setFrame(NSRect::new(
        NSPoint::new(FIELD_X, results_height),
        NSSize::new(width - 2.0 * FIELD_X, BASE_HEIGHT - FIELD_TOP_PAD),
    ));
    results_view.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(width, results_height),
    ));
}

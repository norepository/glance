//! A single result row: app icon + name, with a selection highlight.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSColor, NSFont, NSImageScaling, NSImageView, NSRectFill, NSTextField, NSView, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::core::app_index::AppEntry;

pub const ROW_HEIGHT: f64 = 48.0;
const ICON_SIZE: f64 = 32.0;
const ICON_X: f64 = 14.0;
const LABEL_X: f64 = 58.0;
const LABEL_HEIGHT: f64 = 24.0;
const LABEL_FONT_SIZE: f64 = 16.0;

define_class!(
    // A view that paints a selection background behind its contents.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlanceRowView"]
    #[ivars = Cell<bool>]
    struct RowView;

    impl RowView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if self.ivars().get() {
                NSColor::selectedContentBackgroundColor().setFill();
                NSRectFill(self.bounds());
            }
        }
    }
);

/// Builds a result row sized `width` × [`ROW_HEIGHT`]. The caller positions it.
pub fn build_row(
    mtm: MainThreadMarker,
    entry: &AppEntry,
    width: f64,
    selected: bool,
) -> Retained<NSView> {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, ROW_HEIGHT));
    let row: Retained<RowView> = {
        let this = RowView::alloc(mtm).set_ivars(Cell::new(selected));
        unsafe { msg_send![super(this), initWithFrame: frame] }
    };

    // Icon.
    let icon_frame = NSRect::new(
        NSPoint::new(ICON_X, (ROW_HEIGHT - ICON_SIZE) / 2.0),
        NSSize::new(ICON_SIZE, ICON_SIZE),
    );
    let image_view = NSImageView::initWithFrame(NSImageView::alloc(mtm), icon_frame);
    let path = entry.path.to_string_lossy();
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path));
    image_view.setImage(Some(&icon));
    image_view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    row.addSubview(&image_view);

    // Name label.
    let label_frame = NSRect::new(
        NSPoint::new(LABEL_X, (ROW_HEIGHT - LABEL_HEIGHT) / 2.0),
        NSSize::new(width - LABEL_X - ICON_X, LABEL_HEIGHT),
    );
    let label = NSTextField::initWithFrame(NSTextField::alloc(mtm), label_frame);
    label.setStringValue(&NSString::from_str(&entry.name));
    label.setBezeled(false);
    label.setBordered(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);
    label.setFont(Some(&NSFont::systemFontOfSize(LABEL_FONT_SIZE)));
    let text_color = if selected {
        NSColor::whiteColor()
    } else {
        NSColor::labelColor()
    };
    label.setTextColor(Some(&text_color));
    row.addSubview(&label);

    Retained::into_super(row)
}

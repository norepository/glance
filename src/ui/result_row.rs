//! A single result row: icon + title (+ optional subtitle), with a selection
//! highlight.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSColor, NSFont, NSImageScaling, NSImageView, NSRectFill, NSTextField, NSView, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::core::item::SearchItem;

pub const ROW_HEIGHT: f64 = 48.0;
const ICON_SIZE: f64 = 32.0;
const ICON_X: f64 = 14.0;
const LABEL_X: f64 = 58.0;
const TITLE_FONT_SIZE: f64 = 16.0;
const SUBTITLE_FONT_SIZE: f64 = 11.0;

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
    item: &SearchItem,
    width: f64,
    selected: bool,
) -> Retained<NSView> {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, ROW_HEIGHT));
    let row: Retained<RowView> = {
        let this = RowView::alloc(mtm).set_ivars(Cell::new(selected));
        unsafe { msg_send![super(this), initWithFrame: frame] }
    };

    // Icon (apps + files; commands may have none).
    if let Some(icon_path) = &item.icon_path {
        let icon_frame = NSRect::new(
            NSPoint::new(ICON_X, (ROW_HEIGHT - ICON_SIZE) / 2.0),
            NSSize::new(ICON_SIZE, ICON_SIZE),
        );
        let image_view = NSImageView::initWithFrame(NSImageView::alloc(mtm), icon_frame);
        let path = icon_path.to_string_lossy();
        let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path));
        image_view.setImage(Some(&icon));
        image_view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        row.addSubview(&image_view);
    }

    let label_width = width - LABEL_X - ICON_X;
    let title_color = if selected {
        NSColor::whiteColor()
    } else {
        NSColor::labelColor()
    };

    if let Some(subtitle) = &item.subtitle {
        // Two lines: title above, subtitle below.
        let title = make_label(mtm, &item.title, TITLE_FONT_SIZE, &title_color, label_width);
        title.setFrame(NSRect::new(
            NSPoint::new(LABEL_X, 22.0),
            NSSize::new(label_width, 20.0),
        ));
        row.addSubview(&title);

        let subtitle_color = if selected {
            NSColor::whiteColor()
        } else {
            NSColor::secondaryLabelColor()
        };
        let sub = make_label(mtm, subtitle, SUBTITLE_FONT_SIZE, &subtitle_color, label_width);
        sub.setFrame(NSRect::new(
            NSPoint::new(LABEL_X, 7.0),
            NSSize::new(label_width, 14.0),
        ));
        row.addSubview(&sub);
    } else {
        // Single centered title.
        let title = make_label(mtm, &item.title, TITLE_FONT_SIZE, &title_color, label_width);
        title.setFrame(NSRect::new(
            NSPoint::new(LABEL_X, (ROW_HEIGHT - 22.0) / 2.0),
            NSSize::new(label_width, 22.0),
        ));
        row.addSubview(&title);
    }

    Retained::into_super(row)
}

fn make_label(
    mtm: MainThreadMarker,
    text: &str,
    font_size: f64,
    color: &NSColor,
    width: f64,
) -> Retained<NSTextField> {
    let label = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, 20.0)),
    );
    label.setStringValue(&NSString::from_str(text));
    label.setBezeled(false);
    label.setBordered(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);
    label.setFont(Some(&NSFont::systemFontOfSize(font_size)));
    label.setTextColor(Some(color));
    label
}

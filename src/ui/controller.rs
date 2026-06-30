//! `SearchController` — the search field's delegate. Runs live fuzzy search on
//! every keystroke, drives keyboard navigation of the result list, and launches
//! the selected app.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSControl, NSControlTextEditingDelegate, NSTextField, NSTextFieldDelegate, NSTextView,
    NSView, NSWorkspace,
};
use objc2_foundation::{NSNotification, NSObject, NSPoint, NSRect, NSSize, NSString, NSURL};

use crate::core::app_index::AppEntry;
use crate::core::search::SearchEngine;
use crate::ui::panel::{self, GlancePanel, PANEL_WIDTH};
use crate::ui::result_row::{build_row, ROW_HEIGHT};

const MAX_RESULTS: usize = 8;

pub struct SearchControllerIvars {
    panel: Retained<GlancePanel>,
    field: Retained<NSTextField>,
    results_view: Retained<NSView>,
    engine: RefCell<SearchEngine>,
    results: RefCell<Vec<AppEntry>>,
    selected: Cell<usize>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlanceSearchController"]
    #[ivars = SearchControllerIvars]
    pub struct SearchController;

    unsafe impl NSObjectProtocol for SearchController {}
    unsafe impl NSControlTextEditingDelegate for SearchController {}
    unsafe impl NSTextFieldDelegate for SearchController {}

    impl SearchController {
        // Live search on every keystroke.
        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, _notification: &NSNotification) {
            let query = self.ivars().field.stringValue().to_string();
            let results = self.ivars().engine.borrow_mut().search(&query, MAX_RESULTS);
            *self.ivars().results.borrow_mut() = results;
            self.ivars().selected.set(0);
            self.render();
        }

        // Intercept navigation/commit keys while the field keeps focus.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(
            &self,
            _control: &NSControl,
            _text_view: &NSTextView,
            selector: Sel,
        ) -> bool {
            if selector == sel!(moveDown:) {
                self.move_selection(1);
                true
            } else if selector == sel!(moveUp:) {
                self.move_selection(-1);
                true
            } else if selector == sel!(insertNewline:) {
                self.launch_selected();
                true
            } else {
                false
            }
        }
    }
);

impl SearchController {
    pub fn new(
        mtm: MainThreadMarker,
        panel: Retained<GlancePanel>,
        field: Retained<NSTextField>,
        results_view: Retained<NSView>,
        engine: SearchEngine,
    ) -> Retained<Self> {
        let ivars = SearchControllerIvars {
            panel,
            field,
            results_view,
            engine: RefCell::new(engine),
            results: RefCell::new(Vec::new()),
            selected: Cell::new(0),
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Rebuilds the result rows, resizes the panel to fit, and highlights the
    /// current selection.
    fn render(&self) {
        let mtm = self.mtm();
        let iv = self.ivars();

        for view in iv.results_view.subviews().to_vec() {
            view.removeFromSuperview();
        }

        let results = iv.results.borrow();
        let selected = iv.selected.get();
        let results_height = results.len() as f64 * ROW_HEIGHT;

        panel::resize(&iv.panel, &iv.field, &iv.results_view, results_height);

        for (i, entry) in results.iter().enumerate() {
            let row = build_row(mtm, entry, PANEL_WIDTH, i == selected);
            // Non-flipped coords: row 0 sits at the top.
            let y = results_height - (i as f64 + 1.0) * ROW_HEIGHT;
            row.setFrame(NSRect::new(
                NSPoint::new(0.0, y),
                NSSize::new(PANEL_WIDTH, ROW_HEIGHT),
            ));
            iv.results_view.addSubview(&row);
        }
    }

    fn move_selection(&self, delta: isize) {
        let iv = self.ivars();
        let count = iv.results.borrow().len();
        if count == 0 {
            return;
        }
        let current = iv.selected.get() as isize;
        let next = (current + delta).clamp(0, count as isize - 1);
        iv.selected.set(next as usize);
        self.render();
    }

    fn launch_selected(&self) {
        let iv = self.ivars();
        let entry = {
            let results = iv.results.borrow();
            results.get(iv.selected.get()).cloned()
        };
        let Some(entry) = entry else {
            return;
        };

        let path = entry.path.to_string_lossy();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
        let _ = NSWorkspace::sharedWorkspace().openURL(&url);

        self.reset();
        iv.panel.orderOut(None);
    }

    /// Clears the field and results, shrinking the panel back to its base size.
    /// Called each time the panel is summoned so it always opens clean.
    pub fn reset(&self) {
        let iv = self.ivars();
        iv.field.setStringValue(&NSString::from_str(""));
        iv.results.borrow_mut().clear();
        iv.selected.set(0);
        self.render();
    }
}

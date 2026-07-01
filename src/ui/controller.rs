//! `SearchController` — the search field's delegate. Runs live fuzzy search on
//! every keystroke, drives keyboard navigation, activates results, and owns the
//! active timers (list, drill-down management, and firing).

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSApplication, NSControl, NSControlTextEditingDelegate, NSPasteboard,
    NSPasteboardTypeString, NSSound, NSTextField, NSTextFieldDelegate, NSTextView, NSView,
    NSWorkspace,
};
use objc2_foundation::{
    NSNotification, NSNumber, NSObject, NSPoint, NSRect, NSSize, NSString, NSTimer, NSURL,
};

use crate::app::shared::Shared;
use crate::core::item::{Action, SearchItem};
use crate::plugins::timer::{format_clock, format_human};
use crate::ui::panel::{self, GlancePanel, PANEL_WIDTH};
use crate::ui::result_row::{build_row, ROW_HEIGHT};
use crate::ui::settings::SettingsController;

/// How often the timer list/detail refreshes its countdown while visible.
const TICK_INTERVAL: f64 = 1.0;

enum TimerState {
    Running { deadline: Instant },
    Paused { remaining: Duration },
}

struct ActiveTimer {
    id: u64,
    label: Option<String>,
    total: Duration,
    state: TimerState,
    nstimer: Option<Retained<NSTimer>>,
}

impl ActiveTimer {
    fn remaining(&self) -> Duration {
        match self.state {
            TimerState::Running { deadline } => deadline.saturating_duration_since(Instant::now()),
            TimerState::Paused { remaining } => remaining,
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.state, TimerState::Running { .. })
    }
}

pub struct SearchControllerIvars {
    panel: Retained<GlancePanel>,
    field: Retained<NSTextField>,
    results_view: Retained<NSView>,
    shared: Shared,
    results: RefCell<Vec<SearchItem>>,
    selected: Cell<usize>,
    settings: RefCell<Option<Retained<SettingsController>>>,
    timers: RefCell<Vec<ActiveTimer>>,
    next_id: Cell<u64>,
    /// `Some(id)` while showing one timer's management actions.
    detail: Cell<Option<u64>>,
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
            self.ivars().detail.set(None);
            self.ivars().selected.set(0);
            self.compute_results();
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
                self.activate_selected();
                true
            } else {
                false
            }
        }

        // Repeating 1s refresh so timer countdowns stay live while visible.
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            if !self.ivars().panel.isVisible() {
                return;
            }
            let query = self.ivars().field.stringValue().to_string();
            let q = query.trim();
            if self.ivars().detail.get().is_some() || q.is_empty() || q == "t" {
                self.compute_results();
            }
        }

        // A timer reached zero.
        #[unsafe(method(timerFired:))]
        fn timer_fired(&self, timer: &NSTimer) {
            let id = timer.userInfo()
                .and_then(|obj| obj.downcast::<NSNumber>().ok())
                .map(|num| num.unsignedLongLongValue());
            if let Some(id) = id {
                self.fire_timer(id);
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
        shared: Shared,
    ) -> Retained<Self> {
        let ivars = SearchControllerIvars {
            panel,
            field,
            results_view,
            shared,
            results: RefCell::new(Vec::new()),
            selected: Cell::new(0),
            settings: RefCell::new(None),
            timers: RefCell::new(Vec::new()),
            next_id: Cell::new(1),
            detail: Cell::new(None),
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };

        // Drive the live countdown.
        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                TICK_INTERVAL,
                &this,
                sel!(tick:),
                None,
                true,
            );
        }
        this
    }

    // --- result computation ----------------------------------------------

    /// Picks the row set from current state (timer detail, timer list, or a
    /// plugin search) and renders it, keeping the selection in range.
    fn compute_results(&self) {
        let iv = self.ivars();
        let query = iv.field.stringValue().to_string();
        let q = query.trim();

        let items = if let Some(id) = iv.detail.get() {
            if self.timer_exists(id) {
                self.detail_rows(id)
            } else {
                iv.detail.set(None);
                self.list_rows()
            }
        } else if q.is_empty() || q == "t" {
            self.list_rows()
        } else {
            let limit = iv.shared.max_results.get();
            iv.shared.engine.borrow_mut().search(&query, limit)
        };

        let sel = iv.selected.get();
        if items.is_empty() {
            iv.selected.set(0);
        } else if sel >= items.len() {
            iv.selected.set(items.len() - 1);
        }
        *iv.results.borrow_mut() = items;
        self.render();
    }

    fn list_rows(&self) -> Vec<SearchItem> {
        let timers = self.ivars().timers.borrow();
        let mut entries: Vec<&ActiveTimer> = timers.iter().collect();
        entries.sort_by_key(|t| t.remaining());
        entries
            .iter()
            .map(|t| {
                let state = if t.is_running() { "running" } else { "paused" };
                SearchItem {
                    title: t.label.clone().unwrap_or_else(|| "Timer".to_string()),
                    subtitle: Some(format!("{} · {state}", format_clock(t.remaining().as_secs()))),
                    icon_path: None,
                    action: Action::ShowTimer(t.id),
                }
            })
            .collect()
    }

    fn detail_rows(&self, id: u64) -> Vec<SearchItem> {
        let timers = self.ivars().timers.borrow();
        let Some(t) = timers.iter().find(|t| t.id == id) else {
            return Vec::new();
        };
        let name = t.label.clone().unwrap_or_else(|| "Timer".to_string());
        let clock = format_clock(t.remaining().as_secs());
        let toggle = if t.is_running() { "Pause" } else { "Resume" };
        vec![
            SearchItem {
                title: toggle.to_string(),
                subtitle: Some(format!("{name} · {clock}")),
                icon_path: None,
                action: Action::ToggleTimer(id),
            },
            SearchItem {
                title: "Restart".to_string(),
                subtitle: Some(format!("back to {}", format_human(t.total.as_secs()))),
                icon_path: None,
                action: Action::RestartTimer(id),
            },
            SearchItem {
                title: "Cancel".to_string(),
                subtitle: Some(format!("remove “{name}”")),
                icon_path: None,
                action: Action::CancelTimer(id),
            },
            SearchItem {
                title: "← Back".to_string(),
                subtitle: None,
                icon_path: None,
                action: Action::ShowTimerList,
            },
        ]
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

        for (i, item) in results.iter().enumerate() {
            let row = build_row(mtm, item, PANEL_WIDTH, i == selected);
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

    // --- activation -------------------------------------------------------

    fn activate_selected(&self) {
        let action = {
            let results = self.ivars().results.borrow();
            results.get(self.ivars().selected.get()).map(|i| i.action.clone())
        };
        let Some(action) = action else {
            return;
        };

        match action {
            Action::Open(path) => {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                let _ = NSWorkspace::sharedWorkspace().openURL(&url);
                self.hide(false);
            }
            Action::OpenUrl(url) => {
                if let Some(url) = NSURL::URLWithString(&NSString::from_str(&url)) {
                    let _ = NSWorkspace::sharedWorkspace().openURL(&url);
                }
                self.hide(false);
            }
            Action::Copy(text) => {
                let pasteboard = NSPasteboard::generalPasteboard();
                pasteboard.clearContents();
                unsafe {
                    pasteboard
                        .setString_forType(&NSString::from_str(&text), NSPasteboardTypeString);
                }
                self.hide(true);
            }
            Action::Run { program, args } => {
                let _ = std::process::Command::new(program).args(args).spawn();
                self.hide(false);
            }
            Action::OpenSettings => {
                self.hide(false);
                self.open_settings();
            }
            Action::ReloadPlugins => {
                self.ivars().shared.engine.borrow_mut().reload_plugins();
                self.hide(true);
            }
            Action::StartTimer { seconds, label } => {
                self.start_timer(seconds, label);
                self.hide(true);
            }
            Action::ShowTimer(id) => {
                self.ivars().detail.set(Some(id));
                self.ivars().selected.set(0);
                self.compute_results();
            }
            Action::ToggleTimer(id) => {
                self.toggle_timer(id);
                self.compute_results();
            }
            Action::RestartTimer(id) => {
                self.restart_timer(id);
                self.compute_results();
            }
            Action::CancelTimer(id) => {
                self.cancel_timer(id);
                self.ivars().detail.set(None);
                self.ivars().selected.set(0);
                self.compute_results();
            }
            Action::ShowTimerList => {
                self.ivars().detail.set(None);
                self.ivars().selected.set(0);
                self.compute_results();
            }
        }
    }

    // --- timer management -------------------------------------------------

    fn timer_exists(&self, id: u64) -> bool {
        self.ivars().timers.borrow().iter().any(|t| t.id == id)
    }

    /// Schedules a one-shot `NSTimer` that fires `timerFired:` with the id.
    fn schedule(&self, id: u64, seconds: f64) -> Retained<NSTimer> {
        let number = NSNumber::numberWithUnsignedLongLong(id);
        let info: &AnyObject = &number;
        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                seconds.max(0.0),
                self,
                sel!(timerFired:),
                Some(info),
                false,
            )
        }
    }

    fn start_timer(&self, seconds: u64, label: Option<String>) {
        let id = self.ivars().next_id.get();
        self.ivars().next_id.set(id + 1);
        let nstimer = self.schedule(id, seconds as f64);
        let total = Duration::from_secs(seconds);
        self.ivars().timers.borrow_mut().push(ActiveTimer {
            id,
            label,
            total,
            state: TimerState::Running {
                deadline: Instant::now() + total,
            },
            nstimer: Some(nstimer),
        });
    }

    fn toggle_timer(&self, id: u64) {
        let mut timers = self.ivars().timers.borrow_mut();
        let Some(t) = timers.iter_mut().find(|t| t.id == id) else {
            return;
        };
        match t.state {
            TimerState::Running { deadline } => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if let Some(nst) = t.nstimer.take() {
                    nst.invalidate();
                }
                t.state = TimerState::Paused { remaining };
            }
            TimerState::Paused { remaining } => {
                let nstimer = self.schedule(id, remaining.as_secs_f64());
                t.state = TimerState::Running {
                    deadline: Instant::now() + remaining,
                };
                t.nstimer = Some(nstimer);
            }
        }
    }

    fn restart_timer(&self, id: u64) {
        let total = self
            .ivars()
            .timers
            .borrow()
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.total);
        let Some(total) = total else {
            return;
        };
        let nstimer = self.schedule(id, total.as_secs_f64());
        let mut timers = self.ivars().timers.borrow_mut();
        if let Some(t) = timers.iter_mut().find(|t| t.id == id) {
            if let Some(old) = t.nstimer.take() {
                old.invalidate();
            }
            t.state = TimerState::Running {
                deadline: Instant::now() + total,
            };
            t.nstimer = Some(nstimer);
        }
    }

    fn cancel_timer(&self, id: u64) {
        let mut timers = self.ivars().timers.borrow_mut();
        if let Some(pos) = timers.iter().position(|t| t.id == id) {
            if let Some(nst) = timers[pos].nstimer.take() {
                nst.invalidate();
            }
            timers.remove(pos);
        }
    }

    fn fire_timer(&self, id: u64) {
        let label = {
            let mut timers = self.ivars().timers.borrow_mut();
            match timers.iter().position(|t| t.id == id) {
                Some(pos) => timers.remove(pos).label,
                None => return,
            }
        };
        self.notify_timer_done(label);
        if self.ivars().panel.isVisible() {
            self.compute_results();
        }
    }

    fn notify_timer_done(&self, label: Option<String>) {
        let mtm = self.mtm();
        if let Some(sound) = NSSound::soundNamed(&NSString::from_str("Glass")) {
            sound.play();
        }
        NSApplication::sharedApplication(mtm).activate();
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str("Timer"));
        alert.setInformativeText(&NSString::from_str(
            &label.unwrap_or_else(|| "Time's up!".to_string()),
        ));
        alert.runModal();
    }

    // --- misc -------------------------------------------------------------

    /// Clears state and hides the panel after an action. When `restore` is set,
    /// focus is handed back to the app that was frontmost before Glance opened.
    fn hide(&self, restore: bool) {
        self.reset();
        self.ivars().panel.orderOut(None);
        if restore {
            self.ivars().panel.restore_previous_app();
        }
    }

    fn open_settings(&self) {
        let mtm = self.mtm();
        let iv = self.ivars();
        let mut slot = iv.settings.borrow_mut();
        let controller =
            slot.get_or_insert_with(|| SettingsController::new(mtm, iv.shared.clone()));
        controller.show();
    }

    /// Resets the field to empty; the empty query shows the active-timer list.
    /// Called each time the panel is summoned so it always opens clean.
    pub fn reset(&self) {
        let iv = self.ivars();
        iv.field.setStringValue(&NSString::from_str(""));
        iv.detail.set(None);
        iv.selected.set(0);
        self.compute_results();
    }
}

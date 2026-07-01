//! Built-in plugin: lists upcoming calendar events via EventKit.
//! Triggered by the `cal` keyword. Requires Calendar permission, which is tied
//! to the signed `.app` identity — test via `scripts/bundle.sh`, not `cargo run`.

use std::cell::Cell;
use std::path::PathBuf;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEvent, EKEventStore};
use objc2_foundation::{NSDate, NSDateFormatter, NSError, NSString};

use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

const KEYWORD: &str = "cal";
const CAL_SCORE: u32 = 7_000;
/// How far ahead to list events.
const WINDOW_SECS: f64 = 7.0 * 24.0 * 60.0 * 60.0;
const MAX_EVENTS: usize = 8;

pub struct Calendar {
    store: Retained<EKEventStore>,
    requested: Cell<bool>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            store: unsafe { EKEventStore::new() },
            requested: Cell::new(false),
        }
    }

    fn info(title: &str) -> Vec<Scored> {
        vec![Scored {
            item: SearchItem {
                title: title.to_string(),
                subtitle: Some("Calendar".to_string()),
                icon_path: Some(PathBuf::from("/System/Applications/Calendar.app")),
                action: Action::Open(PathBuf::from("/System/Applications/Calendar.app")),
            },
            score: CAL_SCORE,
        }]
    }

    fn events(&self) -> Vec<Scored> {
        let now = NSDate::now();
        let end = NSDate::dateWithTimeIntervalSinceNow(WINDOW_SECS);
        let predicate = unsafe {
            self.store
                .predicateForEventsWithStartDate_endDate_calendars(&now, &end, None)
        };
        let events = unsafe { self.store.eventsMatchingPredicate(&predicate) };

        let formatter = NSDateFormatter::new();
        formatter.setDateFormat(Some(&NSString::from_str("EEE HH:mm")));

        let mut out = Vec::new();
        for (i, event) in events.iter().enumerate().take(MAX_EVENTS) {
            let title = unsafe { event.title() }.to_string();
            let title = if title.is_empty() {
                "(untitled)".to_string()
            } else {
                title
            };
            let when = event_time(&event, &formatter);
            out.push(Scored {
                item: SearchItem {
                    title,
                    subtitle: Some(when),
                    icon_path: Some(PathBuf::from("/System/Applications/Calendar.app")),
                    action: Action::Open(PathBuf::from("/System/Applications/Calendar.app")),
                },
                score: CAL_SCORE.saturating_sub(i as u32),
            });
        }
        if out.is_empty() {
            return Self::info("No upcoming events");
        }
        out
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for Calendar {
    fn id(&self) -> &'static str {
        "calendar"
    }

    fn search(&mut self, query: &str, _cx: &mut PluginCx) -> Vec<Scored> {
        // Keyword-gated: "cal" or "cal <anything>".
        if query != KEYWORD && !query.starts_with(&format!("{KEYWORD} ")) {
            return Vec::new();
        }

        let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
        if status == EKAuthorizationStatus::FullAccess {
            self.events()
        } else if status == EKAuthorizationStatus::NotDetermined {
            if !self.requested.get() {
                self.requested.set(true);
                let completion = RcBlock::new(|_granted: Bool, _err: *mut NSError| {});
                unsafe {
                    self.store.requestFullAccessToEventsWithCompletion(
                        &*completion as *const _ as *mut _,
                    );
                }
            }
            Self::info("Requesting Calendar access — try again")
        } else {
            Self::info("Calendar access denied — enable in System Settings")
        }
    }
}

fn event_time(event: &EKEvent, formatter: &NSDateFormatter) -> String {
    let date = unsafe { event.startDate() };
    formatter.stringFromDate(&date).to_string()
}

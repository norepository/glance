//! Built-in plugin: parses `t <duration>` into a "start timer" result. The
//! active-timer list and management (pause/restart/cancel) live in the search
//! controller, since they're stateful and schedule real `NSTimer`s.

use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

const KEYWORD: &str = "t";
const TIMER_SCORE: u32 = 9_500;

pub struct Timer;

impl Timer {
    pub fn new() -> Self {
        Timer
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for Timer {
    fn id(&self) -> &'static str {
        "timer"
    }

    fn search(&mut self, query: &str, _cx: &mut PluginCx) -> Vec<Scored> {
        // Only "t <value>". Bare "t" (the active-timer list) is handled by the
        // controller, since it needs live timer state.
        let Some(rest) = query.strip_prefix(KEYWORD).and_then(|r| r.strip_prefix(' ')) else {
            return Vec::new();
        };
        let Some((seconds, label)) = parse_duration(rest) else {
            return Vec::new();
        };

        let title = match &label {
            Some(l) => format!("Set a {} timer — {l}", format_human(seconds)),
            None => format!("Set a {} timer", format_human(seconds)),
        };
        vec![Scored {
            item: SearchItem {
                title,
                subtitle: Some("Press Return to start".to_string()),
                icon_path: None,
                action: Action::StartTimer { seconds, label },
            },
            score: TIMER_SCORE,
        }]
    }
}

/// Parses a duration + optional trailing label. Returns `None` for empty, zero,
/// or unparseable input.
pub fn parse_duration(input: &str) -> Option<(u64, Option<String>)> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // Colon format (M:S or H:M:S) on the first whitespace-delimited token.
    let mut parts = input.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap();
    if first.contains(':') {
        let seconds = parse_colon(first)?;
        let label = parts.next().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
        return finish(seconds, label);
    }

    // Otherwise consume `number [unit]` groups from the start; the first token
    // that isn't part of a duration begins the label.
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut total: u64 = 0;
    let mut groups = 0u32;
    let label: String;
    loop {
        while i < n && chars[i].is_whitespace() {
            i += 1;
        }
        let num_start = i;
        while i < n && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            // No number here — the remainder is the label.
            label = chars[num_start..].iter().collect::<String>().trim().to_string();
            break;
        }
        let num: u64 = chars[num_start..i].iter().collect::<String>().parse().ok()?;

        while i < n && chars[i] == ' ' {
            i += 1;
        }
        let unit_start = i;
        while i < n && chars[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit: String = chars[unit_start..i].iter().collect::<String>().to_ascii_lowercase();

        match unit_mult(&unit) {
            Some(mult) => {
                total = total.saturating_add(num.saturating_mul(mult));
                groups += 1;
            }
            None => {
                // Bare number (no valid unit): default to minutes if it's the
                // first group, else seconds. The non-unit text starts the label.
                let mult = if groups == 0 { 60 } else { 1 };
                total = total.saturating_add(num.saturating_mul(mult));
                groups += 1;
                label = chars[unit_start..].iter().collect::<String>().trim().to_string();
                break;
            }
        }
    }

    if groups == 0 {
        return None;
    }
    finish(total, non_empty(label))
}

/// `M:SS` under an hour, else `H:MM:SS`.
pub fn format_clock(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Compact human form: `5m`, `1h 30m`, `1m 30s`.
pub fn format_human(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{h}h"));
    }
    if m > 0 {
        parts.push(format!("{m}m"));
    }
    if s > 0 {
        parts.push(format!("{s}s"));
    }
    if parts.is_empty() {
        "0s".to_string()
    } else {
        parts.join(" ")
    }
}

fn parse_colon(s: &str) -> Option<u64> {
    let nums: Option<Vec<u64>> = s.split(':').map(|p| p.parse::<u64>().ok()).collect();
    match nums?.as_slice() {
        [m, s] => Some(m * 60 + s),
        [h, m, s] => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

fn unit_mult(unit: &str) -> Option<u64> {
    match unit {
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(3600),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(60),
        "s" | "sec" | "secs" | "second" | "seconds" => Some(1),
        _ => None,
    }
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn finish(seconds: u64, label: Option<String>) -> Option<(u64, Option<String>)> {
    (seconds > 0).then_some((seconds, label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("5"), Some((300, None)));
        assert_eq!(parse_duration("30s"), Some((30, None)));
        assert_eq!(parse_duration("2h"), Some((7200, None)));
        assert_eq!(parse_duration("1h30m"), Some((5400, None)));
        assert_eq!(parse_duration("1h 30m 10s"), Some((5410, None)));
        assert_eq!(parse_duration("90"), Some((5400, None)));
        assert_eq!(parse_duration("5:30"), Some((330, None)));
        assert_eq!(parse_duration("1:30:00"), Some((5400, None)));
    }

    #[test]
    fn parses_labels() {
        assert_eq!(parse_duration("25m pomodoro"), Some((1500, Some("pomodoro".into()))));
        assert_eq!(parse_duration("5:00 tea"), Some((300, Some("tea".into()))));
        assert_eq!(parse_duration("90 nap"), Some((5400, Some("nap".into()))));
    }

    #[test]
    fn rejects_invalid() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("0"), None);
        assert_eq!(parse_duration("abc"), None);
    }

    #[test]
    fn formats_clock() {
        assert_eq!(format_clock(90), "1:30");
        assert_eq!(format_clock(5400), "1:30:00");
        assert_eq!(format_clock(5), "0:05");
    }
}

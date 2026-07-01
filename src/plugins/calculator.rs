//! Built-in plugin: inline calculator + unit conversion via fend-core.
//! `2+2`, `sqrt(2)`, `5 km in miles`, `1gb to mb`. Return copies the result.

use fend_core::{evaluate, Context};

use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

/// High so a clear calculation tops the list.
const CALC_SCORE: u32 = 10_000;

pub struct Calculator {
    context: Context,
}

impl Calculator {
    pub fn new() -> Self {
        Self {
            context: Context::new(),
        }
    }
}

impl Default for Calculator {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for Calculator {
    fn id(&self) -> &'static str {
        "calculator"
    }

    fn search(&mut self, query: &str, _cx: &mut PluginCx) -> Vec<Scored> {
        // Only attempt math-looking queries so plain words don't trigger it.
        let looks_mathy = query.chars().any(|c| c.is_ascii_digit())
            || query.contains(|c: char| "+-*/^%".contains(c));
        if !looks_mathy {
            return Vec::new();
        }

        let Ok(result) = evaluate(query, &mut self.context) else {
            return Vec::new();
        };
        let value = result.get_main_result().to_string();
        // Skip empties and trivial echoes (e.g. "5" -> "5").
        if value.is_empty() || value == query.trim() {
            return Vec::new();
        }

        vec![Scored {
            item: SearchItem {
                title: value.clone(),
                subtitle: Some(format!("{} = {}  ·  ⏎ to copy", query.trim(), value)),
                icon_path: None,
                action: Action::Copy(value),
            },
            score: CALC_SCORE,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nucleo::pattern::{CaseMatching, Normalization, Pattern};
    use nucleo::{Config, Matcher};

    fn run(calc: &mut Calculator, q: &str) -> Vec<Scored> {
        let pattern = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut cx = PluginCx {
            pattern: &pattern,
            matcher: &mut matcher,
            limit: 8,
        };
        calc.search(q, &mut cx)
    }

    #[test]
    fn evaluates_math_and_units() {
        let mut calc = Calculator::new();
        assert_eq!(run(&mut calc, "2+2")[0].item.title, "4");
        assert!(run(&mut calc, "5 km in miles")[0]
            .item
            .title
            .contains("mile"));
        // Plain words and bare numbers don't produce a result.
        assert!(run(&mut calc, "safari").is_empty());
        assert!(run(&mut calc, "5").is_empty());
    }
}

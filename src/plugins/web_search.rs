//! Built-in plugin: keyword web searches (`g rust traits`, `yt lofi`) and bare
//! URLs/domains, opened in the default browser. The keyword→URL list is
//! config-driven (edited in settings), so it's read live on each search.

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config::WebLink;
use crate::core::item::{Action, SearchItem};

use super::{Plugin, PluginCx, Scored};

const WEB_SCORE: u32 = 9_000;

pub struct WebSearch {
    links: Rc<RefCell<Vec<WebLink>>>,
}

impl WebSearch {
    pub fn new(links: Rc<RefCell<Vec<WebLink>>>) -> Self {
        Self { links }
    }
}

impl Plugin for WebSearch {
    fn id(&self) -> &'static str {
        "web"
    }

    fn search(&mut self, query: &str, _cx: &mut PluginCx) -> Vec<Scored> {
        // "<keyword> <terms>" → site search.
        if let Some((head, rest)) = query.split_once(' ') {
            let rest = rest.trim();
            if !rest.is_empty() {
                if let Some(link) = self.links.borrow().iter().find(|l| l.keyword == head) {
                    let url = link.url.replace("{}", &urlencode(rest));
                    return vec![item(
                        format!("Search {} for \u{201c}{rest}\u{201d}", link.name),
                        url,
                    )];
                }
            }
        }

        // Bare URL / domain → open directly.
        if looks_like_url(query) {
            let url = if query.contains("://") {
                query.to_string()
            } else {
                format!("https://{query}")
            };
            return vec![item(format!("Open {query}"), url)];
        }

        Vec::new()
    }
}

fn item(title: String, url: String) -> Scored {
    Scored {
        item: SearchItem {
            title,
            subtitle: Some(url.clone()),
            icon_path: None,
            action: Action::OpenUrl(url),
        },
        score: WEB_SCORE,
    }
}

fn looks_like_url(q: &str) -> bool {
    if q.is_empty() || q.contains(char::is_whitespace) {
        return false;
    }
    if q.contains("://") {
        return q.chars().any(|c| c.is_ascii_alphabetic());
    }
    match q.find('.') {
        // A dot with content on both sides and at least one letter (so "5.0"
        // isn't treated as a domain).
        Some(dot) if dot > 0 && dot < q.len() - 1 => {
            q.chars().any(|c| c.is_ascii_alphabetic())
                && q.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-._/?=&#%:".contains(c))
        }
        _ => false,
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nucleo::pattern::{CaseMatching, Normalization, Pattern};
    use nucleo::{Config, Matcher};

    fn run(q: &str) -> Vec<Scored> {
        let links = Rc::new(RefCell::new(vec![WebLink {
            keyword: "g".to_string(),
            name: "Google".to_string(),
            url: "https://www.google.com/search?q={}".to_string(),
        }]));
        let mut web = WebSearch::new(links);
        let pattern = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut cx = PluginCx {
            pattern: &pattern,
            matcher: &mut matcher,
            limit: 8,
        };
        web.search(q, &mut cx)
    }

    #[test]
    fn builds_keyword_search_url() {
        let results = run("g rust traits");
        match &results[0].item.action {
            Action::OpenUrl(url) => {
                assert!(url.starts_with("https://www.google.com/search?q="));
                assert!(url.contains("rust%20traits"));
            }
            _ => panic!("expected OpenUrl"),
        }
    }

    #[test]
    fn detects_bare_domain_but_not_numbers() {
        assert!(matches!(
            run("github.com")[0].item.action,
            Action::OpenUrl(_)
        ));
        assert!(run("3.14").is_empty());
        assert!(run("hello world").is_empty());
    }
}

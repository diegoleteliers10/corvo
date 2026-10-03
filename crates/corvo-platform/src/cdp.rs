//! Chrome DevTools Protocol over the plain HTTP endpoints. Live tab
//! enumeration on Windows and Linux needs the browser running with
//! `--remote-debugging-port`; a browser that does not answer its port
//! simply reports no tabs. The endpoints used here (`/json/list`,
//! `/json/activate`) need no WebSocket.

use std::time::Duration;

use serde_json::Value;

use crate::BrowserTab;

/// Every CDP request gets two seconds; a closed local port refuses the
/// connection immediately, so this only guards a hung browser.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(REQUEST_TIMEOUT)
        .build()
}

/// Reads the browser's open pages from `/json/list`.
pub fn list_tabs(port: u16) -> Option<Vec<BrowserTab>> {
    let body = agent()
        .get(&format!("http://127.0.0.1:{port}/json/list"))
        .call()
        .ok()?
        .into_string()
        .ok()?;
    Some(parse_targets(&body))
}

/// Parses a `/json/list` payload into user-visible tabs: page targets
/// with a http, https, or file URL. Extension pages, devtools windows,
/// and background workers stay out of the list.
pub fn parse_targets(body: &str) -> Vec<BrowserTab> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("page"))
        .filter_map(|entry| {
            let url = entry.get("url")?.as_str()?.to_owned();
            let visible = ["http://", "https://", "file://"]
                .iter()
                .any(|scheme| url.starts_with(scheme));
            if !visible {
                return None;
            }
            Some(BrowserTab {
                title: entry
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or(&url)
                    .to_owned(),
                url,
            })
        })
        .collect()
}

/// Activates the tab with this URL: lists targets, matches the URL
/// exactly, and activates the target, which also raises the browser
/// window. Newer Chromium builds only accept PUT on `/json/activate`;
/// older ones take GET, so both are tried.
pub fn focus_tab_by_url(port: u16, url: &str) -> bool {
    let Some(id) = page_target_id(port, url) else {
        return false;
    };
    let base = format!("http://127.0.0.1:{port}/json/activate/{id}");
    if agent()
        .put(&base)
        .call()
        .map(|response| response.status() == 200)
        .unwrap_or(false)
    {
        return true;
    }
    agent()
        .get(&base)
        .call()
        .map(|response| response.status() == 200)
        .unwrap_or(false)
}

/// The target id of the first page target serving this exact URL.
fn page_target_id(port: u16, url: &str) -> Option<String> {
    let body = agent()
        .get(&format!("http://127.0.0.1:{port}/json/list"))
        .call()
        .ok()?
        .into_string()
        .ok()?;
    let value = serde_json::from_str::<Value>(&body).ok()?;
    value
        .as_array()?
        .iter()
        .find(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("page")
                && entry.get("url").and_then(Value::as_str) == Some(url)
        })
        .and_then(|entry| entry.get("id"))?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
        {
            "id": "first-target",
            "type": "page",
            "title": "GitHub",
            "url": "https://github.com/"
        },
        {
            "id": "extension-target",
            "type": "page",
            "title": "Extension",
            "url": "chrome-extension://abc/popup.html"
        },
        {
            "id": "worker-target",
            "type": "service_worker",
            "title": "Background",
            "url": "https://example.com/sw.js"
        },
        {
            "id": "second-target",
            "type": "page",
            "title": "Docs",
            "url": "https://doc.rust-lang.org/std/"
        }
    ]"#;

    #[test]
    fn parse_keeps_visible_pages_only() {
        let tabs = parse_targets(SAMPLE);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].title, "GitHub");
        assert_eq!(tabs[0].url, "https://github.com/");
        assert_eq!(tabs[1].url, "https://doc.rust-lang.org/std/");
    }

    #[test]
    fn parse_tolerates_garbage() {
        assert!(parse_targets("not json").is_empty());
        assert!(parse_targets("{}").is_empty());
    }

    #[test]
    fn closed_ports_report_no_tabs() {
        // Port 1 is never a debugging endpoint; the connection is
        // refused, not timed out.
        assert!(list_tabs(1).is_none());
    }
}

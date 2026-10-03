//! Browser tabs and bookmarks for Chromium browsers (Chrome, Brave,
//! Edge) on macOS: search open tabs across them and jump to one, or
//! open a bookmark. Tabs come from AppleScript with a short cache;
//! bookmarks are parsed straight from each profile's `Bookmarks` JSON.
//! History (SQLite) stays out of the MVP.

use corvo_core::{
    phosphor_svgs, search_match_score, Action, Command, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult,
};

const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

/// (display label, AppleScript application name)
const BROWSERS: &[(&str, &str)] = &[
    ("Chrome", "Google Chrome"),
    ("Brave", "Brave Browser"),
    ("Edge", "Microsoft Edge"),
];

/// One open tab.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub browser: &'static str,
    pub title: String,
    pub url: String,
}

/// One bookmark.
#[derive(Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

type TabsCache = std::sync::Mutex<Option<(std::time::Instant, Vec<Tab>)>>;

fn tabs_cache() -> &'static TabsCache {
    static CACHE: std::sync::OnceLock<TabsCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Enumerates open tabs across Chromium browsers. Blocking (one
/// osascript per running browser); the UI calls it unblocked.
pub fn fetch_tabs() -> Vec<Tab> {
    if let Ok(guard) = tabs_cache().lock() {
        if let Some((fetched_at, tabs)) = guard.as_ref() {
            if fetched_at.elapsed() < CACHE_TTL {
                return tabs.clone();
            }
        }
    }
    let mut tabs = Vec::new();
    for (label, script_name) in BROWSERS {
        tabs.extend(tabs_of(label, script_name));
    }
    if let Ok(mut guard) = tabs_cache().lock() {
        *guard = Some((std::time::Instant::now(), tabs.clone()));
    }
    tabs
}

/// The cached value only, never spawning AppleScript.
pub fn cached_tabs() -> Vec<Tab> {
    tabs_cache()
        .lock()
        .ok()
        .and_then(|guard| {
            let (fetched_at, tabs) = guard.as_ref()?;
            (fetched_at.elapsed() < CACHE_TTL).then(|| tabs.clone())
        })
        .unwrap_or_default()
}

fn tabs_of(label: &'static str, script_name: &str) -> Vec<Tab> {
    let script = format!(
        r#"if application "{script_name}" is running then
  tell application "{script_name}"
    set output to ""
    repeat with w in windows
      repeat with t in tabs of w
        set output to output & (title of t) & linefeed & (URL of t) & linefeed
      end repeat
    end repeat
    return output
  end tell
else
  return ""
end if"#
    );
    let Ok(output) = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    // The output interleaves title and URL lines; focusing later finds
    // the tab by URL, so window indices are not needed here.
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let mut tabs = Vec::new();
    while let Some(title) = lines.next() {
        let Some(url) = lines.next() else { break };
        tabs.push(Tab {
            browser: label,
            title: title.to_owned(),
            url: url.to_owned(),
        });
    }
    tabs
}

/// Brings the tab with this URL to the front of its browser.
pub fn focus_tab(browser: &str, url: &str) -> bool {
    let Some((_, script_name)) = BROWSERS.iter().find(|(label, _)| *label == browser) else {
        return false;
    };
    let mut command = focus_tab_command(script_name, url);
    let Ok(output) = command.output() else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "ok"
}

fn focus_tab_command(script_name: &str, url: &str) -> std::process::Command {
    let script = format!(
        r#"on run argv
set targetURL to item 1 of argv
tell application "{script_name}"
  activate
  repeat with w in windows
    repeat with t in tabs of w
      if (URL of t) is targetURL then
        set active tab of w to t
        set index of w to 1
        return "ok"
      end if
    end repeat
  end repeat
end tell
return "missing"
end run"#
    );
    let mut command = std::process::Command::new("osascript");
    command.arg("-e").arg(script).arg(url);
    command
}

/// Reads bookmarks from the first Chromium profile that has them.
pub fn load_bookmarks() -> Vec<Bookmark> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Vec::new();
    };
    let candidates = [
        "Library/Application Support/Google/Chrome/Default/Bookmarks",
        "Library/Application Support/BraveSoftware/Brave-Browser/Default/Bookmarks",
        "Library/Application Support/Microsoft Edge/Default/Bookmarks",
    ];
    for candidate in candidates {
        let path = home.join(candidate);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                let mut bookmarks = Vec::new();
                for root in ["bookmark_bar", "other", "synced"] {
                    if let Some(node) = value.pointer(&format!("/roots/{root}")) {
                        walk_bookmarks(node, &mut bookmarks);
                    }
                }
                if !bookmarks.is_empty() {
                    return bookmarks;
                }
            }
        }
    }
    Vec::new()
}

fn walk_bookmarks(node: &serde_json::Value, out: &mut Vec<Bookmark>) {
    let Some(children) = node
        .get("children")
        .and_then(|children| children.as_array())
    else {
        return;
    };
    for child in children {
        if let Some(url) = child.get("url").and_then(|url| url.as_str()) {
            out.push(Bookmark {
                title: child
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or(url)
                    .to_owned(),
                url: url.to_owned(),
            });
        }
        walk_bookmarks(child, out);
    }
}

fn tab_result(tab: &Tab, score: i32) -> SearchResult {
    SearchResult {
        id: format!("browser-tabs:tab:{}:{}", tab.browser, tab.url),
        title: tab.title.clone(),
        subtitle: Some(truncate(&tab.url, 80)),
        icon: Icon::Web,
        score,
        accessory: Some(tab.browser.to_owned()),
    }
}

fn bookmark_result(bookmark: &Bookmark, score: i32) -> SearchResult {
    SearchResult {
        id: format!("browser-tabs:bookmark:{}", bookmark.url),
        title: bookmark.title.clone(),
        subtitle: Some(truncate(&bookmark.url, 80)),
        icon: Icon::Link,
        score,
        accessory: Some("Bookmark".into()),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(limit).collect();
        format!("{cut}…")
    }
}

fn open_result(score: i32) -> SearchResult {
    SearchResult {
        id: "browser-tabs:open".into(),
        title: "Browser Tabs".into(),
        subtitle: Some("Search open tabs and bookmarks".into()),
        icon: Icon::Svg(phosphor_svgs::style::regular::APP_WINDOW),
        score,
        accessory: None,
    }
}

#[derive(Default)]
pub struct BrowserTabsCommand;

// macOS only for now, like brew and media-control.
#[cfg(target_os = "macos")]
corvo_core::register_command!(BrowserTabsCommand);

#[async_trait::async_trait]
impl Command for BrowserTabsCommand {
    fn id(&self) -> &'static str {
        "browser-tabs"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["tab", "tabs", "bookmark", "bookmarks", "browser"]
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        // Dedicated page: cached tabs first, bookmarks on a filter.
        if let Some(filter) = query.strip_prefix("tabs-page:") {
            let needle = filter.trim().to_lowercase();
            let mut results: Vec<SearchResult> = cached_tabs()
                .iter()
                .filter_map(|tab| {
                    if needle.is_empty() {
                        return Some(tab_result(tab, 700));
                    }
                    let score =
                        search_match_score(&needle, &[tab.title.as_str(), tab.url.as_str()])?;
                    Some(tab_result(tab, score))
                })
                .collect();
            if !needle.is_empty() {
                for bookmark in load_bookmarks() {
                    if let Some(score) = search_match_score(
                        &needle,
                        &[bookmark.title.as_str(), bookmark.url.as_str()],
                    ) {
                        results.push(bookmark_result(&bookmark, score));
                    }
                }
            }
            results.sort_by_key(|result| std::cmp::Reverse(result.score));
            results.truncate(ctx.max_results);
            return results;
        }

        let trimmed = query.trim();
        if trimmed.is_empty() {
            return vec![open_result(1000)];
        }
        let first = trimmed.split_whitespace().next().unwrap_or_default();
        let lowered = first.to_lowercase();
        if ["tab", "tabs", "bookmark", "bookmarks", "browser"].contains(&lowered.as_str()) {
            return vec![open_result(1000)];
        }
        corvo_core::search_match_score(
            trimmed,
            &["Browser Tabs", "browser tabs bookmarks chromium chrome"],
        )
        .map(|score| vec![open_result(score + 120)])
        .unwrap_or_default()
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some(key) = result_id.strip_prefix("browser-tabs:") else {
            return Err(CommandError::NotFound);
        };
        if key == "open" {
            return Ok(Action::ShowToast("Browser Tabs".into()));
        }
        if let Some(rest) = key.strip_prefix("tab:") {
            let Some((browser, url)) = rest.split_once(':') else {
                return Err(CommandError::NotFound);
            };
            let browser = browser.to_owned();
            let url = url.to_owned();
            return smol::unblock(move || focus_tab(&browser, &url))
                .await
                .then_some(Action::CloseWindow)
                .ok_or_else(|| CommandError::Platform("could not focus that tab".into()));
        }
        if let Some(url) = key.strip_prefix("bookmark:") {
            return Ok(Action::OpenUrl(url.to_owned()));
        }
        Err(CommandError::NotFound)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn focus_passes_the_url_as_data() {
        let url = "https://example.test/\"\\\n& do shell script \"bad\"";
        let command = focus_tab_command("Google Chrome", url);
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args[2], url);
        assert!(!args[1].to_string_lossy().contains(url));
        assert!(args[1].to_string_lossy().contains("item 1 of argv"));
    }

    #[test]
    fn walks_bookmark_trees() {
        let tree: serde_json::Value = serde_json::from_str(
            r#"{"roots": {"bookmark_bar": {"children": [
                {"name": "Rust", "url": "https://rust-lang.org"},
                {"name": "Docs", "children": [
                    {"name": "std", "url": "https://doc.rust-lang.org/std/"},
                    {"name": "More", "children": [
                        {"name": "cargo", "url": "https://doc.rust-lang.org/cargo/"}
                    ]}
                ]}
            ]}}}"#,
        )
        .unwrap();
        let mut bookmarks = Vec::new();
        walk_bookmarks(&tree["roots"]["bookmark_bar"], &mut bookmarks);
        assert_eq!(bookmarks.len(), 3);
        assert_eq!(bookmarks[0].title, "Rust");
        assert_eq!(bookmarks[2].url, "https://doc.rust-lang.org/cargo/");
    }

    #[test]
    fn root_routing_responds_to_keywords() {
        let search = |query: &str| {
            smol::block_on(<BrowserTabsCommand as Command>::search(
                &BrowserTabsCommand,
                query,
                &SearchContext::default(),
            ))
        };
        assert_eq!(search("tabs")[0].id, "browser-tabs:open");
        assert_eq!(search("")[0].id, "browser-tabs:open");
        assert!(search("hello world").is_empty());
    }

    #[test]
    fn execute_rejects_malformed_ids() {
        let result = smol::block_on(<BrowserTabsCommand as Command>::execute(
            &BrowserTabsCommand,
            "browser-tabs:tab:onlyonepart",
            &ExecutionContext::default(),
        ));
        assert_eq!(result.unwrap_err(), CommandError::NotFound);
    }
}

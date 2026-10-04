//! One launcher extension per browser — Chrome, Brave, Edge, Firefox,
//! Arc, Dia, Safari, and Aside — each with its own page, its real app
//! icon, its open tabs, and its bookmarks. A browser that is not
//! installed does not appear in the launcher at all.
//!
//! Tabs come from AppleScript on macOS and from the browser's CDP
//! endpoint on Windows and Linux (it must run with
//! `--remote-debugging-port`; the per-browser ports are documented in
//! EXTENSIONS_PLAN.md). Bookmarks are parsed from each Chromium
//! profile's `Bookmarks` JSON or, for Safari, from `Bookmarks.plist`.

mod bookmarks;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use corvo_core::{
    phosphor_svgs, search_match_score, Action, Command, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult,
};
pub use corvo_platform::AppleScriptFocus;
pub use corvo_platform::BrowserTab as Tab;

/// One bookmark.
#[derive(Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

const CACHE_TTL: Duration = Duration::from_secs(5);

/// Everything one browser extension needs. `chromium_dirs` holds the
/// profile location per OS (macOS, Windows, Linux) in the Chromium
/// layout; `None` when the browser keeps bookmarks somewhere else.
pub struct BrowserSpec {
    /// Command id and page key, e.g. "chrome".
    pub id: &'static str,
    /// Display name, e.g. "Google Chrome".
    pub name: &'static str,
    /// Root search keywords.
    pub keywords: &'static [&'static str],
    /// AppleScript application name (macOS).
    pub applescript_name: &'static str,
    /// AppleScript focus flavor (macOS).
    pub applescript_focus: AppleScriptFocus,
    /// CDP port (Windows and Linux live tabs).
    pub cdp_port: Option<u16>,
    /// App bundle file name on macOS, e.g. "Google Chrome.app".
    pub macos_bundle: &'static str,
    /// Executable path relative to a Program Files root on Windows.
    pub windows_exe: &'static str,
    /// `.desktop` file stem on Linux.
    pub linux_desktop: &'static str,
    /// Chromium profile directory per OS, or `None` when the browser
    /// does not keep a Chromium `Bookmarks` JSON.
    pub chromium_dirs: Option<(&'static str, &'static str, &'static str)>,
    /// Safari reads `Bookmarks.plist` instead.
    pub safari_bookmarks: bool,
}

/// The browsers that get their own launcher extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrowserId {
    Chrome,
    Brave,
    Edge,
    Firefox,
    Arc,
    Dia,
    Safari,
    Aside,
}

const SPECS: [BrowserSpec; 8] = [
    BrowserSpec {
        id: "chrome",
        name: "Google Chrome",
        keywords: &["chrome", "google chrome", "tabs", "bookmarks", "browser"],
        applescript_name: "Google Chrome",
        applescript_focus: AppleScriptFocus::ChromiumTabIndex,
        cdp_port: Some(9222),
        macos_bundle: "Google Chrome.app",
        windows_exe: r"Google\Chrome\Application\chrome.exe",
        linux_desktop: "google-chrome",
        chromium_dirs: Some(("Google/Chrome", r"Google\Chrome\User Data", "google-chrome")),
        safari_bookmarks: false,
    },
    BrowserSpec {
        id: "brave",
        name: "Brave",
        keywords: &["brave", "brave browser", "tabs", "bookmarks", "browser"],
        applescript_name: "Brave Browser",
        applescript_focus: AppleScriptFocus::ChromiumTabIndex,
        cdp_port: Some(9224),
        macos_bundle: "Brave Browser.app",
        windows_exe: r"BraveSoftware\Brave-Browser\Application\brave.exe",
        linux_desktop: "brave-browser",
        chromium_dirs: Some((
            "BraveSoftware/Brave-Browser",
            r"BraveSoftware\Brave-Browser\User Data",
            "brave-browser",
        )),
        safari_bookmarks: false,
    },
    BrowserSpec {
        id: "edge",
        name: "Microsoft Edge",
        keywords: &["edge", "microsoft edge", "tabs", "bookmarks", "browser"],
        applescript_name: "Microsoft Edge",
        applescript_focus: AppleScriptFocus::ChromiumTabIndex,
        cdp_port: Some(9223),
        macos_bundle: "Microsoft Edge.app",
        windows_exe: r"Microsoft\Edge\Application\msedge.exe",
        linux_desktop: "microsoft-edge",
        chromium_dirs: Some(("Microsoft Edge", r"Microsoft\Edge\User Data", "microsoft-edge")),
        safari_bookmarks: false,
    },
    BrowserSpec {
        id: "firefox",
        name: "Firefox",
        keywords: &["firefox", "mozilla firefox", "tabs", "bookmarks", "browser"],
        // Firefox removed its CDP endpoint, so live tabs are macOS
        // AppleScript only; bookmarks stay pending on places.sqlite.
        applescript_name: "Firefox",
        applescript_focus: AppleScriptFocus::SafariCurrentTab,
        cdp_port: None,
        macos_bundle: "Firefox.app",
        windows_exe: r"Mozilla Firefox\firefox.exe",
        linux_desktop: "firefox",
        chromium_dirs: None,
        safari_bookmarks: false,
    },
    BrowserSpec {
        id: "arc",
        name: "Arc",
        keywords: &["arc", "arc browser", "tabs", "bookmarks", "browser"],
        // Arc syncs bookmarks in its own store, so only tabs apply.
        applescript_name: "Arc",
        applescript_focus: AppleScriptFocus::ChromiumTabIndex,
        cdp_port: Some(9225),
        macos_bundle: "Arc.app",
        windows_exe: "",
        linux_desktop: "arc",
        chromium_dirs: None,
        safari_bookmarks: false,
    },
    BrowserSpec {
        id: "dia",
        name: "Dia",
        keywords: &["dia", "dia browser", "tabs", "bookmarks", "browser"],
        applescript_name: "Dia",
        applescript_focus: AppleScriptFocus::ChromiumTabIndex,
        cdp_port: Some(9226),
        macos_bundle: "Dia.app",
        windows_exe: "",
        linux_desktop: "dia",
        chromium_dirs: Some(("Dia", r"Dia\User Data", "dia")),
        safari_bookmarks: false,
    },
    BrowserSpec {
        id: "safari",
        name: "Safari",
        keywords: &["safari", "tabs", "bookmarks", "browser"],
        applescript_name: "Safari",
        applescript_focus: AppleScriptFocus::SafariCurrentTab,
        cdp_port: None,
        macos_bundle: "Safari.app",
        windows_exe: "",
        linux_desktop: "",
        chromium_dirs: None,
        safari_bookmarks: true,
    },
    BrowserSpec {
        id: "aside",
        name: "Aside",
        keywords: &["aside", "aside browser", "tabs", "bookmarks", "browser"],
        applescript_name: "Aside",
        applescript_focus: AppleScriptFocus::ChromiumTabIndex,
        cdp_port: Some(9227),
        macos_bundle: "Aside.app",
        windows_exe: "",
        linux_desktop: "",
        chromium_dirs: Some(("Aside", r"Aside\User Data", "aside")),
        safari_bookmarks: false,
    },
];

impl BrowserId {
    /// Every browser extension, in launch order.
    pub const ALL: [BrowserId; 8] = [
        BrowserId::Chrome,
        BrowserId::Brave,
        BrowserId::Edge,
        BrowserId::Firefox,
        BrowserId::Arc,
        BrowserId::Dia,
        BrowserId::Safari,
        BrowserId::Aside,
    ];

    pub fn spec(self) -> &'static BrowserSpec {
        &SPECS[self as usize]
    }

    /// Resolves a command id such as "chrome" back to its browser.
    pub fn from_command_id(id: &str) -> Option<BrowserId> {
        BrowserId::ALL
            .iter()
            .copied()
            .find(|browser| browser.spec().id == id)
    }
}

fn installed_flags() -> &'static Mutex<[Option<bool>; 8]> {
    static FLAGS: OnceLock<Mutex<[Option<bool>; 8]>> = OnceLock::new();
    FLAGS.get_or_init(|| Mutex::new(std::array::from_fn(|_| None)))
}

/// Whether this browser is installed on the current OS. Filesystem
/// stats, cached after the first answer.
fn browser_installed(id: BrowserId) -> bool {
    let index = id as usize;
    if let Ok(guard) = installed_flags().lock() {
        if let Some(known) = guard[index] {
            return known;
        }
    }
    let spec = id.spec();
    let installed = corvo_platform::browser_app_installed(
        spec.macos_bundle,
        spec.windows_exe,
        spec.linux_desktop,
    );
    if let Ok(mut guard) = installed_flags().lock() {
        guard[index] = Some(installed);
    }
    installed
}

fn browser_icons() -> &'static Mutex<[Option<PathBuf>; 8]> {
    static ICONS: OnceLock<Mutex<[Option<PathBuf>; 8]>> = OnceLock::new();
    ICONS.get_or_init(|| Mutex::new(std::array::from_fn(|_| None)))
}

/// The browser's real app icon when the warm-up pass has resolved it.
fn browser_icon(id: BrowserId) -> Icon {
    browser_icons()
        .lock()
        .ok()
        .and_then(|guard| guard[id as usize].clone())
        .map(Icon::Image)
        .unwrap_or(Icon::Web)
}

type TabsEntry = Option<(Instant, Vec<Tab>)>;

fn tabs_cache() -> &'static Mutex<[TabsEntry; 8]> {
    static CACHE: OnceLock<Mutex<[TabsEntry; 8]>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::array::from_fn(|_| None)))
}

/// Warms one browser's tabs and icon off the search path. Blocking
/// (one OS round trip); the UI calls it through an unblock executor.
pub fn fetch_tabs(id: BrowserId) -> Vec<Tab> {
    let spec = id.spec();
    if let Ok(guard) = tabs_cache().lock() {
        if let Some((fetched_at, tabs)) = guard[id as usize].as_ref() {
            if fetched_at.elapsed() < CACHE_TTL {
                return tabs.clone();
            }
        }
    }
    let tabs = corvo_platform::browser_tabs(spec.applescript_name, spec.cdp_port);
    if let Ok(mut guard) = tabs_cache().lock() {
        guard[id as usize] = Some((Instant::now(), tabs.clone()));
    }
    // The icon resolves once per browser and is cached by the OS
    // pipeline afterwards; the search path only reads the result.
    if let Some(png) = corvo_platform::browser_app_icon(
        spec.macos_bundle,
        spec.windows_exe,
        spec.linux_desktop,
    ) {
        if let Ok(mut guard) = browser_icons().lock() {
            guard[id as usize] = Some(png);
        }
    }
    tabs
}

/// The cached tabs only, never touching the OS: safe on the search
/// path.
pub fn cached_tabs(id: BrowserId) -> Vec<Tab> {
    tabs_cache()
        .lock()
        .ok()
        .and_then(|guard| {
            let (fetched_at, tabs) = guard[id as usize].as_ref()?;
            (fetched_at.elapsed() < CACHE_TTL).then(|| tabs.clone())
        })
        .unwrap_or_default()
}

/// True when a fetch has completed and found no tabs, so the page can
/// explain itself instead of looking broken.
fn tabs_resolved_empty(id: BrowserId) -> bool {
    tabs_cache()
        .lock()
        .ok()
        .and_then(|guard| {
            let (fetched_at, tabs) = guard[id as usize].as_ref()?;
            (fetched_at.elapsed() < CACHE_TTL).then_some(tabs.is_empty())
        })
        .unwrap_or(false)
}

fn opener_result(id: BrowserId, score: i32) -> SearchResult {
    let spec = id.spec();
    SearchResult {
        id: format!("{}:open", spec.id),
        title: spec.name.to_owned(),
        subtitle: Some("Tabs and bookmarks".into()),
        icon: browser_icon(id),
        score,
        accessory: None,
        section: None,
        accessories: Vec::new(),
    }
}

fn tab_result(id: BrowserId, tab: &Tab, score: i32) -> SearchResult {
    SearchResult {
        id: format!("{}:tab:{}", id.spec().id, tab.url),
        title: tab.title.clone(),
        subtitle: Some(truncate(&tab.url, 80)),
        icon: browser_icon(id),
        score,
        accessory: Some("Tab".into()),
        section: None,
        accessories: Vec::new(),
    }
}

fn bookmark_result(id: BrowserId, bookmark: &Bookmark, score: i32) -> SearchResult {
    SearchResult {
        id: format!("{}:bookmark:{}", id.spec().id, bookmark.url),
        title: bookmark.title.clone(),
        subtitle: Some(truncate(&bookmark.url, 80)),
        icon: Icon::Link,
        score,
        accessory: Some("Bookmark".into()),
        section: None,
        accessories: Vec::new(),
    }
}

/// What a page with no tabs says, per platform. `None` keeps the page
/// silent.
fn hint_text(id: BrowserId) -> Option<String> {
    let spec = id.spec();
    #[cfg(target_os = "macos")]
    {
        Some(format!("Open {} to see its tabs here", spec.name))
    }
    #[cfg(not(target_os = "macos"))]
    {
        match spec.cdp_port {
            Some(port) => Some(format!(
                "Launch {} with --remote-debugging-port={port} to list live tabs",
                spec.name
            )),
            None => Some(format!(
                "{} live tabs work on macOS only for now",
                spec.name
            )),
        }
    }
}

fn hint_result(id: BrowserId) -> Option<SearchResult> {
    let text = hint_text(id)?;
    Some(SearchResult {
        id: format!("{}:hint", id.spec().id),
        title: text,
        subtitle: None,
        icon: Icon::Svg(phosphor_svgs::style::regular::INFO),
        score: 1,
        accessory: None,
        section: None,
        accessories: Vec::new(),
    })
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(limit).collect();
        format!("{cut}…")
    }
}

async fn search_browser(
    id: BrowserId,
    query: &str,
    ctx: &SearchContext,
) -> Vec<SearchResult> {
    let spec = id.spec();
    let page_prefix = format!("{}-page:", spec.id);

    // Dedicated page: cached tabs first, bookmarks on a filter.
    if let Some(filter) = query.strip_prefix(&page_prefix) {
        let needle = filter.trim().to_lowercase();
        let mut results: Vec<SearchResult> = cached_tabs(id)
            .iter()
            .filter_map(|tab| {
                if needle.is_empty() {
                    return Some(tab_result(id, tab, 700));
                }
                let score =
                    search_match_score(&needle, &[tab.title.as_str(), tab.url.as_str()])?;
                Some(tab_result(id, tab, score))
            })
            .collect();
        if !needle.is_empty() {
            for bookmark in bookmarks::load(id) {
                if let Some(score) = search_match_score(
                    &needle,
                    &[bookmark.title.as_str(), bookmark.url.as_str()],
                ) {
                    results.push(bookmark_result(id, &bookmark, score));
                }
            }
        }
        if results.is_empty() && tabs_resolved_empty(id) {
            results.extend(hint_result(id));
        }
        results.sort_by_key(|result| std::cmp::Reverse(result.score));
        results.truncate(ctx.max_results);
        return results;
    }

    // Root search: this browser's opener, only while it is installed.
    // Empty queries stay unanswered so the root list is not flooded
    // with one row per browser; typing "tabs" or a browser name lists
    // them.
    let trimmed = query.trim();
    if !trimmed.is_empty() {
        let fuzzy_target = [spec.name, "tabs bookmarks browser"];
        let first = trimmed.split_whitespace().next().unwrap_or_default();
        let lowered = first.to_lowercase();
        let matched = spec.keywords.contains(&lowered.as_str())
            || search_match_score(trimmed, &fuzzy_target).is_some();
        if matched && browser_installed(id) {
            let score =
                search_match_score(trimmed, &fuzzy_target).map_or(1000, |score| score + 120);
            return vec![opener_result(id, score)];
        }
    }
    Vec::new()
}

async fn execute_browser(
    id: BrowserId,
    result_id: &str,
    _ctx: &ExecutionContext,
) -> Result<Action, CommandError> {
    let spec = id.spec();
    let prefix = format!("{}:", spec.id);
    let Some(key) = result_id.strip_prefix(&prefix) else {
        return Err(CommandError::NotFound);
    };
    if key == "open" {
        return Ok(Action::ShowToast(spec.name.into()));
    }
    if key == "hint" {
        let text = hint_text(id).unwrap_or_else(|| spec.name.into());
        return Ok(Action::ShowToast(text));
    }
    if let Some(url) = key.strip_prefix("tab:") {
        let app = spec.applescript_name.to_owned();
        let focus = spec.applescript_focus;
        let port = spec.cdp_port;
        let url = url.to_owned();
        return smol::unblock(move || corvo_platform::focus_browser_tab(&app, focus, port, &url))
            .await
            .then_some(Action::CloseWindow)
            .ok_or_else(|| CommandError::Platform("could not focus that tab".into()));
    }
    if let Some(url) = key.strip_prefix("bookmark:") {
        return Ok(Action::OpenUrl(url.to_owned()));
    }
    Err(CommandError::NotFound)
}

/// Implements `Command` for one browser. Every browser registers on
/// every OS; availability is decided per machine by the install check.
macro_rules! browser_command {
    ($command:ident, $browser:ident) => {
        #[derive(Default)]
        pub struct $command;

        corvo_core::register_command!($command);

        #[async_trait::async_trait]
        impl Command for $command {
            fn id(&self) -> &'static str {
                BrowserId::$browser.spec().id
            }

            fn keywords(&self) -> &'static [&'static str] {
                BrowserId::$browser.spec().keywords
            }

            fn priority(&self) -> u8 {
                60
            }

            async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
                search_browser(BrowserId::$browser, query, ctx).await
            }

            async fn execute(
                &self,
                result_id: &str,
                ctx: &ExecutionContext,
            ) -> Result<Action, CommandError> {
                execute_browser(BrowserId::$browser, result_id, ctx).await
            }
        }
    };
}

browser_command!(ChromeCommand, Chrome);
browser_command!(BraveCommand, Brave);
browser_command!(EdgeCommand, Edge);
browser_command!(FirefoxCommand, Firefox);
browser_command!(ArcCommand, Arc);
browser_command!(DiaCommand, Dia);
browser_command!(SafariCommand, Safari);
browser_command!(AsideCommand, Aside);

#[cfg(test)]
mod tests {
    use super::*;

    fn search(id: BrowserId, query: &str) -> Vec<SearchResult> {
        let command: Box<dyn Command> = match id {
            BrowserId::Chrome => Box::new(ChromeCommand),
            BrowserId::Brave => Box::new(BraveCommand),
            BrowserId::Edge => Box::new(EdgeCommand),
            BrowserId::Firefox => Box::new(FirefoxCommand),
            BrowserId::Arc => Box::new(ArcCommand),
            BrowserId::Dia => Box::new(DiaCommand),
            BrowserId::Safari => Box::new(SafariCommand),
            BrowserId::Aside => Box::new(AsideCommand),
        };
        smol::block_on(command.search(query, &SearchContext::default()))
    }

    #[test]
    fn spec_ids_and_ports_are_unique() {
        let ids: Vec<_> = BrowserId::ALL.iter().map(|id| id.spec().id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "ids repeat: {ids:?}");

        let ports: Vec<_> = BrowserId::ALL
            .iter()
            .filter_map(|id| id.spec().cdp_port)
            .collect();
        let mut sorted = ports.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ports.len(), sorted.len(), "ports repeat: {ports:?}");

        for id in BrowserId::ALL {
            let spec = id.spec();
            assert!(
                spec.keywords.contains(&"tabs") && spec.keywords.contains(&"bookmarks"),
                "{} misses the shared keywords",
                spec.id
            );
            assert_eq!(BrowserId::from_command_id(spec.id), Some(id));
        }
    }

    #[test]
    fn page_search_reads_the_cache_only() {
        // No warm-up has run in this process, so every page query must
        // answer from an empty cache without touching the OS.
        for id in BrowserId::ALL {
            assert!(search(id, &format!("{}-page:", id.spec().id)).is_empty());
            assert!(
                search(id, &format!("{}-page:rust docs", id.spec().id)).is_empty(),
                "unwarmed {} answered a filtered page query",
                id.spec().id
            );
        }
    }

    #[test]
    fn foreign_page_prefixes_stay_unanswered() {
        // A chrome query must not reach the aside command.
        assert!(search(BrowserId::Aside, "chrome-page:x").is_empty());
    }

    #[test]
    fn execute_rejects_malformed_ids() {
        for (id, result_id) in [
            (BrowserId::Chrome, "chrome:onlyword"),
            (BrowserId::Chrome, "noprefix:tab:x"),
            (BrowserId::Safari, "safari:"),
        ] {
            let result = smol::block_on(execute_browser(id, result_id, &ExecutionContext::default()));
            assert_eq!(result.unwrap_err(), CommandError::NotFound, "{result_id}");
        }
    }

    #[test]
    fn hints_wait_for_a_resolved_fetch() {
        // Before any fetch, the page stays quiet even with no tabs.
        for id in BrowserId::ALL {
            assert!(!tabs_resolved_empty(id));
        }
        let hint = hint_result(BrowserId::Chrome).expect("chrome hint exists");
        assert_eq!(hint.id, "chrome:hint");
        assert!(!hint.title.is_empty());
    }

    #[test]
    fn empty_root_queries_stay_unanswered() {
        // Otherwise every installed browser floods the root list.
        for id in BrowserId::ALL {
            assert!(search(id, "").is_empty(), "{} answered an empty query", id.spec().id);
        }
    }
}

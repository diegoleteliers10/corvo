//! Last-resort result: send the query to a web search engine. The lowest
//! priority in the registry, shown only when nothing else matched.

use corvo_core::{Action, Command, CommandError, ExecutionContext, Icon, SearchContext, SearchResult};

const DUCK_URL: &str = "https://duckduckgo.com/?q=";
const GOOGLE_URL: &str = "https://www.google.com/search?q=";

#[derive(Default)]
pub struct WebSearchFallbackCommand;

corvo_core::register_command!(WebSearchFallbackCommand);

#[async_trait::async_trait]
impl Command for WebSearchFallbackCommand {
    fn id(&self) -> &'static str {
        "web-search-fallback"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["search", "web", "google", "duckduckgo"]
    }

    fn priority(&self) -> u8 {
        1
    }

    async fn search(&self, query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        vec![
            SearchResult {
                id: format!("web-search-fallback:google:{q}"),
                title: format!("Search Google for \"{q}\""),
                subtitle: Some("Google".into()),
                icon: Icon::Web,
                score: 0.02,
                accessory: None,
            },
            SearchResult {
                id: format!("web-search-fallback:duckduckgo:{q}"),
                title: format!("Search DuckDuckGo for \"{q}\""),
                subtitle: Some("DuckDuckGo".into()),
                icon: Icon::Web,
                score: 0.01,
                accessory: None,
            },
        ]
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        if let Some(query) = result_id.strip_prefix("web-search-fallback:google:") {
            Ok(Action::OpenUrl(format!("{GOOGLE_URL}{}", percent_encode(query))))
        } else if let Some(query) = result_id.strip_prefix("web-search-fallback:duckduckgo:") {
            Ok(Action::OpenUrl(format!("{DUCK_URL}{}", percent_encode(query))))
        } else if let Some(query) = result_id.strip_prefix("web-search-fallback:") {
            Ok(Action::OpenUrl(format!("{GOOGLE_URL}{}", percent_encode(query))))
        } else {
            Err(CommandError::NotFound)
        }
    }

    fn actions(&self, result_id: &str) -> Vec<corvo_core::CommandAction> {
        let url = if let Some(query) = result_id.strip_prefix("web-search-fallback:google:") {
            format!("{GOOGLE_URL}{}", percent_encode(query))
        } else if let Some(query) = result_id.strip_prefix("web-search-fallback:duckduckgo:") {
            format!("{DUCK_URL}{}", percent_encode(query))
        } else if let Some(query) = result_id.strip_prefix("web-search-fallback:") {
            format!("{GOOGLE_URL}{}", percent_encode(query))
        } else {
            return Vec::new();
        };
        vec![
            corvo_core::CommandAction {
                id: "web-search-fallback:open".into(),
                label: "Open in Browser".into(),
                action: Action::OpenUrl(url.clone()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::ARROW_UP_RIGHT),
                group: corvo_core::ActionGroup::Primary,
                hotkey: Some("↵"),
            },
            corvo_core::CommandAction {
                id: "web-search-fallback:copy".into(),
                label: "Copy Search URL".into(),
                action: Action::ShowToast(format!("copy:{url}")),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                group: corvo_core::ActionGroup::Standard,
                hotkey: Some("⌘↵"),
            },
        ]
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

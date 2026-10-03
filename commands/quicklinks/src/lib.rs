use std::hash::{Hash, Hasher};
use std::sync::{OnceLock, RwLock};

use corvo_core::{
    search_match_score, Action, ActionGroup, Command, CommandAction, CommandError,
    ExecutionContext, Icon, Quicklink, SearchContext, SearchResult,
};

/// Placeholder inside a quicklink URL that receives the typed argument.
const ARGUMENT_PLACEHOLDER: &str = "{argument}";

fn cached_quicklinks() -> &'static RwLock<Vec<Quicklink>> {
    static CACHE: OnceLock<RwLock<Vec<Quicklink>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
}

fn quicklink_key(link: &Quicklink) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    link.name.hash(&mut hasher);
    link.url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// ASCII-only lowercasing: byte length never changes, so a prefix match
/// on the lowered text maps straight back onto the original by offset.
fn ascii_lower(text: &str) -> String {
    text.chars().map(|c| c.to_ascii_lowercase()).collect()
}

/// Splits `<trigger> <argument>` for a quicklink, where the trigger is
/// its alias or its full name. The argument keeps its original case.
fn split_argument(query: &str, link: &Quicklink) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowered = ascii_lower(trimmed);
    let triggers = [
        link.alias
            .as_deref()
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(ascii_lower),
        Some(ascii_lower(link.name.trim())),
    ];
    for trigger in triggers.into_iter().flatten() {
        if let Some(rest) = lowered.strip_prefix(&trigger) {
            if rest.starts_with(char::is_whitespace) {
                // `trigger.len()` is a valid boundary in `trimmed` too:
                // the two strings differ only in ASCII case.
                let argument = trimmed[trigger.len()..].trim();
                if !argument.is_empty() {
                    return Some(argument.to_owned());
                }
            }
        }
    }
    None
}

/// Percent-encodes an argument for a URL, keeping unreserved characters.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Builds the final URL: replaces `{argument}` with the encoded
/// argument, or appends the argument when the URL has no placeholder.
fn build_url(url: &str, argument: &str) -> String {
    let argument = argument.trim();
    if url.contains(ARGUMENT_PLACEHOLDER) {
        return url.replace(ARGUMENT_PLACEHOLDER, &percent_encode(argument));
    }
    if argument.is_empty() {
        return url.to_owned();
    }
    format!("{url}{}", percent_encode(argument))
}

#[derive(Default)]
pub struct QuicklinksCommand;

corvo_core::register_command!(QuicklinksCommand);

#[async_trait::async_trait]
impl Command for QuicklinksCommand {
    fn id(&self) -> &'static str {
        "quicklinks"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["link", "links", "quicklink", "open"]
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let Some(store) = ctx.store.as_ref() else {
            return Vec::new();
        };
        let q = query.trim().to_lowercase();
        let links = store.quicklinks();
        if let Ok(mut cache) = cached_quicklinks().write() {
            *cache = links.clone();
        }
        let mut results: Vec<SearchResult> = links
            .iter()
            .filter_map(|link| {
                if link.hidden {
                    return None;
                }
                // `<alias or name> <argument>` takes over the link and
                // carries the argument in the result id.
                if let Some(argument) = split_argument(query, link) {
                    return Some(SearchResult {
                        id: format!("quicklinks:{}::{}", quicklink_key(link), argument),
                        title: link.name.clone(),
                        subtitle: Some(build_url(&link.url, &argument)),
                        icon: Icon::Link,
                        score: 900,
                        accessory: link.hotkey.clone(),
                    });
                }
                let score = if q.is_empty() {
                    700
                } else {
                    search_match_score(
                        &q,
                        &[
                            link.name.as_str(),
                            link.alias.as_deref().unwrap_or(""),
                            link.url.as_str(),
                        ],
                    )?
                };
                Some(SearchResult {
                    id: format!("quicklinks:{}", quicklink_key(link)),
                    title: link.name.clone(),
                    subtitle: Some(link.url.clone()),
                    icon: Icon::Link,
                    score,
                    accessory: link.hotkey.clone(),
                })
            })
            .collect();
        results.sort_by_key(|result| std::cmp::Reverse(result.score));
        results.truncate(ctx.max_results);
        results
    }

    async fn execute(
        &self,
        result_id: &str,
        ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some((index, argument)) = split_result_id(result_id) else {
            return Err(CommandError::NotFound);
        };
        let Some(store) = ctx.store.as_ref() else {
            return Err(CommandError::NotFound);
        };
        let links = store.quicklinks();
        let Some(link) = links
            .iter()
            .find(|link| quicklink_key(link) == index && !link.hidden)
        else {
            return Err(CommandError::NotFound);
        };
        Ok(Action::OpenUrl(build_url(&link.url, argument)))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some((index, argument)) = split_result_id(result_id) else {
            return Vec::new();
        };
        let Ok(cache) = cached_quicklinks().read() else {
            return Vec::new();
        };
        let Some(link) = cache.iter().find(|link| quicklink_key(link) == index) else {
            return Vec::new();
        };
        let url = build_url(&link.url, argument);

        vec![
            CommandAction {
                id: "quicklinks-action:open".into(),
                label: "Open in Browser".into(),
                action: Action::OpenUrl(url.clone()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::ARROW_UP_RIGHT),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            },
            CommandAction {
                id: "quicklinks-action:copy".into(),
                label: "Copy URL".into(),
                action: Action::ShowToast(format!("copy:{url}")),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Standard,
                hotkey: Some("cmd+enter"),
            },
        ]
    }
}

/// The result id is `quicklinks:<16-hex key>` or, when an argument was
/// typed, `quicklinks:<16-hex key>::<argument>`.
fn split_result_id(result_id: &str) -> Option<(&str, &str)> {
    let tail = result_id.strip_prefix("quicklinks:")?;
    if tail.len() > 18 && tail.as_bytes()[16] == b':' && tail.as_bytes()[17] == b':' {
        Some((&tail[..16], &tail[18..]))
    } else {
        Some((tail, ""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(name: &str, url: &str, alias: Option<&str>) -> Quicklink {
        Quicklink {
            name: name.to_owned(),
            url: url.to_owned(),
            alias: alias.map(str::to_owned),
            hotkey: None,
            hidden: false,
        }
    }

    #[test]
    fn build_url_replaces_the_placeholder() {
        assert_eq!(
            build_url("https://duckduckgo.com/?q={argument}", "rust lang"),
            "https://duckduckgo.com/?q=rust%20lang"
        );
    }

    #[test]
    fn build_url_appends_without_placeholder() {
        assert_eq!(
            build_url("https://example.com/", "a b"),
            "https://example.com/a%20b"
        );
        assert_eq!(build_url("https://example.com", ""), "https://example.com");
    }

    #[test]
    fn build_url_drops_placeholder_when_argument_is_empty() {
        assert_eq!(
            build_url("https://duckduckgo.com/?q={argument}", ""),
            "https://duckduckgo.com/?q="
        );
    }

    #[test]
    fn split_argument_matches_alias_and_name() {
        let l = link(
            "Google Search",
            "https://google.com/search?q={argument}",
            Some("g"),
        );
        assert_eq!(
            split_argument("g rust lang", &l).as_deref(),
            Some("rust lang")
        );
        assert_eq!(
            split_argument("google search rust", &l).as_deref(),
            Some("rust")
        );
        // Case preserved in the argument, ignored in the trigger.
        assert_eq!(
            split_argument("G RustLang", &l).as_deref(),
            Some("RustLang")
        );
        // No argument: not an argument match.
        assert_eq!(split_argument("g", &l), None);
        assert_eq!(split_argument("unrelated g rust", &l), None);
    }

    #[test]
    fn result_id_roundtrips_the_argument() {
        let (index, argument) = split_result_id("quicklinks:0123456789abcdef::a b:c").unwrap();
        assert_eq!(index, "0123456789abcdef");
        assert_eq!(argument, "a b:c");

        let (index, argument) = split_result_id("quicklinks:0123456789abcdef").unwrap();
        assert_eq!(index, "0123456789abcdef");
        assert_eq!(argument, "");

        assert!(split_result_id("snippets:x").is_none());
    }
}

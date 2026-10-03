//! Text transforms in a dedicated launcher page: case conversions,
//! encoders and decoders, JSON formatting, hashes, line tools, and id
//! generators.
//!
//! Routing: root search shows a `text-utilities:open` row (carrying any
//! input typed after a keyword) that opens the page; the page queries
//! arrive here as `text-page:<input>`, where empty input falls back to
//! the clipboard. `uuid` is a root-level quick command that generates
//! immediately, like the port-kill shortcut of the ports manager.

pub mod text;

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use corvo_core::{
    phosphor_svgs, Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext,
    Icon, SearchContext, SearchResult,
};

/// First words that route a query to this command. Stripped from the
/// input before transforming.
const KEYWORDS: &[&str] = &[
    "text", "case", "encode", "decode", "base64", "url", "hex", "json", "hash", "sha", "uuid",
    "lines",
];

/// Characters shown in a subtitle before it is cut with an ellipsis.
const SUBTITLE_LIMIT: usize = 140;

/// One transform offered for the current input. The full output is kept
/// out of the result id (clipboard input can be large) in a cache that
/// lives exactly as long as the result list built from it.
struct Utility {
    key: &'static str,
    title: &'static str,
    category: &'static str,
    score: i32,
    output: String,
}

fn output_cache() -> &'static RwLock<HashMap<String, String>> {
    static CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn category_icon(category: &str) -> Icon {
    match category {
        "Case" => Icon::Svg(phosphor_svgs::style::regular::TEXT_AA),
        "Encode" | "Decode" => Icon::Svg(phosphor_svgs::style::regular::CODE),
        "JSON" => Icon::Svg(phosphor_svgs::style::regular::BRACKETS_CURLY),
        "Hash" => Icon::Svg(phosphor_svgs::style::regular::FINGERPRINT),
        "Lines" => Icon::Svg(phosphor_svgs::style::regular::LIST),
        "Count" => Icon::Svg(phosphor_svgs::style::regular::CALCULATOR),
        _ => Icon::Svg(phosphor_svgs::style::regular::SHUFFLE),
    }
}

/// Splits a query into its input. `None` means this command should not
/// answer; `Some(None)` means keyword-only, so the input falls back to
/// the clipboard. The input is cut by offset, not re-joined from words,
/// so newlines and spacing survive for the line tools.
fn routed_input(query: &str) -> Option<Option<String>> {
    let trimmed = query.trim();
    let first = trimmed.split_whitespace().next()?;
    if !KEYWORDS.contains(&first.to_lowercase().as_str()) {
        return None;
    }
    let rest = trimmed[first.len()..].trim();
    if rest.is_empty() {
        Some(None)
    } else {
        Some(Some(rest.to_owned()))
    }
}

/// The root-search row that opens the dedicated page. Any input typed
/// after the keyword rides along in the id and pre-fills the page.
fn open_result(input: &str, score: i32) -> SearchResult {
    let subtitle = if input.is_empty() {
        "Commands".to_owned()
    } else {
        truncate_for_display(&format!("Transform “{input}”"))
    };
    SearchResult {
        id: if input.is_empty() {
            "text-utilities:open".into()
        } else {
            format!("text-utilities:open:{input}")
        },
        title: "Text Utilities".into(),
        subtitle: Some(subtitle),
        icon: Icon::Svg(phosphor_svgs::style::regular::TEXT_AA),
        score,
        accessory: None,
    }
}

/// (result key, display title, transform) for a static list of tools.
type TextTool = (&'static str, &'static str, fn(&str) -> String);

fn case_utilities(input: &str) -> Vec<Utility> {
    let cases: [TextTool; 11] = [
        ("upper", "Upper Case", text::to_upper),
        ("lower", "Lower Case", text::to_lower),
        ("title", "Title Case", text::to_title),
        ("sentence", "Sentence Case", text::to_sentence),
        ("camel", "camelCase", text::to_camel),
        ("pascal", "PascalCase", text::to_pascal),
        ("snake", "snake_case", text::to_snake),
        ("kebab", "kebab-case", text::to_kebab),
        ("constant", "CONSTANT_CASE", text::to_constant),
        ("path", "path/case", text::to_path_case),
        ("dot", "dot.case", text::to_dot),
    ];
    cases
        .into_iter()
        .enumerate()
        .map(|(index, (key, title, transform))| Utility {
            key,
            title,
            category: "Case",
            score: 600 - index as i32,
            output: transform(input),
        })
        .collect()
}

fn transform_utilities(input: &str) -> Vec<Utility> {
    let mut utilities = case_utilities(input);

    if let Some(decoded) = text::base64_decode(input) {
        utilities.push(Utility {
            key: "base64-decode",
            title: "Decode Base64",
            category: "Decode",
            score: 640,
            output: decoded,
        });
    }
    if let Some(decoded) = text::url_decode(input) {
        utilities.push(Utility {
            key: "url-decode",
            title: "Decode URL",
            category: "Decode",
            score: 639,
            output: decoded,
        });
    }
    if let Some(decoded) = text::hex_decode(input) {
        utilities.push(Utility {
            key: "hex-decode",
            title: "Decode Hex",
            category: "Decode",
            score: 638,
            output: decoded,
        });
    }

    if let Some(pretty) = text::json_pretty(input) {
        utilities.push(Utility {
            key: "json-pretty",
            title: "Pretty-print JSON",
            category: "JSON",
            score: 630,
            output: pretty,
        });
        utilities.push(Utility {
            key: "json-minify",
            title: "Minify JSON",
            category: "JSON",
            score: 629,
            output: text::json_minify(input).unwrap_or_default(),
        });
    }

    utilities.push(Utility {
        key: "base64-encode",
        title: "Encode Base64",
        category: "Encode",
        score: 580,
        output: text::base64_encode(input),
    });
    utilities.push(Utility {
        key: "url-encode",
        title: "Encode URL",
        category: "Encode",
        score: 579,
        output: text::url_encode(input),
    });
    utilities.push(Utility {
        key: "hex-encode",
        title: "Encode Hex",
        category: "Encode",
        score: 578,
        output: text::hex_encode(input),
    });

    utilities.push(Utility {
        key: "sha256",
        title: "SHA-256",
        category: "Hash",
        score: 570,
        output: text::sha256_hex(input),
    });
    utilities.push(Utility {
        key: "sha1",
        title: "SHA-1",
        category: "Hash",
        score: 569,
        output: text::sha1_hex(input),
    });

    utilities.push(Utility {
        key: "count",
        title: "Count",
        category: "Count",
        score: 560,
        output: text::count_stats(input),
    });

    if input.contains('\n') {
        let line_tools: [TextTool; 4] = [
            ("sort-lines", "Sort Lines", text::sort_lines),
            ("dedupe-lines", "Remove Duplicate Lines", text::dedupe_lines),
            ("reverse-lines", "Reverse Lines", text::reverse_lines),
            ("trim-lines", "Trim Lines", text::trim_lines),
        ];
        for (index, (key, title, transform)) in line_tools.into_iter().enumerate() {
            utilities.push(Utility {
                key,
                title,
                category: "Lines",
                score: 530 - index as i32,
                output: transform(input),
            });
        }
    }

    utilities
}

fn generator_utilities() -> Vec<Utility> {
    vec![
        Utility {
            key: "uuid-v4",
            title: "Generate UUID v4",
            category: "Generate",
            score: 560,
            output: text::uuid_v4(),
        },
        Utility {
            key: "uuid-v7",
            title: "Generate UUID v7",
            category: "Generate",
            score: 559,
            output: text::uuid_v7(),
        },
        Utility {
            key: "random-hex",
            title: "Generate Random Hex",
            category: "Generate",
            score: 558,
            output: text::random_hex(),
        },
    ]
}

fn truncate_for_display(value: &str) -> String {
    if value.chars().count() <= SUBTITLE_LIMIT {
        value.to_owned()
    } else {
        let cut: String = value.chars().take(SUBTITLE_LIMIT).collect();
        format!("{cut}…")
    }
}

#[derive(Default)]
pub struct TextUtilitiesCommand;

corvo_core::register_command!(TextUtilitiesCommand);

/// Ranks, truncates, caches the full outputs, and renders the utility
/// list. The cache lives exactly as long as the result list built from
/// it, which is what `execute` and `actions` read.
fn cache_and_render(mut utilities: Vec<Utility>, max_results: usize) -> Vec<SearchResult> {
    utilities.sort_by_key(|utility| std::cmp::Reverse(utility.score));
    utilities.truncate(max_results);

    if let Ok(mut cache) = output_cache().write() {
        cache.clear();
        cache.extend(
            utilities
                .iter()
                .map(|utility| (format!("text-utilities:{}", utility.key), utility.output.clone())),
        );
    }

    utilities
        .into_iter()
        .map(|utility| SearchResult {
            id: format!("text-utilities:{}", utility.key),
            title: utility.title.to_owned(),
            subtitle: Some(truncate_for_display(&utility.output)),
            icon: category_icon(utility.category),
            score: utility.score,
            accessory: Some(utility.category.to_owned()),
        })
        .collect()
}

/// Results for the dedicated page: transforms of the typed input, or of
/// the clipboard when the page input is empty, or generators when there
/// is nothing to transform.
async fn page_results(input: &str, ctx: &SearchContext) -> Vec<SearchResult> {
    let input = if input.trim().is_empty() {
        smol::unblock(corvo_platform::read_clipboard_text).await
    } else {
        Some(input.to_owned())
    };

    let utilities = match input.as_deref() {
        Some(text) if !text.trim().is_empty() => transform_utilities(text),
        _ => generator_utilities(),
    };
    cache_and_render(utilities, ctx.max_results)
}

#[async_trait::async_trait]
impl Command for TextUtilitiesCommand {
    fn id(&self) -> &'static str {
        "text-utilities"
    }

    fn keywords(&self) -> &'static [&'static str] {
        KEYWORDS
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        // Dedicated page: the raw typed input, no keyword routing.
        if let Some(page_input) = query.strip_prefix("text-page:") {
            return page_results(page_input.trim(), ctx).await;
        }

        let trimmed = query.trim();
        if trimmed.is_empty() {
            return vec![open_result("", 1000)];
        }

        // Quick command: `uuid` generates immediately from root search.
        if trimmed
            .split_whitespace()
            .next()
            .is_some_and(|first| first.eq_ignore_ascii_case("uuid"))
        {
            let mut results = cache_and_render(generator_utilities(), 3);
            results.push(open_result("", 500));
            return results;
        }

        // Keyword-first queries open the page, carrying the input.
        if let Some(explicit) = routed_input(trimmed) {
            let input = explicit.unwrap_or_default();
            return vec![open_result(&input, 1000)];
        }

        // Plain fuzzy match on the command name, like emoji-picker.
        corvo_core::search_match_score(
            trimmed,
            &[
                "Text Utilities",
                "text case encode decode base64 url hex json hash sha uuid lines transform",
            ],
        )
        .map(|score| vec![open_result("", score + 120)])
        .unwrap_or_default()
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some(key) = result_id.strip_prefix("text-utilities:") else {
            return Err(CommandError::NotFound);
        };
        // The UI intercepts the open row and shows the page; this is the
        // placeholder for any other caller.
        if key == "open" || key.starts_with("open:") {
            return Ok(Action::ShowToast("Text Utilities".into()));
        }
        let cache = output_cache()
            .read()
            .map_err(|_| CommandError::NotFound)?;
        cache
            .get(result_id)
            .cloned()
            .map(Action::Copy)
            .ok_or(CommandError::NotFound)
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(key) = result_id.strip_prefix("text-utilities:") else {
            return Vec::new();
        };
        if key == "open" || key.starts_with("open:") {
            return vec![CommandAction {
                id: "text-utilities:open-page".into(),
                label: "Open Text Utilities".into(),
                action: Action::ShowToast("Text Utilities".into()),
                icon: Icon::Svg(phosphor_svgs::style::regular::TEXT_AA),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            }];
        }
        let Ok(cache) = output_cache().read() else {
            return Vec::new();
        };
        let Some(output) = cache.get(result_id) else {
            return Vec::new();
        };
        vec![
            CommandAction {
                id: "text-utilities:copy".into(),
                label: "Copy".into(),
                action: Action::Copy(output.clone()),
                icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            },
            CommandAction {
                id: "text-utilities:paste".into(),
                label: "Paste to Active App".into(),
                action: Action::PasteText(output.clone()),
                icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                group: ActionGroup::Standard,
                hotkey: Some("cmd+enter"),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The output cache is a process-wide static; tests that read or
    /// write it through `search`/`execute` must not interleave.
    static CACHE_LOCK: Mutex<()> = Mutex::new(());

    fn search(query: &str) -> Vec<SearchResult> {
        let _guard = CACHE_LOCK.lock().unwrap();
        search_unlocked(query)
    }

    fn search_unlocked(query: &str) -> Vec<SearchResult> {
        smol::block_on(<TextUtilitiesCommand as Command>::search(
            &TextUtilitiesCommand,
            query,
            &SearchContext::default(),
        ))
    }

    fn output_of(results: &[SearchResult], key: &str) -> String {
        let id = format!("text-utilities:{key}");
        results
            .iter()
            .find(|result| result.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
            .subtitle
            .clone()
            .unwrap_or_default()
    }

    #[test]
    fn root_search_offers_the_page_entry() {
        // Match: keyword plus input becomes a single open row that
        // carries the input to the page.
        let results = search("text hello world");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "text-utilities:open:hello world");
        assert_eq!(results[0].title, "Text Utilities");

        // Match: empty query surfaces the entry in the initial list.
        let results = search("");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "text-utilities:open");

        // Non-match: unrelated text stays silent.
        assert!(search("hello world").is_empty());
    }

    #[test]
    fn keyword_matching_is_case_insensitive_and_carries_input() {
        let results = search("BASE64 aGVsbG8=");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "text-utilities:open:aGVsbG8=");
    }

    #[test]
    fn uuid_is_a_root_quick_command() {
        let results = search("uuid");
        assert!(results
            .iter()
            .any(|result| result.id == "text-utilities:uuid-v4"));
        assert!(results
            .iter()
            .any(|result| result.id == "text-utilities:open"));
    }

    #[test]
    fn page_queries_transform_the_input() {
        let results = search("text-page:hello world");
        assert!(!results.is_empty());
        assert_eq!(output_of(&results, "camel"), "helloWorld");
        assert_eq!(output_of(&results, "snake"), "hello_world");
    }

    #[test]
    fn decode_rows_appear_only_for_decodable_input() {
        let results = search("text-page:aGVsbG8=");
        assert_eq!(output_of(&results, "base64-decode"), "hello");

        let results = search("text-page:hello world");
        assert!(results
            .iter()
            .all(|result| result.id != "text-utilities:base64-decode"));
    }

    #[test]
    fn json_rows_appear_only_for_valid_json() {
        let results = search("text-page:{\"a\": 1}");
        assert!(output_of(&results, "json-minify").starts_with("{\"a\":1}"));

        let results = search("text-page:not json at all");
        assert!(results
            .iter()
            .all(|result| result.id != "text-utilities:json-pretty"));
    }

    #[test]
    fn line_tools_need_multiple_lines() {
        let single = search("text-page:only one line");
        assert!(single
            .iter()
            .all(|result| result.id != "text-utilities:sort-lines"));

        let multi = search("text-page:b\na");
        assert_eq!(output_of(&multi, "sort-lines"), "a\nb");
    }

    #[test]
    fn execute_copies_the_cached_output() {
        let _guard = CACHE_LOCK.lock().unwrap();
        let results = search_unlocked("text-page:hi");
        assert!(!results.is_empty());
        let action =
            smol::block_on(<TextUtilitiesCommand as Command>::execute(
                &TextUtilitiesCommand,
                "text-utilities:hex-encode",
                &ExecutionContext::default(),
            ))
            .unwrap();
        assert_eq!(action, Action::Copy("6869".to_owned()));

        let open = smol::block_on(<TextUtilitiesCommand as Command>::execute(
            &TextUtilitiesCommand,
            "text-utilities:open:hi",
            &ExecutionContext::default(),
        ))
        .unwrap();
        assert_eq!(open, Action::ShowToast("Text Utilities".to_owned()));

        let missing = smol::block_on(<TextUtilitiesCommand as Command>::execute(
            &TextUtilitiesCommand,
            "text-utilities:unknown",
            &ExecutionContext::default(),
        ));
        assert_eq!(missing.unwrap_err(), CommandError::NotFound);
    }

    #[test]
    fn actions_menu_reads_the_cache() {
        let _guard = CACHE_LOCK.lock().unwrap();
        assert!(search_unlocked("text-page:hi").iter().any(|result| result.id == "text-utilities:sha256"));
        let actions = <TextUtilitiesCommand as Command>::actions(
            &TextUtilitiesCommand,
            "text-utilities:sha256",
        );
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].hotkey, Some("enter"));

        assert!(<TextUtilitiesCommand as Command>::actions(
            &TextUtilitiesCommand,
            "text-utilities:unknown",
        )
        .is_empty());
    }

    #[test]
    fn long_outputs_are_truncated_for_display_only() {
        let _guard = CACHE_LOCK.lock().unwrap();
        let long = "a".repeat(300);
        let results = search_unlocked(&format!("text-page:{long}"));
        let subtitle = output_of(&results, "upper");
        assert_eq!(subtitle.chars().count(), SUBTITLE_LIMIT + 1);
        assert!(subtitle.ends_with('…'));

        let action = smol::block_on(<TextUtilitiesCommand as Command>::execute(
            &TextUtilitiesCommand,
            "text-utilities:upper",
            &ExecutionContext::default(),
        ))
        .unwrap();
        assert_eq!(action, Action::Copy(long.to_uppercase()));
    }
}


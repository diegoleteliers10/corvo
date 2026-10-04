//! Example Corvo extension — copy me, rename me, ship me.
//!
//! This is the standard starting point for a new command. It shows
//! every part of the extension contract: root routing, a dedicated
//! page fed from a cache, sections, actions, preferences, and tests.
//! Everything below compiles as-is; replace `Example` with your
//! extension's name and grow from here.
//!
//! # Make it yours — four edits
//!
//! 1. Copy this folder to `commands/<your-id>/` (kebab-case, matches
//!    the command id below).
//! 2. In `Cargo.toml`: rename the package to `corvo-<your-id>`.
//! 3. In this file: change the `Command::id` to `<your-id>`, the
//!    keywords, the title strings, and the item catalog.
//! 4. Register the crate in the root `Cargo.toml`: one line under
//!    `[workspace] members` (`"commands/<your-id>"`) and one under
//!    `[dependencies]` (`corvo-<your-id> = { path = "commands/<your-id>" }`).
//!    Nothing else — `build.rs` links you automatically and
//!    `src/main.rs` is never edited.
//!
//! Then `cargo test -p corvo-<your-id> && cargo clippy --workspace --all-targets -- -D warnings`.
//!
//! The full guide lives in the extension book (`book/` in the repo,
//! published on GitHub Pages); the module reference for `corvo_ext`
//! items is rustdoc (`cargo doc -p corvo-ext --open`). For a rich
//! declarative page (Blocks/Detail/Grid/Form) and a manifest with
//! declared commands, read `book/src/views.md` and
//! `book/src/modules/pages.md`; `commands/countdown` is the
//! reference implementation.

use corvo_core::{
    phosphor_svgs, Action, Command, CommandError, CommandAction, ExecutionContext, Icon,
    SearchContext, SearchResult,
};
use corvo_ext::actions;
use corvo_ext::cache::TtlCache;
use corvo_ext::list::{self, ListItem};
use corvo_ext::prefs;
use corvo_ext::routing;

/// The command id and namespace for every result id. Keep it equal to
/// the crate directory name.
const ID: &str = "example";

/// Root-search keywords: the words a user types to reach this
/// command. Include the command name and its natural typos' stem.
const KEYWORDS: &[&str] = &["example", "demo"];

/// Per-extension preferences, read from
/// `config_dir()/extensions/example.toml`. Missing file = defaults.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ExamplePrefs {
    /// Show only the items in this section on the page.
    favorite_section: String,
}

/// One catalog row the demo "serves". In a real extension this is a
/// file, a database, an HTTP API, or OS data.
struct Item {
    name: &'static str,
    detail: &'static str,
    url: &'static str,
    section: &'static str,
}

const CATALOG: &[Item] = &[
    Item { name: "Rust", detail: "systems language", url: "https://rust-lang.org", section: "Languages" },
    Item { name: "Python", detail: "scripting language", url: "https://python.org", section: "Languages" },
    Item { name: "Cargo", detail: "build tool", url: "https://doc.rust-lang.org/cargo/", section: "Tools" },
    Item { name: "Rustfmt", detail: "formatter", url: "https://github.com/rust-lang/rustfmt", section: "Tools" },
];

/// The demo cache. A real extension wraps its slow source — HTTP,
/// AppleScript, SQLite — and calls `get_or_fetch` from the UI's
/// unblock executor, never from `search`.
fn cache() -> &'static TtlCache<Vec<&'static str>> {
    static CACHE: std::sync::OnceLock<TtlCache<Vec<&'static str>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(TtlCache::new)
}

/// Pretend this is a network call. Deterministic and instant here.
fn load_items() -> Vec<&'static str> {
    CATALOG.iter().map(|item| item.name).collect()
}

/// The page's item source. A real extension warms the cache off the
/// search path (a UI spawn + `smol::unblock`) and reads
/// `cache().get(TTL)` inside `search`, answering empty until the
/// warm-up lands. The demo's source is instant, so it may
/// `get_or_fetch` inline.
fn page_items() -> Vec<&'static str> {
    cache().get_or_fetch(std::time::Duration::from_secs(60), load_items)
}

#[derive(Default)]
pub struct ExampleCommand;

corvo_core::register_command!(ExampleCommand);

#[async_trait::async_trait]
impl Command for ExampleCommand {
    fn id(&self) -> &'static str {
        ID
    }

    fn keywords(&self) -> &'static [&'static str] {
        KEYWORDS
    }

    fn priority(&self) -> u8 {
        // 50 is the default; raise only to win ties against another
        // command answering the same query.
        50
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        // The dedicated page: the UI sends `{id}-page:{filter}` and
        // expects an answer from the cache.
        if let Some(filter) = query.strip_prefix(&format!("{ID}-page:")) {
            let needle = filter.trim().to_lowercase();
            // Preferences can preselect one section; empty means all.
            let favorite = prefs::load::<ExamplePrefs>(ID).favorite_section;
            let mut results: Vec<SearchResult> = CATALOG
                .iter()
                .filter(|item| page_items().contains(&item.name))
                .filter(|item| {
                    if needle.is_empty() {
                        // An empty filter honors the preselected
                        // section; a typed filter searches everywhere.
                        favorite.is_empty() || favorite == item.section
                    } else {
                        corvo_core::search_match_score(
                            &needle,
                            &[item.name, item.detail],
                        )
                        .is_some()
                    }
                })
                .map(|item| item_row(item, 700))
                .collect();
            if results.is_empty() {
                results.push(list::empty_state(ID, "No matching items"));
            }
            results.truncate(ctx.max_results);
            return results;
        }

        // Root search: the opener row, plus a top item for quick
        // access. Empty queries stay unanswered.
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let first = routing::first_word(trimmed);
        if KEYWORDS.contains(&first.as_str()) {
            let mut results = vec![open_entry(1000)];
            if let Some(item) = CATALOG.first() {
                results.push(item_row(item, 900));
            }
            return results;
        }
        list::fuzzy_open(trimmed, &["Example", "example demo extension"], &open_entry(0))
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let key = routing::key(result_id, ID)?;
        if key == "open" {
            return Ok(corvo_ext::feedback::toast("Example"));
        }
        if let Some(url) = key.strip_prefix("item:") {
            return Ok(Action::OpenUrl(url.to_owned()));
        }
        Err(CommandError::NotFound)
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        // Menus run concrete `Action`s through the UI, not through
        // `execute`. Copy and open cover most list rows.
        let Some(url) = result_id
            .strip_prefix(&format!("{ID}:item:"))
        else {
            return Vec::new();
        };
        let mut menu = actions::open_url("example:url", url.to_owned());
        menu.hotkey = Some("cmd+c");
        vec![menu, actions::copy("example:copy", url.to_owned())]
    }
}

fn open_entry(score: i32) -> SearchResult {
    list::open_entry(ID, "Example", "Demo items and actions", Icon::Svg(phosphor_svgs::style::regular::SPARKLE), score)
}

fn item_row(item: &Item, score: i32) -> SearchResult {
    ListItem::new(item.name)
        .subtitle(item.detail)
        .icon(Icon::Svg(phosphor_svgs::style::regular::GLOBE))
        .accessory(item.url)
        .section(item.section)
        .build(format!("{ID}:item:{}", item.url), score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use corvo_core::ActionGroup;

    fn search(query: &str) -> Vec<SearchResult> {
        smol::block_on(<ExampleCommand as Command>::search(
            &ExampleCommand,
            query,
            &SearchContext::default(),
        ))
    }

    #[test]
    fn root_search_matches_a_keyword_and_rejects_the_rest() {
        assert_eq!(search("example")[0].id, "example:open");
        assert!(search("hello world").is_empty());
    }

    #[test]
    fn page_answers_from_the_catalog_with_sections() {
        let results = search("example-page:");
        assert!(results.len() >= 4);
        assert!(results.iter().all(|row| row.section.is_some()));

        let filtered = search("example-page:rust");
        assert!(filtered.iter().any(|row| row.title == "Rust"));
    }

    #[test]
    fn cache_fetches_once_and_pages_stay_on_cache() {
        let cache = cache();
        let _ = cache.get_or_fetch(std::time::Duration::from_secs(60), load_items);
        assert_eq!(cache.get(std::time::Duration::from_secs(60)).unwrap(), load_items());
    }

    #[test]
    fn execute_routes_items_and_rejects_malformed_ids() {
        assert_eq!(
            smol::block_on(<ExampleCommand as Command>::execute(
                &ExampleCommand,
                "example:item:https://rust-lang.org",
                &ExecutionContext::default(),
            ))
            .unwrap(),
            Action::OpenUrl("https://rust-lang.org".into())
        );
        assert_eq!(
            smol::block_on(<ExampleCommand as Command>::execute(
                &ExampleCommand,
                "example:unknown",
                &ExecutionContext::default(),
            ))
            .unwrap_err(),
            CommandError::NotFound
        );
        assert_eq!(
            smol::block_on(<ExampleCommand as Command>::execute(
                &ExampleCommand,
                "other:open",
                &ExecutionContext::default(),
            ))
            .unwrap_err(),
            CommandError::NotFound
        );
    }

    #[test]
    fn prefs_default_when_absent() {
        let prefs = prefs::load::<ExamplePrefs>(ID);
        assert!(prefs.favorite_section.is_empty());
    }

    #[test]
    fn actions_menu_builds_for_items() {
        let menu = <ExampleCommand as Command>::actions(
            &ExampleCommand,
            "example:item:https://rust-lang.org",
        );
        assert_eq!(menu.len(), 2);
        assert_eq!(menu[0].group, ActionGroup::Standard);
        assert!(matches!(menu[0].action, Action::OpenUrl(_)));
    }
}

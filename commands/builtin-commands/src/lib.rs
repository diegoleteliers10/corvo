//! Search results for the built-in launcher commands.
//!
//! Twelve Commands-pane rows had a Settings entry, an alias and a hotkey
//! but no search result at all: typing the alias found nothing and the row
//! could not be reached from the launcher. Only the seven extension page
//! entries had a command emitting them. This crate emits the other twelve
//! from `corvo_core::BUILTIN_COMMANDS`, plus the five feature-pane rows
//! from `corvo_core::FEATURE_COMMANDS`, so the lists cannot drift.

use corvo_core::{
    search_match_score, Action, Command, CommandError, ExecutionContext, Icon, SearchContext,
    SearchResult, BUILTIN_COMMANDS, FEATURE_COMMANDS,
};

#[derive(Default)]
pub struct BuiltinCommandsCommand;

corvo_core::register_command!(BuiltinCommandsCommand);

/// The entries this crate emits from the Commands pane table. The
/// extension page entries name another command as their owner, and that
/// command already emits them.
fn owned_entries() -> impl Iterator<Item = &'static corvo_core::BuiltinCommandDef> {
    BUILTIN_COMMANDS
        .iter()
        .filter(|def| def.result_id.starts_with("builtin-commands:"))
}

/// Whether the Settings pane a feature row belongs to is switched on.
fn section_enabled(settings: &corvo_config::Settings, section: &str) -> bool {
    match section {
        "clipboard" => settings.clipboard.enabled,
        "snippets" => settings.snippets.enabled,
        "navigation" => settings.navigation.enabled,
        "quicklinks" => settings.quicklinks.enabled,
        _ => true,
    }
}

#[async_trait::async_trait]
impl Command for BuiltinCommandsCommand {
    fn id(&self) -> &'static str {
        "builtin-commands"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["command", "corvo", "settings"]
    }

    fn priority(&self) -> u8 {
        70
    }

    async fn search(&self, query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        let q = query.trim().to_lowercase();
        let settings = corvo_config::Settings::load();
        if !settings.commands.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();
        for def in owned_entries() {
            // The Commands pane keys this section by command id.
            let config = settings.commands.items.get(def.id);
            if config.is_some_and(|c| c.hidden) {
                continue;
            }

            // Search the alias too, so typing it reaches the row without
            // relying on the rescue pass in the launcher.
            let alias = config.and_then(|c| c.alias.as_deref());
            let mut fields = vec![def.title];
            if let Some(alias) = alias {
                fields.push(alias);
            }
            fields.push(def.keywords);

            let score = if q.is_empty() {
                Some(900)
            } else {
                search_match_score(&q, &fields)
            };

            if let Some(score) = score {
                results.push(SearchResult {
                    id: def.result_id.to_string(),
                    title: def.title.to_string(),
                    subtitle: Some("Commands".into()),
                    icon: Icon::Svg(def.icon),
                    score,
                    accessory: Some("Command".into()),
                    section: None,
                    accessories: Vec::new(),
                });
            }
        }

        results.extend(feature_rows(q.is_empty(), &q, &settings));
        results.sort_by_key(|result| std::cmp::Reverse(result.score));
        results
    }

    /// Selecting one of these rows opens a page or runs a launcher action,
    /// which lives in the shell rather than here. The launcher checks the
    /// `builtin-commands:` prefix and dispatches through the same path a
    /// hotkey takes, so this only reports the fallthrough.
    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let _ = result_id;
        Err(CommandError::NotFound)
    }
}

/// The feature-pane rows. Each one is gated on its pane's toggle, hides
/// with its row checkbox, and matches its alias, the same three rules the
/// Commands-pane rows follow.
fn feature_rows(
    empty_query: bool,
    q: &str,
    settings: &corvo_config::Settings,
) -> Vec<SearchResult> {
    let mut results = Vec::new();
    for def in FEATURE_COMMANDS {
        if def.macos_only && !cfg!(target_os = "macos") {
            continue;
        }
        if !section_enabled(settings, def.section) {
            continue;
        }
        // The pane writes this row's config under its own id, in its own
        // section map. The emitted id resolves there through the
        // builtin-commands prefix stripping in corvo-config.
        let config = settings
            .result_item(def.result_id, def.title)
            .or_else(|| settings.result_item(def.id, def.title));
        if config.as_ref().is_some_and(|c| c.hidden) {
            continue;
        }
        let alias = config.and_then(|c| c.alias.as_deref());
        let mut fields = vec![def.title];
        if let Some(alias) = alias {
            fields.push(alias);
        }
        fields.push(def.keywords);

        let score = if empty_query {
            Some(900)
        } else {
            search_match_score(q, &fields)
        };
        if let Some(score) = score {
            results.push(SearchResult {
                id: def.result_id.to_string(),
                title: def.title.to_string(),
                subtitle: Some("Commands".into()),
                icon: Icon::Svg(def.icon),
                score,
                accessory: Some("Command".into()),
                section: None,
                accessories: Vec::new(),
            });
        }
    }
    results
}

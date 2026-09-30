use std::hash::{Hash, Hasher};
use std::sync::{OnceLock, RwLock};

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult, Snippet, search_match_score,
};

fn cached_snippets() -> &'static RwLock<Vec<Snippet>> {
    static CACHE: OnceLock<RwLock<Vec<Snippet>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
}

fn snippet_key(snippet: &Snippet) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    snippet.name.hash(&mut hasher);
    snippet.keyword.hash(&mut hasher);
    snippet.body.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Default)]
pub struct SnippetsCommand;

corvo_core::register_command!(SnippetsCommand);

#[async_trait::async_trait]
impl Command for SnippetsCommand {
    fn id(&self) -> &'static str {
        "snippets"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["snippet", "snippets", "text"]
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let Some(store) = ctx.store.as_ref() else {
            return Vec::new();
        };
        let q = query.trim().to_lowercase();
        let snippets = store.snippets();
        if let Ok(mut cache) = cached_snippets().write() {
            *cache = snippets.clone();
        }
        let mut results: Vec<SearchResult> = snippets
            .iter()
            .filter_map(|s| {
                let score = if q.is_empty() {
                    750
                } else {
                    search_match_score(
                        &q,
                        &[s.name.as_str(), s.keyword.as_deref().unwrap_or(""), s.body.as_str()],
                    )?
                };
                Some((s, score))
            })
            .map(|(s, score)| SearchResult {
                id: format!("snippets:{}", snippet_key(s)),
                title: s.name.clone(),
                subtitle: Some(s.body.clone()),
                icon: Icon::Snippet,
                score,
                accessory: s.keyword.clone(),
            })
            .collect();
        results.sort_by(|left, right| right.score.cmp(&left.score));
        results.truncate(ctx.max_results);
        results
    }

    async fn execute(&self, result_id: &str, ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let Some(index) = result_id.strip_prefix("snippets:") else {
            return Err(CommandError::NotFound);
        };
        let Some(store) = ctx.store.as_ref() else {
            return Err(CommandError::NotFound);
        };
        let snippets = store.snippets();
        let Some(snippet) = snippets.iter().find(|snippet| snippet_key(snippet) == index) else {
            return Err(CommandError::NotFound);
        };
        Ok(Action::PasteText(snippet.body.clone()))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(index) = result_id.strip_prefix("snippets:") else {
            return Vec::new();
        };
        let Ok(cache) = cached_snippets().read() else {
            return Vec::new();
        };
        let Some(snippet) = cache.iter().find(|snippet| snippet_key(snippet) == index) else {
            return Vec::new();
        };

        vec![
            CommandAction {
                id: "snippets-action:paste".into(),
                label: "Paste to Active App".into(),
                action: Action::PasteText(snippet.body.clone()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            },
            CommandAction {
                id: "snippets-action:copy".into(),
                label: "Copy Snippet".into(),
                action: Action::ShowToast(format!("copy:{}", snippet.body)),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Standard,
                hotkey: Some("cmd+enter"),
            },
        ]
    }
}

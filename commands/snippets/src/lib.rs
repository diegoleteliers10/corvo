use std::sync::{OnceLock, RwLock};

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult, Snippet,
};

fn cached_snippets() -> &'static RwLock<Vec<Snippet>> {
    static CACHE: OnceLock<RwLock<Vec<Snippet>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
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
        snippets
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                q.is_empty()
                    || s.name.to_lowercase().contains(&q)
                    || s.keyword.as_deref().is_some_and(|k| k.to_lowercase().contains(&q))
            })
            .take(ctx.max_results)
            .map(|(index, s)| SearchResult {
                id: format!("snippets:{index}"),
                title: s.name.clone(),
                subtitle: Some(s.body.clone()),
                icon: Icon::Snippet,
                score: if q.is_empty() { 75.0 } else { 80.0 },
                accessory: s.keyword.clone(),
            })
            .collect()
    }

    async fn execute(&self, result_id: &str, ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let Some(index) = result_id.strip_prefix("snippets:") else {
            return Err(CommandError::NotFound);
        };
        let Some(store) = ctx.store.as_ref() else {
            return Err(CommandError::NotFound);
        };
        let Ok(index) = index.parse::<usize>() else {
            return Err(CommandError::NotFound);
        };
        let snippets = store.snippets();
        let Some(snippet) = snippets.get(index) else {
            return Err(CommandError::NotFound);
        };
        Ok(Action::Copy(snippet.body.clone()))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(index) = result_id.strip_prefix("snippets:") else {
            return Vec::new();
        };
        let Ok(index) = index.parse::<usize>() else {
            return Vec::new();
        };
        let Ok(cache) = cached_snippets().read() else {
            return Vec::new();
        };
        let Some(snippet) = cache.get(index) else {
            return Vec::new();
        };

        vec![
            CommandAction {
                id: "snippets-action:paste".into(),
                label: "Paste to Active App".into(),
                action: Action::Copy(snippet.body.clone()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                group: ActionGroup::Primary,
                hotkey: Some("↵"),
            },
            CommandAction {
                id: "snippets-action:copy".into(),
                label: "Copy Snippet".into(),
                action: Action::ShowToast(format!("copy:{}", snippet.body)),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Standard,
                hotkey: Some("⌘↵"),
            },
        ]
    }
}

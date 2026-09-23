use std::sync::{OnceLock, RwLock};

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    Quicklink, SearchContext, SearchResult,
};

fn cached_quicklinks() -> &'static RwLock<Vec<Quicklink>> {
    static CACHE: OnceLock<RwLock<Vec<Quicklink>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
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
        links
            .iter()
            .enumerate()
            .filter(|(_, link)| q.is_empty() || link.name.to_lowercase().contains(&q))
            .take(ctx.max_results)
            .map(|(index, link)| SearchResult {
                id: format!("quicklinks:{index}"),
                title: link.name.clone(),
                subtitle: Some(link.url.clone()),
                icon: Icon::Link,
                score: if q.is_empty() { 70.0 } else { 80.0 },
                accessory: None,
            })
            .collect()
    }

    async fn execute(&self, result_id: &str, ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let Some(index) = result_id.strip_prefix("quicklinks:") else {
            return Err(CommandError::NotFound);
        };
        let Some(store) = ctx.store.as_ref() else {
            return Err(CommandError::NotFound);
        };
        let Ok(index) = index.parse::<usize>() else {
            return Err(CommandError::NotFound);
        };
        let links = store.quicklinks();
        let Some(link) = links.get(index) else {
            return Err(CommandError::NotFound);
        };
        Ok(Action::OpenUrl(link.url.clone()))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(index) = result_id.strip_prefix("quicklinks:") else {
            return Vec::new();
        };
        let Ok(index) = index.parse::<usize>() else {
            return Vec::new();
        };
        let Ok(cache) = cached_quicklinks().read() else {
            return Vec::new();
        };
        let Some(link) = cache.get(index) else {
            return Vec::new();
        };

        vec![
            CommandAction {
                id: "quicklinks-action:open".into(),
                label: "Open in Browser".into(),
                action: Action::OpenUrl(link.url.clone()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::ARROW_UP_RIGHT),
                group: ActionGroup::Primary,
                hotkey: Some("↵"),
            },
            CommandAction {
                id: "quicklinks-action:copy".into(),
                label: "Copy URL".into(),
                action: Action::ShowToast(format!("copy:{}", link.url)),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Standard,
                hotkey: Some("⌘↵"),
            },
        ]
    }
}

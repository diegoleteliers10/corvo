pub mod platform;
pub use platform::{get_system_actions, get_system_settings, ActionExecution, SystemActionDef, SystemSettingDef};

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult, search_match_score,
};

#[derive(Default)]
pub struct SystemActionsCommand;

corvo_core::register_command!(SystemActionsCommand);

fn map_execution_to_action(exec: &ActionExecution) -> Action {
    match exec {
        ActionExecution::RunShell(cmd) => Action::RunShell(cmd.clone()),
        ActionExecution::OpenUrl(url) => Action::OpenUrl(url.clone()),
        ActionExecution::AdjustBrightness(delta) => Action::AdjustBrightness(*delta),
    }
}

#[async_trait::async_trait]
impl Command for SystemActionsCommand {
    fn id(&self) -> &'static str {
        "system-actions"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "system", "settings", "preferences", "lock", "sleep", "shutdown", "restart",
            "trash", "dark", "mute", "volume", "brightness", "wifi", "bluetooth", "display",
        ]
    }

    fn priority(&self) -> u8 {
        75
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let q = query.trim().to_lowercase();
        let settings = corvo_config::Settings::load();
        let mut results = Vec::new();

        // System Actions
        if settings.system_actions.enabled {
            for action in get_system_actions() {
                let item_config = settings
                    .system_actions
                    .items
                    .get(action.id)
                    .or_else(|| settings.system_actions.items.get(action.title));

                if item_config.is_some_and(|c| c.hidden) {
                    continue;
                }

                let alias = item_config.and_then(|config| config.alias.as_deref());
                let mut fields = vec![action.title];
                if let Some(alias) = alias {
                    fields.push(alias);
                }
                fields.push(action.keywords);
                let score = if q.is_empty() {
                    Some(900)
                } else {
                    search_match_score(&q, &fields)
                };

                if let Some(score) = score {
                    results.push(SearchResult {
                        id: format!("system-actions:action:{}", action.id),
                        title: action.title.into(),
                        subtitle: Some("System Action".into()),
                        icon: Icon::Svg(action.icon),
                        score,
                        accessory: Some("System Action".into()),
                    });
                }
            }
        }

        // System Settings
        if settings.system_settings.enabled {
            for setting in get_system_settings() {
                let item_config = settings
                    .system_settings
                    .items
                    .get(setting.id)
                    .or_else(|| settings.system_settings.items.get(setting.title));

                if item_config.is_some_and(|c| c.hidden) {
                    continue;
                }

                let alias = item_config.and_then(|config| config.alias.as_deref());
                let mut fields = vec![setting.title];
                if let Some(alias) = alias {
                    fields.push(alias);
                }
                fields.push(setting.keywords);
                let score = if q.is_empty() {
                    Some(850)
                } else {
                    search_match_score(&q, &fields)
                };

                if let Some(score) = score {
                    results.push(SearchResult {
                        id: format!("system-actions:setting:{}", setting.id),
                        title: setting.title.into(),
                        subtitle: Some("System Settings".into()),
                        icon: Icon::Svg(setting.icon),
                        score,
                        accessory: Some("System Setting".into()),
                    });
                }
            }
        }

        results.sort_by_key(|a| std::cmp::Reverse(a.score));

        if !q.is_empty() {
            results.truncate(ctx.max_results);
        }
        results
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        if let Some(setting_id) = result_id.strip_prefix("system-actions:setting:") {
            if let Some(setting) = get_system_settings().into_iter().find(|s| s.id == setting_id) {
                return Ok(map_execution_to_action(&setting.execution));
            }
        } else if let Some(action_id) = result_id.strip_prefix("system-actions:action:") {
            if let Some(action) = get_system_actions().into_iter().find(|a| a.id == action_id) {
                return Ok(map_execution_to_action(&action.execution));
            }
        }
        Err(CommandError::NotFound)
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        if let Some(setting_id) = result_id.strip_prefix("system-actions:setting:") {
            if let Some(setting) = get_system_settings().into_iter().find(|s| s.id == setting_id) {
                return vec![CommandAction {
                    id: "system-actions:open-setting".into(),
                    label: "Open Setting".into(),
                    action: map_execution_to_action(&setting.execution),
                    icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::GEAR),
                    group: ActionGroup::Primary,
                    hotkey: Some("↵"),
                }];
            }
        } else if let Some(action_id) = result_id.strip_prefix("system-actions:action:") {
            if let Some(action) = get_system_actions().into_iter().find(|a| a.id == action_id) {
                return vec![CommandAction {
                    id: "system-actions:run-action".into(),
                    label: "Run Action".into(),
                    action: map_execution_to_action(&action.execution),
                    icon: Icon::Svg(action.icon),
                    group: ActionGroup::Primary,
                    hotkey: Some("↵"),
                }];
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_action_by_keyword() {
        let cmd = SystemActionsCommand;
        let ctx = SearchContext { max_results: 10, store: None };
        let results = smol::block_on(cmd.search("lock", &ctx));
        assert!(results.iter().any(|r| r.id == "system-actions:action:lock"));
    }

    #[test]
    fn finds_setting_by_name() {
        let cmd = SystemActionsCommand;
        let ctx = SearchContext { max_results: 10, store: None };
        let results = smol::block_on(cmd.search("displays", &ctx));
        assert!(results.iter().any(|r| r.id == "system-actions:setting:displays"));
    }

    #[test]
    fn execute_returns_action() {
        let cmd = SystemActionsCommand;
        let ctx = ExecutionContext::default();
        let action = smol::block_on(cmd.execute("system-actions:setting:sound", &ctx)).unwrap();
        match action {
            Action::OpenUrl(_) | Action::RunShell(_) => {}
            _ => panic!("expected OpenUrl or RunShell"),
        }
    }

    #[test]
    fn execute_returns_run_shell_for_action() {
        let cmd = SystemActionsCommand;
        let ctx = ExecutionContext::default();
        let action = smol::block_on(cmd.execute("system-actions:action:empty-trash", &ctx)).unwrap();
        match action {
            Action::RunShell(cmd) => assert!(!cmd.is_empty()),
            _ => panic!("expected RunShell"),
        }
    }
}

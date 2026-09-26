//! Focus and arrange windows through native platform scripting.

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult, search_match_score,
};

struct WindowActionDef {
    id: &'static str,
    title: &'static str,
    keywords: &'static str,
    icon: &'static str,
    hotkey: Option<&'static str>,
}

const WINDOW_ACTIONS: &[WindowActionDef] = &[
    WindowActionDef {
        id: "left-half",
        title: "Left Half",
        keywords: "left half tile window 50% split",
        icon: phosphor_svgs::style::regular::SIDEBAR_SIMPLE,
        hotkey: Some("⌥⌘←"),
    },
    WindowActionDef {
        id: "right-half",
        title: "Right Half",
        keywords: "right half tile window 50% split",
        icon: phosphor_svgs::style::regular::SIDEBAR_SIMPLE,
        hotkey: Some("⌥⌘→"),
    },
    WindowActionDef {
        id: "top-half",
        title: "Top Half",
        keywords: "top half tile window 50% split",
        icon: phosphor_svgs::style::regular::ROWS,
        hotkey: Some("⌥⌘↑"),
    },
    WindowActionDef {
        id: "bottom-half",
        title: "Bottom Half",
        keywords: "bottom half tile window 50% split",
        icon: phosphor_svgs::style::regular::ROWS,
        hotkey: Some("⌥⌘↓"),
    },
    // Thirds
    WindowActionDef {
        id: "first-third",
        title: "First Third",
        keywords: "first third left 33% tile window column",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("⌃⌥D"),
    },
    WindowActionDef {
        id: "center-third",
        title: "Center Third",
        keywords: "center third middle 33% tile window column",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("⌃⌥F"),
    },
    WindowActionDef {
        id: "last-third",
        title: "Last Third",
        keywords: "last third right 33% tile window column",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("⌃⌥G"),
    },
    WindowActionDef {
        id: "first-two-thirds",
        title: "First Two Thirds",
        keywords: "first two thirds 66% wide left tile window",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("⌃⌥E"),
    },
    WindowActionDef {
        id: "last-two-thirds",
        title: "Last Two Thirds",
        keywords: "last two thirds 66% wide right tile window",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("⌃⌥T"),
    },
    // Quarters
    WindowActionDef {
        id: "top-left",
        title: "Top Left Quarter",
        keywords: "top left quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("⌃⌥U"),
    },
    WindowActionDef {
        id: "top-right",
        title: "Top Right Quarter",
        keywords: "top right quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("⌃⌥I"),
    },
    WindowActionDef {
        id: "bottom-left",
        title: "Bottom Left Quarter",
        keywords: "bottom left quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("⌃⌥J"),
    },
    WindowActionDef {
        id: "bottom-right",
        title: "Bottom Right Quarter",
        keywords: "bottom right quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("⌃⌥K"),
    },
    // Whole Screen & Centering
    WindowActionDef {
        id: "maximize",
        title: "Maximize",
        keywords: "maximize full zoom window 100% fullscreen",
        icon: phosphor_svgs::style::regular::ARROWS_OUT,
        hotkey: Some("⌃⌥↵"),
    },
    WindowActionDef {
        id: "almost-maximize",
        title: "Almost Maximize",
        keywords: "almost maximize large window 90%",
        icon: phosphor_svgs::style::regular::ARROWS_OUT_SIMPLE,
        hotkey: None,
    },
    WindowActionDef {
        id: "center",
        title: "Center Window",
        keywords: "center window middle screen align",
        icon: phosphor_svgs::style::regular::FRAME_CORNERS,
        hotkey: Some("⌥⌘C"),
    },
    WindowActionDef {
        id: "restore",
        title: "Restore Previous Size",
        keywords: "restore previous window size undo revert frame",
        icon: phosphor_svgs::style::regular::ARROW_COUNTER_CLOCKWISE,
        hotkey: Some("⌃⌥⌫"),
    },
    // Multi-Display
    WindowActionDef {
        id: "next-display",
        title: "Move to Next Display",
        keywords: "next display monitor screen move window switch",
        icon: phosphor_svgs::style::regular::DESKTOP,
        hotkey: Some("⌃⌥⌘→"),
    },
    WindowActionDef {
        id: "prev-display",
        title: "Move to Previous Display",
        keywords: "previous prev display monitor screen move window switch",
        icon: phosphor_svgs::style::regular::DESKTOP,
        hotkey: Some("⌃⌥⌘←"),
    },
];

#[derive(Default)]
pub struct WindowManagementCommand;

corvo_core::register_command!(WindowManagementCommand);

#[async_trait::async_trait]
impl Command for WindowManagementCommand {
    fn id(&self) -> &'static str {
        "window-management"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "window", "windows", "focus", "tile", "layout", "left", "right", "half",
            "maximize", "center", "split",
        ]
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let q = query.trim().to_lowercase();
        let mut results = Vec::new();
        for action in WINDOW_ACTIONS {
            let score = if q.is_empty() {
                Some(800)
            } else {
                search_match_score(&q, &[action.title, action.keywords])
            };
            if let Some(score) = score {
                results.push(SearchResult {
                    id: format!("window-management:{}", action.id),
                    title: action.title.into(),
                    subtitle: Some("Window Management".into()),
                    icon: Icon::Svg(action.icon),
                    score,
                    accessory: Some(action.hotkey.unwrap_or("Window Action").into()),
                });
            }
        }
        if !q.is_empty() {
            results.sort_by(|left, right| right.score.cmp(&left.score));
            results.truncate(ctx.max_results);
        }
        results
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let Some(action_id) = result_id.strip_prefix("window-management:") else {
            return Err(CommandError::NotFound);
        };
        if WINDOW_ACTIONS.iter().any(|a| a.id == action_id) {
            Ok(Action::TileWindow(action_id.to_string()))
        } else {
            Err(CommandError::NotFound)
        }
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(action_id) = result_id.strip_prefix("window-management:") else {
            return Vec::new();
        };
        if let Some(action) = WINDOW_ACTIONS.iter().find(|a| a.id == action_id) {
            vec![CommandAction {
                id: "window-management:apply".into(),
                label: format!("Apply {}", action.title),
                action: Action::TileWindow(action.id.to_string()),
                icon: Icon::Svg(action.icon),
                group: ActionGroup::Primary,
                hotkey: Some("↵"),
            }]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_action_by_keyword() {
        let cmd = WindowManagementCommand;
        let ctx = SearchContext { max_results: 10, store: None };
        let results = smol::block_on(cmd.search("left", &ctx));
        assert!(results.iter().any(|r| r.id == "window-management:left-half"));
    }

    #[test]
    fn finds_action_by_title() {
        let cmd = WindowManagementCommand;
        let ctx = SearchContext { max_results: 10, store: None };
        let results = smol::block_on(cmd.search("maximize", &ctx));
        assert!(results.iter().any(|r| r.id == "window-management:maximize"));
    }

    #[test]
    fn empty_query_returns_all() {
        let cmd = WindowManagementCommand;
        let ctx = SearchContext { max_results: 10, store: None };
        let results = smol::block_on(cmd.search("", &ctx));
        assert_eq!(results.len(), WINDOW_ACTIONS.len());
    }

    #[test]
    fn execute_left_half() {
        let cmd = WindowManagementCommand;
        let ctx = ExecutionContext::default();
        let action = smol::block_on(cmd.execute("window-management:left-half", &ctx)).unwrap();
        assert_eq!(action, Action::TileWindow("left-half".into()));
    }
}

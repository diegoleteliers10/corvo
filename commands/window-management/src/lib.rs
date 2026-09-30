//! Focus and arrange windows through native platform scripting.

use std::cmp::Reverse;

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult, search_match_score,
};

pub struct WindowActionDef {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: &'static str,
    pub icon: &'static str,
    pub hotkey: Option<&'static str>,
}

pub const WINDOW_ACTIONS: &[WindowActionDef] = &[
    WindowActionDef {
        id: "left-half",
        title: "Left Half",
        keywords: "left half tile window 50% split",
        icon: phosphor_svgs::style::regular::SIDEBAR_SIMPLE,
        hotkey: Some("alt+cmd+←"),
    },
    WindowActionDef {
        id: "right-half",
        title: "Right Half",
        keywords: "right half tile window 50% split",
        icon: phosphor_svgs::style::regular::SIDEBAR_SIMPLE,
        hotkey: Some("alt+cmd+→"),
    },
    WindowActionDef {
        id: "top-half",
        title: "Top Half",
        keywords: "top half tile window 50% split",
        icon: phosphor_svgs::style::regular::ROWS,
        hotkey: Some("alt+cmd+↑"),
    },
    WindowActionDef {
        id: "bottom-half",
        title: "Bottom Half",
        keywords: "bottom half tile window 50% split",
        icon: phosphor_svgs::style::regular::ROWS,
        hotkey: Some("alt+cmd+↓"),
    },
    // Thirds
    WindowActionDef {
        id: "first-third",
        title: "First Third",
        keywords: "first third left 33% tile window column",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("ctrl+alt+D"),
    },
    WindowActionDef {
        id: "center-third",
        title: "Center Third",
        keywords: "center third middle 33% tile window column",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("ctrl+alt+F"),
    },
    WindowActionDef {
        id: "last-third",
        title: "Last Third",
        keywords: "last third right 33% tile window column",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("ctrl+alt+G"),
    },
    WindowActionDef {
        id: "first-two-thirds",
        title: "First Two Thirds",
        keywords: "first two thirds 66% wide left tile window",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("ctrl+alt+E"),
    },
    WindowActionDef {
        id: "last-two-thirds",
        title: "Last Two Thirds",
        keywords: "last two thirds 66% wide right tile window",
        icon: phosphor_svgs::style::regular::COLUMNS,
        hotkey: Some("ctrl+alt+T"),
    },
    // Quarters
    WindowActionDef {
        id: "top-left",
        title: "Top Left Quarter",
        keywords: "top left quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("ctrl+alt+U"),
    },
    WindowActionDef {
        id: "top-right",
        title: "Top Right Quarter",
        keywords: "top right quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("ctrl+alt+I"),
    },
    WindowActionDef {
        id: "bottom-left",
        title: "Bottom Left Quarter",
        keywords: "bottom left quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("ctrl+alt+J"),
    },
    WindowActionDef {
        id: "bottom-right",
        title: "Bottom Right Quarter",
        keywords: "bottom right quarter 25% corner corner tile",
        icon: phosphor_svgs::style::regular::SQUARES_FOUR,
        hotkey: Some("ctrl+alt+K"),
    },
    // Whole Screen & Centering
    WindowActionDef {
        id: "maximize",
        title: "Maximize",
        keywords: "maximize full zoom window 100% fullscreen",
        icon: phosphor_svgs::style::regular::ARROWS_OUT,
        hotkey: Some("ctrl+alt+enter"),
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
        hotkey: Some("alt+cmd+C"),
    },
    WindowActionDef {
        id: "restore",
        title: "Restore Previous Size",
        keywords: "restore previous window size undo revert frame",
        icon: phosphor_svgs::style::regular::ARROW_COUNTER_CLOCKWISE,
        hotkey: Some("ctrl+alt+backspace"),
    },
    // Multi-Display
    WindowActionDef {
        id: "next-display",
        title: "Move to Next Display",
        keywords: "next display monitor screen move window switch",
        icon: phosphor_svgs::style::regular::DESKTOP,
        hotkey: Some("ctrl+alt+cmd+→"),
    },
    WindowActionDef {
        id: "prev-display",
        title: "Move to Previous Display",
        keywords: "previous prev display monitor screen move window switch",
        icon: phosphor_svgs::style::regular::DESKTOP,
        hotkey: Some("ctrl+alt+cmd+←"),
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
        let settings = corvo_config::Settings::load();
        let mut results = Vec::new();

        // 1. Window Layouts (if enabled)
        if settings.window_management.show_layouts_in_launcher {
            for layout in &settings.window_management.layouts {
                let score = if q.is_empty() {
                    Some(850)
                } else {
                    search_match_score(&q, &[&layout.name, "layout", "window layout"])
                };
                if let Some(score) = score {
                    let subtitle = if layout.placements.is_empty() {
                        "Window Layout".to_string()
                    } else {
                        layout
                            .placements
                            .iter()
                            .map(|p| format!("{}: {}", p.app_name, p.position))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    results.push(SearchResult {
                        id: format!("window-management:layout:{}", layout.id),
                        title: format!("Apply Layout: {}", layout.name),
                        subtitle: Some(subtitle),
                        icon: Icon::Svg(phosphor_svgs::style::regular::SQUARES_FOUR),
                        score,
                        accessory: layout.hotkey.clone(),
                    });
                }
            }
        }

        // 2. Standard Window Actions
        for action in WINDOW_ACTIONS {
            let score = if q.is_empty() {
                Some(800)
            } else {
                search_match_score(&q, &[action.title, action.keywords])
            };
            if let Some(score) = score {
                let hotkey = settings
                    .window_management
                    .command_items
                    .get(action.id)
                    .and_then(|c| c.hotkey.as_deref())
                    .or(action.hotkey);

                results.push(SearchResult {
                    id: format!("window-management:{}", action.id),
                    title: action.title.into(),
                    subtitle: Some("Window Management".into()),
                    icon: Icon::Svg(action.icon),
                    score,
                    accessory: Some(hotkey.unwrap_or("Window Action").into()),
                });
            }
        }

        if !q.is_empty() {
            results.sort_by_key(|result| Reverse(result.score));
            results.truncate(ctx.max_results);
        }
        results
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let Some(suffix) = result_id.strip_prefix("window-management:") else {
            return Err(CommandError::NotFound);
        };
        if let Some(layout_id) = suffix.strip_prefix("layout:") {
            let settings = corvo_config::Settings::load();
            if let Some(layout) = settings
                .window_management
                .layouts
                .iter()
                .find(|l| l.id == layout_id)
            {
                let placements: Vec<(String, String)> = layout
                    .placements
                    .iter()
                    .map(|p| (p.app_name.clone(), p.position.clone()))
                    .collect();
                let _ = corvo_platform::apply_window_layout(&placements);
                return Ok(Action::CloseWindow);
            }
        }
        if WINDOW_ACTIONS.iter().any(|a| a.id == suffix) {
            Ok(Action::TileWindow(suffix.to_string()))
        } else {
            Err(CommandError::NotFound)
        }
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(suffix) = result_id.strip_prefix("window-management:") else {
            return Vec::new();
        };
        if let Some(layout_id) = suffix.strip_prefix("layout:") {
            let settings = corvo_config::Settings::load();
            if let Some(layout) = settings
                .window_management
                .layouts
                .iter()
                .find(|l| l.id == layout_id)
            {
                return vec![CommandAction {
                    id: "window-management:apply-layout".into(),
                    label: format!("Apply {}", layout.name),
                    action: Action::TileWindow(format!("layout:{}", layout.id)),
                    icon: Icon::Svg(phosphor_svgs::style::regular::SQUARES_FOUR),
                    group: ActionGroup::Primary,
                    hotkey: Some("enter"),
                }];
            }
        }
        if let Some(action) = WINDOW_ACTIONS.iter().find(|a| a.id == suffix) {
            vec![CommandAction {
                id: "window-management:apply".into(),
                label: format!("Apply {}", action.title),
                action: Action::TileWindow(action.id.to_string()),
                icon: Icon::Svg(action.icon),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
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

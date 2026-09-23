use gpui::{
    div, prelude::*, px, rgb, size, AnyElement, App, AppContext, Bounds, ClickEvent, Context,
    Div, FontWeight, Global, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};

const COLOR_BG: u32 = 0x1b1c1e;
const COLOR_SIDEBAR_BG: u32 = 0x161718;
const COLOR_DIVIDER: u32 = 0x2a2b2e;
const COLOR_ROW_SELECTED: u32 = 0x2e3034;
const COLOR_TEXT: u32 = 0xffffff;
const COLOR_TEXT_DIM: u32 = 0x8a8b90;
const COLOR_ACCENT_BLUE: u32 = 0x2563eb;
const COLOR_TOGGLE_OFF: u32 = 0x3f3f46;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Permissions,
    Applications,
    SystemSettings,
    SystemActions,
    Commands,
    Quicklinks,
    AppleShortcuts,
    Fallbacks,
    Clipboard,
    Snippets,
    FileSearch,
    WindowManagement,
    Navigation,
    Notes,
    Calendar,
}

impl SettingsTab {
    fn title(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Permissions => "Permissions",
            Self::Applications => "Applications",
            Self::SystemSettings => "System Settings",
            Self::SystemActions => "System Actions",
            Self::Commands => "Commands",
            Self::Quicklinks => "Quicklinks",
            Self::AppleShortcuts => "Apple Shortcuts",
            Self::Fallbacks => "Fallbacks",
            Self::Clipboard => "Clipboard",
            Self::Snippets => "Snippets",
            Self::FileSearch => "File Search",
            Self::WindowManagement => "Window Management",
            Self::Navigation => "Navigation",
            Self::Notes => "Notes",
            Self::Calendar => "Calendar",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Self::General => phosphor_svgs::style::regular::SLIDERS,
            Self::Permissions => phosphor_svgs::style::regular::SHIELD_CHECK,
            Self::Applications => phosphor_svgs::style::regular::SQUARES_FOUR,
            Self::SystemSettings => phosphor_svgs::style::regular::GEAR,
            Self::SystemActions => phosphor_svgs::style::regular::LIGHTNING,
            Self::Commands => phosphor_svgs::style::regular::TERMINAL_WINDOW,
            Self::Quicklinks => phosphor_svgs::style::regular::LINK,
            Self::AppleShortcuts => phosphor_svgs::style::regular::STACK,
            Self::Fallbacks => phosphor_svgs::style::regular::ARROW_BEND_DOWN_RIGHT,
            Self::Clipboard => phosphor_svgs::style::regular::CLIPBOARD_TEXT,
            Self::Snippets => phosphor_svgs::style::regular::CODE,
            Self::FileSearch => phosphor_svgs::style::regular::MAGNIFYING_GLASS,
            Self::WindowManagement => phosphor_svgs::style::regular::SIDEBAR_SIMPLE,
            Self::Navigation => phosphor_svgs::style::regular::ARROWS_OUT_CARDINAL,
            Self::Notes => phosphor_svgs::style::regular::NOTE,
            Self::Calendar => phosphor_svgs::style::regular::CALENDAR,
        }
    }
}

pub struct SettingsView {
    selected_tab: SettingsTab,
    search_query: String,
    launch_at_login: bool,
    show_menu_bar: bool,
    compact_mode: bool,
    pop_to_root_option: usize,
    escape_behavior_option: usize,
    auto_switch_input: usize,
    theme_option: usize,
    interface_size_option: usize,
    transparency_level: usize, // 0..=4
}

impl SettingsView {
    pub fn new() -> Self {
        Self {
            selected_tab: SettingsTab::General,
            search_query: String::new(),
            launch_at_login: false,
            show_menu_bar: true,
            compact_mode: false,
            pop_to_root_option: 1, // "After 90 seconds"
            escape_behavior_option: 0, // "Navigate back or close window"
            auto_switch_input: 0, // "None"
            theme_option: 0, // "System"
            interface_size_option: 1, // Medium
            transparency_level: 2, // Middle slider position
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let sections: &[(&str, &[SettingsTab])] = &[
            ("General", &[SettingsTab::General, SettingsTab::Permissions]),
            (
                "Launcher",
                &[
                    SettingsTab::Applications,
                    SettingsTab::SystemSettings,
                    SettingsTab::SystemActions,
                    SettingsTab::Commands,
                    SettingsTab::Quicklinks,
                    SettingsTab::AppleShortcuts,
                    SettingsTab::Fallbacks,
                ],
            ),
            (
                "Features",
                &[
                    SettingsTab::Clipboard,
                    SettingsTab::Snippets,
                    SettingsTab::FileSearch,
                    SettingsTab::WindowManagement,
                    SettingsTab::Navigation,
                    SettingsTab::Notes,
                    SettingsTab::Calendar,
                ],
            ),
        ];

        let query = self.search_query.to_lowercase();

        let mut section_views: Vec<AnyElement> = Vec::new();
        for (sec_title, tabs) in sections {
            let filtered_tabs: Vec<_> = tabs
                .iter()
                .copied()
                .filter(|tab| query.is_empty() || tab.title().to_lowercase().contains(&query))
                .collect();

            if filtered_tabs.is_empty() {
                continue;
            }

            section_views.push(
                div()
                    .pt_3()
                    .pb_1()
                    .px_3()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child(*sec_title)
                    .into_any_element(),
            );

            for tab in filtered_tabs {
                let is_selected = tab == self.selected_tab;
                section_views.push(
                    div()
                        .id(SharedString::from(format!("tab-{}", tab.title())))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_1p5()
                        .mx_2()
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_selected, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
                        .hover(|style| style.bg(rgb(COLOR_ROW_SELECTED)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.selected_tab = tab;
                            cx.notify();
                        }))
                        .child(crate::icons::render_phosphor_svg(
                            tab.icon(),
                            if is_selected { rgb(0x60a5fa) } else { rgb(COLOR_TEXT_DIM) },
                            15.0,
                        ))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(if is_selected { rgb(COLOR_TEXT) } else { rgb(0xd1d5db) })
                                .child(tab.title()),
                        )
                        .into_any_element(),
                );
            }
        }

        div()
            .flex_none()
            .w(px(210.0))
            .h_full()
            .bg(rgb(COLOR_SIDEBAR_BG))
            .border_r_1()
            .border_color(rgb(COLOR_DIVIDER))
            .flex()
            .flex_col()
            .child(
                // Traffic light row + search input
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        // Traffic light buttons
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("traffic-red")
                                    .size(px(12.0))
                                    .rounded_full()
                                    .bg(rgb(0xff5f56))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|_this, _: &ClickEvent, window, _cx| {
                                        window.remove_window();
                                    })),
                            )
                            .child(div().size(px(12.0)).rounded_full().bg(rgb(0xffbd2e)))
                            .child(div().size(px(12.0)).rounded_full().bg(rgb(0x27c93f))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1p5()
                            .rounded_md()
                            .bg(rgb(0x232427))
                            .border_1()
                            .border_color(rgb(COLOR_DIVIDER))
                            .child(crate::icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                rgb(COLOR_TEXT_DIM),
                                13.0,
                            ))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child("Search"),
                            ),
                    ),
            )
            .child(
                div()
                    .id("settings-sidebar-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(section_views),
            )
    }

    fn render_content(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .h_full()
            .bg(rgb(COLOR_BG))
            .flex()
            .flex_col()
            .child(
                // Navigation header
                div()
                    .h(px(48.0))
                    .px_6()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(rgb(COLOR_DIVIDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child("‹"),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child("›"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child(self.selected_tab.title()),
                    ),
            )
            .child(
                div()
                    .id("settings-content-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_6()
                    .child(match self.selected_tab {
                        SettingsTab::General => self.render_general_pane(cx),
                        SettingsTab::Permissions => self.render_permissions_pane(cx),
                        _ => self.render_placeholder_pane(self.selected_tab),
                    }),
            )
    }

    fn render_general_pane(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_6()
            // Global Shortcuts
            .child(
                self.section_group(
                    "Global Shortcuts",
                    vec![self.shortcut_row("App Launcher", "⌘ Space")],
                ),
            )
            // General
            .child(
                self.section_group(
                    "General",
                    vec![
                        self.toggle_row(
                            "toggle-launch-at-login",
                            "Launch at login",
                            None,
                            self.launch_at_login,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.launch_at_login = !this.launch_at_login;
                                cx.notify();
                            }),
                        ),
                        self.toggle_row(
                            "toggle-show-menu-bar",
                            "Show in menu bar",
                            Some("Shortcuts still work when hidden."),
                            self.show_menu_bar,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.show_menu_bar = !this.show_menu_bar;
                                cx.notify();
                            }),
                        ),
                        self.dropdown_row(
                            "Pop to Root Search",
                            Some("After the launcher closes."),
                            match self.pop_to_root_option {
                                0 => "Immediately",
                                1 => "After 90 seconds",
                                _ => "Never",
                            },
                        ),
                        self.dropdown_row(
                            "Escape Key Behavior",
                            Some("When the search field is empty."),
                            match self.escape_behavior_option {
                                0 => "Navigate back or close window",
                                _ => "Close window",
                            },
                        ),
                        self.dropdown_row(
                            "Auto-switch input source",
                            Some("While the launcher is open."),
                            match self.auto_switch_input {
                                0 => "None",
                                _ => "ABC",
                            },
                        ),
                    ],
                ),
            )
            // Appearance
            .child(
                self.section_group(
                    "Appearance",
                    vec![
                        self.dropdown_row("Theme", None, match self.theme_option {
                            0 => "System",
                            1 => "Dark",
                            _ => "Light",
                        }),
                        self.segmented_row(
                            "Interface size",
                            Some("Scales the launcher and its panels, not Settings."),
                            self.interface_size_option,
                            cx,
                        ),
                        self.slider_row(
                            "Background transparency",
                            self.transparency_level,
                            cx,
                        ),
                        self.toggle_row(
                            "toggle-compact-mode",
                            "Compact mode",
                            Some("A slim search bar that expands as you type."),
                            self.compact_mode,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.compact_mode = !this.compact_mode;
                                cx.notify();
                            }),
                        ),
                    ],
                ),
            )
    }

    fn render_permissions_pane(&self, _cx: &mut Context<Self>) -> Div {
        let is_trusted = corvo_platform::is_accessibility_trusted(false);
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                self.section_group(
                    "System Permissions",
                    vec![
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .py_2()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("Accessibility"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child("Required for Auto-Paste and Window Management."),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .bg(if is_trusted { rgb(0x14532d) } else { rgb(0x7f1d1d) })
                                            .text_size(px(11.0))
                                            .text_color(if is_trusted { rgb(0x86efac) } else { rgb(0xfca5a5) })
                                            .child(if is_trusted { "Granted" } else { "Not Granted" }),
                                    )
                                    .child(
                                        div()
                                            .id("request-permission-btn")
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(0x27272a))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0x3f3f46)))
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT))
                                            .on_click(|_: &ClickEvent, _window, _cx| {
                                                corvo_platform::is_accessibility_trusted(true);
                                            })
                                            .child("Request Permission"),
                                    ),
                            ),
                    ],
                ),
            )
    }

    fn render_placeholder_pane(&self, tab: SettingsTab) -> Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py_12()
            .gap_3()
            .child(crate::icons::render_phosphor_svg(
                tab.icon(),
                rgb(COLOR_TEXT_DIM),
                32.0,
            ))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child(tab.title()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child(format!("Configuration options for {} are enabled by default.", tab.title())),
            )
    }

    fn section_group(&self, title: &'static str, rows: Vec<Div>) -> Div {
        let mut row_children = Vec::new();
        for (i, row) in rows.into_iter().enumerate() {
            if i > 0 {
                row_children.push(div().h(px(1.0)).bg(rgb(COLOR_DIVIDER)));
            }
            row_children.push(row);
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child(title),
            )
            .child(
                div()
                    .rounded_lg()
                    .bg(rgb(0x202225))
                    .border_1()
                    .border_color(rgb(COLOR_DIVIDER))
                    .px_4()
                    .py_2()
                    .flex()
                    .flex_col()
                    .children(row_children),
            )
    }

    fn shortcut_row(&self, title: &str, shortcut: &str) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py_2p5()
            .child(div().text_size(px(13.0)).text_color(rgb(COLOR_TEXT)).child(title.to_string()))
            .child(
                div()
                    .px_2p5()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x2e3034))
                    .border_1()
                    .border_color(rgb(COLOR_DIVIDER))
                    .text_size(px(12.0))
                    .text_color(rgb(COLOR_TEXT))
                    .child(shortcut.to_string()),
            )
    }

    fn toggle_row(
        &self,
        id: &'static str,
        title: &str,
        subtitle: Option<&str>,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py_2p5()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_size(px(13.0)).text_color(rgb(COLOR_TEXT)).child(title.to_string()))
                    .when_some(subtitle, |el, sub| {
                        el.child(div().text_size(px(12.0)).text_color(rgb(COLOR_TEXT_DIM)).child(sub.to_string()))
                    }),
            )
            .child(
                // iOS / macOS style toggle switch
                div()
                    .id(id)
                    .cursor_pointer()
                    .w(px(38.0))
                    .h(px(22.0))
                    .rounded_full()
                    .bg(if active { rgb(COLOR_ACCENT_BLUE) } else { rgb(COLOR_TOGGLE_OFF) })
                    .p(px(2.0))
                    .flex()
                    .items_center()
                    .when(active, |t| t.justify_end())
                    .when(!active, |t| t.justify_start())
                    .on_click(on_click)
                    .child(
                        div()
                            .size(px(18.0))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .shadow_sm(),
                    ),
            )
    }

    fn dropdown_row(&self, title: &str, subtitle: Option<&str>, value: &str) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py_2p5()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_size(px(13.0)).text_color(rgb(COLOR_TEXT)).child(title.to_string()))
                    .when_some(subtitle, |el, sub| {
                        el.child(div().text_size(px(12.0)).text_color(rgb(COLOR_TEXT_DIM)).child(sub.to_string()))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2p5()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x232427))
                    .border_1()
                    .border_color(rgb(COLOR_DIVIDER))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xd1d5db))
                            .child(value.to_string()),
                    )
                    .child(crate::icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::CARET_UP_DOWN,
                        rgb(COLOR_TEXT_DIM),
                        11.0,
                    )),
            )
    }

    fn segmented_row(
        &self,
        title: &str,
        subtitle: Option<&str>,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py_2p5()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_size(px(13.0)).text_color(rgb(COLOR_TEXT)).child(title.to_string()))
                    .when_some(subtitle, |el, sub| {
                        el.child(div().text_size(px(12.0)).text_color(rgb(COLOR_TEXT_DIM)).child(sub.to_string()))
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .p(px(2.0))
                    .rounded_md()
                    .bg(rgb(0x161719))
                    .border_1()
                    .border_color(rgb(COLOR_DIVIDER))
                    .children([("Aa", px(11.0)), ("Aa", px(13.0)), ("Aa", px(15.0))].into_iter().enumerate().map(|(idx, (label, sz))| {
                        let active = idx == selected;
                        div()
                            .id(SharedString::from(format!("segmented-size-{idx}")))
                            .cursor_pointer()
                            .px_2p5()
                            .py_0p5()
                            .rounded_sm()
                            .when(active, |el| el.bg(rgb(0x3a3c40)))
                            .text_size(sz)
                            .text_color(if active { rgb(COLOR_TEXT) } else { rgb(COLOR_TEXT_DIM) })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.interface_size_option = idx;
                                cx.notify();
                            }))
                            .child(label)
                    })),
            )
    }

    fn slider_row(&self, title: &str, level: usize, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py_2p5()
            .child(div().text_size(px(13.0)).text_color(rgb(COLOR_TEXT)).child(title.to_string()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_size(px(11.0)).text_color(rgb(COLOR_TEXT_DIM)).child("Less"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .children((0..5).map(|idx| {
                                let active = idx <= level;
                                div()
                                    .id(SharedString::from(format!("transparency-dot-{idx}")))
                                    .cursor_pointer()
                                    .size(px(8.0))
                                    .rounded_full()
                                    .bg(if active { rgb(0x60a5fa) } else { rgb(0x3f3f46) })
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                        this.transparency_level = idx;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(div().text_size(px(11.0)).text_color(rgb(COLOR_TEXT_DIM)).child("More"))
                    .child(
                        div()
                            .id("reset-transparency-btn")
                            .cursor_pointer()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(rgb(0x28292c))
                            .border_1()
                            .border_color(rgb(COLOR_DIVIDER))
                            .text_size(px(11.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .hover(|s| s.text_color(rgb(COLOR_TEXT)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.transparency_level = 2;
                                cx.notify();
                            }))
                            .child("Reset"),
                    ),
            )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .bg(rgb(COLOR_BG))
            .text_color(rgb(COLOR_TEXT))
            .font_family("Helvetica")
            .child(self.render_sidebar(cx))
            .child(self.render_content(cx))
    }
}

struct SettingsWindow(WindowHandle<SettingsView>);
impl Global for SettingsWindow {}

/// Opens the Settings and Preferences window.
pub fn open_settings(cx: &mut App) {
    if let Some(handle) = cx.try_global::<SettingsWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |_view, window, _cx| {
                window.activate_window();
            });
            return;
        }
    }

    let display_id = corvo_platform::active_display_id().map(|id| gpui::DisplayId::new(id as u64));
    let window_size = size(px(780.0), px(540.0));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(display_id, window_size, cx))),
        display_id,
        titlebar: None,
        kind: WindowKind::Normal,
        is_resizable: true,
        is_movable: true,
        focus: true,
        show: true,
        ..Default::default()
    };

    let opened = cx.open_window(options, |_window, cx| {
        cx.new(|_cx| SettingsView::new())
    });

    if let Ok(handle) = opened {
        cx.set_global(SettingsWindow(handle));
    }
}

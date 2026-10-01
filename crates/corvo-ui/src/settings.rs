use gpui::{
    div, img, prelude::*, px, rgb, rgba, size, AnyElement, App, AppContext, Bounds, ClickEvent,
    Context, Div, FocusHandle, FontWeight, Global, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Render, ScrollHandle, SharedString, Stateful, Styled, Window,
    WindowBounds, WindowControlArea, WindowHandle, WindowKind, WindowOptions,
};

// Corvo Settings Palette — Deep dark graphite aesthetic with emerald accents and crisp optical contrast
pub(crate) const COLOR_BG: u32 = 0x17181a;
pub(crate) const COLOR_SIDEBAR_BG: u32 = 0x131416;
pub(crate) const COLOR_CARD_BG: u32 = 0x1d1e22;
pub(crate) const COLOR_DIVIDER: u32 = 0x282a2d;
pub(crate) const COLOR_BORDER_SUBTLE: u32 = 0x242629;
pub(crate) const COLOR_ROW_SELECTED: u32 = 0x1a3329; // Subtle dark emerald tint
pub(crate) const COLOR_ROW_HOVER: u32 = 0x202226;
pub(crate) const COLOR_TEXT: u32 = 0xffffff;
pub(crate) const COLOR_TEXT_MUTED: u32 = 0xd1d5db;
pub(crate) const COLOR_TEXT_DIM: u32 = 0x8e8e93;
pub(crate) const COLOR_ACCENT: u32 = 0x34d399; // Corvo Emerald Accent
pub(crate) const COLOR_TOGGLE_OFF: u32 = 0x32353b;
pub(crate) const COLOR_CONTROL_BG: u32 = 0x222428;
pub(crate) const COLOR_CONTROL_HOVER: u32 = 0x2c2f35;
pub(crate) const COLOR_CONTROL_BORDER: u32 = 0x33363c;
pub(crate) const COLOR_DESTRUCTIVE: u32 = 0xef4444;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Permissions,
    Applications,
    SystemSettings,
    SystemActions,
    Commands,
    Quicklinks,
    Fallbacks,
    Clipboard,
    Snippets,
    FileSearch,
    WindowManagement,
    Navigation,
    Calendar,
    Emojis,
    Extensions,
    Backup,
    About,
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
            Self::Fallbacks => "Fallbacks",
            Self::Clipboard => "Clipboard",
            Self::Snippets => "Snippets",
            Self::FileSearch => "File Search",
            Self::WindowManagement => "Window Management",
            Self::Navigation => "Navigation",
            Self::Calendar => "Calendar",
            Self::Emojis => "Emoji & Symbols",
            Self::Extensions => "Extensions",
            Self::Backup => "Backup",
            Self::About => "About",
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
            Self::Fallbacks => phosphor_svgs::style::regular::ARROW_BEND_DOWN_RIGHT,
            Self::Clipboard => phosphor_svgs::style::regular::CLIPBOARD_TEXT,
            Self::Snippets => phosphor_svgs::style::regular::CODE,
            Self::FileSearch => phosphor_svgs::style::regular::MAGNIFYING_GLASS,
            Self::WindowManagement => phosphor_svgs::style::regular::SIDEBAR_SIMPLE,
            Self::Navigation => phosphor_svgs::style::regular::ARROWS_OUT_CARDINAL,
            Self::Calendar => phosphor_svgs::style::regular::CALENDAR,
            Self::Emojis => phosphor_svgs::style::regular::SMILEY,
            Self::Extensions => phosphor_svgs::style::regular::STOREFRONT,
            Self::Backup => phosphor_svgs::style::regular::FLOPPY_DISK,
            Self::About => phosphor_svgs::style::regular::INFO,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinCommandDef {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
}

pub fn get_builtin_commands() -> &'static [BuiltinCommandDef] {
    &[
        BuiltinCommandDef {
            id: "about-corvo",
            title: "About Corvo",
            icon: phosphor_svgs::style::regular::INFO,
        },
        BuiltinCommandDef {
            id: "calculator-history",
            title: "Calculator History",
            icon: phosphor_svgs::style::regular::PLUS_MINUS,
        },
        BuiltinCommandDef {
            id: "check-for-updates",
            title: "Check for Updates",
            icon: phosphor_svgs::style::regular::ARROW_CIRCLE_DOWN,
        },
        BuiltinCommandDef {
            id: "define-word",
            title: "Define Word",
            icon: phosphor_svgs::style::regular::BOOK_OPEN,
        },
        BuiltinCommandDef {
            id: "export-backup",
            title: "Export Backup",
            icon: phosphor_svgs::style::regular::EXPORT,
        },
        BuiltinCommandDef {
            id: "import-backup",
            title: "Import Backup",
            icon: phosphor_svgs::style::regular::DOWNLOAD_SIMPLE,
        },
        BuiltinCommandDef {
            id: "import-from-raycast",
            title: "Import from Raycast",
            icon: phosphor_svgs::style::regular::FILE_ARROW_DOWN,
        },
        BuiltinCommandDef {
            id: "open-camera",
            title: "Open Camera",
            icon: phosphor_svgs::style::regular::CAMERA,
        },
        BuiltinCommandDef {
            id: "quit-corvo",
            title: "Quit Corvo",
            icon: phosphor_svgs::style::regular::POWER,
        },
        BuiltinCommandDef {
            id: "reload-applications",
            title: "Reload Applications",
            icon: phosphor_svgs::style::regular::ARROWS_CLOCKWISE,
        },
        BuiltinCommandDef {
            id: "open-settings",
            title: "Open Settings",
            icon: phosphor_svgs::style::regular::GEAR,
        },
        BuiltinCommandDef {
            id: "toggle-system-appearance",
            title: "Toggle System Appearance",
            icon: phosphor_svgs::style::regular::SUN,
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveDropdown {
    PopToRoot,
    EscapeBehavior,
    AutoSwitchInput,
    Theme,
    ClipboardRetention,
    WindowCycling,
    CalendarUpcomingMeetings,
    CalendarJoinCard,
    CalendarBrowser,
    UpdateChannel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatusUI {
    Idle,
    Checking,
    UpToDate,
    Available(Box<corvo_platform::UpdateRelease>),
    Downloading {
        downloaded: u64,
        total: u64,
        percent: f32,
    },
    ReadyToInstall(Box<corvo_platform::UpdateRelease>, std::path::PathBuf),
    Installing {
        release: Box<corvo_platform::UpdateRelease>,
    },
    InstallFailed {
        release: Box<corvo_platform::UpdateRelease>,
        staged: std::path::PathBuf,
        message: String,
    },
    Error(String),
}

pub struct SettingsView {
    selected_tab: SettingsTab,
    history: Vec<SettingsTab>,
    history_index: usize,
    search_query: String,
    sidebar_search_focused: bool,
    settings: corvo_config::Settings,
    store: std::sync::Arc<dyn corvo_core::DataStore>,
    active_dropdown: Option<ActiveDropdown>,
    apps: Vec<corvo_platform::AppEntry>,
    filtered_apps_cache: Vec<usize>,
    app_search_query: String,
    system_settings_search_query: String,
    system_actions_search_query: String,
    commands_search_query: String,
    quicklinks_search_query: String,
    quicklinks_file: corvo_config::QuicklinksFile,
    snippets_file: corvo_config::SnippetsFile,
    adding_scope: bool,
    new_scope_text: String,
    adding_file_scope: bool,
    new_file_scope_text: String,
    adding_ignore_pattern: bool,
    new_ignore_pattern_text: String,
    adding_nav_disabled_app: bool,
    new_nav_disabled_app_text: String,
    adding_snippet: bool,
    editing_alias_item: Option<(SettingsTab, String)>,
    alias_input_text: String,
    recording_hotkey_item: Option<(SettingsTab, String)>,
    adding_quicklink: bool,
    editing_quicklink_index: Option<usize>,
    quicklink_name_input: String,
    quicklink_url_input: String,
    quicklink_active_field: usize,
    confirming_clear_clipboard: bool,
    clipboard_clear_feedback: Option<bool>,
    save_error: Option<String>,
    content_scroll_handle: ScrollHandle,
    focus_handle: FocusHandle,
    pub update_status: UpdateStatusUI,
    cancel_update_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// The keycaps to draw for a stored hotkey, in press order.
///
/// Delegates to `corvo_core::shortcut` so a hotkey reads the same way in
/// the settings pane, the actions menu, and the footer. macOS draws
/// glyphs; Windows and Linux draw the words users expect.
pub(crate) fn format_hotkey_keycaps(hotkey: &str) -> Vec<String> {
    corvo_core::shortcut::keycaps(hotkey)
}

impl SettingsView {
    pub fn new(
        cx: &mut Context<Self>,
        store: std::sync::Arc<dyn corvo_core::DataStore>,
        window: &mut Window,
    ) -> Self {
        let settings = corvo_config::Settings::load();
        let quicklinks_file = corvo_config::QuicklinksFile::load();
        let snippets_file = corvo_config::SnippetsFile::load();
        let app_updates = corvo_app_launcher::subscribe_corpus_changes();
        let release_updates = app_updates.clone();
        cx.on_release(move |_, cx| {
            release_updates.close();
            if let Some(global) = cx.try_global::<crate::HotkeyManagerGlobal>() {
                global.0.borrow_mut().set_suppressed(false);
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            while app_updates.recv().await.is_ok() {
                let apps = corvo_app_launcher::cached_apps();
                if this
                    .update(cx, |view, cx| {
                        view.apps = apps;
                        view.update_filtered_apps();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let mut view = Self {
            selected_tab: SettingsTab::General,
            history: vec![SettingsTab::General],
            history_index: 0,
            search_query: String::new(),
            sidebar_search_focused: false,
            settings,
            store,
            active_dropdown: None,
            apps: Vec::new(),
            filtered_apps_cache: Vec::new(),
            app_search_query: String::new(),
            system_settings_search_query: String::new(),
            system_actions_search_query: String::new(),
            commands_search_query: String::new(),
            quicklinks_search_query: String::new(),
            quicklinks_file,
            snippets_file,
            adding_scope: false,
            new_scope_text: String::new(),
            adding_file_scope: false,
            new_file_scope_text: String::new(),
            adding_ignore_pattern: false,
            new_ignore_pattern_text: String::new(),
            adding_nav_disabled_app: false,
            new_nav_disabled_app_text: String::new(),
            adding_snippet: false,
            editing_alias_item: None,
            alias_input_text: String::new(),
            recording_hotkey_item: None,
            adding_quicklink: false,
            editing_quicklink_index: None,
            quicklink_name_input: String::new(),
            quicklink_url_input: String::new(),
            quicklink_active_field: 0,
            confirming_clear_clipboard: false,
            clipboard_clear_feedback: None,
            save_error: None,
            content_scroll_handle: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            update_status: UpdateStatusUI::Idle,
            cancel_update_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        // Every edit saves immediately, so reloading the snapshot when this
        // window regains focus picks up writes made elsewhere (the onboarding
        // wizard, the updater) instead of reverting them on the next save.
        let focus_handle = view.focus_handle.clone();
        cx.on_focus_in(&focus_handle, window, |view, _window, cx| {
            view.settings = corvo_config::Settings::load();
            cx.notify();
        })
        .detach();
        // A background check may have found a newer release before this
        // window opened. Show its changelog with Skip / Download buttons
        // right away instead of an empty update section.
        let pending = corvo_platform::load_pending_release()
            .filter(|release| !corvo_platform::is_version_dismissed(&release.tag_name));
        match (pending, corvo_platform::take_install_error()) {
            (Some(release), Some(message)) => {
                view.update_status = match corvo_platform::cached_update_archive(&release) {
                    Some(staged) => UpdateStatusUI::InstallFailed {
                        release: Box::new(release),
                        staged,
                        message,
                    },
                    None => UpdateStatusUI::Error(message),
                };
            }
            (Some(release), None) => {
                view.update_status = UpdateStatusUI::Available(Box::new(release));
            }
            (None, Some(message)) => view.update_status = UpdateStatusUI::Error(message),
            (None, None) => {}
        }
        view
    }

    fn save_quicklinks_file(&mut self) {
        match self.quicklinks_file.save() {
            Ok(()) => {
                self.save_error = None;
                self.store.replace_quicklinks(
                    self.quicklinks_file
                        .quicklinks
                        .iter()
                        .map(|link| corvo_core::Quicklink {
                            name: link.name.clone(),
                            url: link.url.clone(),
                            alias: link.alias.clone(),
                            hotkey: link.hotkey.clone(),
                            hidden: link.hidden,
                        })
                        .collect(),
                );
                corvo_platform::hotkey::notify_hotkeys_changed();
            }
            Err(error) => {
                corvo_platform::diagnostics::record_error("settings", "quicklinks_save_failed");
                self.save_error = Some(format!("Could not save Quicklinks: {error}"));
            }
        }
    }

    fn save_snippets_file(&mut self) {
        match self.snippets_file.save() {
            Ok(()) => {
                self.save_error = None;
                self.store.replace_snippets(
                    self.snippets_file
                        .snippets
                        .iter()
                        .map(|snippet| corvo_core::Snippet {
                            name: snippet.name.clone(),
                            keyword: snippet.keyword.clone(),
                            body: snippet.body.clone(),
                        })
                        .collect(),
                );
            }
            Err(error) => {
                corvo_platform::diagnostics::record_error("settings", "snippets_save_failed");
                self.save_error = Some(format!("Could not save Snippets: {error}"));
            }
        }
    }

    /// Routes recorder state through one helper so the global hotkeys are
    /// unregistered exactly while a binding is being recorded.
    fn set_recording_hotkey_item(
        &mut self,
        item: Option<(SettingsTab, String)>,
        cx: &mut Context<Self>,
    ) {
        self.recording_hotkey_item = item;
        if let Some(global) = cx.try_global::<crate::HotkeyManagerGlobal>() {
            global
                .0
                .borrow_mut()
                .set_suppressed(self.recording_hotkey_item.is_some());
        }
    }

    fn save_settings_file(&mut self) {
        match self.settings.save() {
            Ok(()) => {
                self.save_error = None;
                self.store
                    .replace_command_availability(corvo_config::command_availability(
                        &self.settings,
                    ));
                self.store.replace_emoji_preferences(
                    self.settings.emojis.column_count,
                    self.settings.emojis.skin_tone,
                );
                self.store
                    .replace_clipboard_auto_paste(self.settings.clipboard.auto_paste);
                corvo_clipboard_manager::set_preferences(
                    corvo_clipboard_manager::ClipboardPreferences {
                        enabled: self.settings.clipboard.enabled,
                        retention_days: self.settings.clipboard.retention_days,
                        max_entries: self.settings.clipboard.max_entries,
                        save_images: self.settings.clipboard.save_images,
                        save_colors: self.settings.clipboard.save_colors,
                    },
                );
                self.store
                    .replace_file_search_options(corvo_core::FileSearchOptions {
                        enabled: self.settings.file_search.enabled,
                        search_scopes: self.settings.file_search.search_scopes.clone(),
                        ignore_patterns: self.settings.file_search.ignore_patterns.clone(),
                    });
                self.store
                    .replace_escape_behavior(self.settings.escape_behavior_option == 1);
                self.store.replace_interface_appearance(
                    self.settings.interface_size_option,
                    self.settings.transparency_level,
                );
                self.store.replace_compact_mode(self.settings.compact_mode);
                self.store
                    .replace_update_settings(corvo_config::update_settings(&self.settings.updates));
                corvo_platform::set_window_gap(self.settings.window_management.gap_between_windows);
                corvo_platform::hotkey::notify_hotkeys_changed();
            }
            Err(error) => {
                corvo_platform::diagnostics::record_error("settings", "settings_save_failed");
                self.save_error = Some(format!("Could not save Settings: {error}"));
            }
        }
    }

    fn navigate_to_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        if self.selected_tab == tab {
            return;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(tab);
        self.history_index = self.history.len() - 1;
        self.set_tab(tab, cx);
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.history_index > 0 {
            self.history_index -= 1;
            let tab = self.history[self.history_index];
            self.set_tab(tab, cx);
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let tab = self.history[self.history_index];
            self.set_tab(tab, cx);
        }
    }

    fn set_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.selected_tab = tab;
        self.active_dropdown = None;
        self.editing_alias_item = None;
        self.set_recording_hotkey_item(None, cx);
        self.adding_scope = false;
        self.adding_file_scope = false;
        self.adding_ignore_pattern = false;
        self.adding_nav_disabled_app = false;
        self.adding_snippet = false;
        self.adding_quicklink = false;
        self.editing_quicklink_index = None;
        self.confirming_clear_clipboard = false;
        self.clipboard_clear_feedback = None;
        self.content_scroll_handle
            .set_offset(gpui::point(px(0.0), px(0.0)));
        if tab == SettingsTab::Applications {
            self.reload_apps();
        }
        cx.notify();
    }

    fn reload_apps(&mut self) {
        corvo_app_launcher::reload_corpus();
        self.apps = corvo_app_launcher::cached_apps();
        self.update_filtered_apps();
    }

    fn update_filtered_apps(&mut self) {
        let query_lower = self.app_search_query.to_lowercase();
        self.filtered_apps_cache = self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| {
                if query_lower.is_empty() {
                    return true;
                }
                app.name.to_lowercase().contains(&query_lower)
                    || self
                        .settings
                        .applications
                        .app_configs
                        .get(&app.name)
                        .and_then(|c| c.alias.as_deref())
                        .is_some_and(|a| a.to_lowercase().contains(&query_lower))
            })
            .map(|(idx, _)| idx)
            .collect();
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
                    SettingsTab::Calendar,
                    SettingsTab::Emojis,
                ],
            ),
            ("Store", &[SettingsTab::Extensions]),
            ("Advanced", &[SettingsTab::Backup, SettingsTab::About]),
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
                        .gap_2p5()
                        .px(px(10.0))
                        .py(px(6.5))
                        .mx(px(8.0))
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_selected, |row| {
                            row.bg(rgb(COLOR_ROW_SELECTED))
                                .border_1()
                                .border_color(rgb(0x1e4a3b))
                        })
                        .when(!is_selected, |row| {
                            row.hover(|style| style.bg(rgb(COLOR_ROW_HOVER)))
                        })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.navigate_to_tab(tab, cx);
                        }))
                        .child(crate::icons::render_phosphor_svg(
                            tab.icon(),
                            if is_selected {
                                rgb(COLOR_ACCENT)
                            } else {
                                rgb(COLOR_TEXT_DIM)
                            },
                            15.0,
                        ))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(if is_selected {
                                    FontWeight::MEDIUM
                                } else {
                                    FontWeight::NORMAL
                                })
                                .text_color(if is_selected {
                                    rgb(COLOR_TEXT)
                                } else {
                                    rgb(COLOR_TEXT_MUTED)
                                })
                                .child(tab.title()),
                        )
                        .into_any_element(),
                );
            }
        }

        div()
            .flex_none()
            .w(px(220.0))
            .h_full()
            .bg(rgb(COLOR_SIDEBAR_BG))
            .border_r_1()
            .border_color(rgb(COLOR_DIVIDER))
            .flex()
            .flex_col()
            .child(
                // Top area: on macOS provide draggable area to clear native traffic lights;
                // on Windows/Linux provide standard top spacing.
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(if cfg!(target_os = "macos") {
                                px(38.0)
                            } else {
                                px(14.0)
                            })
                            .w_full()
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(MouseButton::Left, |ev, window, _cx| {
                                if ev.click_count == 2 {
                                    window.zoom_window();
                                } else {
                                    window.start_window_move();
                                }
                            }),
                    )
                    .child(
                        div().px(px(10.0)).pb(px(8.0)).child(
                            div()
                                .id("settings-sidebar-search-bar")
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2p5()
                                .py(px(5.0))
                                .rounded_md()
                                .bg(rgb(0x191a1d))
                                .border_1()
                                .border_color(if self.sidebar_search_focused {
                                    rgb(COLOR_ACCENT)
                                } else {
                                    rgb(COLOR_BORDER_SUBTLE)
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                    this.sidebar_search_focused = true;
                                    cx.notify();
                                }))
                                .child(crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                    if self.sidebar_search_focused {
                                        rgb(COLOR_ACCENT)
                                    } else {
                                        rgb(COLOR_TEXT_DIM)
                                    },
                                    13.0,
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.0))
                                        .text_color(if self.search_query.is_empty() {
                                            rgb(COLOR_TEXT_DIM)
                                        } else {
                                            rgb(COLOR_TEXT)
                                        })
                                        .child(if self.search_query.is_empty() {
                                            "Search settings...".to_string()
                                        } else {
                                            self.search_query.clone()
                                        }),
                                )
                                .when(!self.search_query.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .id("clear-sidebar-search")
                                            .size(px(16.0))
                                            .rounded_full()
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(rgb(COLOR_ROW_HOVER)))
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.search_query.clear();
                                                    cx.notify();
                                                },
                                            ))
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::X,
                                                rgb(COLOR_TEXT_DIM),
                                                10.0,
                                            )),
                                    )
                                }),
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
        let can_go_back = self.history_index > 0;
        let can_go_forward = self.history_index + 1 < self.history.len();

        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .bg(rgb(COLOR_BG))
            .flex()
            .flex_col()
            .child(
                // Navigation header
                div()
                    .h(px(48.0))
                    .px_8()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(COLOR_DIVIDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    // Chevron left (Back)
                                    .child(
                                        div()
                                            .id("settings-nav-back")
                                            .size(px(24.0))
                                            .rounded_md()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(can_go_back, |b| {
                                                b.cursor_pointer()
                                                    .hover(|s| s.bg(rgb(COLOR_ROW_HOVER)))
                                                    .on_click(cx.listener(
                                                        |this, _: &ClickEvent, _window, cx| {
                                                            this.go_back(cx);
                                                        },
                                                    ))
                                            })
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::CARET_LEFT,
                                                if can_go_back {
                                                    rgb(COLOR_TEXT)
                                                } else {
                                                    rgb(0x4b5563)
                                                },
                                                16.0,
                                            )),
                                    )
                                    // Chevron right (Forward)
                                    .child(
                                        div()
                                            .id("settings-nav-forward")
                                            .size(px(24.0))
                                            .rounded_md()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(can_go_forward, |b| {
                                                b.cursor_pointer()
                                                    .hover(|s| s.bg(rgb(COLOR_ROW_HOVER)))
                                                    .on_click(cx.listener(
                                                        |this, _: &ClickEvent, _window, cx| {
                                                            this.go_forward(cx);
                                                        },
                                                    ))
                                            })
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::CARET_RIGHT,
                                                if can_go_forward {
                                                    rgb(COLOR_TEXT)
                                                } else {
                                                    rgb(0x4b5563)
                                                },
                                                16.0,
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(crate::icons::render_phosphor_svg(
                                        self.selected_tab.icon(),
                                        rgb(COLOR_ACCENT),
                                        15.0,
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child(self.selected_tab.title()),
                                    ),
                            ),
                    )
                    // Draggable header area to move window
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(MouseButton::Left, |ev, window, _cx| {
                                if ev.click_count == 2 {
                                    window.zoom_window();
                                } else {
                                    window.start_window_move();
                                }
                            }),
                    )
                    // Windows / Linux window control buttons (Minimize, Maximize/Restore, Close)
                    .when(!cfg!(target_os = "macos"), |header| {
                        header.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                // Minimize
                                .child(
                                    div()
                                        .id("settings-win-min")
                                        .w(px(32.0))
                                        .h(px(28.0))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(COLOR_ROW_SELECTED)))
                                        .when(cfg!(target_os = "windows"), |b| {
                                            b.window_control_area(WindowControlArea::Min)
                                        })
                                        .on_mouse_down(MouseButton::Left, |_ev, _window, _cx| {})
                                        .child(
                                            div()
                                                .w(px(10.0))
                                                .h(px(1.0))
                                                .rounded(px(1.0))
                                                .bg(rgb(COLOR_TEXT_DIM)),
                                        ),
                                )
                                // Maximize / Zoom
                                .child(
                                    div()
                                        .id("settings-win-max")
                                        .w(px(32.0))
                                        .h(px(28.0))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(COLOR_ROW_SELECTED)))
                                        .when(cfg!(target_os = "windows"), |b| {
                                            b.window_control_area(WindowControlArea::Max)
                                        })
                                        .on_mouse_down(MouseButton::Left, |_ev, window, _cx| {
                                            window.zoom_window();
                                        })
                                        .child(
                                            div()
                                                .w(px(10.0))
                                                .h(px(10.0))
                                                .border_1()
                                                .border_color(rgb(COLOR_TEXT_DIM))
                                                .rounded(px(1.0)),
                                        ),
                                )
                                // Close
                                .child(
                                    div()
                                        .id("settings-win-close")
                                        .w(px(32.0))
                                        .h(px(28.0))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(0xe81123)))
                                        .when(cfg!(target_os = "windows"), |b| {
                                            b.window_control_area(WindowControlArea::Close)
                                        })
                                        .on_mouse_down(MouseButton::Left, |_ev, window, cx| {
                                            close_settings_window(window, cx);
                                        })
                                        .child(crate::icons::render_phosphor_svg(
                                            phosphor_svgs::style::regular::X,
                                            rgb(COLOR_TEXT_DIM),
                                            12.0,
                                        )),
                                ),
                        )
                    }),
            )
            .when(self.save_error.is_some(), |view| {
                view.child(
                    div()
                        .mx_6()
                        .mt_3()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(rgb(0x3a2224))
                        .text_size(px(12.0))
                        .text_color(rgb(0xfca5a5))
                        .child(self.save_error.clone().unwrap_or_default()),
                )
            })
            .child(
                div()
                    .id("settings-content-scroll")
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.content_scroll_handle)
                    .on_scroll_wheel(cx.listener(|this, _ev, _window, cx| {
                        if this.selected_tab == SettingsTab::Applications {
                            cx.notify();
                        }
                    }))
                    .px_6()
                    .pb_6()
                    .pt(px(28.0))
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .child(match self.selected_tab {
                                SettingsTab::General => self.render_general_pane(cx),
                                SettingsTab::Permissions => self.render_permissions_pane(cx),
                                SettingsTab::Applications => self.render_applications_pane(cx),
                                SettingsTab::SystemSettings => self.render_system_settings_pane(cx),
                                SettingsTab::SystemActions => self.render_system_actions_pane(cx),
                                SettingsTab::Commands => self.render_commands_pane(cx),
                                SettingsTab::Quicklinks => self.render_quicklinks_pane(cx),
                                SettingsTab::Clipboard => self.render_clipboard_pane(cx),
                                SettingsTab::Snippets => self.render_snippets_pane(cx),
                                SettingsTab::FileSearch => self.render_file_search_pane(cx),
                                SettingsTab::WindowManagement => {
                                    self.render_window_management_pane(cx)
                                }
                                SettingsTab::Navigation => self.render_navigation_pane(cx),
                                SettingsTab::Calendar => self.render_calendar_pane(cx),
                                SettingsTab::Emojis => self.render_emojis_pane(cx),
                                SettingsTab::Extensions => self.render_extensions_pane(cx),
                                SettingsTab::Backup => self.render_backup_pane(),
                                SettingsTab::About => self.render_about_pane(cx),
                                SettingsTab::Fallbacks => {
                                    self.render_placeholder_pane(self.selected_tab)
                                }
                            }),
                    ),
            )
    }

    fn render_general_pane(&self, cx: &mut Context<Self>) -> Div {
        let mut pane = div()
            .relative()
            .flex()
            .flex_col()
            .gap_6()
            // Global Shortcuts
            .child(self.section_group(
                "Global Shortcuts",
                vec![self.hotkey_setting_row(
                    "App Launcher",
                    SettingsTab::General,
                    "launcher",
                    if self.settings.hotkey.trim().is_empty() {
                        None
                    } else {
                        Some(&self.settings.hotkey)
                    },
                    cx,
                )],
            ))
            // General
            .child(self.section_group(
                "General",
                vec![
                    self.toggle_row(
                        "toggle-launch-at-login",
                        "Launch at login",
                        None,
                        self.settings.launch_at_login,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            let enabled = !this.settings.launch_at_login;
                            this.settings.launch_at_login = enabled;
                            std::thread::spawn(move || {
                                let _ = corvo_platform::set_launch_at_login(enabled);
                            });
                            this.save_settings_file();
                            cx.notify();
                        }),
                    ),
                    self.toggle_row(
                        "toggle-show-menu-bar",
                        if cfg!(target_os = "macos") {
                            "Show in menu bar"
                        } else {
                            "Show in system tray"
                        },
                        Some("Shortcuts still work when hidden."),
                        self.settings.show_menu_bar,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.settings.show_menu_bar = !this.settings.show_menu_bar;
                            this.save_settings_file();
                            cx.notify();
                        }),
                    ),
                    self.interactive_dropdown_trigger(
                        "pop-to-root",
                        ("Pop to Root Search", Some("After the launcher closes.")),
                        ActiveDropdown::PopToRoot,
                        self.settings.pop_to_root_option,
                        &["Immediately", "After 90 seconds", "Never"],
                        cx,
                    ),
                    self.interactive_dropdown_trigger(
                        "escape-behavior",
                        ("Escape Key Behavior", Some("When the search field is empty.")),
                        ActiveDropdown::EscapeBehavior,
                        self.settings.escape_behavior_option,
                        &["Navigate back or close window", "Close window"],
                        cx,
                    ),
                    self.interactive_dropdown_trigger(
                        "auto-switch-input",
                        ("Auto-switch input source", Some("While the launcher is open.")),
                        ActiveDropdown::AutoSwitchInput,
                        self.settings.auto_switch_input,
                        &["None", "ABC"],
                        cx,
                    ),
                ],
            ))
            // Appearance
            .child(self.section_group(
                "Appearance",
                vec![
                    self.interactive_dropdown_trigger(
                        "theme-select",
                        ("Theme", None),
                        ActiveDropdown::Theme,
                        self.settings.theme_option,
                        &["System", "Dark", "Light"],
                        cx,
                    ),
                    self.segmented_row(
                        "Interface size",
                        Some("Scales the launcher and its panels, not Settings."),
                        self.settings.interface_size_option,
                        cx,
                    ),
                    self.slider_row(
                        "Background transparency",
                        self.settings.transparency_level,
                        cx,
                    ),
                    self.toggle_row(
                        "toggle-compact-mode",
                        "Compact mode",
                        Some("A slim search bar that expands as you type."),
                        self.settings.compact_mode,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.settings.compact_mode = !this.settings.compact_mode;
                            this.save_settings_file();
                            super::sync_launcher_preferences(cx);
                            cx.notify();
                        }),
                    ),
                ],
            ));

        pane = pane.child(self.section_group("Onboarding", vec![self.onboarding_rerun_row(cx)]));

        if let Some(active) = self.active_dropdown {
            pane = pane
                .child(
                    div()
                        .id("settings-dropdown-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.active_dropdown = None;
                            cx.notify();
                        })),
                )
                .child(self.render_active_dropdown_popover(active, cx));
        }

        pane
    }

    fn onboarding_rerun_row(&self, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(54.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(COLOR_TEXT))
                            .child("Run first-launch setup"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("Shortcut, permissions, and the welcome tour."),
                    ),
            )
            .child(
                div()
                    .id("onboarding-rerun-open")
                    .cursor_pointer()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(COLOR_CONTROL_BG))
                    .border_1()
                    .border_color(rgb(COLOR_CONTROL_BORDER))
                    .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(COLOR_TEXT))
                    .on_click(cx.listener(|_this, _: &ClickEvent, _window, cx| {
                        crate::onboarding::open_welcome(cx);
                    }))
                    .child("Open"),
            )
    }

    fn permission_card(
        &self,
        section_title: &'static str,
        title: &'static str,
        subtitle: &'static str,
        is_granted: bool,
        on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child(section_title),
            )
            .child(
                div()
                    .w_full()
                    .rounded_lg()
                    .bg(rgb(COLOR_CARD_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .px_4()
                    .child(
                        div()
                            .w_full()
                            .min_h(px(54.0))
                            .py_2()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child(subtitle),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .child(crate::icons::render_phosphor_svg(
                                                if is_granted {
                                                    phosphor_svgs::style::fill::CHECK_CIRCLE
                                                } else {
                                                    phosphor_svgs::style::regular::X_CIRCLE
                                                },
                                                if is_granted {
                                                    rgb(COLOR_ACCENT)
                                                } else {
                                                    rgb(0xef4444)
                                                },
                                                15.0,
                                            ))
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(if is_granted {
                                                        rgb(COLOR_ACCENT)
                                                    } else {
                                                        rgb(0xef4444)
                                                    })
                                                    .child(if is_granted {
                                                        "Granted"
                                                    } else {
                                                        "Not Granted"
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "open-permission-{title}"
                                            )))
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .hover(|s| {
                                                s.bg(rgb(COLOR_CONTROL_HOVER))
                                                    .border_color(rgb(0x44474e))
                                            })
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .on_click(on_open)
                                            .child("Open..."),
                                    ),
                            ),
                    ),
            )
    }

    fn start_permissions_polling(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            for _ in 0..40 {
                smol::Timer::after(std::time::Duration::from_millis(300)).await;
                let res = this.update(cx, |_view, cx| {
                    cx.notify();
                });
                if res.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn render_permissions_pane(&self, cx: &mut Context<Self>) -> Div {
        use corvo_platform::permissions::{self, PermissionKind};

        let permissions_list = [
            PermissionKind::Accessibility,
            PermissionKind::Calendars,
            PermissionKind::ScreenRecording,
            PermissionKind::FullDiskAccess,
        ];

        div()
            .flex()
            .flex_col()
            .gap_6()
            .children(permissions_list.iter().map(|&kind| {
                let is_granted = permissions::is_granted(kind);
                self.permission_card(
                    kind.title(),
                    kind.title(),
                    kind.description(),
                    is_granted,
                    cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        permissions::request(kind);
                        this.start_permissions_polling(cx);
                        cx.notify();
                    }),
                )
            }))
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;

        // Global settings window shortcuts (when not recording hotkey or editing text)
        if self.recording_hotkey_item.is_none()
            && self.editing_alias_item.is_none()
            && !self.adding_scope
            && !self.adding_file_scope
            && !self.adding_ignore_pattern
            && !self.adding_nav_disabled_app
            && !self.adding_snippet
            && !self.adding_quicklink
            && self.editing_quicklink_index.is_none()
        {
            // The settings window uses the platform's primary modifier:
            // Command on macOS, Control on Windows and Linux. Reading
            // `modifiers.platform` here meant the Windows key, which the
            // shell intercepts before the window sees it.
            let primary = match corvo_core::Primary::current() {
                corvo_core::Primary::Command => {
                    keystroke.modifiers.platform && !keystroke.modifiers.control
                }
                corvo_core::Primary::Control => keystroke.modifiers.control,
            };
            if primary && !keystroke.modifiers.alt {
                match keystroke.key.as_str() {
                    "[" => {
                        self.go_back(cx);
                        return;
                    }
                    "]" => {
                        self.go_forward(cx);
                        return;
                    }
                    "w" => {
                        close_settings_window(window, cx);
                        return;
                    }
                    _ => {}
                }
            }
        }

        // Quicklink modal input
        if self.adding_quicklink || self.editing_quicklink_index.is_some() {
            match keystroke.key.as_str() {
                "escape" => {
                    self.adding_quicklink = false;
                    self.editing_quicklink_index = None;
                    cx.notify();
                }
                "tab" => {
                    self.quicklink_active_field = if self.quicklink_active_field == 0 {
                        1
                    } else {
                        0
                    };
                    cx.notify();
                }
                "enter" => {
                    self.save_quicklink(cx);
                }
                "backspace" => {
                    if self.quicklink_active_field == 0 {
                        self.quicklink_name_input.pop();
                    } else {
                        self.quicklink_url_input.pop();
                    }
                    cx.notify();
                }
                "space" => {
                    if self.quicklink_active_field == 0 {
                        self.quicklink_name_input.push(' ');
                    } else {
                        self.quicklink_url_input.push(' ');
                    }
                    cx.notify();
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        if self.quicklink_active_field == 0 {
                            self.quicklink_name_input.push_str(key);
                        } else {
                            self.quicklink_url_input.push_str(key);
                        }
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Recording hotkey for an item (App, System Setting, System Action, Command, Quicklink, Snippets, FileSearch, WindowManagement, Navigation, Emojis)
        if let Some((tab, item_key)) = self.recording_hotkey_item.clone() {
            if keystroke.key == "escape" {
                self.set_recording_hotkey_item(None, cx);
                cx.notify();
                return;
            }
            if keystroke.key == "backspace" || keystroke.key == "delete" {
                match tab {
                    SettingsTab::General => {
                        self.settings.hotkey.clear();
                        self.save_settings_file();
                    }
                    SettingsTab::Applications => {
                        let entry = self
                            .settings
                            .applications
                            .app_configs
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::SystemSettings => {
                        let entry = self
                            .settings
                            .system_settings
                            .items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::SystemActions => {
                        let entry = self
                            .settings
                            .system_actions
                            .items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::Commands => {
                        let entry = self.settings.commands.items.entry(item_key).or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::Quicklinks => {
                        if let Some(entry) = self
                            .quicklinks_file
                            .quicklinks
                            .iter_mut()
                            .find(|q| q.name == item_key)
                        {
                            entry.hotkey = None;
                            self.save_quicklinks_file();
                        } else {
                            let entry = self
                                .settings
                                .quicklinks
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.hotkey = None;
                            self.save_settings_file();
                        }
                    }
                    SettingsTab::Clipboard => {
                        let entry = self
                            .settings
                            .clipboard
                            .command_items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::Snippets => {
                        let entry = self
                            .settings
                            .snippets
                            .command_items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::FileSearch => {
                        let entry = self
                            .settings
                            .file_search
                            .command_items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::WindowManagement => {
                        if let Some(layout_id) = item_key.strip_prefix("layout:") {
                            if let Some(l) = self
                                .settings
                                .window_management
                                .layouts
                                .iter_mut()
                                .find(|l| l.id == layout_id)
                            {
                                l.hotkey = None;
                                self.save_settings_file();
                            }
                        } else {
                            let entry = self
                                .settings
                                .window_management
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.hotkey = Some(String::new());
                            self.save_settings_file();
                        }
                    }
                    SettingsTab::Navigation => {
                        let entry = self
                            .settings
                            .navigation
                            .command_items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    SettingsTab::Emojis => {
                        let entry = self
                            .settings
                            .emojis
                            .command_items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = None;
                        self.save_settings_file();
                    }
                    _ => {}
                }
                self.set_recording_hotkey_item(None, cx);
                cx.notify();
                return;
            }
            if matches!(
                keystroke.key.as_str(),
                "cmd" | "command" | "control" | "ctrl" | "alt" | "shift" | "meta"
            ) {
                return;
            }
            let mut parts = Vec::new();
            if keystroke.modifiers.control {
                parts.push("ctrl");
            }
            if keystroke.modifiers.alt {
                parts.push("alt");
            }
            if keystroke.modifiers.platform {
                // `cmd` means the primary modifier, which the parser
                // resolves per platform. Storing the physical `super`
                // here bound the Windows key on Windows and displayed a
                // command glyph in the list.
                parts.push(if corvo_core::Primary::is_command() {
                    "cmd"
                } else {
                    "super"
                });
            }
            if keystroke.modifiers.shift {
                parts.push("shift");
            }
            let key = match keystroke.key.as_str() {
                " " | "space" => "space",
                k => k,
            };
            parts.push(key);
            let hotkey_str = parts.join("+");

            match tab {
                SettingsTab::General => {
                    self.settings.hotkey = hotkey_str;
                    self.save_settings_file();
                }
                SettingsTab::Applications => {
                    let entry = self
                        .settings
                        .applications
                        .app_configs
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::SystemSettings => {
                    let entry = self
                        .settings
                        .system_settings
                        .items
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::SystemActions => {
                    let entry = self
                        .settings
                        .system_actions
                        .items
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::Commands => {
                    let entry = self.settings.commands.items.entry(item_key).or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::Quicklinks => {
                    if let Some(entry) = self
                        .quicklinks_file
                        .quicklinks
                        .iter_mut()
                        .find(|q| q.name == item_key)
                    {
                        entry.hotkey = Some(hotkey_str);
                        self.save_quicklinks_file();
                    } else {
                        let entry = self
                            .settings
                            .quicklinks
                            .command_items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = Some(hotkey_str);
                        self.save_settings_file();
                    }
                }
                SettingsTab::Clipboard => {
                    let entry = self
                        .settings
                        .clipboard
                        .command_items
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::Snippets => {
                    let entry = self
                        .settings
                        .snippets
                        .command_items
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::FileSearch => {
                    let entry = self
                        .settings
                        .file_search
                        .command_items
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::WindowManagement => {
                    if let Some(layout_id) = item_key.strip_prefix("layout:") {
                        if let Some(l) = self
                            .settings
                            .window_management
                            .layouts
                            .iter_mut()
                            .find(|l| l.id == layout_id)
                        {
                            l.hotkey = Some(hotkey_str);
                            self.save_settings_file();
                        }
                    } else {
                        let entry = self
                            .settings
                            .window_management
                            .command_items
                            .entry(item_key)
                            .or_default();
                        entry.hotkey = Some(hotkey_str);
                        self.save_settings_file();
                    }
                }
                SettingsTab::Navigation => {
                    let entry = self
                        .settings
                        .navigation
                        .command_items
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                SettingsTab::Emojis => {
                    let entry = self
                        .settings
                        .emojis
                        .command_items
                        .entry(item_key)
                        .or_default();
                    entry.hotkey = Some(hotkey_str);
                    self.save_settings_file();
                }
                _ => {}
            }
            self.set_recording_hotkey_item(None, cx);
            cx.notify();
            return;
        }

        // Editing alias for an item (App, System Setting, System Action, Command, Quicklink, Snippets, FileSearch, WindowManagement, Navigation, Emojis)
        if let Some((tab, item_key)) = self.editing_alias_item.clone() {
            match keystroke.key.as_str() {
                "escape" => {
                    self.editing_alias_item = None;
                    self.alias_input_text.clear();
                    cx.notify();
                }
                "enter" => {
                    let trimmed = self.alias_input_text.trim();
                    let alias_val = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    };
                    match tab {
                        SettingsTab::Applications => {
                            let entry = self
                                .settings
                                .applications
                                .app_configs
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::SystemSettings => {
                            let entry = self
                                .settings
                                .system_settings
                                .items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::SystemActions => {
                            let entry = self
                                .settings
                                .system_actions
                                .items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::Commands => {
                            let entry = self.settings.commands.items.entry(item_key).or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::Quicklinks => {
                            if let Some(entry) = self
                                .quicklinks_file
                                .quicklinks
                                .iter_mut()
                                .find(|q| q.name == item_key)
                            {
                                entry.alias = alias_val;
                                self.save_quicklinks_file();
                            } else {
                                let entry = self
                                    .settings
                                    .quicklinks
                                    .command_items
                                    .entry(item_key)
                                    .or_default();
                                entry.alias = alias_val;
                                self.save_settings_file();
                            }
                        }
                        SettingsTab::Clipboard => {
                            let entry = self
                                .settings
                                .clipboard
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::Snippets => {
                            let entry = self
                                .settings
                                .snippets
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::FileSearch => {
                            let entry = self
                                .settings
                                .file_search
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::WindowManagement => {
                            let entry = self
                                .settings
                                .window_management
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::Navigation => {
                            let entry = self
                                .settings
                                .navigation
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        SettingsTab::Emojis => {
                            let entry = self
                                .settings
                                .emojis
                                .command_items
                                .entry(item_key)
                                .or_default();
                            entry.alias = alias_val;
                            self.save_settings_file();
                        }
                        _ => {}
                    }
                    self.editing_alias_item = None;
                    self.alias_input_text.clear();
                    cx.notify();
                }
                "backspace" => {
                    self.alias_input_text.pop();
                    cx.notify();
                }
                "space" => {
                    self.alias_input_text.push(' ');
                    cx.notify();
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        self.alias_input_text.push_str(key);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Adding applications scope
        if self.adding_scope {
            match keystroke.key.as_str() {
                "escape" => {
                    self.adding_scope = false;
                    self.new_scope_text.clear();
                    cx.notify();
                }
                "enter" => {
                    let trimmed = self.new_scope_text.trim().to_string();
                    if !trimmed.is_empty()
                        && !self.settings.applications.search_scopes.contains(&trimmed)
                    {
                        self.settings.applications.search_scopes.push(trimmed);
                        self.save_settings_file();
                        self.reload_apps();
                    }
                    self.adding_scope = false;
                    self.new_scope_text.clear();
                    cx.notify();
                }
                "backspace" => {
                    self.new_scope_text.pop();
                    cx.notify();
                }
                "space" => {
                    self.new_scope_text.push(' ');
                    cx.notify();
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        self.new_scope_text.push_str(key);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Adding file search scope
        if self.adding_file_scope {
            match keystroke.key.as_str() {
                "escape" => {
                    self.adding_file_scope = false;
                    self.new_file_scope_text.clear();
                    cx.notify();
                }
                "enter" => {
                    let trimmed = self.new_file_scope_text.trim().to_string();
                    if !trimmed.is_empty()
                        && !self.settings.file_search.search_scopes.contains(&trimmed)
                    {
                        self.settings.file_search.search_scopes.push(trimmed);
                        self.save_settings_file();
                    }
                    self.adding_file_scope = false;
                    self.new_file_scope_text.clear();
                    cx.notify();
                }
                "backspace" => {
                    self.new_file_scope_text.pop();
                    cx.notify();
                }
                "space" => {
                    self.new_file_scope_text.push(' ');
                    cx.notify();
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        self.new_file_scope_text.push_str(key);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Adding file search ignore pattern
        if self.adding_ignore_pattern {
            match keystroke.key.as_str() {
                "escape" => {
                    self.adding_ignore_pattern = false;
                    self.new_ignore_pattern_text.clear();
                    cx.notify();
                }
                "enter" => {
                    let trimmed = self.new_ignore_pattern_text.trim().to_string();
                    if !trimmed.is_empty()
                        && !self.settings.file_search.ignore_patterns.contains(&trimmed)
                    {
                        self.settings.file_search.ignore_patterns.push(trimmed);
                        self.save_settings_file();
                    }
                    self.adding_ignore_pattern = false;
                    self.new_ignore_pattern_text.clear();
                    cx.notify();
                }
                "backspace" => {
                    self.new_ignore_pattern_text.pop();
                    cx.notify();
                }
                "space" => {
                    self.new_ignore_pattern_text.push(' ');
                    cx.notify();
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        self.new_ignore_pattern_text.push_str(key);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Adding disabled navigation application
        if self.adding_nav_disabled_app {
            match keystroke.key.as_str() {
                "escape" => {
                    self.adding_nav_disabled_app = false;
                    self.new_nav_disabled_app_text.clear();
                    cx.notify();
                }
                "enter" => {
                    let trimmed = self.new_nav_disabled_app_text.trim().to_string();
                    if !trimmed.is_empty()
                        && !self
                            .settings
                            .navigation
                            .disabled_applications
                            .contains(&trimmed)
                    {
                        self.settings.navigation.disabled_applications.push(trimmed);
                        self.save_settings_file();
                    }
                    self.adding_nav_disabled_app = false;
                    self.new_nav_disabled_app_text.clear();
                    cx.notify();
                }
                "backspace" => {
                    self.new_nav_disabled_app_text.pop();
                    cx.notify();
                }
                "space" => {
                    self.new_nav_disabled_app_text.push(' ');
                    cx.notify();
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        self.new_nav_disabled_app_text.push_str(key);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Search within active tab or sidebar search
        if self.sidebar_search_focused {
            match keystroke.key.as_str() {
                "escape" => {
                    self.search_query.clear();
                    self.sidebar_search_focused = false;
                    cx.notify();
                }
                "backspace" => {
                    self.search_query.pop();
                    cx.notify();
                }
                "space" => {
                    self.search_query.push(' ');
                    cx.notify();
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        self.search_query.push_str(key);
                        cx.notify();
                    }
                }
            }
            return;
        }

        let is_app_tab = self.selected_tab == SettingsTab::Applications;
        let active_query = match self.selected_tab {
            SettingsTab::Applications => Some(&mut self.app_search_query),
            SettingsTab::SystemSettings => Some(&mut self.system_settings_search_query),
            SettingsTab::SystemActions => Some(&mut self.system_actions_search_query),
            SettingsTab::Commands => Some(&mut self.commands_search_query),
            SettingsTab::Quicklinks => Some(&mut self.quicklinks_search_query),
            _ => None,
        };

        let mut query_changed = false;
        if let Some(query) = active_query {
            match keystroke.key.as_str() {
                "escape" => {
                    if !query.is_empty() {
                        query.clear();
                        query_changed = true;
                    }
                }
                "backspace" => {
                    if !query.is_empty() {
                        query.pop();
                        query_changed = true;
                    }
                }
                "space" => {
                    query.push(' ');
                    query_changed = true;
                }
                key => {
                    if !keystroke.modifiers.platform
                        && !keystroke.modifiers.control
                        && !keystroke.modifiers.alt
                        && key.len() == 1
                    {
                        query.push_str(key);
                        query_changed = true;
                    }
                }
            }
        }

        if query_changed {
            if is_app_tab {
                self.update_filtered_apps();
            }
            cx.notify();
        }
    }

    fn render_applications_pane(&self, cx: &mut Context<Self>) -> Div {
        let scopes_list = div().flex().flex_col().children(
            self.settings
                .applications
                .search_scopes
                .iter()
                .enumerate()
                .map(|(idx, scope)| {
                    let is_app = scope.ends_with(".app");
                    div()
                        .id(SharedString::from(format!("scope-row-{idx}")))
                        .flex()
                        .items_center()
                        .justify_between()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(COLOR_BORDER_SUBTLE))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2p5()
                                .child(crate::icons::render_phosphor_svg(
                                    if is_app {
                                        phosphor_svgs::style::regular::APP_WINDOW
                                    } else {
                                        phosphor_svgs::style::regular::FOLDER
                                    },
                                    rgb(COLOR_TEXT_DIM),
                                    15.0,
                                ))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .text_color(rgb(COLOR_TEXT))
                                        .child(scope.clone()),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("remove-scope-{idx}")))
                                .size(px(20.0))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    if idx < this.settings.applications.search_scopes.len() {
                                        this.settings.applications.search_scopes.remove(idx);
                                        this.save_settings_file();
                                        this.reload_apps();
                                        cx.notify();
                                    }
                                }))
                                .child(crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::X_CIRCLE,
                                    rgb(COLOR_TEXT_DIM),
                                    14.0,
                                )),
                        )
                }),
        );

        let add_scope_view = if self.adding_scope {
            div()
                .id("new-scope-input-row")
                .mt_2()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(rgb(COLOR_CARD_BG))
                        .border_1()
                        .border_color(rgb(COLOR_ACCENT))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(crate::icons::render_phosphor_svg(
                            phosphor_svgs::style::regular::FOLDER,
                            rgb(COLOR_TEXT_DIM),
                            14.0,
                        ))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(if self.new_scope_text.is_empty() {
                                    rgb(COLOR_TEXT_DIM)
                                } else {
                                    rgb(COLOR_TEXT)
                                })
                                .child(if self.new_scope_text.is_empty() {
                                    // Suggest a folder that exists on the
                                    // running platform.
                                    let example = if cfg!(target_os = "macos") {
                                        "~/Applications"
                                    } else {
                                        "~/Documents"
                                    };
                                    format!("Type path (e.g. {example}) and press Enter...")
                                } else {
                                    self.new_scope_text.clone()
                                }),
                        ),
                )
                .child(
                    div()
                        .id("save-new-scope-btn")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(COLOR_ACCENT))
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x064e3b))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            let trimmed = this.new_scope_text.trim().to_string();
                            if !trimmed.is_empty()
                                && !this.settings.applications.search_scopes.contains(&trimmed)
                            {
                                this.settings.applications.search_scopes.push(trimmed);
                                this.save_settings_file();
                                this.reload_apps();
                            }
                            this.adding_scope = false;
                            this.new_scope_text.clear();
                            cx.notify();
                        }))
                        .child("Add"),
                )
                .child(
                    div()
                        .id("cancel-new-scope-btn")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(COLOR_CONTROL_BG))
                        .border_1()
                        .border_color(rgb(COLOR_CONTROL_BORDER))
                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .text_color(rgb(COLOR_TEXT_MUTED))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.adding_scope = false;
                            this.new_scope_text.clear();
                            cx.notify();
                        }))
                        .child("Cancel"),
                )
        } else {
            div()
                .id("add-scope-btn")
                .mt_2()
                .w(px(70.0))
                .px_3()
                .py_1()
                .rounded_md()
                .bg(rgb(COLOR_CONTROL_BG))
                .border_1()
                .border_color(rgb(COLOR_CONTROL_BORDER))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(COLOR_TEXT))
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.adding_scope = true;
                    this.new_scope_text.clear();
                    cx.notify();
                }))
                .child("Add...")
        };

        div()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: Search Scopes
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Search Scopes"),
                    )
                    .child(scopes_list)
                    .child(add_scope_view),
            )
            // Section 2: Applications
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Applications"),
                    )
                    .child(self.toggle_row(
                        "toggle-enable-apps",
                        "Enable Applications",
                        Some("Off hides all of them and stops their shortcuts."),
                        self.settings.applications.enabled,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.settings.applications.enabled =
                                !this.settings.applications.enabled;
                            this.save_settings_file();
                            cx.notify();
                        }),
                    ))
                    // Card container
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .py_2()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // Search row inside card
                            .child(
                                div()
                                    .h(px(38.0))
                                    .px_3()
                                    .border_b_1()
                                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                                    .flex()
                                    .items_center()
                                    .gap_2p5()
                                    .child(crate::icons::render_phosphor_svg(
                                        phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                        rgb(COLOR_TEXT_DIM),
                                        14.0,
                                    ))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(13.0))
                                            .text_color(if self.app_search_query.is_empty() {
                                                rgb(COLOR_TEXT_DIM)
                                            } else {
                                                rgb(COLOR_TEXT)
                                            })
                                            .child(if self.app_search_query.is_empty() {
                                                "Search applications...".to_string()
                                            } else {
                                                self.app_search_query.clone()
                                            }),
                                    )
                                    .when(!self.app_search_query.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .id("clear-app-search")
                                                .size(px(18.0))
                                                .rounded_full()
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                .on_click(cx.listener(
                                                    |this, _: &ClickEvent, _window, cx| {
                                                        this.app_search_query.clear();
                                                        this.update_filtered_apps();
                                                        cx.notify();
                                                    },
                                                ))
                                                .child(crate::icons::render_phosphor_svg(
                                                    phosphor_svgs::style::regular::X,
                                                    rgb(COLOR_TEXT_DIM),
                                                    11.0,
                                                )),
                                        )
                                    }),
                            )
                            // App list items (Windowed Virtualization for locked 120 FPS scrolling)
                            .child({
                                const APP_ROW_HEIGHT: f32 = 40.0;
                                const OVERSCAN_ROWS: usize = 12;

                                let num_scopes = self.settings.applications.search_scopes.len();
                                let scopes_h = (num_scopes as f32) * 35.0
                                    + if self.adding_scope { 40.0 } else { 34.0 };
                                let apps_offset_top = 28.0
                                    + 20.0
                                    + 8.0
                                    + scopes_h
                                    + 24.0
                                    + 20.0
                                    + 12.0
                                    + 56.0
                                    + 12.0
                                    + 38.0;

                                let scroll_top =
                                    (-self.content_scroll_handle.offset().y).max(px(0.0));
                                let viewport_h = {
                                    let h = self.content_scroll_handle.bounds().size.height;
                                    if h <= px(0.0) {
                                        px(600.0)
                                    } else {
                                        h
                                    }
                                };

                                let total_apps = self.filtered_apps_cache.len();

                                let card_scroll_top =
                                    (scroll_top - px(apps_offset_top)).max(px(0.0));
                                let card_scroll_bottom =
                                    (scroll_top + viewport_h - px(apps_offset_top)).max(px(0.0));

                                let first_visible =
                                    (card_scroll_top / px(APP_ROW_HEIGHT)).floor() as usize;
                                let last_visible =
                                    (card_scroll_bottom / px(APP_ROW_HEIGHT)).ceil() as usize;

                                let start_idx =
                                    first_visible.saturating_sub(OVERSCAN_ROWS).min(total_apps);
                                let end_idx = (last_visible + OVERSCAN_ROWS).min(total_apps);

                                let top_spacer_h = (start_idx as f32) * APP_ROW_HEIGHT;
                                let bottom_spacer_h =
                                    ((total_apps - end_idx) as f32) * APP_ROW_HEIGHT;

                                div()
                                    .flex()
                                    .flex_col()
                                    .when(top_spacer_h > 0.0, |d| {
                                        d.child(div().h(px(top_spacer_h)))
                                    })
                                    .children((start_idx..end_idx).map(|row_idx| {
                                        let app_idx = self.filtered_apps_cache[row_idx];
                                        let app = &self.apps[app_idx];
                                        let app_name = &app.name;
                                        let icon_el = if let Some(ref icon_path) = app.icon_png {
                                            img(icon_path.clone()).size(px(24.0)).into_any_element()
                                        } else {
                                            crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::APP_WINDOW,
                                                rgb(COLOR_TEXT_DIM),
                                                22.0,
                                            )
                                            .into_any_element()
                                        };

                                        self.render_item_row(
                                            SettingsTab::Applications,
                                            row_idx,
                                            app_name,
                                            app_name,
                                            icon_el,
                                            cx,
                                        )
                                    }))
                                    .when(bottom_spacer_h > 0.0, |d| {
                                        d.child(div().h(px(bottom_spacer_h)))
                                    })
                                    .when(
                                        total_apps == 0 && !self.app_search_query.is_empty(),
                                        |d| {
                                            d.child(
                                                div()
                                                    .h(px(60.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_size(px(13.0))
                                                    .text_color(rgb(COLOR_TEXT_DIM))
                                                    .child("No applications found"),
                                            )
                                        },
                                    )
                            }),
                    ),
            )
    }
}

fn system_setting_icon_bg(id: &str) -> u32 {
    match id {
        "accessibility"
        | "airdrop-continuity"
        | "background-security"
        | "bluetooth"
        | "desktop-dock"
        | "displays"
        | "network"
        | "privacy-security"
        | "wi-fi" => 0x007aff,
        "battery" => 0x34c759,
        "applecare-warranty" | "date-time" | "notifications" => 0xff3b30,
        "sound" => 0xff2d55,
        "wallpaper" => 0x32ade6,
        "appearance" => 0x2c2c2e,
        _ => 0x636366,
    }
}

impl SettingsView {
    fn render_system_settings_pane(&self, cx: &mut Context<Self>) -> Div {
        let settings_list = corvo_system_actions::get_system_settings();
        let query_lower = self.system_settings_search_query.to_lowercase();
        let filtered_settings: Vec<_> = settings_list
            .into_iter()
            .filter(|setting| {
                if query_lower.is_empty() {
                    return true;
                }
                setting.title.to_lowercase().contains(&query_lower)
                    || setting.keywords.to_lowercase().contains(&query_lower)
                    || self
                        .settings
                        .system_settings
                        .items
                        .get(setting.id)
                        .or_else(|| self.settings.system_settings.items.get(setting.title))
                        .and_then(|c| c.alias.as_deref())
                        .is_some_and(|a| a.to_lowercase().contains(&query_lower))
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child("System Settings"),
            )
            .child(self.toggle_row(
                "toggle-enable-system-settings",
                "Enable System Settings",
                Some("Off hides all of them and stops their shortcuts."),
                self.settings.system_settings.enabled,
                cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.settings.system_settings.enabled = !this.settings.system_settings.enabled;
                    this.save_settings_file();
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .rounded_lg()
                    .bg(rgb(COLOR_CARD_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Search row inside card
                    .child(
                        div()
                            .h(px(38.0))
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .child(crate::icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                rgb(COLOR_TEXT_DIM),
                                14.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.0))
                                    .text_color(if self.system_settings_search_query.is_empty() {
                                        rgb(COLOR_TEXT_DIM)
                                    } else {
                                        rgb(COLOR_TEXT)
                                    })
                                    .child(if self.system_settings_search_query.is_empty() {
                                        "Search System Settings...".to_string()
                                    } else {
                                        self.system_settings_search_query.clone()
                                    }),
                            )
                            .when(!self.system_settings_search_query.is_empty(), |el| {
                                el.child(
                                    div()
                                        .id("clear-system-settings-search")
                                        .size(px(18.0))
                                        .rounded_full()
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                        .on_click(cx.listener(
                                            |this, _: &ClickEvent, _window, cx| {
                                                this.system_settings_search_query.clear();
                                                cx.notify();
                                            },
                                        ))
                                        .child(crate::icons::render_phosphor_svg(
                                            phosphor_svgs::style::regular::X,
                                            rgb(COLOR_TEXT_DIM),
                                            11.0,
                                        )),
                                )
                            }),
                    )
                    // Settings list items
                    .child(
                        div().flex().flex_col().children(
                            filtered_settings
                                .into_iter()
                                .enumerate()
                                .map(|(idx, setting)| {
                                    let bg_color = system_setting_icon_bg(setting.id);
                                    let icon_el = div()
                                        .size(px(22.0))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(rgb(bg_color))
                                        .child(crate::icons::render_phosphor_svg(
                                            setting.icon,
                                            rgb(0xffffff),
                                            13.0,
                                        ))
                                        .into_any_element();

                                    self.render_item_row(
                                        SettingsTab::SystemSettings,
                                        idx,
                                        setting.id,
                                        setting.title,
                                        icon_el,
                                        cx,
                                    )
                                }),
                        ),
                    ),
            )
    }

    fn render_system_actions_pane(&self, cx: &mut Context<Self>) -> Div {
        let actions_list = corvo_system_actions::get_system_actions();
        let query_lower = self.system_actions_search_query.to_lowercase();
        let filtered_actions: Vec<_> = actions_list
            .into_iter()
            .filter(|action| {
                if query_lower.is_empty() {
                    return true;
                }
                action.title.to_lowercase().contains(&query_lower)
                    || action.keywords.to_lowercase().contains(&query_lower)
                    || self
                        .settings
                        .system_actions
                        .items
                        .get(action.id)
                        .or_else(|| self.settings.system_actions.items.get(action.title))
                        .and_then(|c| c.alias.as_deref())
                        .is_some_and(|a| a.to_lowercase().contains(&query_lower))
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child("System Actions"),
            )
            .child(self.toggle_row(
                "toggle-enable-system-actions",
                "Enable System Actions",
                Some("Off hides all of them and stops their shortcuts."),
                self.settings.system_actions.enabled,
                cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.settings.system_actions.enabled = !this.settings.system_actions.enabled;
                    this.save_settings_file();
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .rounded_lg()
                    .bg(rgb(COLOR_CARD_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Search row inside card
                    .child(
                        div()
                            .h(px(38.0))
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .child(crate::icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                rgb(COLOR_TEXT_DIM),
                                14.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.0))
                                    .text_color(if self.system_actions_search_query.is_empty() {
                                        rgb(COLOR_TEXT_DIM)
                                    } else {
                                        rgb(COLOR_TEXT)
                                    })
                                    .child(if self.system_actions_search_query.is_empty() {
                                        "Search system actions...".to_string()
                                    } else {
                                        self.system_actions_search_query.clone()
                                    }),
                            )
                            .when(!self.system_actions_search_query.is_empty(), |el| {
                                el.child(
                                    div()
                                        .id("clear-system-actions-search")
                                        .size(px(18.0))
                                        .rounded_full()
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                        .on_click(cx.listener(
                                            |this, _: &ClickEvent, _window, cx| {
                                                this.system_actions_search_query.clear();
                                                cx.notify();
                                            },
                                        ))
                                        .child(crate::icons::render_phosphor_svg(
                                            phosphor_svgs::style::regular::X,
                                            rgb(COLOR_TEXT_DIM),
                                            11.0,
                                        )),
                                )
                            }),
                    )
                    // Actions list items
                    .child(
                        div().flex().flex_col().children(
                            filtered_actions
                                .into_iter()
                                .enumerate()
                                .map(|(idx, action)| {
                                    let icon_el = crate::icons::render_phosphor_svg(
                                        action.icon,
                                        rgb(COLOR_TEXT_DIM),
                                        18.0,
                                    )
                                    .into_any_element();

                                    self.render_item_row(
                                        SettingsTab::SystemActions,
                                        idx,
                                        action.id,
                                        action.title,
                                        icon_el,
                                        cx,
                                    )
                                }),
                        ),
                    ),
            )
    }

    fn clear_item_hotkey(&mut self, tab: SettingsTab, key: &str, cx: &mut Context<Self>) {
        let key_str = key.to_string();
        match tab {
            SettingsTab::General => {
                self.settings.hotkey.clear();
                self.save_settings_file();
            }
            SettingsTab::Applications => {
                let entry = self
                    .settings
                    .applications
                    .app_configs
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::SystemSettings => {
                let entry = self
                    .settings
                    .system_settings
                    .items
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::SystemActions => {
                let entry = self
                    .settings
                    .system_actions
                    .items
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::Commands => {
                let entry = self.settings.commands.items.entry(key_str).or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::Quicklinks => {
                if let Some(entry) = self
                    .quicklinks_file
                    .quicklinks
                    .iter_mut()
                    .find(|q| q.name == key_str)
                {
                    entry.hotkey = None;
                    self.save_quicklinks_file();
                } else {
                    let entry = self
                        .settings
                        .quicklinks
                        .command_items
                        .entry(key_str)
                        .or_default();
                    entry.hotkey = None;
                    self.save_settings_file();
                }
            }
            SettingsTab::Clipboard => {
                let entry = self
                    .settings
                    .clipboard
                    .command_items
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::Snippets => {
                let entry = self
                    .settings
                    .snippets
                    .command_items
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::FileSearch => {
                let entry = self
                    .settings
                    .file_search
                    .command_items
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::WindowManagement => {
                if let Some(layout_id) = key.strip_prefix("layout:") {
                    if let Some(l) = self
                        .settings
                        .window_management
                        .layouts
                        .iter_mut()
                        .find(|l| l.id == layout_id)
                    {
                        l.hotkey = None;
                        self.save_settings_file();
                    }
                } else {
                    let entry = self
                        .settings
                        .window_management
                        .command_items
                        .entry(key_str)
                        .or_default();
                    entry.hotkey = Some(String::new());
                    self.save_settings_file();
                }
            }
            SettingsTab::Navigation => {
                let entry = self
                    .settings
                    .navigation
                    .command_items
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            SettingsTab::Emojis => {
                let entry = self
                    .settings
                    .emojis
                    .command_items
                    .entry(key_str)
                    .or_default();
                entry.hotkey = None;
                self.save_settings_file();
            }
            _ => {}
        }
        self.set_recording_hotkey_item(None, cx);
        cx.notify();
    }

    fn render_hotkey_control(
        &self,
        tab: SettingsTab,
        key_str: &str,
        hotkey_opt: Option<&String>,
        is_recording: bool,
        id_prefix: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key_for_click = key_str.to_string();
        let key_for_clear = key_str.to_string();

        if let Some(hotkey) = hotkey_opt {
            let keycaps = format_hotkey_keycaps(hotkey);
            let border_color = if is_recording {
                rgb(0x3b82f6)
            } else {
                rgb(COLOR_CONTROL_BORDER)
            };

            div()
                .id(SharedString::from(format!("{id_prefix}-hotkey-btn")))
                .h(px(26.0))
                .px(px(6.0))
                .rounded_md()
                .bg(rgb(COLOR_CONTROL_BG))
                .border_1()
                .border_color(border_color)
                .flex()
                .items_center()
                .gap(px(4.0))
                .cursor_pointer()
                .hover(move |s| {
                    if is_recording {
                        s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x3b82f6))
                    } else {
                        s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e))
                    }
                })
                .on_click(cx.listener({
                    let key = key_for_click.clone();
                    move |this, _: &ClickEvent, _window, cx| {
                        this.set_recording_hotkey_item(Some((tab, key.clone())), cx);
                        cx.notify();
                    }
                }))
                .children(keycaps.into_iter().map(|cap| {
                    div()
                        .h(px(18.0))
                        .px(px(5.0))
                        .rounded_sm()
                        .bg(rgb(0x32353c))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0xd1d5db))
                                .child(cap),
                        )
                }))
                .child(
                    div()
                        .id(SharedString::from(format!("{id_prefix}-clear-hotkey")))
                        .size(px(16.0))
                        .ml(px(2.0))
                        .rounded_full()
                        .bg(rgb(0x3a3c42))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0x4e525b)))
                        .on_click(cx.listener({
                            let key = key_for_clear.clone();
                            move |this, _: &ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                this.clear_item_hotkey(tab, &key, cx);
                            }
                        }))
                        .child(crate::icons::render_phosphor_svg(
                            phosphor_svgs::style::regular::X,
                            rgb(0x9ca3af),
                            10.0,
                        )),
                )
                .into_any_element()
        } else if is_recording {
            div()
                .id(SharedString::from(format!("{id_prefix}-recording-btn")))
                .h(px(26.0))
                .px_2p5()
                .rounded_md()
                .bg(rgb(COLOR_CONTROL_BG))
                .border_1()
                .border_color(rgb(0x3b82f6))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x3b82f6))
                        .child("Press keys..."),
                )
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("{id_prefix}-record-btn")))
                .h(px(26.0))
                .px_2p5()
                .rounded_md()
                .bg(rgb(COLOR_CONTROL_BG))
                .border_1()
                .border_color(rgb(COLOR_CONTROL_BORDER))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                .on_click(cx.listener({
                    let key = key_for_click.clone();
                    move |this, _: &ClickEvent, _window, cx| {
                        this.set_recording_hotkey_item(Some((tab, key.clone())), cx);
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child("Record"),
                )
                .into_any_element()
        }
    }

    fn render_item_row(
        &self,
        tab: SettingsTab,
        idx: usize,
        key: &str,
        title: &str,
        icon: AnyElement,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key_str = key.to_string();
        let (is_hidden, alias_text, hotkey_text) = match tab {
            SettingsTab::Applications => self
                .settings
                .applications
                .app_configs
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::SystemSettings => self
                .settings
                .system_settings
                .items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::SystemActions => self
                .settings
                .system_actions
                .items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::Commands => self
                .settings
                .commands
                .items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::Quicklinks => {
                if let Some(entry) = self
                    .quicklinks_file
                    .quicklinks
                    .iter()
                    .find(|q| q.name == key)
                {
                    (entry.hidden, entry.alias.clone(), entry.hotkey.clone())
                } else {
                    self.settings
                        .quicklinks
                        .command_items
                        .get(key)
                        .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                        .unwrap_or((false, None, None))
                }
            }
            SettingsTab::Clipboard => self
                .settings
                .clipboard
                .command_items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::Snippets => self
                .settings
                .snippets
                .command_items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::FileSearch => self
                .settings
                .file_search
                .command_items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::WindowManagement => {
                if let Some(layout_id) = key.strip_prefix("layout:") {
                    self.settings
                        .window_management
                        .layouts
                        .iter()
                        .find(|l| l.id == layout_id)
                        .map(|l| (false, None, l.hotkey.clone()))
                        .unwrap_or((false, None, None))
                } else {
                    let default_hk = corvo_window_management::WINDOW_ACTIONS
                        .iter()
                        .find(|a| a.id == key)
                        .and_then(|a| a.hotkey)
                        .map(|h| h.to_string());

                    self.settings
                        .window_management
                        .command_items
                        .get(key)
                        .map(|c| {
                            let hk = match &c.hotkey {
                                Some(h) if h.is_empty() => None,
                                Some(h) => Some(h.clone()),
                                None => default_hk.clone(),
                            };
                            (c.hidden, c.alias.clone(), hk)
                        })
                        .unwrap_or((false, None, default_hk))
                }
            }
            SettingsTab::Navigation => self
                .settings
                .navigation
                .command_items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            SettingsTab::Emojis => self
                .settings
                .emojis
                .command_items
                .get(key)
                .map(|c| (c.hidden, c.alias.clone(), c.hotkey.clone()))
                .unwrap_or((false, None, None)),
            _ => (false, None, None),
        };

        let is_editing_alias = self
            .editing_alias_item
            .as_ref()
            .is_some_and(|(t, k)| *t == tab && k == key);
        let is_recording_hotkey = self
            .recording_hotkey_item
            .as_ref()
            .is_some_and(|(t, k)| *t == tab && k == key);

        let tab_prefix = match tab {
            SettingsTab::Applications => "app",
            SettingsTab::SystemSettings => "system-setting",
            SettingsTab::SystemActions => "system-action",
            SettingsTab::Commands => "command",
            SettingsTab::Quicklinks => "quicklink",
            SettingsTab::Clipboard => "clipboard",
            SettingsTab::Snippets => "snippet",
            SettingsTab::FileSearch => "file-search",
            SettingsTab::WindowManagement => "window-management",
            SettingsTab::Navigation => "navigation",
            SettingsTab::Emojis => "emojis",
            _ => "item",
        };

        div()
            .id(SharedString::from(format!("{tab_prefix}-row-{idx}")))
            .h(px(40.0))
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .border_b_1()
            .border_color(rgb(COLOR_BORDER_SUBTLE))
            .hover(|s| s.bg(rgb(COLOR_ROW_HOVER)))
            .child(
                div().flex().items_center().gap_3().child(icon).child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if is_hidden {
                            rgb(COLOR_TEXT_DIM)
                        } else {
                            rgb(COLOR_TEXT)
                        })
                        .child(title.to_string()),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Add Alias / Alias pill
                    .child(if is_editing_alias {
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(COLOR_CARD_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_ACCENT))
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_TEXT))
                                    .child(if self.alias_input_text.is_empty() {
                                        "Type alias...".to_string()
                                    } else {
                                        self.alias_input_text.clone()
                                    }),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "{tab_prefix}-done-alias-btn-{idx}"
                                    )))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(COLOR_ACCENT))
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x064e3b))
                                    .on_click(cx.listener({
                                        let key_str = key_str.clone();
                                        move |this, _: &ClickEvent, _window, cx| {
                                            let trimmed = this.alias_input_text.trim();
                                            match tab {
                                                SettingsTab::Applications => {
                                                    let entry = this
                                                        .settings
                                                        .applications
                                                        .app_configs
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::SystemSettings => {
                                                    let entry = this
                                                        .settings
                                                        .system_settings
                                                        .items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::SystemActions => {
                                                    let entry = this
                                                        .settings
                                                        .system_actions
                                                        .items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::Commands => {
                                                    let entry = this
                                                        .settings
                                                        .commands
                                                        .items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::Quicklinks => {
                                                    if let Some(entry) = this
                                                        .quicklinks_file
                                                        .quicklinks
                                                        .iter_mut()
                                                        .find(|q| q.name == key_str)
                                                    {
                                                        entry.alias = if trimmed.is_empty() {
                                                            None
                                                        } else {
                                                            Some(trimmed.to_string())
                                                        };
                                                        this.save_quicklinks_file();
                                                    } else {
                                                        let entry = this
                                                            .settings
                                                            .quicklinks
                                                            .command_items
                                                            .entry(key_str.clone())
                                                            .or_default();
                                                        entry.alias = if trimmed.is_empty() {
                                                            None
                                                        } else {
                                                            Some(trimmed.to_string())
                                                        };
                                                        this.save_settings_file();
                                                    }
                                                }
                                                SettingsTab::Clipboard => {
                                                    let entry = this
                                                        .settings
                                                        .clipboard
                                                        .command_items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::Snippets => {
                                                    let entry = this
                                                        .settings
                                                        .snippets
                                                        .command_items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::FileSearch => {
                                                    let entry = this
                                                        .settings
                                                        .file_search
                                                        .command_items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::WindowManagement => {
                                                    let entry = this
                                                        .settings
                                                        .window_management
                                                        .command_items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::Navigation => {
                                                    let entry = this
                                                        .settings
                                                        .navigation
                                                        .command_items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                SettingsTab::Emojis => {
                                                    let entry = this
                                                        .settings
                                                        .emojis
                                                        .command_items
                                                        .entry(key_str.clone())
                                                        .or_default();
                                                    entry.alias = if trimmed.is_empty() {
                                                        None
                                                    } else {
                                                        Some(trimmed.to_string())
                                                    };
                                                    this.save_settings_file();
                                                }
                                                _ => {}
                                            }
                                            this.editing_alias_item = None;
                                            this.alias_input_text.clear();
                                            cx.notify();
                                        }
                                    }))
                                    .child("Done"),
                            )
                            .into_any_element()
                    } else if let Some(ref alias) = alias_text {
                        div()
                            .id(SharedString::from(format!("{tab_prefix}-alias-pill-{idx}")))
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_2p5()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(COLOR_CONTROL_BG))
                            .border_1()
                            .border_color(rgb(COLOR_CONTROL_BORDER))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                            .on_click(cx.listener({
                                let key_str = key_str.clone();
                                let alias = alias.clone();
                                move |this, _: &ClickEvent, _window, cx| {
                                    this.editing_alias_item = Some((tab, key_str.clone()));
                                    this.alias_input_text = alias.clone();
                                    cx.notify();
                                }
                            }))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(COLOR_ACCENT))
                                    .child(format!("alias: {alias}")),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .id(SharedString::from(format!(
                                "{tab_prefix}-add-alias-btn-{idx}"
                            )))
                            .px_2p5()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(COLOR_CONTROL_BG))
                            .border_1()
                            .border_color(rgb(COLOR_CONTROL_BORDER))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                            .on_click(cx.listener({
                                let key_str = key_str.clone();
                                move |this, _: &ClickEvent, _window, cx| {
                                    this.editing_alias_item = Some((tab, key_str.clone()));
                                    this.alias_input_text.clear();
                                    cx.notify();
                                }
                            }))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child("Add Alias"),
                            )
                            .into_any_element()
                    })
                    // Record / Hotkey button
                    .child(self.render_hotkey_control(
                        tab,
                        &key_str,
                        hotkey_text.as_ref(),
                        is_recording_hotkey,
                        &format!("{tab_prefix}-{idx}"),
                        cx,
                    ))
                    // Checkbox
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "{tab_prefix}-toggle-visibility-{idx}"
                            )))
                            .size(px(16.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(!is_hidden, |b| b.bg(rgb(COLOR_ACCENT)))
                            .when(is_hidden, |b| {
                                b.bg(rgb(COLOR_CONTROL_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_CONTROL_BORDER))
                            })
                            .on_click(cx.listener({
                                let key_str = key_str.clone();
                                move |this, _: &ClickEvent, _window, cx| {
                                    match tab {
                                        SettingsTab::Applications => {
                                            let entry = this
                                                .settings
                                                .applications
                                                .app_configs
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::SystemSettings => {
                                            let entry = this
                                                .settings
                                                .system_settings
                                                .items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::SystemActions => {
                                            let entry = this
                                                .settings
                                                .system_actions
                                                .items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::Commands => {
                                            let entry = this
                                                .settings
                                                .commands
                                                .items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::Quicklinks => {
                                            if let Some(entry) = this
                                                .quicklinks_file
                                                .quicklinks
                                                .iter_mut()
                                                .find(|q| q.name == key_str)
                                            {
                                                entry.hidden = !entry.hidden;
                                                this.save_quicklinks_file();
                                            } else {
                                                let entry = this
                                                    .settings
                                                    .quicklinks
                                                    .command_items
                                                    .entry(key_str.clone())
                                                    .or_default();
                                                entry.hidden = !entry.hidden;
                                                this.save_settings_file();
                                            }
                                        }
                                        SettingsTab::Clipboard => {
                                            let entry = this
                                                .settings
                                                .clipboard
                                                .command_items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::Snippets => {
                                            let entry = this
                                                .settings
                                                .snippets
                                                .command_items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::FileSearch => {
                                            let entry = this
                                                .settings
                                                .file_search
                                                .command_items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::WindowManagement => {
                                            let entry = this
                                                .settings
                                                .window_management
                                                .command_items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::Navigation => {
                                            let entry = this
                                                .settings
                                                .navigation
                                                .command_items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        SettingsTab::Emojis => {
                                            let entry = this
                                                .settings
                                                .emojis
                                                .command_items
                                                .entry(key_str.clone())
                                                .or_default();
                                            entry.hidden = !entry.hidden;
                                            this.save_settings_file();
                                        }
                                        _ => {}
                                    }
                                    cx.notify();
                                }
                            }))
                            .when(!is_hidden, |b| {
                                b.child(crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::CHECK,
                                    rgb(0x064e3b),
                                    11.0,
                                ))
                            }),
                    ),
            )
    }

    fn save_quicklink(&mut self, cx: &mut Context<Self>) {
        let name = self.quicklink_name_input.trim().to_string();
        let url = self.quicklink_url_input.trim().to_string();
        if name.is_empty() || url.is_empty() {
            return;
        }

        if let Some(idx) = self.editing_quicklink_index {
            if let Some(entry) = self.quicklinks_file.quicklinks.get_mut(idx) {
                entry.name = name;
                entry.url = url;
            }
        } else if self.adding_quicklink {
            self.quicklinks_file
                .quicklinks
                .push(corvo_config::QuicklinkEntry {
                    name,
                    url,
                    alias: None,
                    hotkey: None,
                    hidden: false,
                });
        }
        self.save_quicklinks_file();
        self.adding_quicklink = false;
        self.editing_quicklink_index = None;
        self.quicklink_name_input.clear();
        self.quicklink_url_input.clear();
        cx.notify();
    }

    fn render_commands_pane(&self, cx: &mut Context<Self>) -> Div {
        let commands = get_builtin_commands();
        let query_lower = self.commands_search_query.to_lowercase();
        let filtered_commands: Vec<_> = commands
            .iter()
            .filter(|cmd| {
                if query_lower.is_empty() {
                    return true;
                }
                cmd.title.to_lowercase().contains(&query_lower)
                    || self
                        .settings
                        .commands
                        .items
                        .get(cmd.id)
                        .or_else(|| self.settings.commands.items.get(cmd.title))
                        .and_then(|c| c.alias.as_deref())
                        .is_some_and(|a| a.to_lowercase().contains(&query_lower))
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child("Commands"),
            )
            .child(self.toggle_row(
                "toggle-enable-commands",
                "Enable Commands",
                Some("Off hides all of them and stops their shortcuts."),
                self.settings.commands.enabled,
                cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.settings.commands.enabled = !this.settings.commands.enabled;
                    this.save_settings_file();
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .rounded_lg()
                    .bg(rgb(COLOR_CARD_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Search row inside card
                    .child(
                        div()
                            .h(px(38.0))
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .child(crate::icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                rgb(COLOR_TEXT_DIM),
                                14.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.0))
                                    .text_color(if self.commands_search_query.is_empty() {
                                        rgb(COLOR_TEXT_DIM)
                                    } else {
                                        rgb(COLOR_TEXT)
                                    })
                                    .child(if self.commands_search_query.is_empty() {
                                        "Search commands...".to_string()
                                    } else {
                                        self.commands_search_query.clone()
                                    }),
                            )
                            .when(!self.commands_search_query.is_empty(), |el| {
                                el.child(
                                    div()
                                        .id("clear-commands-search")
                                        .size(px(18.0))
                                        .rounded_full()
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                        .on_click(cx.listener(
                                            |this, _: &ClickEvent, _window, cx| {
                                                this.commands_search_query.clear();
                                                cx.notify();
                                            },
                                        ))
                                        .child(crate::icons::render_phosphor_svg(
                                            phosphor_svgs::style::regular::X,
                                            rgb(COLOR_TEXT_DIM),
                                            11.0,
                                        )),
                                )
                            }),
                    )
                    // Commands list items
                    .child(div().flex().flex_col().children(
                        filtered_commands.into_iter().enumerate().map(|(idx, cmd)| {
                            let icon_el = crate::icons::render_phosphor_svg(
                                cmd.icon,
                                rgb(COLOR_TEXT_DIM),
                                18.0,
                            )
                            .into_any_element();

                            self.render_item_row(
                                SettingsTab::Commands,
                                idx,
                                cmd.id,
                                cmd.title,
                                icon_el,
                                cx,
                            )
                        }),
                    )),
            )
    }

    fn render_quicklinks_pane(&self, cx: &mut Context<Self>) -> Div {
        let quicklinks_commands: [(&str, &str, &'static str); 4] = [
            (
                "create-quicklink",
                "Create Quicklink",
                phosphor_svgs::style::regular::LINK_SIMPLE,
            ),
            (
                "search-quicklinks",
                "Search Quicklinks",
                phosphor_svgs::style::regular::LINK,
            ),
            (
                "import-quicklinks",
                "Import Quicklinks",
                phosphor_svgs::style::regular::DOWNLOAD_SIMPLE,
            ),
            (
                "export-quicklinks",
                "Export Quicklinks",
                phosphor_svgs::style::regular::EXPORT,
            ),
        ];

        let query_lower = self.quicklinks_search_query.to_lowercase();
        let filtered_user_quicklinks: Vec<(usize, corvo_config::QuicklinkEntry)> = self
            .quicklinks_file
            .quicklinks
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                if query_lower.is_empty() {
                    return true;
                }
                q.name.to_lowercase().contains(&query_lower)
                    || q.url.to_lowercase().contains(&query_lower)
                    || q.alias
                        .as_deref()
                        .is_some_and(|a| a.to_lowercase().contains(&query_lower))
            })
            .map(|(i, q)| (i, q.clone()))
            .collect();

        let mut pane = div()
            .relative()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: Quicklinks toggles
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Quicklinks"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .px_4()
                            .py_2()
                            .flex()
                            .flex_col()
                            .child(
                                self.toggle_row(
                                    "toggle-enable-quicklinks",
                                    "Enable quicklinks",
                                    None,
                                    self.settings.quicklinks.enabled,
                                    cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.settings.quicklinks.enabled = !this.settings.quicklinks.enabled;
                                        this.save_settings_file();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .h(px(1.0))
                                    .bg(rgb(COLOR_BORDER_SUBTLE)),
                            )
                            .child(
                                self.toggle_row(
                                    "toggle-show-in-launcher",
                                    "Show in launcher",
                                    None,
                                    self.settings.quicklinks.show_in_launcher,
                                    cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.settings.quicklinks.show_in_launcher = !this.settings.quicklinks.show_in_launcher;
                                        this.save_settings_file();
                                        cx.notify();
                                    }),
                                ),
                            ),
                    ),
            )
            // Section 2: Commands
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Commands"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .flex()
                            .flex_col()
                            .children(
                                quicklinks_commands.iter().enumerate().map(|(idx, (id, title, icon))| {
                                    let icon_el = crate::icons::render_phosphor_svg(
                                        icon,
                                        rgb(COLOR_TEXT_DIM),
                                        18.0,
                                    )
                                    .into_any_element();

                                    self.render_item_row(
                                        SettingsTab::Quicklinks,
                                        idx,
                                        id,
                                        title,
                                        icon_el,
                                        cx,
                                    )
                                }),
                            ),
                    ),
            )
            // Section 3: User Quicklinks Card
            .child(
                div()
                    .rounded_lg()
                    .bg(rgb(COLOR_CARD_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Search row inside card
                    .child(
                        div()
                            .h(px(38.0))
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .child(crate::icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                rgb(COLOR_TEXT_DIM),
                                14.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.0))
                                    .text_color(if self.quicklinks_search_query.is_empty() { rgb(COLOR_TEXT_DIM) } else { rgb(COLOR_TEXT) })
                                    .child(if self.quicklinks_search_query.is_empty() { "Search quicklinks...".to_string() } else { self.quicklinks_search_query.clone() }),
                            )
                            .when(!self.quicklinks_search_query.is_empty(), |el| {
                                el.child(
                                    div()
                                        .id("clear-quicklinks-search")
                                        .size(px(18.0))
                                        .rounded_full()
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                            this.quicklinks_search_query.clear();
                                            cx.notify();
                                        }))
                                        .child(crate::icons::render_phosphor_svg(
                                            phosphor_svgs::style::regular::X,
                                            rgb(COLOR_TEXT_DIM),
                                            11.0,
                                        )),
                                )
                            }),
                    )
                    // Quicklink rows
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .children(
                                filtered_user_quicklinks.into_iter().map(|(orig_idx, q)| {
                                    let name_clone = q.name.clone();
                                    let url_clone = q.url.clone();
                                    let is_hidden = q.hidden;
                                    let alias_text = q.alias.clone();
                                    let hotkey_text = q.hotkey.clone();
                                    let is_editing_alias = self.editing_alias_item.as_ref().is_some_and(|(t, k)| *t == SettingsTab::Quicklinks && k == &name_clone);
                                    let is_recording_hotkey = self.recording_hotkey_item.as_ref().is_some_and(|(t, k)| *t == SettingsTab::Quicklinks && k == &name_clone);

                                    div()
                                        .id(SharedString::from(format!("user-quicklink-row-{orig_idx}")))
                                        .h(px(46.0))
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .px_3()
                                        .border_b_1()
                                        .border_color(rgb(COLOR_BORDER_SUBTLE))
                                        .hover(|s| s.bg(rgb(COLOR_ROW_HOVER)))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_3()
                                                .child(crate::icons::render_phosphor_svg(
                                                    phosphor_svgs::style::regular::GLOBE,
                                                    rgb(COLOR_TEXT_DIM),
                                                    18.0,
                                                ))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap_0p5()
                                                        .child(
                                                            div()
                                                                .text_size(px(13.0))
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_color(if is_hidden { rgb(COLOR_TEXT_DIM) } else { rgb(COLOR_TEXT) })
                                                                .child(name_clone.clone()),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(11.0))
                                                                .text_color(rgb(COLOR_TEXT_DIM))
                                                                .child(url_clone.clone()),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                // Add Alias / Alias pill
                                                .child(
                                                    if is_editing_alias {
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_1p5()
                                                            .child(
                                                                div()
                                                                    .px_2()
                                                                    .py_1()
                                                                    .rounded_md()
                                                                    .bg(rgb(COLOR_CARD_BG))
                                                                    .border_1()
                                                                    .border_color(rgb(COLOR_ACCENT))
                                                                    .text_size(px(12.0))
                                                                    .text_color(rgb(COLOR_TEXT))
                                                                    .child(if self.alias_input_text.is_empty() { "Type alias...".to_string() } else { self.alias_input_text.clone() }),
                                                            )
                                                            .child(
                                                                div()
                                                                    .id(SharedString::from(format!("quicklink-done-alias-btn-{orig_idx}")))
                                                                    .px_2()
                                                                    .py_1()
                                                                    .rounded_md()
                                                                    .bg(rgb(COLOR_ACCENT))
                                                                    .cursor_pointer()
                                                                    .text_size(px(11.0))
                                                                    .font_weight(FontWeight::SEMIBOLD)
                                                                    .text_color(rgb(0x064e3b))
                                                                    .on_click(cx.listener({
                                                                        let name_clone = name_clone.clone();
                                                                        move |this, _: &ClickEvent, _window, cx| {
                                                                            let trimmed = this.alias_input_text.trim();
                                                                            if let Some(entry) = this.quicklinks_file.quicklinks.iter_mut().find(|q| q.name == name_clone) {
                                                                                entry.alias = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
                                                                                this.save_quicklinks_file();
                                                                            }
                                                                            this.editing_alias_item = None;
                                                                            this.alias_input_text.clear();
                                                                            cx.notify();
                                                                        }
                                                                    }))
                                                                    .child("Done"),
                                                            )
                                                            .into_any_element()
                                                    } else if let Some(ref alias) = alias_text {
                                                        div()
                                                            .id(SharedString::from(format!("quicklink-alias-pill-{orig_idx}")))
                                                            .px_2p5()
                                                            .py_1()
                                                            .rounded_md()
                                                            .bg(rgb(COLOR_CONTROL_BG))
                                                            .border_1()
                                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                                            .cursor_pointer()
                                                            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                                                            .on_click(cx.listener({
                                                                let name_clone = name_clone.clone();
                                                                let alias = alias.clone();
                                                                move |this, _: &ClickEvent, _window, cx| {
                                                                    this.editing_alias_item = Some((SettingsTab::Quicklinks, name_clone.clone()));
                                                                    this.alias_input_text = alias.clone();
                                                                    cx.notify();
                                                                }
                                                            }))
                                                            .child(
                                                                div()
                                                                    .text_size(px(12.0))
                                                                    .font_weight(FontWeight::MEDIUM)
                                                                    .text_color(rgb(COLOR_ACCENT))
                                                                    .child(format!("alias: {alias}")),
                                                            )
                                                            .into_any_element()
                                                    } else {
                                                        div()
                                                            .id(SharedString::from(format!("quicklink-add-alias-btn-{orig_idx}")))
                                                            .px_2p5()
                                                            .py_1()
                                                            .rounded_md()
                                                            .bg(rgb(COLOR_CONTROL_BG))
                                                            .border_1()
                                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                                            .cursor_pointer()
                                                            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                                                            .on_click(cx.listener({
                                                                let name_clone = name_clone.clone();
                                                                move |this, _: &ClickEvent, _window, cx| {
                                                                    this.editing_alias_item = Some((SettingsTab::Quicklinks, name_clone.clone()));
                                                                    this.alias_input_text.clear();
                                                                    cx.notify();
                                                                }
                                                            }))
                                                            .child(
                                                                div()
                                                                    .text_size(px(12.0))
                                                                    .text_color(rgb(COLOR_TEXT_DIM))
                                                                    .child("Add Alias"),
                                                            )
                                                            .into_any_element()
                                                    }
                                                )
                                                // Record / Hotkey button
                                                .child(self.render_hotkey_control(
                                                    SettingsTab::Quicklinks,
                                                    &name_clone,
                                                    hotkey_text.as_ref(),
                                                    is_recording_hotkey,
                                                    &format!("quicklink-{orig_idx}"),
                                                    cx,
                                                ))
                                                // Edit (Pencil) button
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("quicklink-edit-btn-{orig_idx}")))
                                                        .size(px(26.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                        .on_click(cx.listener({
                                                            let name_clone = name_clone.clone();
                                                            let url_clone = url_clone.clone();
                                                            move |this, _: &ClickEvent, _window, cx| {
                                                                this.adding_quicklink = false;
                                                                this.editing_quicklink_index = Some(orig_idx);
                                                                this.quicklink_name_input = name_clone.clone();
                                                                this.quicklink_url_input = url_clone.clone();
                                                                this.quicklink_active_field = 0;
                                                                cx.notify();
                                                            }
                                                        }))
                                                        .child(crate::icons::render_phosphor_svg(
                                                            phosphor_svgs::style::regular::PENCIL_SIMPLE,
                                                            rgb(COLOR_TEXT_DIM),
                                                            14.0,
                                                        )),
                                                )
                                                // Delete (Trash) button
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("quicklink-delete-btn-{orig_idx}")))
                                                        .size(px(26.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .hover(|s| s.bg(rgb(0x3a1e20)))
                                                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                                            if orig_idx < this.quicklinks_file.quicklinks.len() {
                                                                this.quicklinks_file.quicklinks.remove(orig_idx);
                                                                this.save_quicklinks_file();
                                                                cx.notify();
                                                            }
                                                        }))
                                                        .child(crate::icons::render_phosphor_svg(
                                                            phosphor_svgs::style::regular::TRASH,
                                                            rgb(0xef4444),
                                                            14.0,
                                                        )),
                                                )
                                                // Checkbox
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("quicklink-toggle-visibility-{orig_idx}")))
                                                        .size(px(16.0))
                                                        .rounded_sm()
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .when(!is_hidden, |b| b.bg(rgb(COLOR_ACCENT)))
                                                        .when(is_hidden, |b| b.bg(rgb(COLOR_CONTROL_BG)).border_1().border_color(rgb(COLOR_CONTROL_BORDER)))
                                                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                                            if let Some(entry) = this.quicklinks_file.quicklinks.get_mut(orig_idx) {
                                                                entry.hidden = !entry.hidden;
                                                                this.save_quicklinks_file();
                                                                cx.notify();
                                                            }
                                                        }))
                                                        .when(!is_hidden, |b| {
                                                            b.child(crate::icons::render_phosphor_svg(
                                                                phosphor_svgs::style::regular::CHECK,
                                                                rgb(0x064e3b),
                                                                11.0,
                                                            ))
                                                        }),
                                                ),
                                        )
                                }),
                            ),
                    )
                    // Add Quicklink button at bottom of card
                    .child(
                        div()
                            .p_3()
                            .child(
                                div()
                                    .id("add-quicklink-btn")
                                    .w_auto()
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(rgb(COLOR_CONTROL_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_CONTROL_BORDER))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(COLOR_TEXT))
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.adding_quicklink = true;
                                        this.editing_quicklink_index = None;
                                        this.quicklink_name_input.clear();
                                        this.quicklink_url_input.clear();
                                        this.quicklink_active_field = 0;
                                        cx.notify();
                                    }))
                                    .child("Add Quicklink"),
                            ),
                    ),
            )
            // Section 4: Behaviour
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Behaviour"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .px_4()
                            .py_2()
                            .child(self.toggle_row(
                                "toggle-open-in-new-window",
                                "Open in a new window",
                                Some("Where the app supports it."),
                                self.settings.quicklinks.open_in_new_window,
                                cx.listener(|this, _: &ClickEvent, _window, cx| {
                                    this.settings.quicklinks.open_in_new_window = !this.settings.quicklinks.open_in_new_window;
                                    this.save_settings_file();
                                    cx.notify();
                                }),
                            )),
                    ),
            );

        if self.adding_quicklink || self.editing_quicklink_index.is_some() {
            let is_editing = self.editing_quicklink_index.is_some();
            pane = pane.child(
                div()
                    .id("quicklink-modal-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00000099))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("quicklink-modal-card")
                            .w(px(420.0))
                            .rounded_xl()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .p_5()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(15.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child(if is_editing { "Edit Quicklink" } else { "New Quicklink" }),
                                    )
                                    .child(
                                        div()
                                            .id("close-quicklink-modal-btn")
                                            .size(px(20.0))
                                            .rounded_full()
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.adding_quicklink = false;
                                                this.editing_quicklink_index = None;
                                                cx.notify();
                                            }))
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::X,
                                                rgb(COLOR_TEXT_DIM),
                                                12.0,
                                            )),
                                    ),
                            )
                            // Name input
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child("Name"),
                                    )
                                    .child(
                                        div()
                                            .id("quicklink-modal-name-input")
                                            .h(px(34.0))
                                            .px_3()
                                            .rounded_md()
                                            .bg(rgb(0x131416))
                                            .border_1()
                                            .border_color(if self.quicklink_active_field == 0 { rgb(COLOR_ACCENT) } else { rgb(COLOR_BORDER_SUBTLE) })
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.quicklink_active_field = 0;
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .text_color(if self.quicklink_name_input.is_empty() { rgb(COLOR_TEXT_DIM) } else { rgb(COLOR_TEXT) })
                                                    .child(if self.quicklink_name_input.is_empty() { "e.g. GitHub Pull Requests".to_string() } else { self.quicklink_name_input.clone() }),
                                            ),
                                    ),
                            )
                            // URL input
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child("Link / URL"),
                                    )
                                    .child(
                                        div()
                                            .id("quicklink-modal-url-input")
                                            .h(px(34.0))
                                            .px_3()
                                            .rounded_md()
                                            .bg(rgb(0x131416))
                                            .border_1()
                                            .border_color(if self.quicklink_active_field == 1 { rgb(COLOR_ACCENT) } else { rgb(COLOR_BORDER_SUBTLE) })
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.quicklink_active_field = 1;
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .text_color(if self.quicklink_url_input.is_empty() { rgb(COLOR_TEXT_DIM) } else { rgb(COLOR_TEXT) })
                                                    .child(if self.quicklink_url_input.is_empty() { "e.g. https://github.com/pulls or with {argument}".to_string() } else { self.quicklink_url_input.clone() }),
                                            ),
                                    ),
                            )
                            // Action buttons
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap_2()
                                    .pt_2()
                                    .child(
                                        div()
                                            .id("quicklink-modal-cancel-btn")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .cursor_pointer()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.adding_quicklink = false;
                                                this.editing_quicklink_index = None;
                                                cx.notify();
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("quicklink-modal-save-btn")
                                            .px_3p5()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(rgb(COLOR_ACCENT))
                                            .cursor_pointer()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0x064e3b))
                                            .hover(|s| s.bg(rgb(0x3eeaa8)))
                                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.save_quicklink(cx);
                                            }))
                                            .child("Save"),
                                    ),
                            ),
                    ),
            );
        }

        pane
    }

    fn render_clipboard_pane(&self, cx: &mut Context<Self>) -> Div {
        let mut pane = div()
            .relative()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: Clipboard History
            .child(
                self.section_group(
                    "Clipboard History",
                    vec![
                        self.toggle_row(
                            "toggle-enable-clipboard",
                            "Enable Clipboard History",
                            Some("Off stops recording copied items."),
                            self.settings.clipboard.enabled,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.clipboard.enabled = !this.settings.clipboard.enabled;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.toggle_row(
                            "toggle-clipboard-auto-paste",
                            "Direct paste on Enter",
                            Some("Directly paste selected item into frontmost application."),
                            self.settings.clipboard.auto_paste,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.clipboard.auto_paste = !this.settings.clipboard.auto_paste;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.toggle_row(
                            "toggle-clipboard-save-images",
                            "Save copied images",
                            Some("Record images and screenshots copied to clipboard."),
                            self.settings.clipboard.save_images,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.clipboard.save_images = !this.settings.clipboard.save_images;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.toggle_row(
                            "toggle-clipboard-save-colors",
                            "Save copied colors",
                            Some("Detect hex and rgb color codes copied to clipboard."),
                            self.settings.clipboard.save_colors,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.clipboard.save_colors = !this.settings.clipboard.save_colors;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.interactive_dropdown_trigger(
                            "clipboard-retention",
                            (
                                "History retention",
                                Some("How long to keep clipboard history items."),
                            ),
                            ActiveDropdown::ClipboardRetention,
                            self.settings.clipboard.retention_option,
                            &["24 Hours", "7 Days", "30 Days", "3 Months", "1 Year"],
                            cx,
                        ),
                    ],
                ),
            )
            // Section 2: Commands
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Commands"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .child(self.render_item_row(
                                SettingsTab::Clipboard,
                                0,
                                "Clipboard History",
                                "Clipboard History",
                                crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::CLIPBOARD_TEXT,
                                    rgb(COLOR_ACCENT),
                                    16.0,
                                ).into_any_element(),
                                cx,
                            ))
                            .child(self.render_item_row(
                                SettingsTab::Clipboard,
                                1,
                                "Clear Clipboard History",
                                "Clear Clipboard History",
                                crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::TRASH,
                                    rgb(COLOR_TEXT_DIM),
                                    16.0,
                                ).into_any_element(),
                                cx,
                            )),
                    ),
            )
            // Section 3: Storage & Security
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Storage & Security"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .px_4()
                            .py_3()
                            .child(
                                if self.confirming_clear_clipboard {
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_0p5()
                                                .child(
                                                    div()
                                                        .text_size(px(13.0))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(rgb(0xef4444))
                                                        .child("Delete all clipboard history?"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(rgb(COLOR_TEXT_DIM))
                                                        .child("This permanently deletes all clips and images. Cannot be undone."),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .id("cancel-clear-clipboard-btn")
                                                        .px_3()
                                                        .py_1p5()
                                                        .rounded_md()
                                                        .bg(rgb(COLOR_CONTROL_BG))
                                                        .border_1()
                                                        .border_color(rgb(COLOR_CONTROL_BORDER))
                                                        .cursor_pointer()
                                                        .text_size(px(12.0))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(rgb(COLOR_TEXT))
                                                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            this.confirming_clear_clipboard = false;
                                                            this.clipboard_clear_feedback = None;
                                                            cx.notify();
                                                        }))
                                                        .child("Cancel"),
                                                )
                                                .child(
                                                    div()
                                                        .id("confirm-clear-clipboard-btn")
                                                        .px_3()
                                                        .py_1p5()
                                                        .rounded_md()
                                                        .bg(rgb(0xef4444))
                                                        .cursor_pointer()
                                                        .text_size(px(12.0))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(rgb(0xffffff))
                                                        .hover(|s| s.bg(rgb(0xdc2626)))
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            let succeeded = corvo_clipboard_manager::clear_history().is_ok();
                                                            this.confirming_clear_clipboard = false;
                                                            this.clipboard_clear_feedback = Some(succeeded);
                                                            cx.notify();
                                                        }))
                                                        .child("Delete All"),
                                                ),
                                        )
                                } else {
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_0p5()
                                                .child(
                                                    div()
                                                        .text_size(px(13.0))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(rgb(COLOR_TEXT))
                                                        .child("Clear All History"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(rgb(COLOR_TEXT_DIM))
                                                        .child("Permanently remove all recorded clipboard clips and images."),
                                                ),
                                        )
                                        .child(
                                            match self.clipboard_clear_feedback {
                                                Some(true) => div()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_md()
                                                    .bg(rgb(COLOR_ROW_SELECTED))
                                                    .border_1()
                                                    .border_color(rgb(COLOR_ACCENT))
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(COLOR_ACCENT))
                                                    .child("History Cleared ✓")
                                                    .into_any_element(),
                                                Some(false) => div()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_md()
                                                    .bg(rgb(0x3a1e20))
                                                    .border_1()
                                                    .border_color(rgb(0xef4444))
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xef4444))
                                                    .child("Could not clear history")
                                                    .into_any_element(),
                                                None => div()
                                                    .id("trigger-clear-clipboard-btn")
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_md()
                                                    .bg(rgb(COLOR_CONTROL_BG))
                                                    .border_1()
                                                    .border_color(rgb(COLOR_CONTROL_BORDER))
                                                    .cursor_pointer()
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xef4444))
                                                    .hover(|s| s.bg(rgb(0x3a1e20)).border_color(rgb(0x7f1d1d)))
                                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                        this.confirming_clear_clipboard = true;
                                                        this.clipboard_clear_feedback = None;
                                                        cx.notify();
                                                    }))
                                                    .child("Clear History...")
                                                    .into_any_element(),
                                            },
                                        )
                                },
                            ),
                    ),
            );

        if let Some(active) = self.active_dropdown {
            pane = pane
                .child(
                    div()
                        .id("settings-dropdown-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.active_dropdown = None;
                            cx.notify();
                        })),
                )
                .child(self.render_active_dropdown_popover(active, cx));
        }

        pane
    }

    fn render_snippets_pane(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: Snippets toggles
            .child(self.section_group(
                "Snippets",
                vec![
                    self.toggle_row(
                        "toggle-enable-snippets",
                        "Enable snippets",
                        Some("Expand templates from the launcher or by keyword."),
                        self.settings.snippets.enabled,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.settings.snippets.enabled = !this.settings.snippets.enabled;
                            this.save_settings_file();
                            cx.notify();
                        }),
                    ),
                    self.toggle_row(
                        "toggle-snippets-launcher",
                        "Show in launcher",
                        None,
                        self.settings.snippets.show_in_launcher,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.settings.snippets.show_in_launcher =
                                !this.settings.snippets.show_in_launcher;
                            this.save_settings_file();
                            cx.notify();
                        }),
                    ),
                ],
            ))
            // Section 2: Commands
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Commands"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .child(
                                self.render_item_row(
                                    SettingsTab::Snippets,
                                    0,
                                    "Search Snippets",
                                    "Search Snippets",
                                    crate::icons::render_phosphor_svg(
                                        phosphor_svgs::style::regular::CODE,
                                        rgb(COLOR_ACCENT),
                                        16.0,
                                    )
                                    .into_any_element(),
                                    cx,
                                ),
                            )
                            .child(
                                self.render_item_row(
                                    SettingsTab::Snippets,
                                    1,
                                    "Create Snippet",
                                    "Create Snippet",
                                    crate::icons::render_phosphor_svg(
                                        phosphor_svgs::style::regular::FILE_PLUS,
                                        rgb(COLOR_TEXT_DIM),
                                        16.0,
                                    )
                                    .into_any_element(),
                                    cx,
                                ),
                            ),
                    ),
            )
            // Section 3: Library
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Library"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .px_4()
                            .py_2()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(40.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.5))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child(if self.snippets_file.snippets.is_empty() {
                                        "No snippets yet.".to_string()
                                    } else {
                                        format!(
                                            "{} snippet(s) configured.",
                                            self.snippets_file.snippets.len()
                                        )
                                    }),
                            )
                            .child(div().h(px(1.0)).bg(rgb(COLOR_BORDER_SUBTLE)))
                            .child(
                                div()
                                    .h(px(44.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("New Snippet"),
                                    )
                                    .child(
                                        div()
                                            .id("add-snippet-btn")
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .hover(|s| {
                                                s.bg(rgb(COLOR_CONTROL_HOVER))
                                                    .border_color(rgb(0x44474e))
                                            })
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.snippets_file.snippets.push(
                                                        corvo_config::SnippetEntry {
                                                            name: "New Snippet".into(),
                                                            keyword: None,
                                                            body: "".into(),
                                                        },
                                                    );
                                                    this.save_snippets_file();
                                                    cx.notify();
                                                },
                                            ))
                                            .child("Add..."),
                                    ),
                            )
                            .child(div().h(px(1.0)).bg(rgb(COLOR_BORDER_SUBTLE)))
                            .child(
                                div()
                                    .h(px(54.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .text_color(rgb(COLOR_TEXT))
                                                    .child("Snippets Folder"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(rgb(COLOR_TEXT_DIM))
                                                    .child("Plain Markdown files."),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("open-snippets-folder-btn")
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .id("open-snippets-folder-btn")
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .hover(|s| {
                                                s.bg(rgb(COLOR_CONTROL_HOVER))
                                                    .border_color(rgb(0x44474e))
                                            })
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .on_click(cx.listener(
                                                |_this, _: &ClickEvent, _window, _cx| {
                                                    if let Some(folder) =
                                                        corvo_config::Settings::config_dir()
                                                    {
                                                        let folder_url =
                                                            format!("file://{}", folder.display());
                                                        let _ =
                                                            corvo_platform::open_url(&folder_url);
                                                    }
                                                },
                                            ))
                                            .child("Open Folder"),
                                    ),
                            ),
                    ),
            )
    }

    fn render_file_search_pane(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: File Search toggle
            .child(self.section_group(
                "File Search",
                vec![self.toggle_row(
                    "toggle-enable-file-search",
                    "Enable File Search",
                    Some("Uses the Spotlight index, only when you search."),
                    self.settings.file_search.enabled,
                    cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.settings.file_search.enabled = !this.settings.file_search.enabled;
                        this.save_settings_file();
                        cx.notify();
                    }),
                )],
            ))
            // Section 2: Commands
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Commands"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .child(
                                self.render_item_row(
                                    SettingsTab::FileSearch,
                                    0,
                                    "Search Files",
                                    "Search Files",
                                    crate::icons::render_phosphor_svg(
                                        phosphor_svgs::style::regular::FILE_TEXT,
                                        rgb(COLOR_ACCENT),
                                        16.0,
                                    )
                                    .into_any_element(),
                                    cx,
                                ),
                            ),
                    ),
            )
            // Section 3: Search Scopes
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Search Scopes"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .overflow_hidden()
                            .children(
                                self.settings
                                    .file_search
                                    .search_scopes
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, scope)| {
                                        let scope_clone = scope.clone();
                                        div()
                                            .id(SharedString::from(format!("file-scope-row-{idx}")))
                                            .h(px(38.0))
                                            .px_3()
                                            .border_b_1()
                                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                                            .hover(|s| s.bg(rgb(COLOR_ROW_HOVER)))
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2p5()
                                                    .child(crate::icons::render_phosphor_svg(
                                                        phosphor_svgs::style::regular::FOLDER,
                                                        rgb(COLOR_TEXT_DIM),
                                                        16.0,
                                                    ))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(COLOR_TEXT))
                                                            .child(scope_clone.clone()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "remove-file-scope-{idx}"
                                                    )))
                                                    .size(px(18.0))
                                                    .rounded_full()
                                                    .cursor_pointer()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                    .on_click(cx.listener({
                                                        let scope_clone = scope_clone.clone();
                                                        move |this, _: &ClickEvent, _window, cx| {
                                                            this.settings
                                                                .file_search
                                                                .search_scopes
                                                                .retain(|s| s != &scope_clone);
                                                            this.save_settings_file();
                                                            cx.notify();
                                                        }
                                                    }))
                                                    .child(crate::icons::render_phosphor_svg(
                                                        phosphor_svgs::style::regular::X,
                                                        rgb(COLOR_TEXT_DIM),
                                                        10.0,
                                                    )),
                                            )
                                    }),
                            )
                            // Add button / input
                            .child(if self.adding_file_scope {
                                div()
                                    .p_3()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_2p5()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(0x191a1d))
                                            .border_1()
                                            .border_color(rgb(COLOR_ACCENT))
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT))
                                            .child(if self.new_file_scope_text.is_empty() {
                                                "Enter directory path (e.g. ~/Documents)..."
                                                    .to_string()
                                            } else {
                                                self.new_file_scope_text.clone()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("commit-file-scope-btn")
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(COLOR_ACCENT))
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0x064e3b))
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    let trimmed =
                                                        this.new_file_scope_text.trim().to_string();
                                                    if !trimmed.is_empty()
                                                        && !this
                                                            .settings
                                                            .file_search
                                                            .search_scopes
                                                            .contains(&trimmed)
                                                    {
                                                        this.settings
                                                            .file_search
                                                            .search_scopes
                                                            .push(trimmed);
                                                        this.save_settings_file();
                                                    }
                                                    this.adding_file_scope = false;
                                                    this.new_file_scope_text.clear();
                                                    cx.notify();
                                                },
                                            ))
                                            .child("Add"),
                                    )
                                    .child(
                                        div()
                                            .id("cancel-file-scope-btn")
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .text_size(px(11.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.adding_file_scope = false;
                                                    this.new_file_scope_text.clear();
                                                    cx.notify();
                                                },
                                            ))
                                            .child("Cancel"),
                                    )
                            } else {
                                div().p_3().child(
                                    div()
                                        .id("add-file-scope-btn")
                                        .cursor_pointer()
                                        .w_auto()
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .bg(rgb(COLOR_CONTROL_BG))
                                        .border_1()
                                        .border_color(rgb(COLOR_CONTROL_BORDER))
                                        .hover(|s| {
                                            s.bg(rgb(COLOR_CONTROL_HOVER))
                                                .border_color(rgb(0x44474e))
                                        })
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(COLOR_TEXT))
                                        .on_click(cx.listener(
                                            |this, _: &ClickEvent, _window, cx| {
                                                this.adding_file_scope = true;
                                                this.new_file_scope_text.clear();
                                                cx.notify();
                                            },
                                        ))
                                        .child("Add..."),
                                )
                            }),
                    )
                    .child(
                        div()
                            .px_1()
                            .text_size(px(12.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child(
                                "Home covers its visible folders and cloud drives, never Library.",
                            ),
                    ),
            )
            // Section 4: Ignore Patterns
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Ignore Patterns"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .overflow_hidden()
                            .children(
                                self.settings
                                    .file_search
                                    .ignore_patterns
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, pat)| {
                                        let pat_clone = pat.clone();
                                        div()
                                            .id(SharedString::from(format!(
                                                "ignore-pattern-row-{idx}"
                                            )))
                                            .h(px(38.0))
                                            .px_3()
                                            .border_b_1()
                                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                                            .hover(|s| s.bg(rgb(COLOR_ROW_HOVER)))
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2p5()
                                                    .child(crate::icons::render_phosphor_svg(
                                                        phosphor_svgs::style::regular::FILE_TEXT,
                                                        rgb(COLOR_TEXT_DIM),
                                                        15.0,
                                                    ))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(COLOR_TEXT))
                                                            .child(pat_clone.clone()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "remove-ignore-pat-{idx}"
                                                    )))
                                                    .size(px(18.0))
                                                    .rounded_full()
                                                    .cursor_pointer()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                    .on_click(cx.listener({
                                                        let pat_clone = pat_clone.clone();
                                                        move |this, _: &ClickEvent, _window, cx| {
                                                            this.settings
                                                                .file_search
                                                                .ignore_patterns
                                                                .retain(|p| p != &pat_clone);
                                                            this.save_settings_file();
                                                            cx.notify();
                                                        }
                                                    }))
                                                    .child(crate::icons::render_phosphor_svg(
                                                        phosphor_svgs::style::regular::X,
                                                        rgb(COLOR_TEXT_DIM),
                                                        10.0,
                                                    )),
                                            )
                                    }),
                            )
                            .child(if self.adding_ignore_pattern {
                                div()
                                    .p_3()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_2p5()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(0x191a1d))
                                            .border_1()
                                            .border_color(rgb(COLOR_ACCENT))
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT))
                                            .child(if self.new_ignore_pattern_text.is_empty() {
                                                "Enter pattern (e.g. .git)...".to_string()
                                            } else {
                                                self.new_ignore_pattern_text.clone()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("commit-ignore-pattern-btn")
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(COLOR_ACCENT))
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0x064e3b))
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    let trimmed = this
                                                        .new_ignore_pattern_text
                                                        .trim()
                                                        .to_string();
                                                    if !trimmed.is_empty()
                                                        && !this
                                                            .settings
                                                            .file_search
                                                            .ignore_patterns
                                                            .contains(&trimmed)
                                                    {
                                                        this.settings
                                                            .file_search
                                                            .ignore_patterns
                                                            .push(trimmed);
                                                        this.save_settings_file();
                                                    }
                                                    this.adding_ignore_pattern = false;
                                                    this.new_ignore_pattern_text.clear();
                                                    cx.notify();
                                                },
                                            ))
                                            .child("Add"),
                                    )
                                    .child(
                                        div()
                                            .id("cancel-ignore-pattern-btn")
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .text_size(px(11.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.adding_ignore_pattern = false;
                                                    this.new_ignore_pattern_text.clear();
                                                    cx.notify();
                                                },
                                            ))
                                            .child("Cancel"),
                                    )
                            } else {
                                div().p_3().child(
                                    div()
                                        .id("add-ignore-pat-btn")
                                        .cursor_pointer()
                                        .w_auto()
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .bg(rgb(COLOR_CONTROL_BG))
                                        .border_1()
                                        .border_color(rgb(COLOR_CONTROL_BORDER))
                                        .hover(|s| {
                                            s.bg(rgb(COLOR_CONTROL_HOVER))
                                                .border_color(rgb(0x44474e))
                                        })
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(COLOR_TEXT))
                                        .on_click(cx.listener(
                                            |this, _: &ClickEvent, _window, cx| {
                                                this.adding_ignore_pattern = true;
                                                this.new_ignore_pattern_text.clear();
                                                cx.notify();
                                            },
                                        ))
                                        .child("Add..."),
                                )
                            }),
                    ),
            )
    }

    fn create_empty_layout(&mut self, cx: &mut Context<Self>) {
        let next_idx = self.settings.window_management.layouts.len() + 1;
        let id = format!(
            "layout-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        self.settings
            .window_management
            .layouts
            .push(corvo_config::WindowLayoutTemplate {
                id,
                name: format!("Layout {next_idx}"),
                hotkey: None,
                placements: Vec::new(),
            });
        self.save_settings_file();
        cx.notify();
    }

    fn create_layout_from_current_windows(&mut self, cx: &mut Context<Self>) {
        let captured = corvo_platform::capture_current_window_layout();
        let next_idx = self.settings.window_management.layouts.len() + 1;
        let id = format!(
            "layout-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let placements = captured
            .into_iter()
            .map(|(app, pos)| corvo_config::WindowPlacement {
                app_name: app,
                position: pos,
            })
            .collect();
        self.settings
            .window_management
            .layouts
            .push(corvo_config::WindowLayoutTemplate {
                id,
                name: format!("Layout {next_idx}"),
                hotkey: None,
                placements,
            });
        self.save_settings_file();
        cx.notify();
    }

    fn delete_layout(&mut self, id: &str, cx: &mut Context<Self>) {
        self.settings
            .window_management
            .layouts
            .retain(|l| l.id != id);
        self.save_settings_file();
        cx.notify();
    }

    fn apply_layout(&self, id: &str) {
        if let Some(layout) = self
            .settings
            .window_management
            .layouts
            .iter()
            .find(|l| l.id == id)
        {
            let placements: Vec<(String, String)> = layout
                .placements
                .iter()
                .map(|p| (p.app_name.clone(), p.position.clone()))
                .collect();
            smol::spawn(async move {
                let _ = corvo_platform::apply_window_layout(&placements);
            })
            .detach();
        }
    }

    fn render_window_management_pane(&self, cx: &mut Context<Self>) -> Div {
        let mut pane = div()
            .relative()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: Window Management
            .child(
                self.section_group(
                    "Window Management",
                    vec![
                        self.toggle_row(
                            "toggle-enable-window-management",
                            "Enable window management",
                            Some("Moves the last window you used. Needs Accessibility."),
                            self.settings.window_management.enabled,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.window_management.enabled = !this.settings.window_management.enabled;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.toggle_row(
                            "toggle-window-management-launcher",
                            "Show in launcher",
                            None,
                            self.settings.window_management.show_in_launcher,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.window_management.show_in_launcher = !this.settings.window_management.show_in_launcher;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                    ],
                ),
            )
            // Section 2: Options
            .child(
                self.section_group(
                    "Options",
                    vec![
                        self.interactive_dropdown_trigger(
                            "window-cycling",
                            ("Cycling", Some("Repeating a half keeps the same frame.")),
                            ActiveDropdown::WindowCycling,
                            self.settings.window_management.cycling_option,
                            &["None", "Cycle forward", "Cycle forward & backward"],
                            cx,
                        ),
                        {
                            let gap = self.settings.window_management.gap_between_windows;
                            div()
                                .h(px(54.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_size(px(13.0))
                                                .text_color(rgb(COLOR_TEXT))
                                                .child("Gap between windows"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(rgb(COLOR_TEXT_DIM))
                                                .child("Between tiled windows and screen edges."),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(rgb(COLOR_CONTROL_BG))
                                        .border_1()
                                        .border_color(rgb(COLOR_CONTROL_BORDER))
                                        .child(
                                            div()
                                                .id("dec-window-gap-btn")
                                                .size(px(20.0))
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                    if this.settings.window_management.gap_between_windows > 0 {
                                                        this.settings.window_management.gap_between_windows -= 1;
                                                        this.save_settings_file();
                                                        cx.notify();
                                                    }
                                                }))
                                                .child(
                                                    div()
                                                        .w(px(8.0))
                                                        .h(px(1.5))
                                                        .bg(rgb(COLOR_TEXT_DIM)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .px_1p5()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(COLOR_TEXT))
                                                .child(format!("{gap} pt")),
                                        )
                                        .child(
                                            div()
                                                .id("inc-window-gap-btn")
                                                .size(px(20.0))
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                    if this.settings.window_management.gap_between_windows < 64 {
                                                        this.settings.window_management.gap_between_windows += 1;
                                                        this.save_settings_file();
                                                        cx.notify();
                                                    }
                                                }))
                                                .child(crate::icons::render_phosphor_svg(
                                                    phosphor_svgs::style::regular::PLUS,
                                                    rgb(COLOR_TEXT_DIM),
                                                    10.0,
                                                )),
                                        ),
                                )
                        },
                    ],
                ),
            )
            // Section 3: Window Layouts
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Window Layouts"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .flex()
                            .flex_col()
                            .child(
                                div().px_4().py_1().child(
                                    self.toggle_row(
                                        "toggle-show-layouts",
                                        "Show layouts in launcher",
                                        None,
                                        self.settings.window_management.show_layouts_in_launcher,
                                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                                            this.settings.window_management.show_layouts_in_launcher =
                                                !this.settings.window_management.show_layouts_in_launcher;
                                            this.save_settings_file();
                                            cx.notify();
                                        }),
                                    ),
                                ),
                            )
                            .child(div().h(px(1.0)).bg(rgb(COLOR_BORDER_SUBTLE)))
                            .child(
                                div()
                                    .h(px(46.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("Templates"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("new-layout-btn")
                                                    .cursor_pointer()
                                                    .px_2p5()
                                                    .py_1()
                                                    .rounded_md()
                                                    .bg(rgb(COLOR_CONTROL_BG))
                                                    .border_1()
                                                    .border_color(rgb(COLOR_CONTROL_BORDER))
                                                    .hover(|s| {
                                                        s.bg(rgb(COLOR_CONTROL_HOVER))
                                                            .border_color(rgb(0x44474e))
                                                    })
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(COLOR_TEXT))
                                                    .on_click(cx.listener(
                                                        |this, _: &ClickEvent, _window, cx| {
                                                            this.create_empty_layout(cx);
                                                        },
                                                    ))
                                                    .child("New Layout"),
                                            )
                                            .child(
                                                div()
                                                    .id("create-layout-windows-btn")
                                                    .cursor_pointer()
                                                    .px_2p5()
                                                    .py_1()
                                                    .rounded_md()
                                                    .bg(rgb(COLOR_CONTROL_BG))
                                                    .border_1()
                                                    .border_color(rgb(COLOR_CONTROL_BORDER))
                                                    .hover(|s| {
                                                        s.bg(rgb(COLOR_CONTROL_HOVER))
                                                            .border_color(rgb(0x44474e))
                                                    })
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(COLOR_TEXT))
                                                    .on_click(cx.listener(
                                                        |this, _: &ClickEvent, _window, cx| {
                                                            this.create_layout_from_current_windows(
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                    .child("Create from Windows"),
                                            ),
                                    ),
                            )
                            .children(
                                self.settings
                                    .window_management
                                    .layouts
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, layout)| {
                                        let layout_id = layout.id.clone();
                                        let layout_name = layout.name.clone();
                                        let placements_len = layout.placements.len();
                                        let hotkey_item_key = format!("layout:{}", layout.id);
                                        let is_recording_hotkey = self
                                            .recording_hotkey_item
                                            .as_ref()
                                            .is_some_and(|(t, k)| {
                                                *t == SettingsTab::WindowManagement
                                                    && k == &hotkey_item_key
                                            });

                                        div()
                                            .border_t_1()
                                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                                            .h(px(46.0))
                                            .px_4()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2p5()
                                                    .child(crate::icons::render_phosphor_svg(
                                                        phosphor_svgs::style::regular::SQUARES_FOUR,
                                                        rgb(COLOR_ACCENT),
                                                        16.0,
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .child(
                                                                div()
                                                                    .text_size(px(13.0))
                                                                    .font_weight(FontWeight::MEDIUM)
                                                                    .text_color(rgb(COLOR_TEXT))
                                                                    .child(layout_name),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(11.0))
                                                                    .text_color(rgb(
                                                                        COLOR_TEXT_DIM,
                                                                    ))
                                                                    .child(format!(
                                                                        "{placements_len} windows"
                                                                    )),
                                                            ),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "apply-layout-btn-{idx}"
                                                            )))
                                                            .cursor_pointer()
                                                            .px_2()
                                                            .py_0p5()
                                                            .rounded_md()
                                                            .bg(rgb(COLOR_CONTROL_BG))
                                                            .border_1()
                                                            .border_color(rgb(
                                                                COLOR_CONTROL_BORDER,
                                                            ))
                                                            .hover(|s| {
                                                                s.bg(rgb(COLOR_CONTROL_HOVER))
                                                            })
                                                            .text_size(px(11.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(COLOR_TEXT))
                                                            .on_click(cx.listener({
                                                                let id = layout_id.clone();
                                                                move |this,
                                                                      _: &ClickEvent,
                                                                      _window,
                                                                      _cx| {
                                                                    this.apply_layout(&id);
                                                                }
                                                            }))
                                                            .child("Apply"),
                                                    )
                                                    .child(self.render_hotkey_control(
                                                        SettingsTab::WindowManagement,
                                                        &hotkey_item_key,
                                                        layout.hotkey.as_ref(),
                                                        is_recording_hotkey,
                                                        &format!("layout-{idx}"),
                                                        cx,
                                                    ))
                                                    .child(
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "delete-layout-btn-{idx}"
                                                            )))
                                                            .cursor_pointer()
                                                            .size(px(24.0))
                                                            .rounded_md()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .hover(|s| s.bg(rgb(0x3a1d1d)))
                                                            .on_click(cx.listener({
                                                                let id = layout_id.clone();
                                                                move |this,
                                                                      _: &ClickEvent,
                                                                      _window,
                                                                      cx| {
                                                                    this.delete_layout(&id, cx);
                                                                }
                                                            }))
                                                            .child(
                                                                crate::icons::render_phosphor_svg(
                                                                    phosphor_svgs::style::regular::TRASH,
                                                                    rgb(0xe06c75),
                                                                    14.0,
                                                                ),
                                                            ),
                                                    ),
                                            )
                                    }),
                            ),
                    ),
            )
            // Section 4: Window Actions
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Window Actions"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .children(
                                corvo_window_management::WINDOW_ACTIONS
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, action)| {
                                        self.render_item_row(
                                            SettingsTab::WindowManagement,
                                            idx,
                                            action.id,
                                            action.title,
                                            crate::icons::render_phosphor_svg(
                                                action.icon,
                                                rgb(COLOR_TEXT_DIM),
                                                16.0,
                                            )
                                            .into_any_element(),
                                            cx,
                                        )
                                    }),
                            ),
                    ),
            );

        if let Some(active) = self.active_dropdown {
            pane = pane
                .child(
                    div()
                        .id("settings-dropdown-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.active_dropdown = None;
                            cx.notify();
                        })),
                )
                .child(self.render_active_dropdown_popover(active, cx));
        }

        pane
    }

    fn render_navigation_pane(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: Navigation
            .child(
                self.section_group(
                    "Navigation",
                    vec![
                        self.toggle_row(
                            "toggle-enable-navigation",
                            "Enable navigation",
                            Some("Switch windows and search menu bar items."),
                            self.settings.navigation.enabled,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.navigation.enabled = !this.settings.navigation.enabled;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                    ],
                ),
            )
            // Section 2: Commands
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Commands"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .child(self.render_item_row(
                                SettingsTab::Navigation,
                                0,
                                "Switch Windows",
                                "Switch Windows",
                                crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::APP_WINDOW,
                                    rgb(COLOR_ACCENT),
                                    16.0,
                                ).into_any_element(),
                                cx,
                            )),
                    ),
            )
            // Section 3: Search Menu Bar Items. Only macOS has a menu bar
            // or an Apple menu, so the section is hidden elsewhere rather
            // than offering a toggle that does nothing.
            .when(cfg!(target_os = "macos"), |section| {
                section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Search Menu Bar Items"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .child(self.render_item_row(
                                SettingsTab::Navigation,
                                1,
                                "Search Menu Bar Items",
                                "Search Menu Bar Items",
                                crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::MAGNIFYING_GLASS,
                                    rgb(COLOR_TEXT_DIM),
                                    16.0,
                                ).into_any_element(),
                                cx,
                            ))
                            .child(
                                div()
                                    .px_4()
                                    .py_2()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        self.toggle_row(
                                            "toggle-apple-menu-items",
                                            // Only macOS has an Apple menu
                                            // and a menu bar to search.
                                            "Show Apple menu items",
                                            None,
                                            self.settings.navigation.show_apple_menu_items,
                                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.settings.navigation.show_apple_menu_items = !this.settings.navigation.show_apple_menu_items;
                                                this.save_settings_file();
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                    .child(div().h(px(1.0)).bg(rgb(COLOR_BORDER_SUBTLE)))
                                    .child(
                                        div()
                                            .h(px(54.0))
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_0p5()
                                                    .child(
                                                        div()
                                                            .text_size(px(13.0))
                                                            .text_color(rgb(COLOR_TEXT))
                                                            .child("Disabled Applications"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(12.0))
                                                            .text_color(rgb(COLOR_TEXT_DIM))
                                                            .child("Their menus are never searched."),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .id("add-nav-disabled-app-btn")
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1()
                                                    .rounded_md()
                                                    .bg(rgb(COLOR_CONTROL_BG))
                                                    .border_1()
                                                    .border_color(rgb(COLOR_CONTROL_BORDER))
                                                    .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(COLOR_TEXT))
                                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                        this.adding_nav_disabled_app = true;
                                                        this.new_nav_disabled_app_text.clear();
                                                        cx.notify();
                                                    }))
                                                    .child("Add Application..."),
                                            ),
                                    )
                                    .when(self.adding_nav_disabled_app, |el| {
                                        el.child(
                                            div()
                                                .py_2()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .px_2p5()
                                                        .py_1()
                                                        .rounded_md()
                                                        .bg(rgb(0x191a1d))
                                                        .border_1()
                                                        .border_color(rgb(COLOR_ACCENT))
                                                        .text_size(px(12.0))
                                                        .text_color(rgb(COLOR_TEXT))
                                                        .child(if self.new_nav_disabled_app_text.is_empty() { "Application name...".to_string() } else { self.new_nav_disabled_app_text.clone() }),
                                                )
                                                .child(
                                                    div()
                                                        .id("commit-nav-disabled-app-btn")
                                                        .cursor_pointer()
                                                        .px_3()
                                                        .py_1()
                                                        .rounded_md()
                                                        .bg(rgb(COLOR_ACCENT))
                                                        .text_size(px(11.0))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(rgb(0x064e3b))
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            let trimmed = this.new_nav_disabled_app_text.trim().to_string();
                                                            if !trimmed.is_empty() && !this.settings.navigation.disabled_applications.contains(&trimmed) {
                                                                this.settings.navigation.disabled_applications.push(trimmed);
                                                                this.save_settings_file();
                                                            }
                                                            this.adding_nav_disabled_app = false;
                                                            this.new_nav_disabled_app_text.clear();
                                                            cx.notify();
                                                        }))
                                                        .child("Add"),
                                                )
                                                .child(
                                                    div()
                                                        .id("cancel-nav-disabled-app-btn")
                                                        .cursor_pointer()
                                                        .px_3()
                                                        .py_1()
                                                        .rounded_md()
                                                        .bg(rgb(COLOR_CONTROL_BG))
                                                        .border_1()
                                                        .border_color(rgb(COLOR_CONTROL_BORDER))
                                                        .text_size(px(11.0))
                                                        .text_color(rgb(COLOR_TEXT_DIM))
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            this.adding_nav_disabled_app = false;
                                                            this.new_nav_disabled_app_text.clear();
                                                            cx.notify();
                                                        }))
                                                        .child("Cancel"),
                                                ),
                                        )
                                    })
                                    .children(
                                        self.settings.navigation.disabled_applications.iter().enumerate().map(|(idx, app)| {
                                            let app_clone = app.clone();
                                            div()
                                                .id(SharedString::from(format!("nav-disabled-app-{idx}")))
                                                .h(px(32.0))
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .text_size(px(12.5))
                                                        .text_color(rgb(COLOR_TEXT))
                                                        .child(app_clone.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("remove-nav-disabled-app-{idx}")))
                                                        .size(px(16.0))
                                                        .rounded_full()
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
                                                        .on_click(cx.listener({
                                                            let app_clone = app_clone.clone();
                                                            move |this, _: &ClickEvent, _window, cx| {
                                                                this.settings.navigation.disabled_applications.retain(|a| a != &app_clone);
                                                                this.save_settings_file();
                                                                cx.notify();
                                                            }
                                                        }))
                                                        .child(crate::icons::render_phosphor_svg(
                                                            phosphor_svgs::style::regular::X,
                                                            rgb(COLOR_TEXT_DIM),
                                                            10.0,
                                                        )),
                                                )
                                        }),
                                    ),
                            ),
                    ),
                )
            })
    }

    fn render_calendar_pane(&self, cx: &mut Context<Self>) -> Div {
        let mut pane = div()
            .relative()
            .flex()
            .flex_col()
            .gap_6()
            // Section 1: Calendar
            .child(
                self.section_group(
                    "Calendar",
                    vec![
                        self.toggle_row(
                            "toggle-join-meetings",
                            "Join meetings from Corvo",
                            Some("Reads today's and tomorrow's events for join links. Nothing leaves this Mac."),
                            self.settings.calendar.join_meetings,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.calendar.join_meetings = !this.settings.calendar.join_meetings;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.toggle_row(
                            "toggle-calendar-launcher",
                            "Show in launcher",
                            None,
                            self.settings.calendar.show_in_launcher,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.calendar.show_in_launcher = !this.settings.calendar.show_in_launcher;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.interactive_dropdown_trigger(
                            "calendar-upcoming",
                            ("Upcoming meetings in launcher", None),
                            ActiveDropdown::CalendarUpcomingMeetings,
                            self.settings.calendar.upcoming_meetings_option,
                            &["1 next", "3 next", "5 next", "All today"],
                            cx,
                        ),
                    ],
                ),
            )
            // Section 2: Schedule
            .child(
                self.section_group(
                    "Schedule",
                    vec![
                        self.toggle_row(
                            "toggle-include-tomorrow",
                            "Include Tomorrow's Events",
                            None,
                            self.settings.calendar.include_tomorrow,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.calendar.include_tomorrow = !this.settings.calendar.include_tomorrow;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                    ],
                ),
            )
            // Section 3: Joining
            .child(
                self.section_group(
                    "Joining",
                    vec![
                        self.interactive_dropdown_trigger(
                            "calendar-join-card",
                            ("Show the join card", Some("Before and after a meeting starts.")),
                            ActiveDropdown::CalendarJoinCard,
                            self.settings.calendar.join_card_option,
                            &["Immediately", "5 minutes", "10 minutes", "15 minutes"],
                            cx,
                        ),
                        self.toggle_row(
                            "toggle-auto-join-meetings",
                            "Auto Join Meetings",
                            Some("As they start."),
                            self.settings.calendar.auto_join_meetings,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.calendar.auto_join_meetings = !this.settings.calendar.auto_join_meetings;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        div()
                            .h(px(44.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child("Confirm before joining"),
                            )
                            .child(
                                div()
                                    .size(px(16.0))
                                    .rounded_sm()
                                    .bg(rgb(COLOR_CONTROL_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_CONTROL_BORDER))
                                    .opacity(0.4),
                            ),
                        self.toggle_row(
                            "toggle-camera-preview",
                            "Camera Preview",
                            Some("Before joining a meeting."),
                            self.settings.calendar.camera_preview,
                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.calendar.camera_preview = !this.settings.calendar.camera_preview;
                                this.save_settings_file();
                                cx.notify();
                            }),
                        ),
                        self.interactive_dropdown_trigger(
                            "calendar-browser",
                            (
                                "Open Meeting Links In",
                                Some("When no meeting app handles the link."),
                            ),
                            ActiveDropdown::CalendarBrowser,
                            self.settings.calendar.open_meeting_links_in,
                            &["Default Browser", "Google Chrome", "Safari", "Arc"],
                            cx,
                        ),
                    ],
                ),
            );

        if let Some(active) = self.active_dropdown {
            pane = pane
                .child(
                    div()
                        .id("settings-dropdown-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.active_dropdown = None;
                            cx.notify();
                        })),
                )
                .child(self.render_active_dropdown_popover(active, cx));
        }

        pane
    }

    fn render_emojis_pane(&self, cx: &mut Context<Self>) -> Div {
        let hand_emojis = ["👋", "👋🏻", "👋🏼", "👋🏽", "👋🏾", "👋🏿"];
        let column_counts = [6, 7, 8, 9, 10];
        let current_cols = self.settings.emojis.column_count;
        let current_skin = self.settings.emojis.skin_tone;

        div()
            .flex()
            .flex_col()
            .min_w(px(0.0))
            .gap_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Commands"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .child(self.render_item_row(
                                SettingsTab::Emojis,
                                0,
                                "Search Emoji & Symbols",
                                "Search Emoji & Symbols",
                                crate::icons::render_phosphor_svg(
                                    phosphor_svgs::style::regular::SMILEY,
                                    rgb(COLOR_ACCENT),
                                    16.0,
                                ).into_any_element(),
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Appearance"),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(rgb(COLOR_CARD_BG))
                            .border_1()
                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                            .px_4()
                            .py_3()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("Column Count"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .children(column_counts.into_iter().map(|cols| {
                                                let is_selected = cols == current_cols;
                                                let cell_count = cols * cols;
                                                let line_color = if is_selected {
                                                    rgb(0x28634f)
                                                } else {
                                                    rgb(COLOR_BORDER_SUBTLE)
                                                };
                                                let tile_background = if is_selected {
                                                    rgb(COLOR_ROW_SELECTED)
                                                } else {
                                                    rgb(COLOR_CONTROL_BG)
                                                };
                                                div()
                                                    .id(SharedString::from(format!("emoji-cols-{cols}")))
                                                    .flex_1()
                                                    .min_w(px(0.0))
                                                    .rounded_lg()
                                                    .p_2()
                                                    .cursor_pointer()
                                                    .flex()
                                                    .flex_col()
                                                    .items_center()
                                                    .gap_2()
                                                    .when(is_selected, |tile| {
                                                        tile.bg(rgb(COLOR_ROW_SELECTED))
                                                            .border_1()
                                                            .border_color(rgb(COLOR_ACCENT))
                                                    })
                                                    .when(!is_selected, |tile| {
                                                        tile.hover(|style| style.bg(rgb(COLOR_CONTROL_HOVER)))
                                                    })
                                                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                                        this.settings.emojis.column_count = cols;
                                                        this.save_settings_file();
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        div()
                                                            .w_full()
                                                            .h(px(88.0))
                                                            .grid()
                                                            .grid_cols(cols as u16)
                                                            .grid_rows(cols as u16)
                                                            .rounded_md()
                                                            .overflow_hidden()
                                                            .border_1()
                                                            .border_color(if is_selected { rgb(COLOR_ACCENT) } else { rgb(COLOR_CONTROL_BORDER) })
                                                            .children((0..cell_count).map(|_| {
                                                                div()
                                                                    .bg(tile_background)
                                                                    .border_r_1()
                                                                    .border_b_1()
                                                                    .border_color(line_color)
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(12.0))
                                                            .font_weight(if is_selected { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                                                            .text_color(if is_selected { rgb(COLOR_ACCENT) } else { rgb(COLOR_TEXT_DIM) })
                                                            .child(cols.to_string()),
                                                    )
                                            })),
                                    ),
                            )
                            .child(div().h(px(1.0)).bg(rgb(COLOR_BORDER_SUBTLE)))
                            .child(
                                div()
                                    .h(px(44.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("Emoji Skin Tone"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .p(px(2.5))
                                            .rounded_lg()
                                            .bg(rgb(COLOR_SIDEBAR_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_BORDER_SUBTLE))
                                            .children(hand_emojis.into_iter().enumerate().map(|(idx, emoji)| {
                                                let is_selected = idx == current_skin;
                                                div()
                                                    .id(SharedString::from(format!("emoji-skin-{idx}")))
                                                    .size(px(30.0))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .when(is_selected, |button| {
                                                        button.bg(rgb(COLOR_ACCENT))
                                                    })
                                                    .when(!is_selected, |button| {
                                                        button.hover(|style| style.bg(rgb(COLOR_CONTROL_HOVER)))
                                                    })
                                                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                                        this.settings.emojis.skin_tone = idx;
                                                        this.save_settings_file();
                                                        cx.notify();
                                                    }))
                                                    .child(div().text_size(px(16.0)).child(emoji))
                                            })),
                                    ),
                            ),
                    ),
            )
    }

    fn render_extensions_pane(&self, _cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_6()
            // Hero Card
            .child(
                div()
                    .rounded_xl()
                    .bg(rgb(COLOR_CARD_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .size(px(40.0))
                                            .rounded_lg()
                                            .bg(rgb(0x1a3329))
                                            .border_1()
                                            .border_color(rgb(0x1e4a3b))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::STOREFRONT,
                                                rgb(COLOR_ACCENT),
                                                22.0,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(16.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(COLOR_TEXT))
                                                    .child("Corvo Extension Store"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(rgb(COLOR_TEXT_DIM))
                                                    .child("Discover and install community plugins, tools, and workflows."),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_full()
                                    .bg(rgb(0x1a3329))
                                    .border_1()
                                    .border_color(rgb(COLOR_ACCENT))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(COLOR_ACCENT))
                                    .child("Coming Soon"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(COLOR_TEXT_MUTED))
                            .child("A native marketplace of extensions and custom integrations built by the developer community. Extend Corvo with your favorite developer tools, productivity apps, and personal commands."),
                    ),
            )
            // Feature Highlights Grid
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("What to Expect"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_4()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(180.0))
                                    .rounded_lg()
                                    .bg(rgb(COLOR_CARD_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .size(px(32.0))
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::PLUG,
                                                rgb(COLOR_ACCENT),
                                                16.0,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("Community Plugins"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child("One-click installs for GitHub, Spotify, Linear, Notion, and Slack."),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(180.0))
                                    .rounded_lg()
                                    .bg(rgb(COLOR_CARD_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .size(px(32.0))
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::TERMINAL_WINDOW,
                                                rgb(COLOR_ACCENT),
                                                16.0,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("Script Commands"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child("Write lightweight scripts in Bash, Node.js, Python, or Swift and run them in milliseconds."),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(180.0))
                                    .rounded_lg()
                                    .bg(rgb(COLOR_CARD_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .size(px(32.0))
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_CONTROL_BORDER))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::CODE,
                                                rgb(COLOR_ACCENT),
                                                16.0,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(COLOR_TEXT))
                                            .child("Developer API"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child("Build and publish custom extensions using Corvo's native TypeScript and Rust SDK."),
                                    ),
                            ),
                    ),
            )
    }

    fn render_backup_pane(&self) -> Div {
        let option = |label: &'static str, icon: &'static str| {
            div()
                .flex()
                .items_center()
                .gap_2p5()
                .h(px(32.0))
                .child(
                    div()
                        .size(px(20.0))
                        .rounded_md()
                        .bg(rgb(COLOR_ACCENT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::icons::render_phosphor_svg(
                            phosphor_svgs::style::regular::CHECK,
                            rgb(0x064e3b),
                            13.0,
                        )),
                )
                .child(crate::icons::render_phosphor_svg(
                    icon,
                    rgb(COLOR_TEXT_DIM),
                    16.0,
                ))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(COLOR_TEXT_MUTED))
                        .child(label),
                )
        };
        let action_button = |label: &'static str| {
            div()
                .px_3()
                .py_1p5()
                .rounded_md()
                .bg(if label == "Export..." {
                    rgb(COLOR_ACCENT)
                } else {
                    rgb(COLOR_CONTROL_HOVER)
                })
                .text_size(px(13.0))
                .text_color(if label == "Export..." {
                    rgb(0x064e3b)
                } else {
                    rgb(COLOR_TEXT_MUTED)
                })
                .child(label)
        };
        let info_row = |title: &'static str, subtitle: &'static str, action: &'static str| {
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap_4()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(rgb(COLOR_TEXT_MUTED))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(subtitle),
                        ),
                )
                .child(action_button(action))
        };

        div()
            .flex()
            .flex_col()
            .gap_8()
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child("Backup"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Export"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .p_4()
                            .rounded_xl()
                            .bg(rgb(COLOR_CARD_BG))
                            .child(info_row(
                                "Export Backup",
                                "Save the selected items in one backup file.",
                                "Export...",
                            ))
                            .child(div().h(px(1.0)).bg(rgb(COLOR_DIVIDER)))
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(2)
                                    .gap_2()
                                    .child(option(
                                        "Settings & Shortcuts",
                                        phosphor_svgs::style::regular::SLIDERS,
                                    ))
                                    .child(option(
                                        "Clipboard History",
                                        phosphor_svgs::style::regular::CLIPBOARD_TEXT,
                                    ))
                                    .child(option("Snippets", phosphor_svgs::style::regular::CODE))
                                    .child(option("Notes", phosphor_svgs::style::regular::NOTE))
                                    .child(option(
                                        "Launcher Learning",
                                        phosphor_svgs::style::regular::CHART_LINE,
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_ACCENT))
                                    .child("Deselect All"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Import"),
                    )
                    .child(
                        div()
                            .p_4()
                            .rounded_xl()
                            .bg(rgb(COLOR_CARD_BG))
                            .child(info_row(
                                "Backup File",
                                "Choose a backup file exported from Corvo.",
                                "Choose...",
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Import from Raycast"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .p_4()
                            .rounded_xl()
                            .bg(rgb(COLOR_CARD_BG))
                            .child(info_row(
                                "Raycast Export",
                                "A .rayconfig file from Raycast 2.0 or later.",
                                "Choose...",
                            ))
                            .child(div().h(px(1.0)).bg(rgb(COLOR_DIVIDER)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(rgb(COLOR_TEXT_MUTED))
                                            .child("Passphrase"),
                                    )
                                    .child(
                                        div()
                                            .w(px(240.0))
                                            .px_3()
                                            .py_2()
                                            .rounded_md()
                                            .bg(rgb(COLOR_BG))
                                            .border_1()
                                            .border_color(rgb(COLOR_DIVIDER))
                                            .text_size(px(13.0))
                                            .text_color(rgb(COLOR_TEXT_DIM))
                                            .child("Export password"),
                                    ),
                            )
                            .child(div().h(px(1.0)).bg(rgb(COLOR_DIVIDER)))
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(3)
                                    .gap_2()
                                    .child(option(
                                        "Shortcuts",
                                        phosphor_svgs::style::regular::COMMAND,
                                    ))
                                    .child(option("Favorites", phosphor_svgs::style::regular::STAR))
                                    .child(option(
                                        "Aliases",
                                        phosphor_svgs::style::regular::TEXT_A_UNDERLINE,
                                    ))
                                    .child(option(
                                        "Emoji skin tone",
                                        phosphor_svgs::style::regular::SMILEY,
                                    ))
                                    .child(option(
                                        "Launch at login",
                                        phosphor_svgs::style::regular::POWER,
                                    ))
                                    .child(option(
                                        "Menu-bar icon",
                                        phosphor_svgs::style::regular::APP_WINDOW,
                                    ))
                                    .child(option(
                                        "Pop to root",
                                        phosphor_svgs::style::regular::ARROW_U_UP_LEFT,
                                    ))
                                    .child(option("Snippets", phosphor_svgs::style::regular::CODE))
                                    .child(option(
                                        "Quicklinks",
                                        phosphor_svgs::style::regular::LINK,
                                    ))
                                    .child(option(
                                        "Compact mode",
                                        phosphor_svgs::style::regular::BROWSERS,
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_ACCENT))
                                    .child("Deselect All"),
                            )
                            .child(div().h(px(1.0)).bg(rgb(COLOR_DIVIDER)))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child("Unset matching Raycast shortcuts to avoid conflicts."),
                            )
                            .child(div().flex().justify_end().child(action_button("Import"))),
                    ),
            )
    }

    fn open_logs(&mut self, cx: &mut Context<Self>) {
        let Some(directory) = corvo_platform::diagnostics::log_directory() else {
            corvo_platform::diagnostics::record_error("settings", "logs_directory_unavailable");
            self.save_error = Some("Could not find the log folder".to_string());
            cx.notify();
            return;
        };
        if std::fs::create_dir_all(&directory).is_err() {
            corvo_platform::diagnostics::record_error("settings", "logs_directory_create_failed");
            self.save_error = Some("Could not create the log folder".to_string());
        } else if corvo_platform::platform_ops()
            .open_path(&directory)
            .is_err()
        {
            corvo_platform::diagnostics::record_error("settings", "logs_directory_open_failed");
            self.save_error = Some("Could not open the log folder".to_string());
        } else {
            self.save_error = None;
        }
        cx.notify();
    }

    fn render_about_pane(&self, cx: &mut Context<Self>) -> Div {
        const REPOSITORY_URL: &str = "https://github.com/diegoleteliers10/corvo";
        let open_repository = cx.listener(|_: &mut Self, _: &ClickEvent, _window, _cx| {
            let _ = corvo_platform::open_url(REPOSITORY_URL);
        });

        div()
            .flex()
            .flex_col()
            .gap_8()
            .child(div().text_size(px(13.0)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(COLOR_TEXT)).child("About"))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .py_8()
                    .rounded_xl()
                    .bg(rgb(COLOR_CARD_BG))
                    .child(
                        img(std::sync::Arc::new(gpui::Image::from_bytes(
                            gpui::ImageFormat::Png,
                            include_bytes!("../../../assets/images/CorvoMark.png").to_vec(),
                        )))
                        .w(px(68.0))
                        .h(px(51.0)),
                    )
                    .child(
                        div()
                            .text_size(px(25.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Corvo"),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .bg(rgb(COLOR_CONTROL_BG))
                            .border_1()
                            .border_color(rgb(COLOR_CONTROL_BORDER))
                            .text_size(px(12.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(
                        div()
                            .id("about-source-link")
                            .cursor_pointer()
                            .text_size(px(13.0))
                            .text_color(rgb(COLOR_ACCENT))
                            .hover(|style| style.opacity(0.8))
                            .on_click(open_repository)
                            .child("View source on GitHub"),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("A lightweight native launcher."),
                    ),
            )
            .child(self.render_software_update_section(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Project"),
                    )
                    .child(
                        div()
                            .id("about-repository-row")
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_4()
                            .py_3()
                            .rounded_xl()
                            .bg(rgb(COLOR_CARD_BG))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(COLOR_CONTROL_HOVER)))
                            .on_click(cx.listener(|_: &mut Self, _: &ClickEvent, _window, _cx| {
                                let _ = corvo_platform::open_url(REPOSITORY_URL);
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(crate::icons::render_phosphor_svg(
                                        phosphor_svgs::style::regular::GITHUB_LOGO,
                                        rgb(COLOR_TEXT_MUTED),
                                        18.0,
                                    ))
                                    .child(div().text_size(px(14.0)).text_color(rgb(COLOR_TEXT)).child("Source code")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().text_size(px(13.0)).text_color(rgb(COLOR_TEXT_DIM)).child("GitHub"))
                                    .child(crate::icons::render_phosphor_svg(
                                        phosphor_svgs::style::regular::ARROW_UP_RIGHT,
                                        rgb(COLOR_TEXT_DIM),
                                        14.0,
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("about-support-row")
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .px_4()
                            .py_3()
                            .rounded_xl()
                            .bg(rgb(COLOR_CARD_BG))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .size(px(34.0))
                                            .rounded_md()
                                            .bg(rgb(COLOR_CONTROL_BG))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(crate::icons::render_phosphor_svg(
                                                phosphor_svgs::style::regular::HEART,
                                                rgb(COLOR_ACCENT),
                                                18.0,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(COLOR_TEXT)).child("Contribute"))
                                            .child(div().text_size(px(12.0)).text_color(rgb(COLOR_TEXT_DIM)).child("Explore the source and contribute on GitHub.")),
                                    ),
                            )
                            .child(
                                div()
                                    .id("about-support-button")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(rgb(COLOR_ACCENT))
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0x064e3b))
                                    .cursor_pointer()
                                    .hover(|style| style.opacity(0.88))
                                    .on_click(cx.listener(|_: &mut Self, _: &ClickEvent, _window, _cx| {
                                        let _ = corvo_platform::open_url(REPOSITORY_URL);
                                    }))
                                    .child("GitHub"),
                            ),
                    ),
            )
            .child(
                div().flex().justify_end().child(
                    div()
                        .id("about-open-logs")
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(rgb(COLOR_CONTROL_BG))
                        .text_size(px(13.0))
                        .text_color(rgb(COLOR_TEXT))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(COLOR_CONTROL_HOVER)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.open_logs(cx);
                        }))
                        .child("Open logs"),
                ),
            )
            .child(
                div()
                    .w_full()
                    .text_size(px(11.0))
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .text_center()
                    .child("Corvo · Native launcher"),
            )
    }

    pub fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.update_status,
            UpdateStatusUI::Checking
                | UpdateStatusUI::Downloading { .. }
                | UpdateStatusUI::Installing { .. }
        ) {
            return;
        }
        self.update_status = UpdateStatusUI::Checking;
        cx.notify();

        let channel = corvo_platform::UpdateChannel::parse(&self.settings.updates.channel);
        cx.spawn(async move |this, cx| {
            let res = smol::unblock(move || corvo_platform::check_for_updates(channel, true)).await;

            let _ = this.update(cx, |view, cx| {
                match res {
                    Ok(Some(release)) => {
                        view.update_status = UpdateStatusUI::Available(Box::new(release));
                    }
                    Ok(None) => {
                        view.update_status = UpdateStatusUI::UpToDate;
                    }
                    Err(e) => {
                        if !matches!(e, corvo_platform::UpdateError::Cancelled) {
                            corvo_platform::diagnostics::record_error("updater", "check_failed");
                        }
                        view.update_status = UpdateStatusUI::Error(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn start_download(
        &mut self,
        release: corvo_platform::UpdateRelease,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.update_status, UpdateStatusUI::Downloading { .. }) {
            return;
        }
        self.cancel_update_flag
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let cancel_flag = self.cancel_update_flag.clone();
        self.update_status = UpdateStatusUI::Downloading {
            downloaded: 0,
            total: release.asset.size,
            percent: 0.0,
        };
        cx.notify();

        let rel = release.clone();
        cx.spawn(async move |this, cx| {
            let (progress_tx, progress_rx) = smol::channel::unbounded::<(u64, u64)>();
            let cancel_for_thread = cancel_flag.clone();
            let rel_for_thread = rel.clone();
            let dl_task = smol::spawn(smol::unblock(move || {
                corvo_platform::download_and_verify(
                    &rel_for_thread,
                    &cancel_for_thread,
                    Some(&move |downloaded, total| {
                        let _ = progress_tx.try_send((downloaded, total));
                    }),
                )
            }));

            loop {
                smol::future::yield_now().await;
                if let Ok((downloaded, total)) = progress_rx.try_recv() {
                    let percent = if total > 0 {
                        (downloaded as f32 / total as f32) * 100.0
                    } else {
                        0.0
                    };
                    let _ = this.update(cx, |view, cx| {
                        if matches!(view.update_status, UpdateStatusUI::Downloading { .. }) {
                            view.update_status = UpdateStatusUI::Downloading {
                                downloaded,
                                total,
                                percent,
                            };
                            cx.notify();
                        }
                    });
                }
                if dl_task.is_finished() {
                    break;
                }
                smol::Timer::after(std::time::Duration::from_millis(50)).await;
            }

            let res = dl_task.await;
            let _ = this.update(cx, |view, cx| {
                match res {
                    Ok(path) => {
                        view.update_status = UpdateStatusUI::ReadyToInstall(Box::new(rel), path);
                    }
                    Err(corvo_platform::UpdateError::Cancelled) => {
                        view.update_status = UpdateStatusUI::Idle;
                    }
                    Err(e) => {
                        corvo_platform::diagnostics::record_error("updater", "download_failed");
                        view.update_status = UpdateStatusUI::Error(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_install(
        &mut self,
        release: corvo_platform::UpdateRelease,
        staged: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.update_status, UpdateStatusUI::Installing { .. }) {
            return;
        }
        self.update_status = UpdateStatusUI::Installing {
            release: Box::new(release.clone()),
        };
        cx.notify();

        cx.spawn(async move |this, cx| {
            let install_path = staged.clone();
            let result =
                smol::unblock(move || corvo_platform::install_and_restart(&install_path)).await;
            let message = match result {
                Ok(()) => {
                    corvo_platform::diagnostics::record_error("updater", "restart_failed");
                    "Corvo did not restart after the update".to_string()
                }
                Err(error) => {
                    if !matches!(error, corvo_platform::UpdateError::Cancelled) {
                        corvo_platform::diagnostics::record_error("updater", "install_failed");
                    }
                    error.to_string()
                }
            };
            let _ = this.update(cx, |view, cx| {
                view.update_status = UpdateStatusUI::InstallFailed {
                    release: Box::new(release),
                    staged,
                    message,
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn render_software_update_action_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        match &self.update_status {
            UpdateStatusUI::Idle => Some(
                div()
                    .id("btn-check-updates")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(COLOR_ACCENT))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .on_click(cx.listener(|this: &mut Self, _: &ClickEvent, _window, cx| {
                        this.check_for_updates(cx);
                    }))
                    .child("Check for Updates")
                    .into_any_element(),
            ),
            UpdateStatusUI::Checking => Some(
                div()
                    .id("btn-checking")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(COLOR_CONTROL_BG))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child("Checking...")
                    .into_any_element(),
            ),
            UpdateStatusUI::UpToDate => Some(
                div()
                    .id("btn-check-again")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(COLOR_CONTROL_BG))
                    .border_1()
                    .border_color(rgb(COLOR_CONTROL_BORDER))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(COLOR_TEXT))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .on_click(cx.listener(|this: &mut Self, _: &ClickEvent, _window, cx| {
                        this.check_for_updates(cx);
                    }))
                    .child("Check Again")
                    .into_any_element(),
            ),
            UpdateStatusUI::Available(rel) => {
                let rel_clone = (**rel).clone();
                Some(
                    div()
                        .id("btn-download-update")
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(rgb(COLOR_ACCENT))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xffffff))
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.85))
                        .on_click(cx.listener(
                            move |this: &mut Self, _: &ClickEvent, _window, cx| {
                                this.start_download(rel_clone.clone(), cx);
                            },
                        ))
                        .child("Download & Install")
                        .into_any_element(),
                )
            }
            UpdateStatusUI::Downloading { .. } => Some(
                div()
                    .id("btn-cancel-download")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(COLOR_CONTROL_BG))
                    .border_1()
                    .border_color(rgb(COLOR_CONTROL_BORDER))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(COLOR_TEXT))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .on_click(cx.listener(|this: &mut Self, _: &ClickEvent, _window, cx| {
                        this.cancel_update_flag
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        this.update_status = UpdateStatusUI::Idle;
                        cx.notify();
                    }))
                    .child("Cancel")
                    .into_any_element(),
            ),
            UpdateStatusUI::ReadyToInstall(rel, staged)
            | UpdateStatusUI::InstallFailed {
                release: rel,
                staged,
                ..
            } => {
                let rel_clone = (**rel).clone();
                let staged_clone = staged.clone();
                let retry = matches!(self.update_status, UpdateStatusUI::InstallFailed { .. });
                Some(
                    div()
                        .id("btn-relaunch-corvo")
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        // Keep the label whole when the status text beside
                        // it needs the room.
                        .flex_shrink_0()
                        .bg(rgb(COLOR_ACCENT))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xffffff))
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.85))
                        .on_click(cx.listener(
                            move |this: &mut Self, _: &ClickEvent, _window, cx| {
                                this.start_install(rel_clone.clone(), staged_clone.clone(), cx);
                            },
                        ))
                        .child(if retry {
                            "Retry Relaunch"
                        } else {
                            "Relaunch Corvo"
                        })
                        .into_any_element(),
                )
            }
            UpdateStatusUI::Installing { .. } => Some(
                div()
                    .id("btn-installing-update")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(COLOR_CONTROL_BG))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child("Installing...")
                    .into_any_element(),
            ),
            UpdateStatusUI::Error(_) => Some(
                div()
                    .id("btn-retry-check")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(COLOR_CONTROL_BG))
                    .border_1()
                    .border_color(rgb(COLOR_CONTROL_BORDER))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(COLOR_TEXT))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .on_click(cx.listener(|this: &mut Self, _: &ClickEvent, _window, cx| {
                        this.check_for_updates(cx);
                    }))
                    .child("Retry")
                    .into_any_element(),
            ),
        }
    }

    fn render_software_update_section(&self, cx: &mut Context<Self>) -> Div {
        let channel_str = match self.settings.updates.channel.to_lowercase().as_str() {
            "beta" => "Beta",
            _ => "Stable",
        };

        let (icon, title, subtitle, status_color) = match &self.update_status {
            UpdateStatusUI::Idle => (
                phosphor_svgs::style::regular::ARROW_CIRCLE_DOWN,
                format!("Corvo v{}", env!("CARGO_PKG_VERSION")),
                "Automatic updates are enabled.".to_string(),
                rgb(COLOR_TEXT_MUTED),
            ),
            UpdateStatusUI::Checking => (
                phosphor_svgs::style::regular::SPINNER,
                "Checking for updates...".to_string(),
                "Querying GitHub Releases feed...".to_string(),
                rgb(COLOR_ACCENT),
            ),
            UpdateStatusUI::UpToDate => (
                phosphor_svgs::style::regular::CHECK_CIRCLE,
                "Corvo is up to date".to_string(),
                format!(
                    "Version {} ({}) is the latest available.",
                    env!("CARGO_PKG_VERSION"),
                    channel_str
                ),
                rgb(COLOR_ACCENT),
            ),
            UpdateStatusUI::Available(rel) => (
                phosphor_svgs::style::regular::SPARKLE,
                format!("Update available: {}", rel.tag_name),
                format!("Release: {}", rel.title),
                rgb(COLOR_ACCENT),
            ),
            UpdateStatusUI::Downloading {
                downloaded,
                total,
                percent,
            } => (
                phosphor_svgs::style::regular::DOWNLOAD_SIMPLE,
                format!("Downloading update ({:.0}%)...", percent),
                format!(
                    "{:.1} MB of {:.1} MB",
                    *downloaded as f64 / 1_000_000.0,
                    *total as f64 / 1_000_000.0
                ),
                rgb(COLOR_ACCENT),
            ),
            UpdateStatusUI::ReadyToInstall(rel, _) => (
                phosphor_svgs::style::regular::CHECK_CIRCLE,
                format!("Ready to install {}", rel.tag_name),
                "Click Relaunch to apply the update immediately.".to_string(),
                rgb(COLOR_ACCENT),
            ),
            UpdateStatusUI::Installing { release, .. } => (
                phosphor_svgs::style::regular::SPINNER,
                format!("Installing {}", release.tag_name),
                "Corvo will restart when ready.".to_string(),
                rgb(COLOR_ACCENT),
            ),
            UpdateStatusUI::InstallFailed { message, .. } => (
                phosphor_svgs::style::regular::WARNING_CIRCLE,
                "Relaunch failed".to_string(),
                message.clone(),
                rgb(COLOR_DESTRUCTIVE),
            ),
            UpdateStatusUI::Error(err) => (
                phosphor_svgs::style::regular::WARNING_CIRCLE,
                "Update failed".to_string(),
                err.clone(),
                rgb(COLOR_DESTRUCTIVE),
            ),
        };

        let notes_view = if let UpdateStatusUI::Available(rel) = &self.update_status {
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(COLOR_CONTROL_BG))
                    .border_1()
                    .border_color(rgb(COLOR_CONTROL_BORDER))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("Release Notes"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child(rel.release_notes.clone()),
                    )
                    .child(
                        div().flex().justify_end().child(
                            div()
                                .id("btn-skip-version")
                                .cursor_pointer()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_MUTED))
                                .hover(|s| s.text_color(rgb(COLOR_TEXT)))
                                .on_click({
                                    let tag = rel.tag_name.clone();
                                    cx.listener(
                                        move |this: &mut Self, _: &ClickEvent, _window, cx| {
                                            corvo_platform::dismiss_version(&tag);
                                            this.update_status = UpdateStatusUI::Idle;
                                            cx.notify();
                                        },
                                    )
                                })
                                .child("Skip this version (Later)"),
                        ),
                    ),
            )
        } else {
            None
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child("Software Update"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .rounded_xl()
                    .bg(rgb(COLOR_CARD_BG))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            // Without a full width this row sizes to its
                            // content, so a long status message makes it
                            // wider than the card and pushes the action
                            // button outside the card's rounded edge.
                            .w_full()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(crate::icons::render_phosphor_svg(
                                        icon,
                                        status_color,
                                        20.0,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(14.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(COLOR_TEXT))
                                                    .child(title),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(rgb(COLOR_TEXT_DIM))
                                                    .child(subtitle),
                                            ),
                                    ),
                            )
                            .children(self.render_software_update_action_button(cx)),
                    )
                    .children(notes_view),
            )
            .child(self.section_group(
                "Update Settings",
                vec![
                    self.toggle_row(
                        "toggle-check-updates",
                        "Automatically check for updates",
                        Some("Checks once per day in the background."),
                        self.settings.updates.check_updates,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.settings.updates.check_updates =
                                !this.settings.updates.check_updates;
                            this.save_settings_file();
                            cx.notify();
                        }),
                    ),
                    self.toggle_row(
                        "toggle-auto-download",
                        "Automatically download updates",
                        Some("Downloads updates in the background when available."),
                        self.settings.updates.auto_download,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.settings.updates.auto_download =
                                !this.settings.updates.auto_download;
                            this.save_settings_file();
                            cx.notify();
                        }),
                    ),
                    self.interactive_dropdown_trigger(
                        "update-channel",
                        ("Update Channel", Some("Stable releases or early Beta prereleases.")),
                        ActiveDropdown::UpdateChannel,
                        if self.settings.updates.channel.to_lowercase() == "beta" {
                            1
                        } else {
                            0
                        },
                        &["Stable", "Beta (Prereleases)"],
                        cx,
                    ),
                ],
            ))
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
                    .child(format!(
                        "Configuration options for {} are enabled by default.",
                        tab.title()
                    )),
            )
    }

    fn section_group(&self, title: &'static str, rows: Vec<Div>) -> Div {
        let mut row_children = Vec::new();
        for (i, row) in rows.into_iter().enumerate() {
            if i > 0 {
                row_children.push(div().h(px(1.0)).bg(rgb(COLOR_BORDER_SUBTLE)));
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
                    .bg(rgb(COLOR_CARD_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .px_4()
                    .py_2()
                    .flex()
                    .flex_col()
                    .children(row_children),
            )
    }

    fn hotkey_setting_row(
        &self,
        title: &str,
        tab: SettingsTab,
        key: &str,
        hotkey: Option<&String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let is_recording = self
            .recording_hotkey_item
            .as_ref()
            .is_some_and(|(t, k)| *t == tab && k == key);
        let control = self.render_hotkey_control(
            tab,
            key,
            hotkey,
            is_recording,
            &format!("general-{key}"),
            cx,
        );

        div()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(COLOR_TEXT))
                    .child(title.to_string()),
            )
            .child(control)
    }

    fn toggle_row(
        &self,
        id: &'static str,
        title: &str,
        subtitle: Option<&str>,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let has_subtitle = subtitle.is_some();
        div()
            .h(if has_subtitle { px(54.0) } else { px(44.0) })
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(COLOR_TEXT))
                            .child(title.to_string()),
                    )
                    .when_some(subtitle, |el, sub| {
                        el.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(sub.to_string()),
                        )
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
                    .bg(if active {
                        rgb(COLOR_ACCENT)
                    } else {
                        rgb(COLOR_TOGGLE_OFF)
                    })
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

    fn interactive_dropdown_trigger(
        &self,
        id_str: &'static str,
        label: (&str, Option<&str>),
        dropdown_type: ActiveDropdown,
        current_idx: usize,
        options: &[&'static str],
        cx: &mut Context<Self>,
    ) -> Div {
        let (title, subtitle) = label;
        let is_open = self.active_dropdown == Some(dropdown_type);
        let selected_label = options.get(current_idx).copied().unwrap_or("");
        let has_subtitle = subtitle.is_some();

        div()
            .h(if has_subtitle { px(54.0) } else { px(44.0) })
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(COLOR_TEXT))
                            .child(title.to_string()),
                    )
                    .when_some(subtitle, |el, sub| {
                        el.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(sub.to_string()),
                        )
                    }),
            )
            .child(
                div()
                    .id(SharedString::from(format!("dropdown-trigger-{id_str}")))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px_2p5()
                    .py_1()
                    .rounded_md()
                    .bg(if is_open {
                        rgb(COLOR_ROW_SELECTED)
                    } else {
                        rgb(COLOR_CONTROL_BG)
                    })
                    .border_1()
                    .border_color(if is_open {
                        rgb(0x1e4a3b)
                    } else {
                        rgb(COLOR_CONTROL_BORDER)
                    })
                    .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        if this.active_dropdown == Some(dropdown_type) {
                            this.active_dropdown = None;
                        } else {
                            this.active_dropdown = Some(dropdown_type);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(COLOR_TEXT_MUTED))
                            .child(selected_label),
                    )
                    .child(crate::icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::CARET_UP_DOWN,
                        rgb(COLOR_TEXT_DIM),
                        12.0,
                    )),
            )
    }

    fn render_active_dropdown_popover(
        &self,
        active: ActiveDropdown,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let (top_offset, current_idx, options, id_str): (
            f32,
            usize,
            &[&'static str],
            &'static str,
        ) = match active {
            ActiveDropdown::PopToRoot => (
                278.0,
                self.settings.pop_to_root_option,
                &["Immediately", "After 90 seconds", "Never"],
                "pop-to-root",
            ),
            ActiveDropdown::EscapeBehavior => (
                333.0,
                self.settings.escape_behavior_option,
                &["Navigate back or close window", "Close window"],
                "escape-behavior",
            ),
            ActiveDropdown::AutoSwitchInput => (
                388.0,
                self.settings.auto_switch_input,
                &["None", "ABC"],
                "auto-switch-input",
            ),
            ActiveDropdown::Theme => (
                498.0,
                self.settings.theme_option,
                &["System", "Dark", "Light"],
                "theme-select",
            ),
            ActiveDropdown::ClipboardRetention => (
                303.0,
                self.settings.clipboard.retention_option,
                &["24 Hours", "7 Days", "30 Days", "3 Months", "1 Year"],
                "clipboard-retention",
            ),
            ActiveDropdown::WindowCycling => (
                168.0,
                self.settings.window_management.cycling_option,
                &["None", "Cycle forward", "Cycle forward & backward"],
                "window-cycling",
            ),
            ActiveDropdown::CalendarUpcomingMeetings => (
                168.0,
                self.settings.calendar.upcoming_meetings_option,
                &["1 next", "3 next", "5 next", "All today"],
                "calendar-upcoming",
            ),
            ActiveDropdown::CalendarJoinCard => (
                335.0,
                self.settings.calendar.join_card_option,
                &["Immediately", "5 minutes", "10 minutes", "15 minutes"],
                "calendar-join-card",
            ),
            ActiveDropdown::CalendarBrowser => (
                518.0,
                self.settings.calendar.open_meeting_links_in,
                &["Default Browser", "Google Chrome", "Safari", "Arc"],
                "calendar-browser",
            ),
            ActiveDropdown::UpdateChannel => (
                420.0,
                if self.settings.updates.channel.to_lowercase() == "beta" {
                    1
                } else {
                    0
                },
                &["Stable", "Beta (Prereleases)"],
                "update-channel",
            ),
        };

        let menu_items: Vec<_> = options
            .iter()
            .enumerate()
            .map(|(idx, &label)| {
                let is_selected = idx == current_idx;
                div()
                    .id(SharedString::from(format!("{id_str}-opt-{idx}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_selected, |s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .hover(|s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        match active {
                            ActiveDropdown::PopToRoot => this.settings.pop_to_root_option = idx,
                            ActiveDropdown::EscapeBehavior => {
                                this.settings.escape_behavior_option = idx
                            }
                            ActiveDropdown::AutoSwitchInput => {
                                this.settings.auto_switch_input = idx
                            }
                            ActiveDropdown::Theme => {
                                this.settings.theme_option = idx;
                                this.settings.theme = match idx {
                                    1 => "dark".to_string(),
                                    2 => "light".to_string(),
                                    _ => "system".to_string(),
                                };
                            }
                            ActiveDropdown::ClipboardRetention => {
                                this.settings.clipboard.retention_option = idx;
                                this.settings.clipboard.retention_days = match idx {
                                    0 => 1,
                                    1 => 7,
                                    2 => 30,
                                    3 => 90,
                                    _ => 365,
                                };
                            }
                            ActiveDropdown::WindowCycling => {
                                this.settings.window_management.cycling_option = idx;
                            }
                            ActiveDropdown::CalendarUpcomingMeetings => {
                                this.settings.calendar.upcoming_meetings_option = idx;
                            }
                            ActiveDropdown::CalendarJoinCard => {
                                this.settings.calendar.join_card_option = idx;
                            }
                            ActiveDropdown::CalendarBrowser => {
                                this.settings.calendar.open_meeting_links_in = idx;
                            }
                            ActiveDropdown::UpdateChannel => {
                                this.settings.updates.channel = match idx {
                                    1 => "beta".to_string(),
                                    _ => "stable".to_string(),
                                };
                            }
                        }
                        this.save_settings_file();
                        this.active_dropdown = None;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if is_selected {
                                rgb(COLOR_TEXT)
                            } else {
                                rgb(COLOR_TEXT_MUTED)
                            })
                            .font_weight(if is_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(label),
                    )
                    .when(is_selected, |row| {
                        row.child(crate::icons::render_phosphor_svg(
                            phosphor_svgs::style::regular::CHECK,
                            rgb(COLOR_ACCENT),
                            12.0,
                        ))
                    })
            })
            .collect();

        div()
            .id(SharedString::from(format!("popover-{id_str}")))
            .absolute()
            .top(px(top_offset))
            .right(px(16.0))
            .w(px(240.0))
            .bg(rgb(COLOR_CARD_BG))
            .border_1()
            .border_color(rgb(COLOR_BORDER_SUBTLE))
            .rounded_lg()
            .p_1()
            .shadow_xl()
            .flex()
            .flex_col()
            .children(menu_items)
    }

    fn segmented_row(
        &self,
        title: &str,
        subtitle: Option<&str>,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let has_subtitle = subtitle.is_some();
        div()
            .h(if has_subtitle { px(54.0) } else { px(44.0) })
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(COLOR_TEXT))
                            .child(title.to_string()),
                    )
                    .when_some(subtitle, |el, sub| {
                        el.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(sub.to_string()),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .p(px(2.0))
                    .rounded_lg()
                    .bg(rgb(COLOR_SIDEBAR_BG))
                    .border_1()
                    .border_color(rgb(COLOR_BORDER_SUBTLE))
                    .children(
                        [("Aa", px(12.0)), ("Aa", px(14.0)), ("Aa", px(16.0))]
                            .into_iter()
                            .enumerate()
                            .map(|(idx, (label, sz))| {
                                let active = idx == selected;
                                div()
                                    .id(SharedString::from(format!("segmented-size-{idx}")))
                                    .cursor_pointer()
                                    .w(px(36.0))
                                    .h(px(28.0))
                                    .rounded_md()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .border_1()
                                    .border_color(if active {
                                        rgb(COLOR_CONTROL_BORDER)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .bg(if active {
                                        rgb(COLOR_CONTROL_HOVER)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .text_size(sz)
                                    .text_color(if active {
                                        rgb(COLOR_TEXT)
                                    } else {
                                        rgb(COLOR_TEXT_DIM)
                                    })
                                    .font_weight(if active {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .hover(|s| {
                                        if !active {
                                            s.bg(rgb(COLOR_CONTROL_BG)).text_color(rgb(COLOR_TEXT))
                                        } else {
                                            s
                                        }
                                    })
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.settings.interface_size_option = idx;
                                            this.save_settings_file();
                                            cx.notify();
                                        },
                                    ))
                                    .child(
                                        div().flex().items_center().justify_center().child(label),
                                    )
                            }),
                    ),
            )
    }

    fn slider_row(&self, title: &str, level: usize, cx: &mut Context<Self>) -> Div {
        let clamped_level = level.min(4);
        let track_width = 120.0f32;
        let thumb_size = 16.0f32;
        let thumb_left = (clamped_level as f32 / 4.0) * (track_width - thumb_size);

        div()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(COLOR_TEXT))
                    .child(title.to_string()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("Less"),
                    )
                    .child(
                        div()
                            .id("transparency-track")
                            .relative()
                            .w(px(track_width))
                            .h(px(thumb_size))
                            .flex()
                            .items_center()
                            .child(
                                // Track bar
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .h(px(3.0))
                                    .rounded_full()
                                    .bg(rgb(COLOR_BORDER_SUBTLE)),
                            )
                            // 5 clickable dot targets across track
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(6.0))
                                    .children((0..5).map(|idx| {
                                        div()
                                            .id(SharedString::from(format!(
                                                "transparency-dot-{idx}"
                                            )))
                                            .size(px(4.0))
                                            .rounded_full()
                                            .bg(rgb(0x52525b))
                                            .cursor_pointer()
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _window, cx| {
                                                    this.settings.transparency_level = idx;
                                                    this.save_settings_file();
                                                    cx.notify();
                                                },
                                            ))
                                    })),
                            )
                            // White circular thumb knob
                            .child(
                                div()
                                    .id("transparency-thumb")
                                    .absolute()
                                    .left(px(thumb_left))
                                    .size(px(thumb_size))
                                    .rounded_full()
                                    .bg(rgb(0xffffff))
                                    .shadow_md(),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("More"),
                    )
                    .child(
                        div()
                            .id("reset-transparency-btn")
                            .cursor_pointer()
                            .px_2p5()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(COLOR_CONTROL_BG))
                            .border_1()
                            .border_color(rgb(COLOR_CONTROL_BORDER))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(COLOR_TEXT))
                            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.settings.transparency_level = 2;
                                this.save_settings_file();
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
            .key_context("SettingsView")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
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

/// Stops any pending hotkey recording. On Windows the Settings window
/// outlives its close button (hidden, not removed), so without this a
/// recording would keep every global hotkey unregistered.
fn stop_hotkey_recording(cx: &mut App) {
    if let Some(handle) = cx.try_global::<SettingsWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |view, _window, cx| {
                view.set_recording_hotkey_item(None, cx);
            });
        }
    }
}

/// Closes (or hides on Windows) the Settings window.
fn close_settings_window(window: &mut Window, cx: &mut App) {
    stop_hotkey_recording(cx);
    #[cfg(target_os = "windows")]
    set_settings_window_visible(window, false);
    #[cfg(not(target_os = "windows"))]
    window.remove_window();
}

#[cfg(target_os = "windows")]
fn set_settings_window_visible(window: &Window, visible: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(window: isize, command: i32) -> i32;
    }

    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            unsafe { ShowWindow(handle.hwnd.get(), if visible { 5 } else { 0 }) };
        }
    }
}

#[cfg(target_os = "windows")]
fn keep_settings_window_alive(window: &mut Window, cx: &mut App) {
    window.on_window_should_close(cx, |window, cx| {
        stop_hotkey_recording(cx);
        set_settings_window_visible(window, false);
        false
    });
}

/// Shows the settings window via Win32 ShowWindow. Required when the window was
/// pre-created with show:false and needs to be made visible on demand.
#[cfg(target_os = "windows")]
fn show_settings_window_win32(handle: WindowHandle<SettingsView>, cx: &mut App) {
    let _ = handle.update(cx, |_, window, _| {
        set_settings_window_visible(window, true);
    });
}

/// Pre-creates the settings window hidden so the first open is instant.
/// Call once at app startup on Windows.
#[cfg(target_os = "windows")]
pub fn prewarm_settings_window(cx: &mut App) {
    if let Some(handle) = cx.try_global::<SettingsWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            return;
        }
    }
    let display_id = corvo_platform::active_display_id().map(|id| gpui::DisplayId::new(id as u64));
    let window_size = size(px(780.0), px(540.0));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            display_id,
            window_size,
            cx,
        ))),
        display_id,
        titlebar: Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(14.0), px(14.0))),
        }),
        kind: WindowKind::Normal,
        is_resizable: true,
        is_movable: true,
        focus: false,
        show: false,
        ..Default::default()
    };
    let store = cx.global::<crate::StoreGlobal>().0.clone();
    if let Ok(handle) = cx.open_window(options, move |window, cx| {
        keep_settings_window_alive(window, cx);
        cx.new(|cx| SettingsView::new(cx, store, window))
    }) {
        cx.set_global(SettingsWindow(handle));
    }
}

/// Opens the Settings and Preferences window to a specific tab.
pub fn open_settings_tab(tab: SettingsTab, cx: &mut App) {
    if let Some(handle) = cx.try_global::<SettingsWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            #[cfg(target_os = "windows")]
            show_settings_window_win32(handle, cx);
            let _ = handle.update(cx, |view, window, cx| {
                view.navigate_to_tab(tab, cx);
                window.activate_window();
            });
            #[cfg(not(target_os = "windows"))]
            corvo_platform::activate_app(std::process::id() as i32);
            corvo_platform::order_window_front(780.0, 540.0);
            return;
        }
    }

    let display_id = corvo_platform::active_display_id().map(|id| gpui::DisplayId::new(id as u64));
    let window_size = size(px(780.0), px(540.0));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            display_id,
            window_size,
            cx,
        ))),
        display_id,
        titlebar: Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(14.0), px(14.0))),
        }),
        kind: WindowKind::Normal,
        is_resizable: true,
        is_movable: true,
        focus: true,
        show: true,
        ..Default::default()
    };

    let store = cx.global::<crate::StoreGlobal>().0.clone();
    let opened = cx.open_window(options, move |window, cx| {
        #[cfg(target_os = "windows")]
        keep_settings_window_alive(window, cx);
        cx.new(|cx| {
            let mut view = SettingsView::new(cx, store, window);
            if tab != SettingsTab::General {
                view.navigate_to_tab(tab, cx);
            }
            view
        })
    });

    if let Ok(handle) = opened {
        cx.set_global(SettingsWindow(handle));
        let _ = handle.update(cx, |_view, window, _cx| {
            window.activate_window();
        });
        #[cfg(not(target_os = "windows"))]
        corvo_platform::activate_app(std::process::id() as i32);
        corvo_platform::order_window_front(780.0, 540.0);
    }
}

/// Opens the Settings and Preferences window to a specific tab and checks for updates immediately.
pub fn open_settings_tab_with_update_check(tab: SettingsTab, cx: &mut App) {
    if let Some(handle) = cx.try_global::<SettingsWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            #[cfg(target_os = "windows")]
            show_settings_window_win32(handle, cx);
            let _ = handle.update(cx, |view, window, cx| {
                view.navigate_to_tab(tab, cx);
                if !matches!(
                    view.update_status,
                    UpdateStatusUI::Error(_) | UpdateStatusUI::InstallFailed { .. }
                ) {
                    view.check_for_updates(cx);
                }
                window.activate_window();
            });
            #[cfg(not(target_os = "windows"))]
            corvo_platform::activate_app(std::process::id() as i32);
            corvo_platform::order_window_front(780.0, 540.0);
            return;
        }
    }

    let display_id = corvo_platform::active_display_id().map(|id| gpui::DisplayId::new(id as u64));
    let window_size = size(px(780.0), px(540.0));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            display_id,
            window_size,
            cx,
        ))),
        display_id,
        titlebar: Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(14.0), px(14.0))),
        }),
        kind: WindowKind::Normal,
        is_resizable: true,
        is_movable: true,
        focus: true,
        show: true,
        ..Default::default()
    };

    let store = cx.global::<crate::StoreGlobal>().0.clone();
    let opened = cx.open_window(options, move |window, cx| {
        #[cfg(target_os = "windows")]
        keep_settings_window_alive(window, cx);
        cx.new(|cx| {
            let mut view = SettingsView::new(cx, store, window);
            if tab != SettingsTab::General {
                view.navigate_to_tab(tab, cx);
            }
            if !matches!(
                view.update_status,
                UpdateStatusUI::Error(_) | UpdateStatusUI::InstallFailed { .. }
            ) {
                view.check_for_updates(cx);
            }
            view
        })
    });

    if let Ok(handle) = opened {
        cx.set_global(SettingsWindow(handle));
        let _ = handle.update(cx, |_view, window, _cx| {
            window.activate_window();
        });
        #[cfg(not(target_os = "windows"))]
        corvo_platform::activate_app(std::process::id() as i32);
        corvo_platform::order_window_front(780.0, 540.0);
    }
}

/// Opens the Settings and Preferences window.
pub fn open_settings(cx: &mut App) {
    open_settings_tab(SettingsTab::General, cx);
}

fn show_update_window(
    release: corvo_platform::UpdateRelease,
    staged: Option<std::path::PathBuf>,
    cx: &mut App,
) {
    #[cfg(target_os = "windows")]
    crate::hide_launcher_before_settings(cx);
    if let Some(handle) = cx.try_global::<SettingsWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            #[cfg(target_os = "windows")]
            show_settings_window_win32(handle, cx);
            let _ = handle.update(cx, |view, window, cx| {
                view.navigate_to_tab(SettingsTab::About, cx);
                if !matches!(
                    view.update_status,
                    UpdateStatusUI::Error(_) | UpdateStatusUI::InstallFailed { .. }
                ) {
                    match staged {
                        Some(path) => {
                            view.update_status =
                                UpdateStatusUI::ReadyToInstall(Box::new(release), path)
                        }
                        None => {
                            view.update_status = UpdateStatusUI::Available(Box::new(release));
                        }
                    }
                }
                window.activate_window();
                cx.notify();
            });
            #[cfg(not(target_os = "windows"))]
            corvo_platform::activate_app(std::process::id() as i32);
            corvo_platform::order_window_front(780.0, 540.0);
            return;
        }
    }

    let display_id = corvo_platform::active_display_id().map(|id| gpui::DisplayId::new(id as u64));
    let window_size = size(px(780.0), px(540.0));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            display_id,
            window_size,
            cx,
        ))),
        display_id,
        titlebar: Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(14.0), px(14.0))),
        }),
        kind: WindowKind::Normal,
        is_resizable: true,
        is_movable: true,
        focus: true,
        show: true,
        ..Default::default()
    };

    let store = cx.global::<crate::StoreGlobal>().0.clone();
    let opened = cx.open_window(options, move |window, cx| {
        #[cfg(target_os = "windows")]
        keep_settings_window_alive(window, cx);
        cx.new(|cx| {
            let mut view = SettingsView::new(cx, store, window);
            view.navigate_to_tab(SettingsTab::About, cx);
            if !matches!(
                view.update_status,
                UpdateStatusUI::Error(_) | UpdateStatusUI::InstallFailed { .. }
            ) {
                match staged {
                    Some(path) => {
                        view.update_status = UpdateStatusUI::ReadyToInstall(Box::new(release), path)
                    }
                    None => view.update_status = UpdateStatusUI::Available(Box::new(release)),
                }
            }
            view
        })
    });

    if let Ok(handle) = opened {
        cx.set_global(SettingsWindow(handle));
        let _ = handle.update(cx, |_view, window, _cx| {
            window.activate_window();
        });
        #[cfg(not(target_os = "windows"))]
        corvo_platform::activate_app(std::process::id() as i32);
        corvo_platform::order_window_front(780.0, 540.0);
    }
}

/// Shows the About tab with the release changelog plus Skip and
/// Download buttons. Used by the background update check so a found
/// update is visible instead of silently downloaded and dropped.
pub fn open_settings_with_available_update(release: corvo_platform::UpdateRelease, cx: &mut App) {
    show_update_window(release, None, cx);
}

/// Shows the About tab in the ready-to-install state after a background
/// download, so the user only clicks Relaunch to apply the update.
pub fn open_settings_with_ready_update(
    release: corvo_platform::UpdateRelease,
    staged: std::path::PathBuf,
    cx: &mut App,
) {
    show_update_window(release, Some(staged), cx);
}

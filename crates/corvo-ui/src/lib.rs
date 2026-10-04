//! Launcher window (GPUI). Phase 1: typed queries reach the command
//! registry, results render as rows, Enter launches and dismisses.

mod icons;
mod note_editor;
mod onboarding;
mod settings;

pub use settings::{
    open_settings, open_settings_tab, open_settings_tab_with_update_check,
    open_settings_with_available_update, open_settings_with_ready_update, SettingsTab,
};

use corvo_core::{
    Action, ActionGroup, CommandAction, CommandError, CommandRegistry, DataStore, ExecutionContext,
    Icon, SearchContext, SearchResult,
};
use gpui::{
    actions, div, font, img, linear_color_stop, linear_gradient, prelude::*, px, rgb, rgba, size,
    uniform_list, AnyElement, App, AppContext, AsyncApp, Bounds, ClickEvent, Context, Div,
    FocusHandle, FontWeight, Global, InteractiveElement, IntoElement, KeyBinding, KeyDownEvent,
    ParentElement, Pixels, Render, ScrollHandle, ScrollStrategy, SharedString, Size, Stateful,
    Styled, Subscription, TextOverflow, TextRun, UniformList, UniformListScrollHandle, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use smol::channel::Receiver;

const WINDOW_WIDTH: f32 = 750.0;
const WINDOW_HEIGHT: f32 = 475.0;
const COMPACT_WINDOW_HEIGHT: f32 = 58.0;
const ROW_HEIGHT: f32 = 38.0;
const ICON_SIZE: f32 = 26.0;
/// Corner radius of the launcher panel, in logical pixels. `rounded_xl` in
/// the element tree and the HWND region on Windows both read this value, so
/// the painted border and the OS window edge cannot drift apart.
fn launcher_corner_radius() -> f32 {
    12.0
}
const CONFIDENT_SEARCH_SCORE: i32 = 1800;
const MAX_WEAK_SEARCH_RESULTS: usize = 8;

// Raycast-like surfaces, black and gray, with the emerald accent and 90% opacity (10% translucent).
const COLOR_BACKGROUND: u32 = 0x17181ae6;
const COLOR_DIVIDER: u32 = 0x282a2d;
const COLOR_ROW_SELECTED: u32 = 0x113c30;
const COLOR_ACCENT: u32 = 0x34d399;

const COLOR_PILL: u32 = 0x222426;
const COLOR_KEYCAP: u32 = 0x2d3034;
const COLOR_MENU: u32 = 0x1c1e20f0;
const COLOR_TEXT: u32 = 0xffffff;
const COLOR_TEXT_DIM: u32 = 0x8e8e93;
const COLOR_TEXT_ICON: u32 = 0x9a9aa0;
const COLOR_DESTRUCTIVE: u32 = 0xef4444;

fn sort_search_results(results: &mut [SearchResult]) {
    results.sort_by(|left, right| {
        let left_calc = left.id.starts_with("calculator:");
        let right_calc = right.id.starts_with("calculator:");
        right_calc.cmp(&left_calc).then_with(|| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| corvo_core::search::natural_cmp(&left.title, &right.title))
                .then_with(|| left.id.cmp(&right.id))
        })
    });
}

fn command_menu_match_score(query: &str, result: &SearchResult) -> Option<i32> {
    let fields = [
        result.title.as_str(),
        result.subtitle.as_deref().unwrap_or_default(),
        result.accessory.as_deref().unwrap_or_default(),
    ];
    corvo_core::search_match_score(query, &fields).or_else(|| {
        let useful_words = query
            .split_whitespace()
            .filter(|word| !matches!(word.to_lowercase().as_str(), "all" | "the" | "a" | "an"))
            .collect::<Vec<_>>();
        let reduced_query = useful_words.join(" ");
        (!reduced_query.is_empty() && reduced_query != query)
            .then(|| corvo_core::search_match_score(&reduced_query, &fields))
            .flatten()
    })
}

fn command_menu_exact_word_match(query: &str, result: &SearchResult) -> bool {
    let query_words = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| {
            !word.is_empty() && !matches!(word.to_lowercase().as_str(), "all" | "the" | "a" | "an")
        })
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let result_words = [
        result.title.as_str(),
        result.subtitle.as_deref().unwrap_or_default(),
        result.accessory.as_deref().unwrap_or_default(),
    ]
    .into_iter()
    .flat_map(|field| field.split(|character: char| !character.is_alphanumeric()))
    .map(str::to_lowercase)
    .collect::<Vec<_>>();

    !query_words.is_empty()
        && query_words
            .iter()
            .all(|query_word| result_words.iter().any(|word| word == query_word))
}

fn apply_result_preferences(
    results: &mut Vec<SearchResult>,
    settings: &corvo_config::Settings,
    query: &str,
) {
    results.retain(|result| {
        !settings
            .result_item(&result.id, &result.title)
            .is_some_and(|item| item.hidden)
    });

    let q_folded = corvo_core::search::fold(query);
    let q_len = query.chars().count();
    let frecency_store = corvo_config::ranking::FrecencyStore::global();
    let store_guard = frecency_store.lock().ok();

    let candidates: Vec<corvo_core::search::CandidateItem> = results
        .iter()
        .map(|r| {
            let item_pref = settings.result_item(&r.id, &r.title);
            let user_alias = item_pref.and_then(|p| p.alias.as_deref());
            let (has_alias, exact_alias, prefix_alias) = if let Some(alias) = user_alias {
                let a_folded = corvo_core::search::fold(alias);
                (
                    true,
                    !q_folded.is_empty() && a_folded == q_folded,
                    !q_folded.is_empty() && a_folded.starts_with(&q_folded),
                )
            } else {
                (false, false, false)
            };

            let is_favorite = settings.result_is_favorite(&r.id);
            let frecency = store_guard
                .as_ref()
                .map(|g| g.get_frecency(&r.id))
                .unwrap_or(1.0);

            let learned_term = if q_folded.is_empty() {
                None
            } else {
                store_guard.as_ref().and_then(|g| {
                    let terms = g.get_learned_terms(&r.id);
                    let mut best = None;
                    for t in terms {
                        let t_folded = corvo_core::search::fold(&t);
                        if t_folded == q_folded {
                            return Some(corvo_core::search::LearnedTermMatch::Exact);
                        } else if t_folded.starts_with(&q_folded) {
                            best = Some(corvo_core::search::LearnedTermMatch::Prefix);
                        } else if q_folded.starts_with(&t_folded)
                            && q_folded.len() <= t_folded.len() + 3
                            && t_folded.len() >= 3
                        {
                            let delta = q_folded.len() - t_folded.len();
                            best = Some(corvo_core::search::LearnedTermMatch::Overbounds { delta });
                        }
                    }
                    best
                })
            };

            let t_folded = corvo_core::search::fold(&r.title);
            let title_is_exact = !q_folded.is_empty() && t_folded == q_folded;
            let title_is_prefix = !q_folded.is_empty() && t_folded.starts_with(&q_folded);

            let subtitle_is_exact = if let Some(sub) = r.subtitle.as_deref() {
                let s_folded = corvo_core::search::fold(sub);
                !q_folded.is_empty() && s_folded == q_folded
            } else {
                false
            };

            let priority = if is_favorite { 1000 } else { 0 };

            corvo_core::search::CandidateItem {
                id: &r.id,
                title: &r.title,
                subtitle: r.subtitle.as_deref(),
                has_user_alias: has_alias,
                is_exact_user_alias: exact_alias,
                is_prefix_user_alias: prefix_alias,
                is_boosted: is_favorite,
                learned_term,
                frecency,
                quality: r.score,
                title_is_exact,
                title_is_prefix,
                subtitle_is_exact,
                priority,
            }
        })
        .collect();

    let mut indices: Vec<usize> = (0..results.len()).collect();
    indices.sort_by(|&i, &j| {
        let left_calc = results[i].id.starts_with("calculator:");
        let right_calc = results[j].id.starts_with("calculator:");
        right_calc.cmp(&left_calc).then_with(|| {
            corvo_core::search::LauncherOrder::compare(&candidates[i], &candidates[j], q_len)
        })
    });

    let mut sorted_results = Vec::with_capacity(results.len());
    for idx in indices {
        sorted_results.push(results[idx].clone());
    }
    *results = sorted_results;
}

fn cached_query_results(
    query: &str,
    max_results: usize,
    fallback_enabled: bool,
) -> Vec<SearchResult> {
    let mut results: Vec<_> = cached_initial_results()
        .into_iter()
        .filter_map(|mut result| {
            let score = corvo_core::search_match_score(
                query,
                &[&result.title, result.subtitle.as_deref().unwrap_or("")],
            )?;
            result.score = score;
            Some(result)
        })
        .collect();
    sort_search_results(&mut results);
    let has_confident_match = results
        .first()
        .is_some_and(|result| result.score >= CONFIDENT_SEARCH_SCORE);
    results.truncate(if has_confident_match {
        max_results.saturating_sub(2)
    } else {
        MAX_WEAK_SEARCH_RESULTS
    });
    if fallback_enabled {
        results.extend(corvo_web_search_fallback::search_results(query));
    }
    results
}

actions!(corvo_ui, [Dismiss]);

struct RegistryGlobal(CommandRegistry);

impl Global for RegistryGlobal {}

pub(crate) struct StoreGlobal(pub std::sync::Arc<dyn DataStore>);

impl Global for StoreGlobal {}

static INITIAL_RESULTS: std::sync::OnceLock<std::sync::RwLock<Vec<SearchResult>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
fn windows_icon_ready_channel() -> &'static (smol::channel::Sender<()>, smol::channel::Receiver<()>)
{
    static CHANNEL: std::sync::OnceLock<(smol::channel::Sender<()>, smol::channel::Receiver<()>)> =
        std::sync::OnceLock::new();
    CHANNEL.get_or_init(smol::channel::unbounded)
}

pub fn preload_initial_results(registry: &CommandRegistry, store: std::sync::Arc<dyn DataStore>) {
    let ctx = SearchContext {
        max_results: 2000,
        store: Some(store),
    };
    let mut results = Vec::new();
    for command in registry.commands() {
        if command.prefix().is_none()
            && ctx.store.as_ref().is_some_and(|store| {
                store.command_enabled(command.id()) && store.show_command_in_launcher(command.id())
            })
        {
            results.extend(smol::block_on(command.search("", &ctx)));
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(2000);
    #[cfg(target_os = "windows")]
    {
        let icon_paths = results
            .iter()
            .filter_map(|result| match &result.icon {
                Icon::Image(path) => Some(path.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for path in icon_paths.iter().take(24) {
            let _ = windows_render_icon(path);
        }
        let cell = INITIAL_RESULTS.get_or_init(|| std::sync::RwLock::new(Vec::new()));
        if let Ok(mut lock) = cell.write() {
            *lock = results;
        }
        std::thread::spawn(move || {
            for path in icon_paths.iter().skip(24) {
                let _ = windows_render_icon(path);
            }
            let _ = windows_icon_ready_channel().0.try_send(());
        });
        return;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let cell = INITIAL_RESULTS.get_or_init(|| std::sync::RwLock::new(Vec::new()));
        if let Ok(mut lock) = cell.write() {
            *lock = results;
        }
    }
}

pub fn cached_initial_results() -> Vec<SearchResult> {
    INITIAL_RESULTS
        .get()
        .and_then(|lock| lock.read().ok())
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

struct LauncherWindow(WindowHandle<Launcher>);

impl Global for LauncherWindow {}

struct ToastWindowGlobal(WindowHandle<ActionToast>);

impl Global for ToastWindowGlobal {}

#[derive(Clone, Copy)]
enum ToastCategory {
    System,
    Installation,
    Uninstallation,
    Port,
    Clipboard,
    Process,
    Window,
    Brightness,
    Homebrew,
    Application,
    Files,
    Quicklink,
    Snippet,
    Search,
    General,
}

impl ToastCategory {
    fn label(self) -> &'static str {
        match self {
            Self::System => "SYSTEM",
            Self::Installation => "INSTALL",
            Self::Uninstallation => "UNINSTALL",
            Self::Port => "PORT MANAGER",
            Self::Clipboard => "CLIPBOARD",
            Self::Process => "PROCESS",
            Self::Window => "WINDOW",
            Self::Brightness => "DISPLAY",
            Self::Homebrew => "HOMEBREW",
            Self::Application => "APPLICATION",
            Self::Files => "FILES",
            Self::Quicklink => "LINK",
            Self::Snippet => "SNIPPET",
            Self::Search => "SEARCH",
            Self::General => "CORVO",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::System => "⚙",
            Self::Installation => "↓",
            Self::Uninstallation => "⌫",
            Self::Port => "⇥",
            Self::Clipboard => "▣",
            Self::Process => "◉",
            Self::Window => "◩",
            Self::Brightness => "☼",
            Self::Homebrew => "🍺",
            // A window badge, not a modifier: the command glyph here read
            // as a Mac-only shortcut on every platform.
            Self::Application => "▣",
            Self::Files => "▤",
            Self::Quicklink => "↗",
            Self::Snippet => "✎",
            Self::Search => "⌕",
            Self::General => "•",
        }
    }
}

#[derive(Clone, Copy)]
enum ToastOutcome {
    Success,
    Failure,
}

struct ToastNotice {
    category: ToastCategory,
    title: String,
    detail: String,
    outcome: ToastOutcome,
    progress: Option<ToastProgress>,
}

#[derive(Clone, Copy)]
struct ToastProgress {
    percent: f32,
}

impl ToastNotice {
    fn success(
        category: ToastCategory,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            category,
            title: title.into(),
            detail: detail.into(),
            outcome: ToastOutcome::Success,
            progress: None,
        }
    }

    fn failure(
        category: ToastCategory,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        corvo_platform::diagnostics::record_error(category.label(), "action_failed");
        Self {
            category,
            title: title.into(),
            detail: detail.into(),
            outcome: ToastOutcome::Failure,
            progress: None,
        }
    }

    fn with_progress(mut self, percent: f32) -> Self {
        self.progress = Some(ToastProgress {
            percent: percent.clamp(0.0, 100.0),
        });
        self
    }
}

struct ProcessToastContext {
    category: ToastCategory,
    success_title: String,
    success_detail: String,
    failure_title: String,
}

fn process_toast_context(title: &str, args: &[String], is_brew: bool) -> ProcessToastContext {
    if !is_brew {
        return ProcessToastContext {
            category: ToastCategory::General,
            success_title: title.to_string(),
            success_detail: "Completed".into(),
            failure_title: format!("{title} failed"),
        };
    }

    let operation = args.first().map(String::as_str).unwrap_or_default();
    let target = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str);
    let (category, success_title, success_detail, failure_title) = match operation {
        "install" => (
            ToastCategory::Installation,
            "Installed",
            target.unwrap_or("package"),
            "Install failed",
        ),
        "uninstall" => (
            ToastCategory::Uninstallation,
            "Uninstalled",
            target.unwrap_or("package"),
            "Uninstall failed",
        ),
        "upgrade" => (
            ToastCategory::Homebrew,
            if target.is_some() {
                "Upgraded"
            } else {
                "Upgrade complete"
            },
            target.unwrap_or("All outdated packages"),
            "Upgrade failed",
        ),
        "services" => {
            let service_action = args.get(1).map(String::as_str).unwrap_or("updated");
            let service = args.get(2).map(String::as_str).unwrap_or("service");
            let (verb, past) = match service_action {
                "start" => ("Start", "Started"),
                "stop" => ("Stop", "Stopped"),
                "restart" => ("Restart", "Restarted"),
                _ => ("Update", "Updated"),
            };
            (
                ToastCategory::System,
                match past {
                    "Started" => "Service started",
                    "Stopped" => "Service stopped",
                    "Restarted" => "Service restarted",
                    _ => "Service updated",
                },
                service,
                match verb {
                    "Start" => "Could not start service",
                    "Stop" => "Could not stop service",
                    "Restart" => "Could not restart service",
                    _ => "Could not update service",
                },
            )
        }
        "cleanup" => (
            ToastCategory::Homebrew,
            "Cleanup complete",
            "Homebrew cache cleaned",
            "Cleanup failed",
        ),
        _ => (
            ToastCategory::Homebrew,
            title,
            "Completed",
            "Homebrew command failed",
        ),
    };

    ProcessToastContext {
        category,
        success_title: success_title.into(),
        success_detail: success_detail.into(),
        failure_title: failure_title.into(),
    }
}

fn process_failure_detail(error: &str) -> String {
    let detail = error
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(error)
        .trim();
    detail.chars().take(120).collect()
}

async fn run_shell_toast(
    cmd: String,
    category: ToastCategory,
    title: String,
    volume_direction: Option<bool>,
) -> ToastNotice {
    let (result, volume_level) = smol::unblock(move || {
        let result = corvo_platform::run_shell(&cmd);
        let volume_level = if result.is_ok() && volume_direction.is_some() {
            corvo_platform::audio_output_level()
        } else {
            None
        };
        (result, volume_level)
    })
    .await;
    match result {
        Ok(()) => match (volume_direction, volume_level) {
            (Some(increasing), Some(percent)) => ToastNotice::success(
                category,
                if increasing {
                    "Volume increased"
                } else {
                    "Volume decreased"
                },
                "Output volume",
            )
            .with_progress(percent),
            (Some(increasing), None) => ToastNotice::success(
                category,
                if increasing {
                    "Volume increased"
                } else {
                    "Volume decreased"
                },
                "Output level unavailable",
            ),
            _ => ToastNotice::success(category, title, "Completed"),
        },
        Err(error) => ToastNotice::failure(category, format!("{title} failed"), error.to_string()),
    }
}

fn category_for_result(result_id: &str) -> ToastCategory {
    match result_id.split(':').next().unwrap_or_default() {
        "system-actions" => ToastCategory::System,
        "brew" => ToastCategory::Homebrew,
        "kill-process" => {
            if result_id == "kill-process:open-ports" {
                ToastCategory::Port
            } else {
                ToastCategory::Process
            }
        }
        "clipboard-manager" | "emoji-picker" => ToastCategory::Clipboard,
        "window-management" => ToastCategory::Window,
        "app-launcher" => ToastCategory::Application,
        "file-search" => ToastCategory::Files,
        "quicklinks" => ToastCategory::Quicklink,
        "snippets" => ToastCategory::Snippet,
        "web-search-fallback" => ToastCategory::Search,
        _ => ToastCategory::General,
    }
}

fn copy_notice(result_id: &str, result: Result<(), String>, item_kind: &str) -> ToastNotice {
    let category = category_for_result(result_id);
    match result {
        Ok(()) => {
            let detail = match result_id.split(':').next().unwrap_or_default() {
                "emoji-picker" => "Emoji copied to clipboard",
                "quicklinks" => "Link copied to clipboard",
                "snippets" => "Snippet copied to clipboard",
                "web-search-fallback" => "Search link copied to clipboard",
                _ => match item_kind {
                    "image" => "Image copied to clipboard",
                    "emoji" => "Emoji copied to clipboard",
                    "link" => "Link copied to clipboard",
                    _ => "Text copied to clipboard",
                },
            };
            ToastNotice::success(category, "Copied", detail)
        }
        Err(error) => ToastNotice::failure(category, "Copy failed", error),
    }
}

fn paste_notice(result: Result<(), String>, item_kind: &str) -> ToastNotice {
    match result {
        Ok(()) => ToastNotice::success(
            ToastCategory::Clipboard,
            "Pasted",
            format!("{} pasted into the previous app", item_kind),
        ),
        Err(error) => ToastNotice::failure(ToastCategory::Clipboard, "Paste failed", error),
    }
}

struct ActionToast {
    notice: ToastNotice,
}

impl Render for ActionToast {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(progress) = self.notice.progress {
            let is_brightness = matches!(self.notice.category, ToastCategory::Brightness);
            let icon = if is_brightness {
                phosphor_svgs::style::regular::SUN
            } else {
                phosphor_svgs::style::regular::SPEAKER_HIGH
            };
            let fill_width = 172.0 * progress.percent / 100.0;
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(220.0))
                        .h(px(132.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_between()
                        .p_6()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgba(0x343638ff))
                        .bg(rgba(0x17181aff))
                        .child(crate::icons::render_phosphor_svg(
                            icon,
                            rgb(COLOR_TEXT),
                            30.0,
                        ))
                        .child(
                            div().flex().items_center().child(
                                div()
                                    .w(px(172.0))
                                    .h(px(8.0))
                                    .rounded_full()
                                    .bg(rgba(0x343638ff))
                                    .child(
                                        div()
                                            .w(px(fill_width))
                                            .h(px(8.0))
                                            .rounded_full()
                                            .bg(rgb(COLOR_ACCENT)),
                                    ),
                            ),
                        ),
                );
        }

        let (symbol, color) = if matches!(self.notice.outcome, ToastOutcome::Success) {
            ("✓", COLOR_ACCENT)
        } else {
            ("!", COLOR_DESTRUCTIVE)
        };
        let pill = div()
            .w(px(520.0))
            .flex()
            .flex_col()
            .gap_1()
            .px_5()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(rgba(0x343638ff))
            .bg(rgba(0x111214ff))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgba(0x25282aff))
                            .text_size(px(15.0))
                            .text_color(rgb(COLOR_ACCENT))
                            .child(self.notice.category.symbol()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child(self.notice.category.label()),
                            )
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .text_color(rgb(COLOR_TEXT))
                                    .text_overflow(TextOverflow::Truncate(
                                        SharedString::new_static("…"),
                                    ))
                                    .child(format!(
                                        "{} · {}",
                                        self.notice.title, self.notice.detail
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(18.0))
                            .text_color(rgb(color))
                            .child(symbol),
                    ),
            );
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(pill)
    }
}

fn show_action_toast(notice: ToastNotice, cx: &mut App) {
    if let Some(handle) = cx.try_global::<ToastWindowGlobal>().map(|global| global.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |_toast, window, _cx| window.remove_window());
        }
    }

    let toast_size = if notice.progress.is_some() {
        size(px(240.0), px(172.0))
    } else {
        size(px(520.0), px(52.0))
    };
    let display_id = active_display_id();
    let mut bounds = centered_bounds(toast_size, cx);
    if let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| Some(display.id()) == display_id)
        .or_else(|| cx.primary_display())
    {
        let display_bounds = display.bounds();
        bounds.origin.x =
            display_bounds.origin.x + (display_bounds.size.width - toast_size.width) / 2.0;
        bounds.origin.y =
            display_bounds.origin.y + display_bounds.size.height - toast_size.height - px(84.0);
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id,
        titlebar: None,
        kind: window_kind(),
        is_resizable: false,
        is_movable: false,
        is_minimizable: false,
        focus: false,
        show: true,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let opened: Result<WindowHandle<ActionToast>, _> =
        cx.open_window(options, move |window, cx| {
            let toast = cx.new(|_| ActionToast { notice });
            let toast_window = window.window_handle();
            cx.spawn(async move |cx| {
                smol::Timer::after(std::time::Duration::from_millis(2400)).await;
                let _ = toast_window.update(cx, |_view, window, _cx| window.remove_window());
            })
            .detach();
            toast
        });
    if let Ok(handle) = opened {
        cx.set_global(ToastWindowGlobal(handle));
        #[cfg(target_os = "macos")]
        corvo_platform::remove_action_toast_shadow(
            toast_size.width.to_f64(),
            toast_size.height.to_f64(),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherPage {
    Root,
    Emoji,
    Clipboard,
    Files,
    Brew,
    Text,
    Notes,
    /// One page per browser extension; the id picks the browser.
    Browser(corvo_browser_tabs::BrowserId),
    Ports,
    Processes,
    Uninstaller,
    /// A declarative extension page: the command id names the
    /// extension, and the view comes from `Command::page`. Pomodoro,
    /// Weather, and Media Control render through this variant too.
    Extension(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrewPageMode {
    Installed,
    Upgrades,
    Services,
    Search,
}

impl BrewPageMode {
    fn token(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Upgrades => "upgrades",
            Self::Services => "services",
            Self::Search => "search",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::Upgrades => "Upgrades",
            Self::Services => "Services",
            Self::Search => "Search Homebrew",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteSize {
    Compact,
    Extended,
}

impl PaletteSize {
    pub fn height(self, size_scale: f32) -> f32 {
        match self {
            Self::Compact => COMPACT_WINDOW_HEIGHT * size_scale,
            Self::Extended => WINDOW_HEIGHT * size_scale,
        }
    }
}

pub fn palette_size(
    compact_mode: bool,
    force_expanded: bool,
    page: LauncherPage,
    query: &str,
    actions_open: bool,
    burger_menu_open: bool,
    filter_dropdown_open: bool,
) -> PaletteSize {
    if compact_mode
        && !force_expanded
        && page == LauncherPage::Root
        && query.trim().is_empty()
        && !actions_open
        && !burger_menu_open
        && !filter_dropdown_open
    {
        PaletteSize::Compact
    } else {
        PaletteSize::Extended
    }
}

const EMOJI_CATEGORIES: &[&str] = &[
    "All Categories",
    "Smileys & Emotion",
    "People & Body",
    "Hearts & Symbols",
    "Animals & Nature",
    "Food & Drink",
    "Travel & Places",
    "Activities",
    "Objects",
    "Symbols",
    "Office & Study",
];

const CLIPBOARD_FILTERS: &[&str] = &["All Types", "Text", "Links", "Images", "JSON"];

#[derive(Clone, Debug)]
enum RootFlatItem {
    Header(SharedString),
    Row(usize),
}

/// In-progress form input on an extension Form page: one text value
/// and caret per text field, one boolean per checkbox, one selected
/// option per select, plus the focused field index.
#[derive(Default)]
struct ExtensionFormState {
    focused: usize,
    text_values: Vec<String>,
    text_carets: Vec<usize>,
    checks: Vec<bool>,
    selections: Vec<usize>,
}

impl ExtensionFormState {
    fn reset(&mut self, fields: &[corvo_core::FormField]) {
        self.focused = 0;
        self.text_values = fields
            .iter()
            .map(|field| match field {
                corvo_core::FormField::Text { default, .. } => default.clone(),
                _ => String::new(),
            })
            .collect();
        self.text_carets = self.text_values.iter().map(|value| value.chars().count()).collect();
        self.checks = fields
            .iter()
            .map(|field| match field {
                corvo_core::FormField::Checkbox { default, .. } => *default,
                _ => false,
            })
            .collect();
        self.selections = fields
            .iter()
            .map(|field| match field {
                corvo_core::FormField::Select { options, default, .. } => options
                    .iter()
                    .position(|(value, _)| value == default)
                    .unwrap_or(0),
                _ => 0,
            })
            .collect();
    }
}

/// The list header of one section: the section label, or the default
/// group label for unsectioned rows.
fn section_header(section: Option<&str>) -> SharedString {
    section
        .map(SharedString::from)
        .unwrap_or_else(|| SharedString::from("Results"))
}

/// Whether two page views have the same interactive shape: the same
/// kind, and for Forms the same fields. A shape change resets token
/// state; a data refresh (the ticking countdown) must not.
fn same_page_shape(current: &corvo_core::PageView, next: &corvo_core::PageView) -> bool {
    match (current, next) {
        (corvo_core::PageView::Blocks(_), corvo_core::PageView::Blocks(_)) => true,
        (
            corvo_core::PageView::Form { fields, .. },
            corvo_core::PageView::Form {
                fields: next_fields, ..
            },
        ) => {
            fields.len() == next_fields.len()
                && fields
                    .iter()
                    .zip(next_fields.iter())
                    .all(|(field, next_field)| field.id() == next_field.id())
        }
        _ => std::mem::discriminant(current) == std::mem::discriminant(next),
    }
}

/// Minimal inline markdown for Blocks and Detail surfaces: headings,
/// **bold**, *italic*, `code`, and paragraphs. Line-level structure
/// only; the full renderer stays in the note editor.
fn render_inline_markdown(markdown: &str) -> Div {
    let mut column = div().flex().flex_col().gap_1p5();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (size, weight, text) = if let Some(heading) = trimmed.strip_prefix("### ") {
            (px(13.0), FontWeight::BOLD, heading.to_string())
        } else if let Some(heading) = trimmed.strip_prefix("## ") {
            (px(15.0), FontWeight::BOLD, heading.to_string())
        } else if let Some(heading) = trimmed.strip_prefix("# ") {
            (px(17.0), FontWeight::BOLD, heading.to_string())
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            (px(13.0), FontWeight::NORMAL, format!("• {}", &trimmed[2..]))
        } else {
            (px(13.0), FontWeight::NORMAL, trimmed.to_string())
        };
        column = column.child(
            div()
                .text_size(size)
                .font_weight(weight)
                .text_color(rgb(COLOR_TEXT))
                .child(text),
        );
    }
    column
}

    /// Percent-encodes one form value for the submit id.
    fn encode_form_value(value: &str) -> String {
        let mut encoded = String::new();
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char)
                }
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        encoded
    }

#[derive(Clone, Debug)]
enum ClipboardFlatItem {
    Header(SharedString),
    Row(usize),
}

enum FilePreviewState {
    Empty,
    Loading(std::path::PathBuf),
    Ready {
        path: std::path::PathBuf,
        preview: FilePreview,
    },
}

enum FilePreview {
    Folder,
    Image,
    Text(String),
    Unsupported,
    Error(String),
}

#[derive(Clone)]
struct FileSearchCandidate {
    result: SearchResult,
    name: corvo_core::search::SearchText,
    path: corvo_core::search::SearchText,
}

#[derive(Clone, Debug)]
struct UninstallTarget {
    name: String,
    path: std::path::PathBuf,
    icon_png: Option<std::path::PathBuf>,
}

struct UninstallerReady {
    target: UninstallTarget,
    files: Vec<corvo_platform::AppFileEntry>,
    selected_files: Vec<bool>,
    focused: usize,
    confirming: bool,
    scan_in_progress: bool,
    scan_limited: bool,
    message: Option<String>,
}

enum UninstallerState {
    Closed,
    Ready(UninstallerReady),
    Removing { target: UninstallTarget },
}

pub struct Launcher {
    focus_handle: FocusHandle,
    page: LauncherPage,
    query: String,
    cursor_idx: usize,
    port_argument: String,
    port_cursor_idx: usize,
    port_input_active: bool,
    /// The declarative view the extension page currently renders,
    /// plus the token state the interactive views need.
    extension_view: Option<corvo_core::PageView>,
    extension_scroll_handle: ScrollHandle,
    extension_pump_running: bool,
    /// Highlighted button on a Blocks page; ←/→ cycle it.
    extension_button_focus: usize,
    /// Focused field and per-field state on a Form page.
    extension_form: ExtensionFormState,
    cursor_visible: bool,
    results: Vec<SearchResult>,
    selected: usize,
    search_seq: u64,
    results_scroll_handle: ScrollHandle,
    emoji_scroll_handle: ScrollHandle,
    clipboard_scroll_handle: UniformListScrollHandle,
    file_scroll_handle: ScrollHandle,
    file_preview: FilePreviewState,
    file_preview_seq: u64,
    file_default_results: Vec<SearchResult>,
    file_index: std::sync::Arc<Vec<FileSearchCandidate>>,
    file_index_ready: bool,
    file_index_loading: bool,
    filter_dropdown_open: bool,
    filter_dropdown_selected: usize,
    root_flat_items: Vec<RootFlatItem>,
    root_to_flat: Vec<usize>,
    clipboard_flat_items: Vec<ClipboardFlatItem>,
    clipboard_to_flat: Vec<usize>,
    uninstaller: UninstallerState,
    actions_open: bool,
    actions: Vec<CommandAction>,
    actions_title: String,
    actions_filter: String,
    action_selected: usize,
    burger_menu_open: bool,
    /// The app that was frontmost when the panel opened. Closing hands
    /// activation back so pasting lands in it (SPEC §8).
    previous_app: Option<i32>,
    previous_app_name: Option<String>,
    #[cfg(target_os = "windows")]
    visible: bool,
    emoji_category_index: usize,
    emoji_column_count: usize,
    emoji_skin_tone: usize,
    background_color: u32,
    row_height: f32,
    clipboard_filter_index: usize,
    brew_page_mode: BrewPageMode,
    registry: CommandRegistry,
    store: std::sync::Arc<dyn DataStore>,
    #[cfg(target_os = "windows")]
    windows_region_size: Option<(f32, f32, f32)>,
    force_expanded: bool,
    _activation_sub: Subscription,
}

fn is_category_subtitle(result: &SearchResult) -> bool {
    match (result.id.as_str(), result.subtitle.as_deref()) {
        ("clipboard-manager:open" | "emoji-picker:open", Some("Commands")) => true,
        (id, Some("System Action")) if id.starts_with("system-actions:action:") => true,
        (id, Some("System Settings")) if id.starts_with("system-actions:setting:") => true,
        (id, Some("Window Management")) if id.starts_with("window-management:") => true,
        (id, Some("Window Layout")) if id.starts_with("window-management:layout:") => true,
        _ => false,
    }
}

fn cursor_offset_for_query(
    query: &str,
    cursor_idx: usize,
    font_size: Pixels,
    window: &Window,
) -> Pixels {
    if query.is_empty() || cursor_idx == 0 {
        return px(0.0);
    }
    let font_run = TextRun {
        len: query.len(),
        font: font("Helvetica"),
        color: rgb(COLOR_TEXT).into(),
        ..Default::default()
    };
    let shaped = window.text_system().shape_line(
        SharedString::from(query.to_string()),
        font_size,
        &[font_run],
        None,
    );
    let byte_idx = query
        .char_indices()
        .nth(cursor_idx)
        .map(|(i, _)| i)
        .unwrap_or(query.len());
    shaped.x_for_index(byte_idx)
}

fn apply_emoji_skin_tone(emoji: &str, skin_tone: usize) -> String {
    if skin_tone == 0 {
        return emoji.to_string();
    }
    let modifier = char::from_u32(0x1f3fa + skin_tone as u32).unwrap_or(' ');
    let mut output = String::with_capacity(emoji.len() + modifier.len_utf8());
    let mut inserted = false;
    for character in emoji.chars() {
        output.push(character);
        if !inserted && supports_emoji_skin_tone(character as u32) {
            output.push(modifier);
            inserted = true;
        }
    }
    if inserted {
        output
    } else {
        emoji.to_string()
    }
}

fn supports_emoji_skin_tone(codepoint: u32) -> bool {
    matches!(
        codepoint,
        0x261d | 0x26f9 | 0x270a..=0x270d | 0x1f385 | 0x1f3c2..=0x1f3c4 | 0x1f3c7
            | 0x1f3ca..=0x1f3cc | 0x1f442..=0x1f450 | 0x1f466..=0x1f487 | 0x1f4aa
            | 0x1f574..=0x1f575 | 0x1f57a | 0x1f590 | 0x1f595..=0x1f596
            | 0x1f645..=0x1f647 | 0x1f64b..=0x1f64f | 0x1f6a3 | 0x1f6b4..=0x1f6b6
            | 0x1f6c0 | 0x1f6cc | 0x1f90c..=0x1f93e | 0x1f9b5..=0x1f9b6
            | 0x1f9b8..=0x1f9b9 | 0x1f9bb | 0x1f9cd..=0x1f9cf | 0x1f9d1..=0x1f9dd
            | 0x1fac3..=0x1fac5 | 0x1faf0..=0x1faf8
    )
}

fn cursor_index_from_click(
    query: &str,
    local_x: Pixels,
    font_size: Pixels,
    window: &Window,
) -> usize {
    if query.is_empty() {
        return 0;
    }
    let font_run = TextRun {
        len: query.len(),
        font: font("Helvetica"),
        color: rgb(COLOR_TEXT).into(),
        ..Default::default()
    };
    let shaped = window.text_system().shape_line(
        SharedString::from(query.to_string()),
        font_size,
        &[font_run],
        None,
    );
    let byte_idx = shaped.closest_index_for_x(local_x);
    query[..byte_idx.min(query.len())].chars().count()
}

fn render_cursor(cursor_x: Pixels, height: Pixels, visible: bool) -> Div {
    let color = if visible {
        rgb(COLOR_ACCENT)
    } else {
        rgba(0x00000000)
    };
    div()
        .absolute()
        .left(cursor_x)
        .top(px(0.0))
        .bottom(px(0.0))
        .flex()
        .items_center()
        .child(div().w(px(2.0)).h(height).rounded_xs().bg(color))
}

fn load_file_preview(path: &std::path::Path) -> FilePreview {
    use std::io::Read;

    const MAX_PREVIEW_BYTES: u64 = 512 * 1024;
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return FilePreview::Error("Could not read file metadata".into());
    };
    if metadata.file_type().is_symlink() {
        return FilePreview::Unsupported;
    }
    if metadata.is_dir() {
        return FilePreview::Folder;
    }
    if !metadata.is_file() {
        return FilePreview::Unsupported;
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif"
    ) {
        return FilePreview::Image;
    }
    if metadata.len() > MAX_PREVIEW_BYTES {
        return FilePreview::Unsupported;
    }

    let Ok(file) = std::fs::File::open(path) else {
        return FilePreview::Error("Could not read file".into());
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_PREVIEW_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return FilePreview::Error("Could not read file".into());
    }
    if bytes.len() as u64 > MAX_PREVIEW_BYTES || bytes.contains(&0) {
        return FilePreview::Unsupported;
    }
    match String::from_utf8(bytes) {
        Ok(text) => FilePreview::Text(text),
        Err(_) => FilePreview::Unsupported,
    }
}

fn prepare_file_search_index(results: Vec<SearchResult>) -> Vec<FileSearchCandidate> {
    results
        .into_iter()
        .map(|result| FileSearchCandidate {
            name: corvo_core::search::SearchText::new(&result.title),
            path: corvo_core::search::SearchText::new(
                result.subtitle.as_deref().unwrap_or_default(),
            ),
            result,
        })
        .collect()
}

fn filter_file_search_results(
    results: &[FileSearchCandidate],
    query: &str,
    max_results: usize,
) -> Vec<SearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    use corvo_core::search::{quality, FieldRole, SearchText};

    let query = SearchText::new(query);
    let mut matches = results
        .iter()
        .filter_map(|candidate| {
            let score = quality(
                &query,
                &[
                    (FieldRole::Name, &candidate.name),
                    (FieldRole::Subtitle, &candidate.path),
                ],
            )?;
            let mut result = candidate.result.clone();
            result.score = score;
            Some(result)
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| corvo_core::search::natural_cmp(&left.title, &right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    matches.truncate(max_results);
    matches
}

/// Preview kinds rendered as virtualized uniform-height lines. Markdown
/// keeps element-per-line rendering (its rows are not uniform).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ClipPreviewKind {
    Text,
    Link,
    Latex,
    Json,
    Code,
}

/// The wrapped rows of the clipboard entry on screen. uniform_list's row
/// callback is 'static and reaches the rows only through this slot; each
/// render fills it before building the list, and the callback runs
/// within that same frame.
fn clip_preview_rows() -> &'static std::sync::RwLock<(ClipPreviewKind, Vec<String>)> {
    static ROWS: std::sync::OnceLock<std::sync::RwLock<(ClipPreviewKind, Vec<String>)>> =
        std::sync::OnceLock::new();
    ROWS.get_or_init(|| std::sync::RwLock::new((ClipPreviewKind::Text, Vec::new())))
}

fn clip_preview_row(kind: ClipPreviewKind, index: usize, line: &str) -> Div {
    let shown = if line.is_empty() {
        " ".to_string()
    } else {
        line.to_string()
    };
    match kind {
        ClipPreviewKind::Text => div()
            .h(px(18.0))
            .min_w(px(0.0))
            .max_w_full()
            .text_size(px(13.5))
            .text_color(rgb(COLOR_TEXT))
            .child(shown),
        ClipPreviewKind::Link => div()
            .h(px(18.0))
            .min_w(px(0.0))
            .max_w_full()
            .text_size(px(14.0))
            .text_color(rgb(COLOR_ACCENT))
            .child(shown),
        ClipPreviewKind::Latex => div()
            .h(px(17.0))
            .min_w(px(0.0))
            .max_w_full()
            .font_family("Menlo")
            .text_size(px(13.0))
            .text_color(rgb(0xf1f5f9))
            .child(shown),
        ClipPreviewKind::Json | ClipPreviewKind::Code => div()
            .h(px(18.0))
            .min_w(px(0.0))
            .max_w_full()
            .flex()
            .gap_3()
            .items_baseline()
            .child(
                div()
                    .w(px(24.0))
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(rgb(0x52525b))
                    .child(format!("{}", index + 1)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .max_w_full()
                    .font_family("Menlo")
                    .text_size(px(12.5))
                    .text_color(rgb(0xe4e4e7))
                    .child(shown),
            ),
    }
}
/// Builds the virtualized line list for one preview kind: fills the row
/// slot and returns a uniform list that only renders the visible rows,
/// so a multi-thousand-line entry costs the same as a short one.
fn clip_preview_uniform_list(
    list_id: &'static str,
    kind: ClipPreviewKind,
    rows: Vec<String>,
) -> UniformList {
    let count = rows.len();
    if let Ok(mut slot) = clip_preview_rows().write() {
        *slot = (kind, rows);
    }
    uniform_list(list_id, count, move |range, _window, _app| {
        let Ok(slot) = clip_preview_rows().read() else {
            return Vec::new();
        };
        if slot.1.len() != count {
            return Vec::new();
        }
        range
            .map(|index| clip_preview_row(kind, index, &slot.1[index]))
            .collect()
    })
}

/// Preview caps: a multi-megabyte paste would build a wrapped string and
/// a row per line on every render, which stutters selection and
/// scrolling. A clipboard preview never needs more than this much text;
/// the entry itself is still copied in full.
const CLIP_PREVIEW_MAX_CHARS: usize = 120_000;
const CLIP_PREVIEW_MAX_LINES: usize = 2_000;

/// Clips `text` to the preview caps, appending a marker line when
/// content was left out.
fn cap_preview_text(text: &str) -> String {
    let total_lines = text.lines().count();
    if total_lines <= CLIP_PREVIEW_MAX_LINES && text.chars().count() <= CLIP_PREVIEW_MAX_CHARS {
        return text.to_owned();
    }

    let mut capped = String::new();
    let mut lines_taken = 0usize;
    let mut char_count = 0usize;
    for line in text.lines() {
        if lines_taken >= CLIP_PREVIEW_MAX_LINES {
            break;
        }
        let line_len = line.chars().count();
        if char_count + line_len > CLIP_PREVIEW_MAX_CHARS {
            // A line that does not fit whole (or a single huge line):
            // show what fits of it, then stop. The line is not counted
            // as taken, so the marker below reports it as hidden.
            let remaining = CLIP_PREVIEW_MAX_CHARS.saturating_sub(char_count);
            if remaining > 1 {
                let slice: String = line.chars().take(remaining - 1).collect();
                if lines_taken > 0 {
                    capped.push('\n');
                }
                capped.push_str(&slice);
            }
            break;
        }
        if lines_taken > 0 {
            capped.push('\n');
        }
        capped.push_str(line);
        lines_taken += 1;
        char_count += line_len + 1;
    }
    let hidden = total_lines.saturating_sub(lines_taken);
    capped.push_str(&format!("\n… {hidden} more lines not shown"));
    capped
}

fn format_wrapped_preview_lines(text: &str, max_line_len: usize) -> Vec<String> {
    let mut result = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            result.push(String::new());
            continue;
        }
        let indent_len = line.chars().take_while(|c| c.is_whitespace()).count();
        let indent: String = line.chars().take(indent_len).collect();
        let content = &line[indent.len()..];

        let mut current = indent.clone();
        let mut current_len = indent_len;

        for word in content.split_whitespace() {
            let word_len = word.chars().count();
            if word_len > max_line_len {
                if current_len > indent_len {
                    result.push(current);
                    current = indent.clone();
                    current_len = indent_len;
                }
                let mut chunk = String::new();
                let mut chunk_len = 0;
                let chunk_limit = max_line_len.saturating_sub(indent_len).max(20);
                for ch in word.chars() {
                    chunk.push(ch);
                    chunk_len += 1;
                    if chunk_len >= chunk_limit {
                        result.push(format!("{indent}{chunk}"));
                        chunk.clear();
                        chunk_len = 0;
                    }
                }
                if !chunk.is_empty() {
                    current = format!("{indent}{chunk}");
                    current_len = indent_len + chunk_len;
                }
            } else if current_len > indent_len && current_len + 1 + word_len > max_line_len {
                result.push(current);
                current = format!("{indent}{word}");
                current_len = indent_len + word_len;
            } else {
                if current_len > indent_len {
                    current.push(' ');
                    current_len += 1;
                }
                current.push_str(word);
                current_len += word_len;
            }
        }
        if current_len > indent_len {
            result.push(current);
        }
    }
    result
}

fn format_file_size(size_bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = size_bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", size_bytes)
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

impl Launcher {
    fn new(window: &mut Window, cx: &mut Context<Self>, page: LauncherPage, query: String) -> Self {
        let focus_handle = cx.focus_handle();
        let store = cx.global::<StoreGlobal>().0.clone();
        let emoji_column_count = store.emoji_column_count();
        let emoji_skin_tone = store.emoji_skin_tone();
        let background_color = launcher_background(store.transparency_level());
        let row_height = if store.compact_mode() {
            32.0
        } else {
            ROW_HEIGHT
        };
        window.focus(&focus_handle, cx);
        let _activation_sub = cx.observe_window_activation(window, |launcher, window, _cx| {
            if !window.is_window_active() {
                launcher.dismiss(window);
            }
        });
        let cursor_idx = query.chars().count();
        let mut launcher = Self {
            focus_handle,
            page,
            query,
            cursor_idx,
            port_argument: String::new(),
            port_cursor_idx: 0,
            port_input_active: false,
            extension_view: None,
            extension_scroll_handle: ScrollHandle::new(),
            extension_pump_running: false,
            extension_button_focus: 0,
            extension_form: ExtensionFormState::default(),
            cursor_visible: true,
            results: cached_initial_results(),
            selected: 0,
            search_seq: 0,
            results_scroll_handle: ScrollHandle::new(),
            emoji_scroll_handle: ScrollHandle::new(),
            clipboard_scroll_handle: UniformListScrollHandle::new(),
            file_scroll_handle: ScrollHandle::new(),
            file_preview: FilePreviewState::Empty,
            file_preview_seq: 0,
            file_default_results: Vec::new(),
            file_index: std::sync::Arc::new(Vec::new()),
            file_index_ready: false,
            file_index_loading: false,
            filter_dropdown_open: false,
            filter_dropdown_selected: 0,
            root_flat_items: Vec::new(),
            root_to_flat: Vec::new(),
            clipboard_flat_items: Vec::new(),
            clipboard_to_flat: Vec::new(),
            uninstaller: UninstallerState::Closed,
            actions_open: false,
            actions: Vec::new(),
            actions_title: String::new(),
            actions_filter: String::new(),
            action_selected: 0,
            burger_menu_open: false,
            previous_app: None,
            previous_app_name: None,
            #[cfg(target_os = "windows")]
            visible: true,
            emoji_category_index: 0,
            emoji_column_count,
            emoji_skin_tone,
            background_color,
            row_height,
            clipboard_filter_index: 0,
            brew_page_mode: BrewPageMode::Installed,
            registry: cx.global::<RegistryGlobal>().0.clone(),
            store,
            #[cfg(target_os = "windows")]
            windows_region_size: None,
            force_expanded: false,
            _activation_sub,
        };
        launcher.rebuild_root_flat_items();
        launcher.start_cursor_blink(cx);
        match page {
            LauncherPage::Root => launcher.refresh(cx),
            LauncherPage::Emoji => launcher.refresh_emoji(cx),
            LauncherPage::Clipboard => launcher.refresh_clipboard(cx),
            LauncherPage::Files => launcher.refresh_files(cx),
            LauncherPage::Brew => launcher.refresh_brew(cx),
            LauncherPage::Text => launcher.refresh_text(cx),
            LauncherPage::Notes => launcher.refresh_notes(cx),
            LauncherPage::Browser(browser) => launcher.refresh_browser(browser, cx),
            LauncherPage::Extension(command_id) => {
                launcher.refresh_extension(command_id, false, cx)
            }
            LauncherPage::Ports => launcher.refresh_ports(cx),
            LauncherPage::Processes => launcher.refresh_processes(cx),
            LauncherPage::Uninstaller => {}
        }
        launcher
    }

    fn palette_size(&self) -> PaletteSize {
        palette_size(
            self.store.compact_mode(),
            self.force_expanded,
            self.page,
            &self.query,
            self.actions_open,
            self.burger_menu_open,
            self.filter_dropdown_open,
        )
    }

    fn is_compact_collapsed(&self) -> bool {
        self.palette_size() == PaletteSize::Compact
    }

    fn size_scale(&self) -> f32 {
        match self.store.interface_size_option() {
            0 => 0.9,
            2 => 1.1,
            _ => 1.0,
        }
    }

    fn desired_window_height(&self) -> f32 {
        self.palette_size().height(self.size_scale())
    }

    fn desired_window_width(&self) -> f32 {
        WINDOW_WIDTH * self.size_scale()
    }

    fn sync_palette_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target_w = self.desired_window_width();
        let target_h = self.desired_window_height();
        let viewport = window.viewport_size();
        let actual_w: f32 = viewport.width.into();
        let actual_h: f32 = viewport.height.into();
        #[cfg(target_os = "windows")]
        {
            let observed = (actual_w, actual_h, window.scale_factor());
            if self.windows_region_size != Some(observed) {
                self.windows_region_size = Some(observed);
                apply_windows_launcher_region(window, cx);
            }
        }
        if (actual_w - target_w).abs() > 0.5 || (actual_h - target_h).abs() > 0.5 {
            #[cfg(target_os = "macos")]
            {
                cx.spawn(async move |_, _| {
                    corvo_platform::resize_launcher_panel(target_w as f64, target_h as f64);
                })
                .detach();
            }
            #[cfg(not(target_os = "macos"))]
            {
                window.resize(size(px(target_w), px(target_h)));
            }
        }
    }

    fn ensure_window_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_palette_size(window, cx);
    }

    fn start_cursor_blink(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            smol::Timer::after(std::time::Duration::from_millis(530)).await;
            let res = this.update(cx, |launcher, cx| {
                launcher.cursor_visible = !launcher.cursor_visible;
                cx.notify();
            });
            if res.is_err() {
                break;
            }
        })
        .detach();
    }

    fn insert_str(&mut self, text: &str) {
        let byte_idx = self
            .query
            .char_indices()
            .nth(self.cursor_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len());
        self.query.insert_str(byte_idx, text);
        self.cursor_idx += text.chars().count();
    }

    fn backspace_char(&mut self) -> bool {
        if self.cursor_idx > 0 {
            let prev_idx = self.cursor_idx - 1;
            if let Some((byte_start, ch)) = self.query.char_indices().nth(prev_idx) {
                let byte_end = byte_start + ch.len_utf8();
                self.query.drain(byte_start..byte_end);
                self.cursor_idx = prev_idx;
                return true;
            }
        }
        false
    }

    fn delete_char(&mut self) -> bool {
        if self.cursor_idx < self.query.chars().count() {
            if let Some((byte_start, ch)) = self.query.char_indices().nth(self.cursor_idx) {
                let byte_end = byte_start + ch.len_utf8();
                self.query.drain(byte_start..byte_end);
                return true;
            }
        }
        false
    }

    fn cursor_word_left(&mut self) {
        if self.cursor_idx == 0 {
            return;
        }
        let chars: Vec<char> = self.query.chars().collect();
        let mut idx = self.cursor_idx;

        while idx > 0 && chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        if idx > 0 {
            let is_alnum = chars[idx - 1].is_alphanumeric();
            while idx > 0
                && !chars[idx - 1].is_whitespace()
                && (chars[idx - 1].is_alphanumeric() == is_alnum)
            {
                idx -= 1;
            }
        }
        self.cursor_idx = idx;
    }

    fn cursor_word_right(&mut self) {
        let chars: Vec<char> = self.query.chars().collect();
        let len = chars.len();
        if self.cursor_idx >= len {
            return;
        }
        let mut idx = self.cursor_idx;

        let is_alnum = chars[idx].is_alphanumeric();
        while idx < len && !chars[idx].is_whitespace() && (chars[idx].is_alphanumeric() == is_alnum)
        {
            idx += 1;
        }
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        self.cursor_idx = idx;
    }

    fn delete_word_backward(&mut self) -> bool {
        if self.cursor_idx == 0 {
            return false;
        }
        let old_cursor = self.cursor_idx;
        self.cursor_word_left();
        let target_idx = self.cursor_idx;
        self.cursor_idx = old_cursor;

        if target_idx < self.cursor_idx {
            let byte_start = self
                .query
                .char_indices()
                .nth(target_idx)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let byte_end = self
                .query
                .char_indices()
                .nth(self.cursor_idx)
                .map(|(i, _)| i)
                .unwrap_or(self.query.len());
            self.query.drain(byte_start..byte_end);
            self.cursor_idx = target_idx;
            true
        } else {
            false
        }
    }

    fn delete_to_beginning_of_line(&mut self) -> bool {
        if self.cursor_idx == 0 {
            return false;
        }
        let byte_end = self
            .query
            .char_indices()
            .nth(self.cursor_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len());
        self.query.drain(0..byte_end);
        self.cursor_idx = 0;
        true
    }

    fn delete_to_end_of_line(&mut self) -> bool {
        let total_chars = self.query.chars().count();
        if self.cursor_idx >= total_chars {
            return false;
        }
        let byte_start = self
            .query
            .char_indices()
            .nth(self.cursor_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len());
        self.query.truncate(byte_start);
        true
    }

    fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(text) = corvo_platform::read_clipboard_text() {
            let clean: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
            if clean.is_empty() {
                return false;
            }
            if self.actions_open {
                self.actions_filter.push_str(&clean);
                self.action_selected = 0;
                cx.notify();
                return true;
            } else {
                self.insert_str(&clean);
                self.refresh_current_page(cx);
                return true;
            }
        }
        false
    }

    fn refresh_current_page(&mut self, cx: &mut Context<Self>) {
        self.selected = 0;
        self.results_scroll_handle.scroll_to_item(0);
        match self.page {
            LauncherPage::Root => self.refresh(cx),
            LauncherPage::Emoji => self.refresh_emoji(cx),
            LauncherPage::Clipboard => self.refresh_clipboard(cx),
            LauncherPage::Files => self.refresh_files(cx),
            LauncherPage::Brew => self.refresh_brew(cx),
            LauncherPage::Text => self.refresh_text(cx),
            LauncherPage::Notes => self.refresh_notes(cx),
            LauncherPage::Browser(browser) => self.refresh_browser(browser, cx),
            LauncherPage::Extension(command_id) => {
                self.refresh_extension(command_id, false, cx)
            }
            LauncherPage::Ports => self.refresh_ports(cx),
            LauncherPage::Processes => self.refresh_processes(cx),
            LauncherPage::Uninstaller => {
                let visible = self.uninstaller_visible_indices();
                if let UninstallerState::Ready(ready) = &mut self.uninstaller {
                    ready.focused = visible.first().copied().unwrap_or(0);
                    ready.confirming = false;
                    ready.message = None;
                }
                cx.notify();
            }
        }
    }

    fn handle_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.port_input_active {
            self.port_input_active = false;
            self.cursor_idx = self.query.chars().count();
            self.cursor_visible = true;
            cx.notify();
        } else if self.filter_dropdown_open {
            self.close_filter_dropdown(cx);
        } else if self.burger_menu_open {
            self.close_burger_menu(cx);
        } else if self.actions_open {
            self.close_actions(cx);
        } else if self.store.escape_closes_window() {
            self.dismiss(window);
        } else if self.page == LauncherPage::Uninstaller {
            let confirming =
                matches!(&self.uninstaller, UninstallerState::Ready(ready) if ready.confirming);
            if confirming {
                if let UninstallerState::Ready(ready) = &mut self.uninstaller {
                    ready.confirming = false;
                    ready.message = None;
                }
                cx.notify();
            } else if !self.query.is_empty() {
                self.query.clear();
                self.cursor_idx = 0;
                cx.notify();
            } else {
                self.open_root_page(window, cx);
            }
        } else if self.page == LauncherPage::Emoji
            || self.page == LauncherPage::Clipboard
            || self.page == LauncherPage::Files
            || self.page == LauncherPage::Brew
            || self.page == LauncherPage::Text
            || self.page == LauncherPage::Notes
            || matches!(self.page, LauncherPage::Browser(_))
            || matches!(self.page, LauncherPage::Extension(_))
            || self.page == LauncherPage::Ports
            || self.page == LauncherPage::Processes
        {
            if !self.query.is_empty() {
                self.query.clear();
                self.cursor_idx = 0;
                self.refresh_current_page(cx);
            } else {
                self.open_root_page(window, cx);
            }
        } else {
            if !self.query.is_empty() {
                self.query.clear();
                self.cursor_idx = 0;
                self.force_expanded = false;
                self.ensure_window_size(window, cx);
                self.refresh(cx);
            } else if self.force_expanded {
                self.force_expanded = false;
                self.ensure_window_size(window, cx);
                self.refresh(cx);
            } else {
                self.dismiss(window);
            }
        }
    }

    fn execute_secondary_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == LauncherPage::Clipboard {
            self.copy_selected_clipboard(window, cx);
            return;
        }

        if self.page == LauncherPage::Emoji {
            self.copy_selected_emoji(window, cx);
            return;
        }

        let Some(result) = self.selected_result().cloned() else {
            return;
        };
        let Some(command_id) = result.id.split(':').next() else {
            return;
        };
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_id)
            .cloned()
        else {
            return;
        };

        let actions = command.actions(&result.id);
        if let Some(secondary) = actions.get(1) {
            let action = Ok(secondary.action.clone());
            self.perform(action, window, cx);
        }
    }

    fn execute_copy_action(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.page == LauncherPage::Clipboard {
            self.execute_secondary_selected(window, cx);
            return true;
        }
        if self.page == LauncherPage::Emoji {
            self.copy_selected_emoji(window, cx);
            return true;
        }
        let Some(result) = self.selected_result().cloned() else {
            return false;
        };
        let Some(command_id) = result.id.split(':').next() else {
            return false;
        };
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_id)
            .cloned()
        else {
            return false;
        };
        let actions = command.actions(&result.id);
        if let Some(copy_act) = actions
            .iter()
            .find(|a| a.label.to_lowercase().contains("copy"))
        {
            let action = Ok(copy_act.action.clone());
            self.perform(action, window, cx);
            return true;
        }
        false
    }

    fn handle_filter_dropdown_key(&mut self, key: &str, ctrl: bool, cx: &mut Context<Self>) {
        let max_idx = match self.page {
            LauncherPage::Emoji => EMOJI_CATEGORIES.len().saturating_sub(1),
            LauncherPage::Clipboard => CLIPBOARD_FILTERS.len().saturating_sub(1),
            _ => 0,
        };
        match key {
            "escape" | "tab" => {
                self.close_filter_dropdown(cx);
            }
            "enter" => {
                self.apply_filter_dropdown_selection(cx);
            }
            "up" => {
                self.filter_dropdown_selected = self.filter_dropdown_selected.saturating_sub(1);
                cx.notify();
            }
            "p" if ctrl => {
                self.filter_dropdown_selected = self.filter_dropdown_selected.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                self.filter_dropdown_selected = (self.filter_dropdown_selected + 1).min(max_idx);
                cx.notify();
            }
            "n" if ctrl => {
                self.filter_dropdown_selected = (self.filter_dropdown_selected + 1).min(max_idx);
                cx.notify();
            }
            _ => {}
        }
    }

    fn handle_actions_palette_key(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = keystroke.key.as_str();
        let mods = Shortcuts::from_modifiers(&keystroke.modifiers);
        let alt = mods.alt;

        // Primary+1..9: Direct action execution
        if mods.is_command_alone() {
            if let Ok(digit) = key.parse::<usize>() {
                if (1..=9).contains(&digit) {
                    let target_idx = digit - 1;
                    let filtered = self.filtered_actions();
                    if target_idx < filtered.len() {
                        self.action_selected = target_idx;
                        self.run_selected_action(window, cx);
                        return;
                    }
                }
            }
        }

        // Primary+Enter: Execute secondary action
        if mods.is_command_alone() && key == "enter" {
            if self.page == LauncherPage::Uninstaller {
                self.run_selected_action(window, cx);
                return;
            }
            if self.filtered_actions().len() > 1 {
                self.action_selected = 1;
                self.run_selected_action(window, cx);
            }
            return;
        }

        // Primary+V: Paste into actions filter
        if mods.is_command_alone() && key == "v" {
            self.paste_from_clipboard(cx);
            return;
        }

        // Enter: Execute selected action
        if mods.is_unmodified() && key == "enter" {
            self.run_selected_action(window, cx);
            return;
        }

        // Up / ⌃P: Move selection up
        if (mods.is_unmodified() && key == "up") || (mods.is_ctrl_alone() && key == "p") {
            let count = self.filtered_actions().len();
            if count > 0 {
                self.action_selected = if self.action_selected == 0 {
                    count - 1
                } else {
                    self.action_selected - 1
                };
                cx.notify();
            }
            return;
        }

        // Down / ⌃N: Move selection down
        if (mods.is_unmodified() && key == "down") || (mods.is_ctrl_alone() && key == "n") {
            let count = self.filtered_actions().len();
            if count > 0 {
                self.action_selected = if self.action_selected + 1 >= count {
                    0
                } else {
                    self.action_selected + 1
                };
                cx.notify();
            }
            return;
        }

        // Alt+Backspace / Ctrl+W: Delete word backward in actions filter
        if (!mods.command && !mods.ctrl && alt && key == "backspace")
            || (mods.is_ctrl_alone() && key == "w")
        {
            while self.actions_filter.ends_with(char::is_whitespace) {
                self.actions_filter.pop();
            }
            while !self.actions_filter.is_empty()
                && !self.actions_filter.ends_with(char::is_whitespace)
            {
                self.actions_filter.pop();
            }
            self.action_selected = 0;
            cx.notify();
            return;
        }

        // Primary+Backspace / Ctrl+U: Clear actions filter
        if (mods.is_command_alone() && key == "backspace") || (mods.is_ctrl_alone() && key == "u") {
            if !self.actions_filter.is_empty() {
                self.actions_filter.clear();
                self.action_selected = 0;
                cx.notify();
            }
            return;
        }

        // Backspace: Delete character
        if mods.is_unmodified() && key == "backspace" {
            if self.actions_filter.pop().is_some() {
                self.action_selected = 0;
                cx.notify();
            }
            return;
        }

        if mods.is_unmodified()
            && matches!(
                key,
                "tab" | "left" | "right" | "home" | "end" | "pageup" | "pagedown"
            )
        {
            return;
        }

        // Any modifier means the keystroke is a chord, not text.
        if mods.command || mods.ctrl || mods.alt {
            return;
        }

        let typed = keystroke
            .key_char
            .clone()
            .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
        if let Some(text) = typed {
            self.actions_filter.push_str(&text);
            self.action_selected = 0;
            cx.notify();
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        let keystroke = &event.keystroke;
        let key = keystroke.key.as_str();
        let mods = Shortcuts::from_modifiers(&keystroke.modifiers);
        let alt = mods.alt;

        if self.page == LauncherPage::Root && self.selected_result_is_port_action() {
            if mods.is_unmodified() && key == "tab" {
                self.port_input_active = !self.port_input_active;
                self.port_cursor_idx = self.port_argument.chars().count();
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if self.port_input_active && mods.is_unmodified() {
                match key {
                    "escape" => {
                        self.handle_escape(window, cx);
                        cx.stop_propagation();
                        return;
                    }
                    "up" | "down" => {
                        self.port_input_active = false;
                        cx.notify();
                    }
                    _ => {
                        match key {
                            "enter" => {
                                self.kill_port_argument(window, cx);
                                cx.stop_propagation();
                                return;
                            }
                            "right" => {
                                self.port_cursor_idx = (self.port_cursor_idx + 1)
                                    .min(self.port_argument.chars().count());
                                cx.notify();
                                cx.stop_propagation();
                                return;
                            }
                            "left" => {
                                if self.port_cursor_idx == 0 {
                                    self.port_input_active = false;
                                    self.cursor_idx = self.query.chars().count();
                                    self.cursor_visible = true;
                                } else {
                                    self.port_cursor_idx -= 1;
                                }
                                cx.notify();
                                cx.stop_propagation();
                                return;
                            }
                            "backspace" => {
                                if self.port_cursor_idx > 0 {
                                    self.port_argument.remove(self.port_cursor_idx - 1);
                                    self.port_cursor_idx -= 1;
                                }
                                cx.notify();
                                cx.stop_propagation();
                                return;
                            }
                            "delete" => {
                                if self.port_cursor_idx < self.port_argument.len() {
                                    self.port_argument.remove(self.port_cursor_idx);
                                }
                                cx.notify();
                                cx.stop_propagation();
                                return;
                            }
                            _ => {}
                        }
                        let typed = keystroke.key_char.clone();
                        if let Some(text) =
                            typed.filter(|text| text.chars().all(|ch| ch.is_ascii_digit()))
                        {
                            for ch in text.chars() {
                                self.port_argument.insert(self.port_cursor_idx, ch);
                                self.port_cursor_idx += 1;
                            }
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                    }
                }
                if self.port_input_active {
                    cx.stop_propagation();
                    return;
                }
            }
            if mods.is_unmodified() && key == "right" {
                self.port_input_active = true;
                self.port_cursor_idx = self.port_argument.chars().count();
                cx.notify();
                cx.stop_propagation();
                return;
            }
        }

        // Declarative extension pages: ←/→ and Enter are page
        // semantics — the command decides what they mean and renders
        // its own selection; ↑/↓ move the form-field focus; typing
        // edits the focused form field via the default text path.
        if let LauncherPage::Extension(command_id) = self.page {
            if mods.is_unmodified() {
                let view = self.extension_view.clone();
                let is_blocks = matches!(view, Some(corvo_core::PageView::Blocks(_)));
                // ←/→ mean what the page says (pomodoro changes the
                // selected duration, media moves between tracks). An
                // action the page does not implement refreshes it and
                // shows nothing — a deliberate no-op.
                let direction = match key {
                    "left" => Some("left"),
                    "right" => Some("right"),
                    _ => None,
                };
                if let Some(direction) = direction {
                    if is_blocks && self.query.is_empty() {
                        self.run_extension_action(command_id, direction.to_owned(), cx);
                        cx.stop_propagation();
                        return;
                    }
                }
                if key == "up" || key == "down" {
                    match self.extension_view.as_ref() {
                        Some(corvo_core::PageView::Form { fields, .. }) => {
                            let count = fields.len().max(1);
                            self.extension_form.focused = if key == "down" {
                                (self.extension_form.focused + 1) % count
                            } else {
                                (self.extension_form.focused + count - 1) % count
                            };
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        Some(corvo_core::PageView::Grid { .. }) => {
                            // Grid selection follows the click target
                            // for now; keep up/down inert so the page
                            // does not scroll the window.
                            cx.stop_propagation();
                            return;
                        }
                        _ => {}
                    }
                }
                if key == "enter" {
                    match self.extension_view.as_ref() {
                        Some(corvo_core::PageView::Form { fields, .. }) => {
                            let encoded = self.encode_form_values(fields);
                            self.run_extension_action(command_id, format!("form:{encoded}"), cx);
                            cx.stop_propagation();
                            return;
                        }
                        Some(corvo_core::PageView::Blocks(_)) if self.query.is_empty() => {
                            // Enter is the page's primary action —
                            // start the timer, toggle playback, save
                            // the countdown — decided by the command.
                            self.run_extension_action(command_id, "enter".to_owned(), cx);
                            cx.stop_propagation();
                            return;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Stage 0: Global App & Window Lifecycle
        if mods.is_command_alone() {
            match key {
                "q" => {
                    self.dismiss(window);
                    std::process::exit(0);
                }
                "w" => {
                    self.dismiss(window);
                    cx.stop_propagation();
                    return;
                }
                "," => {
                    self.close_burger_menu(cx);
                    self.close_actions(cx);
                    self.close_filter_dropdown(cx);
                    #[cfg(target_os = "windows")]
                    self.dismiss(window);
                    open_settings(cx);
                    cx.stop_propagation();
                    return;
                }
                "k" => {
                    // The visual pages have no result rows, so there is
                    // no actions menu to open on them; the burger is the
                    // only surface there.
                    let is_visual = matches!(self.page, LauncherPage::Extension(_));
                    if !is_visual {
                        self.toggle_actions(cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        // Command+Alt+Space: Switch to Emoji picker. macOS keeps the
        // Control variant, where Command and Control are distinct keys.
        // Windows and Linux use Alt, because there Control is already the
        // primary modifier and cannot be held twice.
        let emoji_chord = match corvo_core::Primary::current() {
            corvo_core::Primary::Command => mods.command && mods.ctrl && !mods.alt,
            corvo_core::Primary::Control => mods.command && mods.alt && !mods.shift,
        };
        if emoji_chord && key == "space" {
            self.open_emoji_page(window, cx);
            cx.stop_propagation();
            return;
        }

        // Command+Alt+C: Switch to Clipboard manager. Cross-platform as
        // written, because the primary modifier plus Alt never collides.
        if mods.command && mods.alt && !mods.ctrl && !mods.shift && key == "c" {
            self.open_clipboard_page(window, cx);
            cx.stop_propagation();
            return;
        }

        // Stage 1: Escape Modal Layer Unwinding
        if mods.is_unmodified() && key == "escape" {
            self.handle_escape(window, cx);
            cx.stop_propagation();
            return;
        }

        // Stage 2: Modal Overlays (Dropdown, Burger, Actions)
        if self.filter_dropdown_open {
            self.handle_filter_dropdown_key(key, mods.ctrl, cx);
            cx.stop_propagation();
            return;
        }

        if self.burger_menu_open {
            self.close_burger_menu(cx);
            cx.stop_propagation();
            return;
        }

        if self.actions_open {
            self.handle_actions_palette_key(keystroke, window, cx);
            cx.stop_propagation();
            return;
        }

        if self.page == LauncherPage::Uninstaller {
            if mods.is_command_alone() && key == "enter" {
                self.begin_uninstall(cx);
                cx.stop_propagation();
                return;
            }
            if mods.is_unmodified() && key == "space" && self.query.is_empty() {
                self.toggle_uninstaller_focus(cx);
                cx.stop_propagation();
                return;
            }
        }

        // Stage 3: Contextual List Actions
        if self.page == LauncherPage::Clipboard {
            // Primary+Backspace / Primary+Delete: Delete selected clipboard entry
            if mods.is_command_alone() && (key == "backspace" || key == "delete") {
                if let Some(res) = self.selected_result() {
                    if let Some(id) = res.id.strip_prefix("clipboard-manager:entry:") {
                        let result = corvo_clipboard_manager::delete_entry(id);
                        self.refresh_clipboard(cx);
                        let notice = match result {
                            Ok(()) => ToastNotice::success(
                                ToastCategory::Clipboard,
                                "Item removed",
                                "Removed from clipboard history",
                            ),
                            Err(error) => ToastNotice::failure(
                                ToastCategory::Clipboard,
                                "Could not remove item",
                                error,
                            ),
                        };
                        show_action_toast(notice, cx);
                        cx.stop_propagation();
                        return;
                    }
                }
            }
            // Primary+C or Primary+Enter: Copy entry without pasting
            if mods.is_command_alone() && (key == "c" || key == "enter") {
                self.execute_secondary_selected(window, cx);
                cx.stop_propagation();
                return;
            }
        }

        // Primary+Enter: Secondary action on the selected row
        if mods.is_command_alone() && key == "enter" {
            self.execute_secondary_selected(window, cx);
            cx.stop_propagation();
            return;
        }

        // Primary+C: Copy the selected item when the row offers a copy action
        // action, otherwise copy the query text itself. Without the
        // second branch, Ctrl+C did nothing at all on Windows and Linux,
        // where the typing guard swallowed the chord.
        if mods.is_command_alone() && key == "c" {
            if self.page != LauncherPage::Uninstaller && self.execute_copy_action(window, cx) {
                cx.stop_propagation();
                return;
            }
            if !self.query.is_empty() {
                self.perform(Ok(Action::Copy(self.query.clone())), window, cx);
                cx.stop_propagation();
                return;
            }
        }

        // Stage 4: List Navigation & Activation
        // Up / ⌃P: Move selection up
        if (mods.is_unmodified() && key == "up") || (mods.is_ctrl_alone() && key == "p") {
            if self.page == LauncherPage::Emoji {
                self.select_emoji_delta(-(self.emoji_column_count as isize), cx);
            } else {
                let count = self.selectable_count();
                if count > 0 {
                    let cur = self.selected_position();
                    let target = if cur == 0 { count - 1 } else { cur - 1 };
                    self.select(target, cx);
                }
            }
            cx.stop_propagation();
            return;
        }

        // Down / ⌃N: Move selection down
        if (mods.is_unmodified() && key == "down") || (mods.is_ctrl_alone() && key == "n") {
            if self.is_compact_collapsed() {
                self.force_expanded = true;
                self.sync_palette_size(window, cx);
                self.selected = 0;
                self.results_scroll_handle.scroll_to_item(0);
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if self.page == LauncherPage::Emoji {
                self.select_emoji_delta(self.emoji_column_count as isize, cx);
            } else {
                let count = self.selectable_count();
                if count > 0 {
                    let cur = self.selected_position();
                    let target = if cur + 1 >= count { 0 } else { cur + 1 };
                    self.select(target, cx);
                }
            }
            cx.stop_propagation();
            return;
        }

        // PageUp / PageDown
        if mods.is_unmodified() && key == "pageup" {
            let delta = if self.page == LauncherPage::Emoji {
                self.emoji_column_count * 3
            } else {
                8
            };
            self.select(self.selected_position().saturating_sub(delta), cx);
            cx.stop_propagation();
            return;
        }
        if mods.is_unmodified() && key == "pagedown" {
            let delta = if self.page == LauncherPage::Emoji {
                self.emoji_column_count * 3
            } else {
                8
            };
            let last = self.selectable_count().saturating_sub(1);
            self.select((self.selected_position() + delta).min(last), cx);
            cx.stop_propagation();
            return;
        }

        // Enter: Execute primary action
        if mods.is_unmodified() && key == "enter" {
            self.execute_selected(window, cx);
            cx.stop_propagation();
            return;
        }

        // Tab: Toggle filter dropdown on Emoji/Clipboard
        if mods.is_unmodified()
            && key == "tab"
            && (self.page == LauncherPage::Emoji || self.page == LauncherPage::Clipboard)
        {
            self.toggle_filter_dropdown(cx);
            cx.stop_propagation();
            return;
        }

        // Emoji grid lateral movement
        if self.page == LauncherPage::Emoji && mods.is_unmodified() {
            if key == "left" {
                self.select_emoji_delta(-1, cx);
                cx.stop_propagation();
                return;
            }
            if key == "right" {
                self.select_emoji_delta(1, cx);
                cx.stop_propagation();
                return;
            }
        }

        // Declarative Form pages: a focused text field consumes every
        // text-editing key — characters, backspace, delete, caret
        // moves, and paste — so the page query stays untouched.
        if matches!(self.page, LauncherPage::Extension(_)) {
            if mods.is_command_alone() && key == "v" {
                if let Some(text) = corvo_platform::read_clipboard_text() {
                    let clean: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
                    if !clean.is_empty() && self.extension_form_edit("text", Some(&clean)) {
                        cx.notify();
                        cx.stop_propagation();
                        return;
                    }
                }
            }
            if mods.is_unmodified() {
                let typed = keystroke
                    .key_char
                    .clone()
                    .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                let is_edit_key = matches!(key, "backspace" | "delete" | "left" | "right")
                    || typed.is_some();
                if is_edit_key && self.extension_form_edit(key, typed.as_deref()) {
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
            }
        }

        // Stage 5: macOS Text Editing Primitives
        // Primary+V: Paste
        if mods.is_command_alone() && key == "v" {
            self.paste_from_clipboard(cx);
            self.ensure_window_size(window, cx);
            cx.stop_propagation();
            return;
        }

        // Alt+Left: Word backward
        if !mods.command && !mods.ctrl && alt && key == "left" {
            self.cursor_word_left();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Alt+Right: Word forward
        if !mods.command && !mods.ctrl && alt && key == "right" {
            self.cursor_word_right();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Primary+Left / Ctrl+A: Line start
        if (mods.is_command_alone() && key == "left") || (mods.is_ctrl_alone() && key == "a") {
            self.cursor_idx = 0;
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Primary+Right / Ctrl+E: Line end
        if (mods.is_command_alone() && key == "right") || (mods.is_ctrl_alone() && key == "e") {
            self.cursor_idx = self.query.chars().count();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Left / Right: Single character movement
        if mods.is_unmodified() && key == "left" && self.page != LauncherPage::Emoji {
            self.cursor_idx = self.cursor_idx.saturating_sub(1);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if mods.is_unmodified() && key == "right" && self.page != LauncherPage::Emoji {
            self.cursor_idx = (self.cursor_idx + 1).min(self.query.chars().count());
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Home / End
        if mods.is_unmodified() && key == "home" {
            if self.query.is_empty() {
                self.select(0, cx);
            } else {
                self.cursor_idx = 0;
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if mods.is_unmodified() && key == "end" {
            if self.query.is_empty() {
                let last = self.selectable_count().saturating_sub(1);
                self.select(last, cx);
            } else {
                self.cursor_idx = self.query.chars().count();
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        // Alt+Backspace / Ctrl+W: Delete word backward
        if (!mods.command && !mods.ctrl && alt && key == "backspace")
            || (mods.is_ctrl_alone() && key == "w")
        {
            if self.delete_word_backward() {
                if self.query.trim().is_empty() {
                    self.force_expanded = false;
                }
                self.ensure_window_size(window, cx);
                self.refresh_current_page(cx);
            }
            cx.stop_propagation();
            return;
        }

        // Primary+Backspace / Ctrl+U: Delete to beginning of line
        if (mods.is_command_alone() && key == "backspace") || (mods.is_ctrl_alone() && key == "u") {
            if self.delete_to_beginning_of_line() {
                if self.query.trim().is_empty() {
                    self.force_expanded = false;
                }
                self.ensure_window_size(window, cx);
                self.refresh_current_page(cx);
            }
            cx.stop_propagation();
            return;
        }

        // ⌃K: Delete to end of line (kill line).
        //
        // On macOS Command+K opens the actions menu, so Control+K is free
        // here. On Windows and Linux the primary modifier is Control, so
        // Ctrl+K is the actions menu and kill-line moves to Ctrl+Alt+K.
        let kill_line_chord = match corvo_core::Primary::current() {
            corvo_core::Primary::Command => mods.is_ctrl_alone(),
            corvo_core::Primary::Control => mods.is_ctrl_alone() && mods.alt,
        };
        if kill_line_chord && key == "k" {
            if self.delete_to_end_of_line() {
                if self.query.trim().is_empty() {
                    self.force_expanded = false;
                }
                self.ensure_window_size(window, cx);
                self.refresh_current_page(cx);
            }
            cx.stop_propagation();
            return;
        }

        // Backspace: Delete character backward
        if mods.is_unmodified() && key == "backspace" {
            if matches!(self.page, LauncherPage::Extension(_))
                && self.extension_form_edit(key, None)
            {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if self.backspace_char() {
                if self.query.trim().is_empty() {
                    self.force_expanded = false;
                }
                self.ensure_window_size(window, cx);
                self.refresh_current_page(cx);
            } else if self.page == LauncherPage::Emoji
                || self.page == LauncherPage::Clipboard
                || self.page == LauncherPage::Files
                || self.page == LauncherPage::Brew
                || self.page == LauncherPage::Text
                || self.page == LauncherPage::Notes
                || matches!(self.page, LauncherPage::Browser(_))
            || matches!(self.page, LauncherPage::Extension(_))
                || self.page == LauncherPage::Ports
                || self.page == LauncherPage::Processes
                || self.page == LauncherPage::Uninstaller
            {
                self.open_root_page(window, cx);
            }
            cx.stop_propagation();
            return;
        }

        // Delete: Delete character forward
        if mods.is_unmodified() && key == "delete" {
            if self.delete_char() {
                if self.query.trim().is_empty() {
                    self.force_expanded = false;
                }
                self.ensure_window_size(window, cx);
                self.refresh_current_page(cx);
            }
            cx.stop_propagation();
            return;
        }

        // Space
        if mods.is_unmodified() && key == "space" {
            self.insert_str(" ");
            self.ensure_window_size(window, cx);
            self.refresh_current_page(cx);
            cx.stop_propagation();
            return;
        }

        // Stage 6: Character Typing Guard
        if mods.command || mods.ctrl || mods.alt {
            return;
        }

        let typed = keystroke
            .key_char
            .clone()
            .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
        if let Some(text) = typed {
            self.insert_str(&text);
            self.ensure_window_size(window, cx);
            self.refresh_current_page(cx);
            cx.stop_propagation();
        }
    }

    fn open_emoji_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Emoji;
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.selected = 0;
        self.emoji_category_index = 0;
        self.actions_open = false;
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.emoji_scroll_handle.scroll_to_item(0);
        self.refresh_emoji(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_root_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Root;
        self.brew_page_mode = BrewPageMode::Installed;
        self.query.clear();
        self.force_expanded = false;
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.results = cached_initial_results();
        self.rebuild_root_flat_items();
        self.selected = 0;
        self.actions_open = false;
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.results_scroll_handle.scroll_to_item(0);
        self.refresh(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_clipboard_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Clipboard;
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.selected = 0;
        self.clipboard_filter_index = 0;
        self.rebuild_clipboard_flat_items();
        self.actions_open = false;
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.clipboard_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        corvo_clipboard_manager::poll_clipboard_with_source(self.previous_app_name.as_deref());
        self.refresh_clipboard(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_files_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Files;
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.results = self.file_default_results.clone();
        self.selected = 0;
        self.file_preview = FilePreviewState::Empty;
        self.file_index_ready = false;
        self.file_scroll_handle.scroll_to_item(0);
        self.actions_open = false;
        self.actions.clear();
        self.actions_filter.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.refresh_files(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_brew_page(&mut self, mode: BrewPageMode, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Brew;
        self.brew_page_mode = mode;
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.actions_open = false;
        self.actions.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.refresh_brew(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_text_page(&mut self, input: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Text;
        self.query = input.to_string();
        self.cursor_idx = self.query.chars().count();
        self.cursor_visible = true;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.actions_open = false;
        self.actions.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.refresh_text(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_pomodoro_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_extension_page("pomodoro", "", window, cx);
    }

    fn open_weather_page(&mut self, city: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_extension_page("weather", city, window, cx);
    }

    fn open_notes_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Notes;
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.actions_open = false;
        self.actions.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.refresh_notes(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_media_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_extension_page("media-control", "", window, cx);
    }

    fn open_browser_page(
        &mut self,
        browser: corvo_browser_tabs::BrowserId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page = LauncherPage::Browser(browser);
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.actions_open = false;
        self.actions.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.refresh_browser(browser, cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_ports_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Ports;
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.actions_open = false;
        self.actions.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.refresh_ports(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_processes_page(&mut self, filter: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.page = LauncherPage::Processes;
        self.query = filter.to_string();
        self.cursor_idx = self.query.chars().count();
        self.cursor_visible = true;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.actions_open = false;
        self.actions.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.refresh_processes(cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    fn open_uninstaller_page(
        &mut self,
        name: String,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path =
            if cfg!(target_os = "linux") && corvo_platform::supports_app_uninstall_path(&path) {
                path
            } else {
                path.canonicalize().unwrap_or(path)
            };
        let icon_png = corvo_platform::extract_app_icon(&path);
        let target = UninstallTarget {
            name,
            path,
            icon_png,
        };
        self.page = LauncherPage::Uninstaller;
        self.sync_palette_size(window, cx);
        self.query.clear();
        self.cursor_idx = 0;
        self.cursor_visible = true;
        self.selected = 0;
        self.actions_open = false;
        self.actions.clear();
        self.actions_filter.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        let app_file = corvo_platform::AppFileEntry {
            path: target.path.clone(),
            location: target
                .path
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default(),
            size_bytes: 0,
            is_application: true,
            matched_by_name: false,
        };
        self.uninstaller = UninstallerState::Ready(UninstallerReady {
            target: target.clone(),
            files: vec![app_file],
            selected_files: vec![true],
            focused: 0,
            confirming: false,
            scan_in_progress: true,
            scan_limited: false,
            message: None,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let scan_target = target.clone();
            let files =
                smol::unblock(move || corvo_platform::associated_app_files(&scan_target.path))
                    .await;
            let _ = this.update(cx, |launcher, cx| {
                let UninstallerState::Ready(ready) = &mut launcher.uninstaller else {
                    return;
                };
                if ready.target.path != target.path {
                    return;
                }
                match files {
                    Ok(scan) => {
                        let previous_selection: std::collections::HashMap<_, _> = ready
                            .files
                            .iter()
                            .zip(&ready.selected_files)
                            .map(|(file, selected)| (file.path.clone(), *selected))
                            .collect();
                        ready.selected_files = scan
                            .files
                            .iter()
                            .map(|file| previous_selection.get(&file.path).copied().unwrap_or(true))
                            .collect();
                        ready.focused = scan
                            .files
                            .iter()
                            .position(|file| file.is_application)
                            .unwrap_or(0);
                        ready.files = scan.files;
                        ready.scan_limited = scan.reached_scan_limit;
                    }
                    Err(error) => {
                        ready.message = Some(error.to_string());
                    }
                }
                ready.scan_in_progress = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn set_clipboard_filter(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.clipboard_filter_index != index {
            self.clipboard_filter_index = index.min(CLIPBOARD_FILTERS.len().saturating_sub(1));
            self.refresh_clipboard(cx);
        }
    }

    fn set_emoji_category(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.emoji_category_index != index {
            self.emoji_category_index = index.min(EMOJI_CATEGORIES.len().saturating_sub(1));
            self.refresh_emoji(cx);
        }
    }

    fn toggle_filter_dropdown(&mut self, cx: &mut Context<Self>) {
        if self.filter_dropdown_open {
            self.filter_dropdown_open = false;
        } else {
            self.actions_open = false;
            self.burger_menu_open = false;
            self.filter_dropdown_open = true;
            self.filter_dropdown_selected = match self.page {
                LauncherPage::Emoji => self.emoji_category_index,
                LauncherPage::Clipboard => self.clipboard_filter_index,
                LauncherPage::Files => 0,
                _ => 0,
            };
        }
        cx.notify();
    }

    fn close_filter_dropdown(&mut self, cx: &mut Context<Self>) {
        self.filter_dropdown_open = false;
        cx.notify();
    }

    fn apply_filter_selection(&mut self, idx: usize, cx: &mut Context<Self>) {
        match self.page {
            LauncherPage::Emoji => {
                self.set_emoji_category(idx, cx);
            }
            LauncherPage::Clipboard => {
                self.set_clipboard_filter(idx, cx);
            }
            LauncherPage::Files => {}
            _ => {}
        }
        self.filter_dropdown_open = false;
        cx.notify();
    }

    fn apply_filter_dropdown_selection(&mut self, cx: &mut Context<Self>) {
        self.apply_filter_selection(self.filter_dropdown_selected, cx);
    }

    fn rebuild_root_flat_items(&mut self) {
        self.root_flat_items.clear();
        self.root_to_flat.clear();
        self.root_to_flat.resize(self.results.len(), 0);

        // Consecutive rows sharing a section land under one header;
        // rows without a section fall into the default group.
        let mut current: Option<String> = None;
        if !self.results.is_empty() {
            current = self.results[0].section.clone();
            self.root_flat_items.push(RootFlatItem::Header(
                section_header(current.as_deref()),
            ));
        }
        for (res_idx, result) in self.results.iter().enumerate() {
            if result.section != current {
                current = result.section.clone();
                self.root_flat_items.push(RootFlatItem::Header(
                    section_header(current.as_deref()),
                ));
            }
            self.root_to_flat[res_idx] = self.root_flat_items.len();
            self.root_flat_items.push(RootFlatItem::Row(res_idx));
        }
    }

    fn rebuild_clipboard_flat_items(&mut self) {
        self.clipboard_flat_items.clear();
        self.clipboard_to_flat.clear();
        self.clipboard_to_flat.resize(self.results.len(), 0);

        let mut current_sec: Option<String> = None;
        for (res_idx, res) in self.results.iter().enumerate() {
            let section = res.subtitle.clone().unwrap_or_else(|| "Today".into());
            if current_sec.as_ref() != Some(&section) {
                current_sec = Some(section.clone());
                self.clipboard_flat_items
                    .push(ClipboardFlatItem::Header(section.into()));
            }
            self.clipboard_to_flat[res_idx] = self.clipboard_flat_items.len();
            self.clipboard_flat_items
                .push(ClipboardFlatItem::Row(res_idx));
        }
    }

    fn select_emoji_delta(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let cur = self.selected as isize;
        let next = (cur + delta).clamp(0, (self.results.len() - 1) as isize) as usize;
        self.select(next, cx);
    }

    fn select_result_index(&mut self, result_index: usize, cx: &mut Context<Self>) {
        self.select(result_index, cx);
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.page == LauncherPage::Uninstaller {
            let visible = self.uninstaller_visible_indices();
            if let UninstallerState::Ready(ready) = &mut self.uninstaller {
                ready.focused = visible.get(index).copied().unwrap_or(0);
                ready.confirming = false;
            }
            cx.notify();
            return;
        }
        if self.results.is_empty() {
            self.selected = 0;
            cx.notify();
            return;
        }
        if self.page == LauncherPage::Clipboard {
            self.selected = index.min(self.results.len().saturating_sub(1));
            let flat_idx = self
                .clipboard_to_flat
                .get(self.selected)
                .copied()
                .unwrap_or(0);
            self.clipboard_scroll_handle
                .scroll_to_item(flat_idx, ScrollStrategy::Nearest);
            cx.notify();
            return;
        }
        if self.page == LauncherPage::Files {
            self.selected = index.min(self.results.len().saturating_sub(1));
            self.file_scroll_handle.scroll_to_item(self.selected);
            self.refresh_file_preview(cx);
            cx.notify();
            return;
        }
        if self.page == LauncherPage::Emoji {
            self.selected = index.min(self.results.len().saturating_sub(1));
            let row = self.selected / self.emoji_column_count;
            self.emoji_scroll_handle.scroll_to_item(row);
            cx.notify();
            return;
        }

        self.selected = index.min(self.results.len().saturating_sub(1));
        if !self.selected_result_is_port_action() {
            self.port_input_active = false;
        }
        let flat_idx = self.root_to_flat.get(self.selected).copied().unwrap_or(0);
        self.results_scroll_handle.scroll_to_item(flat_idx);
        cx.notify();
    }

    fn uninstaller_visible_indices(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        match &self.uninstaller {
            UninstallerState::Ready(ready) => ready
                .files
                .iter()
                .enumerate()
                .filter(|(_, file)| {
                    query.is_empty()
                        || file
                            .path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.to_lowercase().contains(&query))
                        || file.location.to_lowercase().contains(&query)
                        || file.path.to_string_lossy().to_lowercase().contains(&query)
                })
                .map(|(index, _)| index)
                .collect(),
            _ => Vec::new(),
        }
    }

    fn selectable_count(&self) -> usize {
        if self.page == LauncherPage::Uninstaller {
            self.uninstaller_visible_indices().len()
        } else if self.page == LauncherPage::Brew && self.brew_page_mode == BrewPageMode::Upgrades {
            self.root_flat_items
                .iter()
                .filter(|item| matches!(item, RootFlatItem::Row(_)))
                .count()
        } else {
            self.results.len()
        }
    }

    fn selected_position(&self) -> usize {
        if self.page == LauncherPage::Uninstaller {
            let visible = self.uninstaller_visible_indices();
            let focused = match &self.uninstaller {
                UninstallerState::Ready(ready) => ready.focused,
                _ => 0,
            };
            visible
                .iter()
                .position(|index| *index == focused)
                .unwrap_or(0)
        } else {
            self.selected
        }
    }

    fn toggle_uninstaller_focus(&mut self, cx: &mut Context<Self>) {
        if let UninstallerState::Ready(ready) = &mut self.uninstaller {
            if let Some(selected) = ready.selected_files.get_mut(ready.focused) {
                *selected = !*selected;
                ready.confirming = false;
                ready.message = None;
            }
        }
        cx.notify();
    }

    fn begin_uninstall(&mut self, cx: &mut Context<Self>) {
        let UninstallerState::Ready(ready) = &mut self.uninstaller else {
            return;
        };
        if ready.scan_in_progress {
            return;
        }
        let selected_count = ready
            .selected_files
            .iter()
            .filter(|selected| **selected)
            .count();
        if selected_count == 0 {
            ready.message = Some(if cfg!(target_os = "macos") {
                "Select at least one item to move to Trash".into()
            } else {
                "Select the application package to uninstall".into()
            });
            ready.confirming = false;
            cx.notify();
            return;
        }
        if !ready.confirming {
            ready.confirming = true;
            ready.message = Some(if cfg!(target_os = "macos") {
                format!("Move {selected_count} selected items to Trash?")
            } else {
                format!("Uninstall {}?", ready.target.name)
            });
            cx.notify();
            return;
        }
        let target = ready.target.clone();
        let scan_limited = ready.scan_limited;
        let paths: Vec<_> = ready
            .files
            .iter()
            .zip(&ready.selected_files)
            .filter(|(_, selected)| **selected)
            .map(|(file, _)| file.path.clone())
            .collect();
        let files = ready.files.clone();
        self.uninstaller = UninstallerState::Removing {
            target: target.clone(),
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let moved_paths = paths.clone();
            let trash_target = target.clone();
            let result = smol::unblock(move || {
                corvo_platform::move_app_files_to_trash(&trash_target.path, &paths)
                    .map(|()| moved_paths)
            })
            .await;
            let (files, reached_scan_limit, message, notice) = match result {
                Ok(moved_paths) => {
                    if !cfg!(target_os = "macos") || moved_paths.contains(&target.path) {
                        corvo_app_launcher::remove_cached_apps(std::slice::from_ref(&target.path));
                    }
                    corvo_app_launcher::reload_corpus();
                    let moved_paths: std::collections::HashSet<_> =
                        moved_paths.into_iter().collect();
                    let moved_count = moved_paths.len();
                    let remaining: Vec<_> = files
                        .into_iter()
                        .filter(|file| !moved_paths.contains(&file.path))
                        .collect();
                    (
                        remaining,
                        scan_limited,
                        Some(if cfg!(target_os = "macos") {
                            format!("Moved {moved_count} items to Trash")
                        } else {
                            format!("Uninstalled {}", target.name)
                        }),
                        ToastNotice::success(
                            ToastCategory::Uninstallation,
                            if cfg!(target_os = "macos") {
                                "Moved to Trash"
                            } else {
                                "Uninstalled"
                            },
                            format!("{} · {moved_count} items", target.name),
                        ),
                    )
                }
                Err(error) => {
                    corvo_app_launcher::reload_corpus();
                    let notice = ToastNotice::failure(
                        ToastCategory::Uninstallation,
                        format!("Could not remove {}", target.name),
                        error.to_string(),
                    );
                    let scan_target = target.clone();
                    let current_scan = smol::unblock(move || {
                        corvo_platform::associated_app_files(&scan_target.path)
                    })
                    .await;
                    match current_scan {
                        Ok(scan) => (
                            scan.files,
                            scan.reached_scan_limit,
                            Some(error.to_string()),
                            notice,
                        ),
                        Err(_) => (files, scan_limited, Some(error.to_string()), notice),
                    }
                }
            };
            cx.update(|cx| show_action_toast(notice, cx));
            let _ = this.update(cx, |launcher, cx| {
                let selected_files = vec![false; files.len()];
                launcher.uninstaller = UninstallerState::Ready(UninstallerReady {
                    target,
                    files,
                    selected_files,
                    focused: 0,
                    confirming: false,
                    scan_in_progress: false,
                    scan_limited: reached_scan_limit,
                    message,
                });
                launcher.query.clear();
                launcher.cursor_idx = 0;
                cx.notify();
            });
        })
        .detach();
    }

    /// Root-search placeholder for a declared command with arguments:
    /// the selected result's argument hint becomes the search bar
    /// hint, the Raycast arguments behavior.
    fn declared_argument_placeholder(&self) -> Option<SharedString> {
        if self.page != LauncherPage::Root {
            return None;
        }
        let result = self.selected_result()?;
        let command_name = result.id.split(':').next()?;
        let command = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_name)?;
        let spec = command
            .manifest()
            .commands
            .into_iter()
            .find(|spec| spec.mode == corvo_core::CommandMode::View)?;
        let argument = spec.arguments.first()?;
        let hint = argument.placeholder;
        (!hint.is_empty()).then(|| SharedString::from(hint))
    }

    fn selected_result(&self) -> Option<&SearchResult> {
        self.results.get(self.selected)
    }

    /// Runs one declared quick command's execute id without opening
    /// the launcher; the command's toast is the visible confirmation.
    fn execute_declared_quick(&mut self, result_id: &str, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| result_id.starts_with(&format!("{}:", command.id())))
            .cloned()
        else {
            return;
        };
        let result_id = result_id.to_owned();
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let outcome = command
                .execute(&result_id, &ExecutionContext { store: Some(store) })
                .await;
            let _ = this.update(cx, |_launcher, cx| {
                if let Ok(corvo_core::Action::ShowToast(message)) = &outcome {
                    if !message.is_empty() && !message.contains(':') {
                        show_action_toast(
                            ToastNotice::success(
                                ToastCategory::General,
                                command.id().replace('-', " "),
                                message.clone(),
                            ),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    fn selected_result_is_port_action(&self) -> bool {
        self.selected_result()
            .is_some_and(|result| result.id == "kill-process:open-ports")
    }

    fn kill_port_argument(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Ok(port) = self.port_argument.parse::<u16>() else {
            self.port_input_active = false;
            self.cursor_visible = true;
            show_action_toast(
                ToastNotice::failure(
                    ToastCategory::Port,
                    "Invalid port",
                    "Enter a port from 1 to 65535",
                ),
                cx,
            );
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = smol::unblock(move || {
                let listeners = corvo_platform::listening_port_snapshot()
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .filter(|listener| listener.port == port)
                    .map(|listener| listener.process)
                    .collect::<std::collections::HashSet<_>>();
                if listeners.is_empty() {
                    return Err(format!("No process is listening on port {port}"));
                }
                let mut stopped = 0;
                let mut errors = Vec::new();
                for identity in listeners {
                    match corvo_platform::terminate_process(
                        identity,
                        corvo_platform::TerminationMode::Graceful,
                    ) {
                        Ok(()) => stopped += 1,
                        Err(error) => errors.push(error.to_string()),
                    }
                }
                if errors.is_empty() {
                    let process_label = if stopped == 1 { "process" } else { "processes" };
                    Ok(format!("Stopped {stopped} {process_label} on port {port}"))
                } else {
                    Err(format!("Stopped {stopped}. {}", errors.join("; ")))
                }
            })
            .await;
            let notice = match result {
                Ok(message) => ToastNotice::success(ToastCategory::Port, "Port released", message),
                Err(message) => {
                    ToastNotice::failure(ToastCategory::Port, "Could not clear port", message)
                }
            };
            cx.update(|cx| show_action_toast(notice, cx));
            let _ = this.update(cx, |launcher, _cx| {
                launcher.port_input_active = false;
            });
        })
        .detach();
    }

    /// Runs every command's search on the background executor and swaps
    /// the result list when they answer.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.query.clone();
        let commands = self.registry.commands().to_vec();
        let scoped_command_id = commands
            .iter()
            .find(|command| {
                command.prefix().is_some_and(|prefix| {
                    corvo_core::search::matches_command_prefix(&query, prefix)
                }) || (command.id() == "kill-process"
                    && corvo_core::search::matches_command_prefix(&query, "port manager"))
            })
            .map(|command| command.id());
        let launcher_store = self.store.clone();
        let settings = corvo_config::Settings::load();
        let fallback_enabled = self.store.command_enabled("web-search-fallback");
        let max_results = if query.is_empty() { 2000 } else { 100 };
        if query.is_empty() {
            self.results = cached_initial_results();
            apply_result_preferences(&mut self.results, &settings, "");
            self.rebuild_root_flat_items();
            self.selected = 0;
            self.results_scroll_handle.scroll_to_item(0);
            cx.notify();
        } else {
            self.results = if scoped_command_id.is_some() {
                Vec::new()
            } else {
                cached_query_results(&query, max_results, fallback_enabled)
            };
            apply_result_preferences(&mut self.results, &settings, &query);
            self.rebuild_root_flat_items();
            self.selected = 0;
            self.results_scroll_handle.scroll_to_item(0);
            cx.notify();
        }
        // Only the latest refresh may write results; a stale task from
        // an older query never overwrites the current list.
        self.search_seq += 1;
        let seq = self.search_seq;
        let search_settings = settings.clone();
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results,
                store: Some(launcher_store),
            };
            let searchable: Vec<_> = commands
                .into_iter()
                .filter(|command| {
                    command.id() != "web-search-fallback"
                        && scoped_command_id.map_or_else(
                            || command.prefix().is_none() || !query.trim().is_empty(),
                            |id| command.id() == id,
                        )
                        && ctx
                            .store
                            .as_ref()
                            .is_some_and(|store| store.command_enabled(command.id()))
                })
                .collect();
            let search_count = searchable.len();
            let (sender, receiver) = smol::channel::unbounded();
            for command in searchable {
                let sender = sender.clone();
                let command_prefix = command.prefix().map(str::to_owned);
                let typed_query = query.clone();
                let search_command_menu = scoped_command_id.is_none() && command_prefix.is_some();
                let search_query = if search_command_menu {
                    if command.id() == "kill-process" {
                        "Port Manager".to_string()
                    } else {
                        command_prefix.unwrap_or_default()
                    }
                } else {
                    query.clone()
                };
                let ctx = ctx.clone();
                smol::spawn(async move {
                    let mut results = command.search(&search_query, &ctx).await;
                    if search_command_menu {
                        let has_exact_word_match = results.iter().any(|result| {
                            command_menu_exact_word_match(typed_query.trim(), result)
                        });
                        results.retain_mut(|result| {
                            if typed_query.trim().is_empty() {
                                return true;
                            }
                            if has_exact_word_match
                                && !command_menu_exact_word_match(typed_query.trim(), result)
                            {
                                return false;
                            }
                            let Some(score) = command_menu_match_score(typed_query.trim(), result)
                            else {
                                return false;
                            };
                            result.score = score + 600;
                            true
                        });
                    }
                    let _ = sender.send(results).await;
                })
                .detach();
            }
            drop(sender);

            let mut results = Vec::new();
            let mut received = 0;
            // Aggregation window: wait up to 20ms for all in-memory commands to complete
            let mut timer = smol::Timer::after(std::time::Duration::from_millis(20));
            let mut timed_out = false;

            while received < search_count && !timed_out {
                let is_timeout = smol::future::race(
                    async {
                        match receiver.recv().await {
                            Ok(batch) => {
                                results.extend(batch);
                                received += 1;
                                false
                            }
                            Err(_) => {
                                received = search_count;
                                false
                            }
                        }
                    },
                    async {
                        (&mut timer).await;
                        true
                    },
                )
                .await;
                if is_timeout {
                    timed_out = true;
                }
            }

            let mut fast_results = results.clone();
            if query.is_empty() {
                fast_results.retain(|result| {
                    let command_id = result.id.split(':').next().unwrap_or_default();
                    ctx.store
                        .as_ref()
                        .is_some_and(|store| store.show_command_in_launcher(command_id))
                });
            }
            apply_result_preferences(&mut fast_results, &search_settings, &query);
            if !query.is_empty() {
                let has_confident_match = fast_results
                    .first()
                    .is_some_and(|result| result.score >= CONFIDENT_SEARCH_SCORE);
                let limit = if has_confident_match {
                    max_results.saturating_sub(2)
                } else {
                    MAX_WEAK_SEARCH_RESULTS
                };
                fast_results.truncate(limit);
                if scoped_command_id.is_none() && fallback_enabled {
                    fast_results.extend(corvo_web_search_fallback::search_results(&query));
                }
                apply_result_preferences(&mut fast_results, &search_settings, &query);
            } else {
                fast_results.truncate(max_results);
            }
            if !query.is_empty() || received == search_count {
                let _ = this.update(cx, |launcher, cx| {
                    if launcher.search_seq != seq {
                        return;
                    }
                    if query.is_empty() {
                        if let Some(cell) = INITIAL_RESULTS.get() {
                            if let Ok(mut lock) = cell.write() {
                                *lock = fast_results.clone();
                            }
                        }
                    }
                    let prev_selected = launcher.selected;
                    let prev_id = if prev_selected > 0 {
                        launcher.selected_result().map(|r| r.id.clone())
                    } else {
                        None
                    };
                    launcher.results = fast_results;
                    launcher.rebuild_root_flat_items();
                    if let Some(ref id) = prev_id {
                        launcher.selected = launcher
                            .results
                            .iter()
                            .position(|r| &r.id == id)
                            .unwrap_or(0);
                    } else {
                        launcher.selected = 0;
                        launcher.results_scroll_handle.scroll_to_item(0);
                    }
                    launcher.actions_open = false;
                    launcher.actions = Vec::new();
                    launcher.actions_filter = String::new();
                    cx.notify();
                });
            }

            // If a slow command (like disk search) took longer than 20ms, await it and perform a final update
            if received < search_count {
                while received < search_count {
                    if let Ok(batch) = receiver.recv().await {
                        results.extend(batch);
                        received += 1;
                    } else {
                        break;
                    }
                }
                if query.is_empty() {
                    results.retain(|result| {
                        let command_id = result.id.split(':').next().unwrap_or_default();
                        ctx.store
                            .as_ref()
                            .is_some_and(|store| store.show_command_in_launcher(command_id))
                    });
                }
                apply_result_preferences(&mut results, &search_settings, &query);
                if !query.is_empty() {
                    let has_confident_match = results
                        .first()
                        .is_some_and(|result| result.score >= CONFIDENT_SEARCH_SCORE);
                    let limit = if has_confident_match {
                        max_results.saturating_sub(2)
                    } else {
                        MAX_WEAK_SEARCH_RESULTS
                    };
                    results.truncate(limit);
                    if scoped_command_id.is_none() && fallback_enabled {
                        results.extend(corvo_web_search_fallback::search_results(&query));
                    }
                    apply_result_preferences(&mut results, &search_settings, &query);
                } else {
                    results.truncate(max_results);
                }
                let _ = this.update(cx, |launcher, cx| {
                    if launcher.search_seq == seq {
                        if query.is_empty() {
                            if let Some(cell) = INITIAL_RESULTS.get() {
                                if let Ok(mut lock) = cell.write() {
                                    *lock = results.clone();
                                }
                            }
                        }
                        let prev_selected = launcher.selected;
                        let prev_id = if prev_selected > 0 {
                            launcher.selected_result().map(|r| r.id.clone())
                        } else {
                            None
                        };
                        launcher.results = results;
                        launcher.rebuild_root_flat_items();
                        if let Some(ref id) = prev_id {
                            launcher.selected = launcher
                                .results
                                .iter()
                                .position(|r| &r.id == id)
                                .unwrap_or(0);
                        } else {
                            launcher.selected = 0;
                            launcher.results_scroll_handle.scroll_to_item(0);
                        }
                        launcher.actions_open = false;
                        launcher.actions = Vec::new();
                        launcher.actions_filter = String::new();
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn refresh_emoji(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        let query = self.query.clone();
        let selected_category = if self.emoji_category_index > 0 {
            EMOJI_CATEGORIES.get(self.emoji_category_index).copied()
        } else {
            None
        };
        let commands = self.registry.commands().to_vec();
        let launcher_store = self.store.clone();
        let Some(command) = commands.iter().find(|c| c.id() == "emoji-picker").cloned() else {
            cx.notify();
            return;
        };
        if !self.store.command_enabled(command.id()) {
            cx.notify();
            return;
        }

        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 1071,
                store: Some(launcher_store),
            };
            let raw_query = format!("emoji-page:{query}");
            let mut results = command.search(&raw_query, &ctx).await;
            if let Some(cat) = selected_category {
                results.retain(|r| r.subtitle.as_deref() == Some(cat));
            }
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Emoji {
                    launcher.results = results;
                    launcher.selected = 0;
                    launcher.emoji_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_clipboard(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        let query = self.query.clone();
        let filter_name = match self.clipboard_filter_index {
            1 => "text",
            2 => "links",
            3 => "images",
            4 => "json",
            _ => "all",
        };
        let commands = self.registry.commands().to_vec();
        let launcher_store = self.store.clone();
        let Some(command) = commands
            .iter()
            .find(|c| c.id() == "clipboard-manager")
            .cloned()
        else {
            cx.notify();
            return;
        };
        if !self.store.command_enabled(command.id()) {
            cx.notify();
            return;
        }

        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 100,
                store: Some(launcher_store),
            };
            let raw_query = format!("clipboard-page:filter={filter_name}:{query}");
            let results = command.search(&raw_query, &ctx).await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Clipboard {
                    launcher.results = results;
                    launcher.rebuild_clipboard_flat_items();
                    launcher.selected = launcher
                        .selected
                        .min(launcher.results.len().saturating_sub(1));
                    let flat_idx = launcher
                        .clipboard_to_flat
                        .get(launcher.selected)
                        .copied()
                        .unwrap_or(0);
                    launcher
                        .clipboard_scroll_handle
                        .scroll_to_item(flat_idx, ScrollStrategy::Nearest);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_files(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == "file-search")
            .cloned()
        else {
            self.results.clear();
            self.file_index = std::sync::Arc::new(Vec::new());
            self.file_default_results.clear();
            self.file_index_ready = false;
            self.file_index_loading = false;
            self.file_preview = FilePreviewState::Empty;
            cx.notify();
            return;
        };
        if !self.store.command_enabled(command.id()) {
            self.results.clear();
            self.file_index = std::sync::Arc::new(Vec::new());
            self.file_default_results.clear();
            self.file_index_ready = false;
            self.file_index_loading = false;
            self.file_preview = FilePreviewState::Empty;
            cx.notify();
            return;
        }

        self.search_seq += 1;
        let seq = self.search_seq;
        let query = self.query.trim().to_lowercase();

        if self.file_index_ready {
            if query.is_empty() {
                self.set_file_results(self.file_default_results.clone(), cx);
                return;
            }
            let index = self.file_index.clone();
            cx.spawn(async move |this, cx| {
                smol::Timer::after(std::time::Duration::from_millis(40)).await;
                let Ok(is_current) = this.read_with(cx, |launcher, _| {
                    launcher.page == LauncherPage::Files && launcher.search_seq == seq
                }) else {
                    return;
                };
                if !is_current {
                    return;
                }
                let results =
                    smol::unblock(move || filter_file_search_results(&index, &query, 100)).await;
                let _ = this.update(cx, |launcher, cx| {
                    if launcher.page == LauncherPage::Files && launcher.search_seq == seq {
                        launcher.set_file_results(results, cx);
                    }
                });
            })
            .detach();
            return;
        }

        if self.file_index_loading {
            let results = if query.is_empty() {
                self.file_default_results.clone()
            } else {
                let candidates = prepare_file_search_index(self.file_default_results.clone());
                filter_file_search_results(&candidates, &query, 100)
            };
            self.set_file_results(results, cx);
            return;
        }

        self.file_index_loading = true;
        let launcher_store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let shallow_ctx = SearchContext {
                max_results: 100,
                store: Some(launcher_store.clone()),
            };
            let shallow_results = command.search("file-search-page:", &shallow_ctx).await;
            let shallow_candidates = prepare_file_search_index(shallow_results.clone());
            let _ = this.update(cx, |launcher, cx| {
                if launcher.page == LauncherPage::Files {
                    launcher.file_default_results = shallow_results.clone();
                    let query = launcher.query.trim().to_lowercase();
                    let visible = if query.is_empty() {
                        shallow_results
                    } else {
                        filter_file_search_results(&shallow_candidates, &query, 100)
                    };
                    launcher.set_file_results(visible, cx);
                }
            });

            let index_ctx = SearchContext {
                max_results: 20_000,
                store: Some(launcher_store),
            };
            let index = command.search("file-search-page:index", &index_ctx).await;
            let index = smol::unblock(move || prepare_file_search_index(index)).await;
            let _ = this.update(cx, |launcher, cx| {
                launcher.file_index_loading = false;
                launcher.file_index = std::sync::Arc::new(index);
                launcher.file_index_ready = true;
                if launcher.page == LauncherPage::Files {
                    launcher.refresh_files(cx);
                }
            });
        })
        .detach();
    }

    fn set_file_results(&mut self, results: Vec<SearchResult>, cx: &mut Context<Self>) {
        let previous_id = self.selected_result().map(|result| result.id.clone());
        self.results = results;
        self.selected = previous_id
            .and_then(|id| self.results.iter().position(|result| result.id == id))
            .unwrap_or(0)
            .min(self.results.len().saturating_sub(1));
        self.file_scroll_handle.scroll_to_item(self.selected);
        self.refresh_file_preview(cx);
        cx.notify();
    }

    fn refresh_file_preview(&mut self, cx: &mut Context<Self>) {
        let path = self
            .selected_result()
            .and_then(|result| result.id.strip_prefix("file-search:"))
            .filter(|path| *path != "open")
            .map(std::path::PathBuf::from);
        let Some(path) = path else {
            self.file_preview_seq += 1;
            self.file_preview = FilePreviewState::Empty;
            return;
        };
        let already_loaded = match &self.file_preview {
            FilePreviewState::Loading(current) => current == &path,
            FilePreviewState::Ready { path: current, .. } => current == &path,
            FilePreviewState::Empty => false,
        };
        if already_loaded {
            return;
        }

        self.file_preview_seq += 1;
        let seq = self.file_preview_seq;
        self.file_preview = FilePreviewState::Loading(path.clone());
        cx.spawn(async move |this, cx| {
            let preview_path = path.clone();
            let preview = smol::unblock(move || load_file_preview(&preview_path)).await;
            let _ = this.update(cx, |launcher, cx| {
                let selected_path = launcher
                    .selected_result()
                    .and_then(|result| result.id.strip_prefix("file-search:"));
                if launcher.page == LauncherPage::Files
                    && launcher.file_preview_seq == seq
                    && selected_path == Some(path.to_string_lossy().as_ref())
                {
                    launcher.file_preview = FilePreviewState::Ready { path, preview };
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_brew(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == "brew")
            .cloned()
        else {
            self.results.clear();
            self.rebuild_root_flat_items();
            cx.notify();
            return;
        };
        if !self.store.command_enabled(command.id()) {
            self.results.clear();
            self.rebuild_root_flat_items();
            cx.notify();
            return;
        }

        let mode = self.brew_page_mode;
        let query = format!("brew-page:{}:{}", mode.token(), self.query);
        let store = self.store.clone();
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 200,
                store: Some(store),
            };
            let results = command.search(&query, &ctx).await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq
                    && launcher.page == LauncherPage::Brew
                    && launcher.brew_page_mode == mode
                {
                    launcher.results = results;
                    launcher.rebuild_root_flat_items();
                    launcher.selected = if mode == BrewPageMode::Upgrades {
                        launcher
                            .root_flat_items
                            .iter()
                            .find_map(|item| match item {
                                RootFlatItem::Row(index) => Some(*index),
                                RootFlatItem::Header(_) => None,
                            })
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    launcher.results_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_text(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == "text-utilities")
            .cloned()
        else {
            self.results.clear();
            self.rebuild_root_flat_items();
            cx.notify();
            return;
        };
        let input = self.query.clone();
        let store = self.store.clone();
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 200,
                store: Some(store),
            };
            let results = command.search(&format!("text-page:{input}"), &ctx).await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Text {
                    launcher.results = results;
                    launcher.rebuild_root_flat_items();
                    launcher.selected = launcher
                        .selected
                        .min(launcher.results.len().saturating_sub(1));
                    launcher.results_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_notes(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == "notes")
            .cloned()
        else {
            return;
        };
        let filter = self.query.clone();
        let store = self.store.clone();
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 200,
                store: Some(store),
            };
            let results = command.search(&format!("notes-page:{filter}"), &ctx).await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Notes {
                    launcher.results = results;
                    launcher.rebuild_root_flat_items();
                    launcher.selected = launcher
                        .selected
                        .min(launcher.results.len().saturating_sub(1));
                    launcher.results_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Warms one browser's tab cache off the search path, then runs
    /// the cache-reading command search.
    fn refresh_browser(
        &mut self,
        browser: corvo_browser_tabs::BrowserId,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            smol::unblock(move || corvo_browser_tabs::fetch_tabs(browser)).await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.page == LauncherPage::Browser(browser) {
                    launcher.refresh_browser_search(browser, cx);
                }
            });
        })
        .detach();
        self.refresh_browser_search(browser, cx);
    }

    fn refresh_browser_search(
        &mut self,
        browser: corvo_browser_tabs::BrowserId,
        cx: &mut Context<Self>,
    ) {
        let command_id = browser.spec().id;
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_id)
            .cloned()
        else {
            return;
        };
        let filter = self.query.clone();
        let store = self.store.clone();
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 200,
                store: Some(store),
            };
            let results = command
                .search(&format!("{}-page:{filter}", browser.spec().id), &ctx)
                .await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq
                    && launcher.page == LauncherPage::Browser(browser)
                {
                    launcher.results = results;
                    launcher.rebuild_root_flat_items();
                    launcher.selected = launcher
                        .selected
                        .min(launcher.results.len().saturating_sub(1));
                    launcher.results_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Opens a declarative extension page: the command id names the
    /// extension, `query` carries its filter or argument.
    fn open_extension_page(
        &mut self,
        command_id: &'static str,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page = LauncherPage::Extension(command_id);
        self.query = query.to_owned();
        self.cursor_idx = query.chars().count();
        self.cursor_visible = true;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.actions_open = false;
        self.actions.clear();
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        self.extension_view = None;
        self.extension_button_focus = 0;
        self.extension_form = ExtensionFormState::default();
        self.refresh_extension(command_id, false, cx);
        self.sync_palette_size(window, cx);
        cx.notify();
    }

    /// Asks the command for its page view and applies it. Query edits
    /// debounce 400 ms so a fetch-backed page does not fire a request
    /// per keystroke; opens, actions, and pump ticks are immediate.
    fn refresh_extension(
        &mut self,
        command_id: &'static str,
        debounce: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_id)
            .cloned()
        else {
            self.open_root_page_from_extension(cx);
            return;
        };
        let query = self.query.clone();
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            if debounce {
                smol::Timer::after(std::time::Duration::from_millis(400)).await;
                let still_current = this
                    .update(cx, |launcher, _| launcher.search_seq == seq)
                    .unwrap_or(false);
                if !still_current {
                    return;
                }
            }
            let fetched = smol::unblock({
                let command = command.clone();
                let query = query.clone();
                move || command.page(&query)
            })
            .await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq != seq
                    || launcher.page != LauncherPage::Extension(command_id)
                {
                    return;
                }
                match fetched {
                    Some(view) => {
                        launcher.apply_extension_view(&view, cx);
                        launcher.ensure_extension_pump(command_id, cx);
                    }
                    // The command stopped offering a page; go home.
                    None => launcher.open_root_page_from_extension(cx),
                }
            });
        })
        .detach();
    }

    /// Stores the view, resets per-view token state, and repaints.
    fn apply_extension_view(&mut self, view: &corvo_core::PageView, cx: &mut Context<Self>) {
        let changed_shape = self
            .extension_view
            .as_ref()
            .map(|current| !same_page_shape(current, view))
            .unwrap_or(true);
        self.extension_view = Some(view.clone());
        if changed_shape {
            self.extension_button_focus = 0;
            if let corvo_core::PageView::Form { fields, .. } = view {
                self.extension_form.reset(fields);
            }
        }
        cx.notify();
    }

    /// The ticking-page pump: while a page declares `Refresh::Every`,
    /// re-ask the command at that cadence and stop when the page
    /// closes or the cadence changes.
    fn ensure_extension_pump(&mut self, command_id: &'static str, cx: &mut Context<Self>) {
        let Some(period) = self.extension_view.as_ref().map(|view| match view.refresh() {
            corvo_core::Refresh::Manual => None,
            corvo_core::Refresh::Every(secs) => Some(secs.max(1)),
        }) else {
            return;
        };
        let Some(period) = period else {
            return;
        };
        if self.extension_pump_running {
            return;
        }
        self.extension_pump_running = true;
        cx.spawn(async move |this, cx| {
            smol::Timer::after(std::time::Duration::from_secs(period)).await;
            let Ok(state) = this.update(cx, |launcher, _| {
                (
                    launcher.page == LauncherPage::Extension(command_id),
                    launcher
                        .extension_view
                        .as_ref()
                        .map(|view| view.refresh())
                        == Some(corvo_core::Refresh::Every(period)),
                )
            }) else {
                return;
            };
            let (on_page, same_cadence) = state;
            if !on_page || !same_cadence {
                let _ = this.update(cx, |launcher, _| launcher.extension_pump_running = false);
                return;
            }
            // The refresh lands a new view, whose apply re-arms the
            // pump — the tick continues for as long as the page stays
            // open at this cadence.
            let _ = this.update(cx, |launcher, cx| {
                launcher.extension_pump_running = false;
                launcher.refresh_extension(command_id, false, cx);
            });
        })
        .detach();
    }

    fn open_root_page_from_extension(&mut self, cx: &mut Context<Self>) {
        self.extension_view = None;
        self.extension_pump_running = false;
        self.open_root_page_via(cx);
    }

    /// `open_root_page` needs a window handle; this variant covers the
    /// async paths that only have a context.
    fn open_root_page_via(&mut self, cx: &mut Context<Self>) {
        self.page = LauncherPage::Root;
        self.query.clear();
        self.cursor_idx = 0;
        self.selected = 0;
        self.results.clear();
        self.rebuild_root_flat_items();
        self.refresh(cx);
        cx.notify();
    }

    /// The declarative page: renders whatever `Command::page`
    /// returned.
    fn extension_page_view(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let Some(view) = self.extension_view.clone() else {
            return div()
                .id("extension-loading")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child("Loading..."),
                );
        };
        match view {
            corvo_core::PageView::Blocks(_) => self.blocks_page_view(&view, cx),
            corvo_core::PageView::Detail { .. } => self.detail_page_view(&view, cx),
            corvo_core::PageView::Grid { .. } => self.grid_page_view(&view, cx),
            corvo_core::PageView::Form { .. } => self.form_page_view(&view, cx),
        }
    }

    fn tone_color(tone: corvo_core::Tone) -> u32 {
        match tone {
            corvo_core::Tone::Neutral => COLOR_TEXT_DIM,
            corvo_core::Tone::Accent => COLOR_ACCENT,
            corvo_core::Tone::Positive => COLOR_ACCENT,
            corvo_core::Tone::Warning => 0xfbbf24,
            corvo_core::Tone::Destructive => COLOR_DESTRUCTIVE,
        }
    }

    fn blocks_page_view(&mut self, view: &corvo_core::PageView, cx: &mut Context<Self>) -> Stateful<Div> {
        let corvo_core::PageView::Blocks(blocks) = view else {
            return div().id("extension-blocks-fallback");
        };
        // The scroller is top-aligned; the content child centers itself
        // with auto margins, which collapse under overflow — a tall
        // page scrolls instead of clipping both ends like
        // justify-center would.
        let mut inner = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .px(px(24.0))
            .mt_auto()
            .mb_auto();
        for block in &blocks.blocks {
            match block {
                corvo_core::Block::Badge(badge) => {
                    let color = badge.style.color_or(Self::tone_color(badge.tone));
                    let size = badge.style.size.unwrap_or(11) as f32;
                    let bold = badge.style.bold.unwrap_or(false);
                    let mut pill = div()
                        .id(SharedString::from(format!("badge-{}", badge.label)))
                        .px_2p5()
                        .py_0p5()
                        .rounded_full()
                        .text_size(px(size))
                        .font_weight(if bold {
                            FontWeight::BOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .text_color(rgb(color))
                        .child(badge.label.clone());
                    if let Some(background) = badge.style.background {
                        pill = pill.bg(rgb(background));
                    }
                    inner = inner.child(pill);
                }
                corvo_core::Block::Hero(hero) => {
                    let color = hero.style.color_or(Self::tone_color(hero.tone));
                    let value_size = hero.style.size.unwrap_or(56) as f32;
                    let glyph_size = hero.style.glyph_size.unwrap_or(40) as f32;
                    let mut hero_view = div()
                        .id(SharedString::from(format!("hero-{}", hero.title)))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1();
                    if let Some(glyph) = hero.glyph {
                        hero_view = hero_view.child(
                            div().text_size(px(glyph_size)).child(glyph.to_string()),
                        );
                    }
                    if !hero.value.is_empty() {
                        hero_view = hero_view.child(
                            div()
                                .text_size(px(value_size))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(color))
                                .child(hero.value.clone()),
                        );
                    }
                    inner = inner.child(
                        hero_view
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(hero.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child(hero.subtitle.clone()),
                            ),
                    );
                }
                corvo_core::Block::Progress(progress) => {
                    let color = progress.style.color_or(Self::tone_color(progress.tone));
                    let track = progress.style.background.unwrap_or(0x2a2f36);
                    let bar_width = progress.style.width.unwrap_or(300) as f32;
                    let bar_height = progress.style.height.unwrap_or(5) as f32;
                    let fill = (bar_width * (1.0 - progress.fraction)).max(0.0);
                    inner = inner.child(
                        div()
                            .id("extension-progress")
                            .w(px(bar_width))
                            .h(px(bar_height))
                            .rounded_full()
                            .bg(rgb(track))
                            .child(
                                div()
                                    .w(px(fill))
                                    .h_full()
                                    .rounded_full()
                                    .bg(rgb(color)),
                            ),
                    );
                }
                corvo_core::Block::Markdown { text, style } => {
                    let color = style.color_or(COLOR_TEXT);
                    let size = style.size.unwrap_or(13) as f32;
                    inner = inner.child(
                        div()
                            .id(SharedString::from(format!("md-{}", text.len())))
                            .max_w(px(560.0))
                            .text_size(px(size))
                            .text_color(rgb(color))
                            .child(render_inline_markdown(text)),
                    );
                }
                corvo_core::Block::Strip(cards) => {
                    let mut strip = div()
                        .id("extension-strip")
                        .flex()
                        .gap_2()
                        .max_w(px(560.0));
                    for card in cards {
                        let color = card.style.color_or(COLOR_TEXT);
                        let background = card.style.background.unwrap_or(0x181b1e);
                        let glyph_size = card.style.glyph_size.unwrap_or(20) as f32;
                        let value_size = card.style.size.unwrap_or(14) as f32;
                        let mut card_view = div()
                            .id(SharedString::from(format!("card-{}", card.title)))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_0p5()
                            .px_3()
                            .py_1p5()
                            .rounded_lg()
                            .bg(rgb(background));
                        if let Some(glyph) = card.glyph {
                            card_view = card_view.child(
                                div().text_size(px(glyph_size)).child(glyph.to_string()),
                            );
                        }
                        strip = strip.child(
                            card_view
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(COLOR_TEXT_DIM))
                                        .child(card.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(value_size))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(color))
                                        .child(card.value.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(COLOR_TEXT_DIM))
                                        .child(card.subtitle.clone()),
                                ),
                        );
                    }
                    inner = inner.child(strip);
                }
                corvo_core::Block::Buttons(buttons) => {
                    let mut row = div()
                        .id("extension-buttons")
                        .flex()
                        .gap_2()
                        .justify_center();
                    for button in buttons.iter() {
                        let color = button.style.color_or(if button.tone == corvo_core::Tone::Neutral {
                            COLOR_TEXT
                        } else {
                            Self::tone_color(button.tone)
                        });
                        let size = button.style.size.unwrap_or(13) as f32;
                        let bold = button.style.bold.unwrap_or(false);
                        let action_id = button.action_id.clone();
                        let command_id = match self.page {
                            LauncherPage::Extension(id) => id,
                            _ => continue,
                        };
                        let mut tile = div()
                            .id(SharedString::from(format!("ext-btn-{action_id}")))
                            .px_4()
                            .py_1p5()
                            .rounded_lg()
                            .text_size(px(size))
                            .font_weight(if bold {
                                FontWeight::BOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .text_color(rgb(color))
                            .child(button.label.clone());
                        tile = match button.style.background {
                            // Selection lives in the command's styling,
                            // not in a UI focus ring.
                            Some(background) => tile.bg(rgb(background)),
                            None => tile.bg(rgb(0x181b1e)),
                        };
                        row = row.child(tile.on_click(cx.listener(
                            move |launcher, _: &ClickEvent, _window, cx| {
                                launcher.run_extension_action(command_id, action_id.clone(), cx);
                            },
                        )));
                    }
                    inner = inner.child(row);
                }
            }
        }
        // The scroller wraps the self-centering content.
        div()
            .id("extension-blocks")
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.extension_scroll_handle)
            .pb(px(56.0))
            .child(inner)
    }

    fn detail_page_view(&mut self, view: &corvo_core::PageView, cx: &mut Context<Self>) -> Stateful<Div> {
        let corvo_core::PageView::Detail {
            markdown, metadata, ..
        } = view
        else {
            return div().id("extension-detail-fallback");
        };
        let mut row = div()
            .id("extension-detail")
            .flex_1()
            .w_full()
            .flex()
            .gap_4()
            .px(px(24.0))
            .pb(px(56.0));
        row = row.child(
            div()
                .id("extension-detail-markdown")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(&self.results_scroll_handle)
                .text_size(px(13.5))
                .text_color(rgb(COLOR_TEXT))
                .child(render_inline_markdown(markdown)),
        );
        if !metadata.is_empty() {
            let mut panel = div().id("extension-detail-metadata").w(px(220.0)).flex().flex_col().gap_2();
            for entry in metadata {
                match entry {
                    corvo_core::Metadata::Label { title, text, tone } => {
                        panel = panel.child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(rgb(COLOR_TEXT_DIM))
                                        .child(title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(rgb(Self::tone_color(*tone)))
                                        .child(text.clone()),
                                ),
                        );
                    }
                    corvo_core::Metadata::Link { title, text, url } => {
                        let url = url.clone();
                        panel = panel.child(
                            div()
                                .id(SharedString::from(format!("meta-link-{url}")))
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(rgb(COLOR_TEXT_DIM))
                                        .child(title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(rgb(COLOR_ACCENT))
                                        .child(text.clone()),
                                )
                                .on_click(cx.listener(move |_: &mut Self, _: &ClickEvent, _, _| {
                                    let _ = corvo_platform::open_url(&url);
                                })),
                        );
                    }
                    corvo_core::Metadata::Tags { title, tags } => {
                        let mut tag_row = div().flex().flex_col().gap_1();
                        tag_row = tag_row.child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(title.clone()),
                        );
                        let mut tags_row = div().flex().flex_wrap().gap_1();
                        for tag in tags {
                            tags_row = tags_row.child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_full()
                                    .bg(rgb(0x181b1e))
                                    .text_size(px(10.5))
                                    .child(tag.clone()),
                            );
                        }
                        panel = panel.child(tag_row.child(tags_row));
                    }
                    corvo_core::Metadata::Separator => {
                        panel = panel.child(
                            div().w_full().h(px(1.0)).bg(rgb(0x2a2f36)),
                        );
                    }
                }
            }
            row = row.child(panel);
        }
        row
    }

    fn grid_page_view(&mut self, view: &corvo_core::PageView, cx: &mut Context<Self>) -> Stateful<Div> {
        let corvo_core::PageView::Grid {
            items, columns, ..
        } = view
        else {
            return div().id("extension-grid-fallback");
        };
        let needle = self.query.trim().to_lowercase();
        let visible: Vec<&corvo_core::GridItem> = items
            .iter()
            .filter(|item| {
                needle.is_empty()
                    || corvo_core::search_match_score(
                        &needle,
                        &[item.title.as_str(), item.subtitle.as_str()],
                    )
                    .is_some()
            })
            .collect();
        if visible.is_empty() {
            return div()
                .id("extension-grid")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .pb(px(56.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child("No matches"),
                );
        }
        let command_id = match self.page {
            LauncherPage::Extension(id) => id,
            _ => return div().id("extension-grid-fallback"),
        };
        let columns = columns.unwrap_or(6).clamp(1, 8) as usize;
        let tile_size = 84.0;
        let row_width = columns as f32 * (tile_size + 8.0);
        let mut grid_view = div()
            .id("extension-grid")
            .flex_1()
            .w_full()
            .flex()
            .flex_wrap()
            .justify_center()
            .gap_2()
            .max_w(px(row_width))
            .px(px(24.0))
            .pb(px(56.0))
            .overflow_y_scroll()
            .track_scroll(&self.results_scroll_handle);
        for item in visible {
            let selected = self.extension_button_focus.to_string() == item.id;
            let content = match &item.content {
                corvo_core::GridContent::Glyph(glyph) => div()
                    .text_size(px(38.0))
                    .child(glyph.clone()),
                corvo_core::GridContent::Color(color) => div()
                    .size(px(44.0))
                    .rounded_lg()
                    .bg(rgb(*color)),
                corvo_core::GridContent::Image(path) => div()
                    .size(px(44.0))
                    .child(img(path.clone()).size(px(44.0))),
                corvo_core::GridContent::Text(text) => div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::BOLD)
                    .child(text.clone()),
            };
            let item_id = item.id.clone();
            grid_view = grid_view.child(
                div()
                    .id(SharedString::from(format!("grid-{}", item.id)))
                    .w(px(tile_size))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .when(selected, |tile| tile.bg(rgb(COLOR_ROW_SELECTED)))
                    .when(!selected, |tile| tile.hover(|s| s.bg(rgb(0x181b1e))))
                    .child(
                        div()
                            .size(px(48.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(content),
                    )
                    .child(
                        div()
                            .max_w_full()
                            .text_size(px(10.5))
                            .text_color(rgb(COLOR_TEXT))
                            .whitespace_nowrap()
                            .child(item.title.clone()),
                    )
                    .on_click(cx.listener(move |launcher, _: &ClickEvent, _, cx| {
                        launcher.run_extension_action(
                            command_id,
                            format!("grid:{item_id}"),
                            cx,
                        );
                    })),
            );
        }
        grid_view
    }

    fn form_page_view(&mut self, view: &corvo_core::PageView, cx: &mut Context<Self>) -> Stateful<Div> {
        let corvo_core::PageView::Form { title, fields, submit, .. } = view else {
            return div().id("extension-form-fallback");
        };
        let command_id = match self.page {
            LauncherPage::Extension(id) => id,
            _ => return div().id("extension-form-fallback"),
        };
        let mut column = div()
            .id("extension-form")
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .px(px(32.0))
            .overflow_y_scroll()
            .track_scroll(&self.extension_scroll_handle)
            .pb(px(56.0));
        column = column.child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::BOLD)
                .child(title.clone()),
        );
        let mut text_index = 0usize;
        for (index, field) in fields.iter().enumerate() {
            let focused = index == self.extension_form.focused;
            match field {
                corvo_core::FormField::Text {
                    id,
                    title: field_title,
                    placeholder,
                    password,
                    ..
                } => {
                    let value = self
                        .extension_form
                        .text_values
                        .get(text_index)
                        .cloned()
                        .unwrap_or_default();
                    let caret = self.extension_form.text_carets.get(text_index).copied().unwrap_or(0);
                    let displayed = if *password {
                        "*".repeat(value.chars().count())
                    } else {
                        value.clone()
                    };
                    let (before, after) = {
                        let chars: Vec<char> = displayed.chars().collect();
                        let cut = caret.min(chars.len());
                        (
                            chars[..cut].iter().collect::<String>(),
                            chars[cut..].iter().collect::<String>(),
                        )
                    };
                    let field_title = field_title.clone();
                    let placeholder_text = *placeholder;
                    let is_password = *password;
                    let field_index = index;
                    column = column.child(
                        div()
                            .id(SharedString::from(format!("form-field-{id}")))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .on_click(cx.listener(move |launcher, _: &ClickEvent, _, cx| {
                                launcher.extension_form.focused = field_index;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child(field_title),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(30.0))
                                    .px_2p5()
                                    .rounded_lg()
                                    .when(focused, |tile| tile.border_1().border_color(rgb(COLOR_ACCENT)))
                                    .when(!focused, |tile| tile.bg(rgb(0x181b1e)))
                                    .child(div().text_size(px(13.0)).child(before))
                                    .when(focused && self.cursor_visible, |row| {
                                        row.child(
                                            div()
                                                .w(px(1.5))
                                                .h(px(16.0))
                                                .bg(rgb(COLOR_TEXT))
                                                .mx(px(0.5)),
                                        )
                                    })
                                    .child(div().text_size(px(13.0)).child(after))
                                    .when(value.is_empty(), |row| {
                                        row.child(
                                            div()
                                                .text_size(px(13.0))
                                                .text_color(rgb(COLOR_TEXT_DIM))
                                                .child(placeholder_text),
                                        )
                                    }),
                            ),
                    );
                    text_index += 1;
                    let _ = is_password;
                }
                corvo_core::FormField::Checkbox { id, title: field_title, .. } => {
                    let checked = self.extension_form.checks.get(index).copied().unwrap_or(false);
                    let field_title = field_title.clone();
                    let field_index = index;
                    column = column.child(
                        div()
                            .id(SharedString::from(format!("form-field-{id}")))
                            .flex()
                            .items_center()
                            .gap_2()
                            .on_click(cx.listener(move |launcher, _: &ClickEvent, _, cx| {
                                if let Some(check) = launcher.extension_form.checks.get_mut(field_index) {
                                    *check = !*check;
                                }
                                launcher.extension_form.focused = field_index;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .size(px(16.0))
                                    .rounded_sm()
                                    .when(checked, |box_view| box_view.bg(rgb(COLOR_ACCENT)))
                                    .when(!checked, |box_view| box_view.bg(rgb(0x181b1e)).border_1().border_color(rgb(0x2a2f36)))
                                    .child(if checked { div().text_size(px(11.0)).child("✓") } else { div() }),
                            )
                            .child(div().text_size(px(12.5)).child(field_title)),
                    );
                }
                corvo_core::FormField::Select { id, title: field_title, options, .. } => {
                    let selected = self.extension_form.selections.get(index).copied().unwrap_or(0);
                    let field_title = field_title.clone();
                    let field_index = index;
                    let mut select_view = div()
                        .id(SharedString::from(format!("form-field-{id}")))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(field_title),
                        );
                    for (option_index, (_, option_title)) in options.iter().enumerate() {
                        let option_title = option_title.clone();
                        select_view = select_view.child(
                            div()
                                .id(SharedString::from(format!("form-option-{id}-{option_index}")))
                                .px_2p5()
                                .py_1()
                                .rounded_lg()
                                .when(option_index == selected, |tile| tile.bg(rgb(COLOR_ROW_SELECTED)))
                                .when(option_index != selected, |tile| tile.hover(|s| s.bg(rgb(0x181b1e))))
                                .text_size(px(12.5))
                                .child(option_title)
                                .on_click(cx.listener(move |launcher, _: &ClickEvent, _, cx| {
                                    if let Some(selection) = launcher.extension_form.selections.get_mut(field_index) {
                                        *selection = option_index;
                                    }
                                    launcher.extension_form.focused = field_index;
                                    cx.notify();
                                })),
                        );
                    }
                    column = column.child(select_view);
                }
            }
        }
        let encoded = self.encode_form_values(fields);
        column = column.child(
            div()
                .id("form-submit")
                .self_start()
                .px_4()
                .py_1p5()
                .rounded_lg()
                .bg(rgb(COLOR_ACCENT))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .child(submit.clone())
                .on_click(cx.listener(move |launcher, _: &ClickEvent, _, cx| {
                    launcher.run_extension_action(
                        command_id,
                        format!("form:{encoded}"),
                        cx,
                    );
                })),
        );
        column
    }

    /// Routes a key into the focused form field's text caret. Returns
    /// false when the key is not a form-editing key, so the default
    /// query path takes it.
    fn extension_form_edit(&mut self, key: &str, typed: Option<&str>) -> bool {
        let Some(corvo_core::PageView::Form { fields, .. }) = self.extension_view.as_ref() else {
            return false;
        };
        let focused = self.extension_form.focused;
        if fields.get(focused).map(corvo_core::FormField::id).is_none() {
            return false;
        }
        let is_text_field = matches!(
            fields.get(focused),
            Some(corvo_core::FormField::Text { .. })
        );
        if !is_text_field {
            return false;
        }
        let Some(value_slot) = self.extension_form.text_values.get_mut(focused) else {
            return false;
        };
        let Some(caret_slot) = self.extension_form.text_carets.get_mut(focused) else {
            return false;
        };
        let caret = *caret_slot;
        match (key, typed) {
            ("backspace", _) => {
                if caret > 0 {
                    if let Some((byte_start, ch)) = value_slot.char_indices().nth(caret - 1) {
                        let byte_end = byte_start + ch.len_utf8();
                        value_slot.drain(byte_start..byte_end);
                        *caret_slot = caret - 1;
                    }
                }
                true
            }
            ("delete", _) => {
                let total = value_slot.chars().count();
                if caret < total {
                    if let Some((byte_start, ch)) = value_slot.char_indices().nth(caret) {
                        let byte_end = byte_start + ch.len_utf8();
                        value_slot.drain(byte_start..byte_end);
                    }
                }
                true
            }
            ("left", _) => {
                *caret_slot = caret.saturating_sub(1);
                true
            }
            ("right", _) => {
                *caret_slot = (caret + 1).min(value_slot.chars().count());
                true
            }
            (_, Some(text)) if text.chars().count() == 1 && !text.contains('\n') => {
                let byte_idx = value_slot
                    .char_indices()
                    .nth(caret)
                    .map(|(i, _)| i)
                    .unwrap_or(value_slot.len());
                value_slot.insert_str(byte_idx, text);
                *caret_slot = caret + text.chars().count();
                true
            }
            _ => false,
        }
    }

    /// Encodes the current form values as `field=value&...` with
    /// percent-escaping, the payload of the submit id.
    fn encode_form_values(&self, fields: &[corvo_core::FormField]) -> String {        let mut text_index = 0usize;
        let mut parts = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            match field {
                corvo_core::FormField::Text { id, .. } => {
                    let value = self.extension_form.text_values.get(text_index).cloned().unwrap_or_default();
                    text_index += 1;
                    parts.push(format!("{id}={}", encode_form_value(&value)));
                }
                corvo_core::FormField::Checkbox { id, .. } => {
                    let checked = self.extension_form.checks.get(index).copied().unwrap_or(false);
                    parts.push(format!("{id}={checked}"));
                }
                corvo_core::FormField::Select { id, options, .. } => {
                    let selected = self.extension_form.selections.get(index).copied().unwrap_or(0);
                    let value = options
                        .get(selected)
                        .map(|(value, _)| value.clone())
                        .unwrap_or_default();
                    parts.push(format!("{id}={}", encode_form_value(&value)));
                }
            }
        }
        parts.join("&")
    }

    /// Runs one page action through the owning command's `execute`,
    /// then refreshes the page. Every interactive element on a
    /// declarative page funnels here.
    fn run_extension_action(&mut self, command_id: &'static str, action: String, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_id)
            .cloned()
        else {
            return;
        };
        let result_id = format!("{command_id}:page:{action}");
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let outcome = command
                .execute(&result_id, &ExecutionContext { store: Some(store.clone()) })
                .await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.page == LauncherPage::Extension(command_id) {
                    if let Ok(corvo_core::Action::ShowToast(message)) = &outcome {
                        // An empty message means "act silently"; the
                        // toast-prefix sentinels belong to `perform`,
                        // which page actions deliberately bypass.
                        if !message.is_empty() && !message.contains(':') {
                            show_action_toast(
                                ToastNotice::success(
                                    ToastCategory::General,
                                    command_id.replace('-', " "),
                                    message.clone(),
                                ),
                                cx,
                            );
                        }
                    }
                    launcher.refresh_extension(command_id, false, cx);
                }
            });
        })
        .detach();
    }



    fn notes_subheader(&self) -> Div {
        div()
            .flex_none()
            .px_4()
            .pb_1()
            .text_size(px(12.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(COLOR_TEXT_DIM))
            .child("NOTES")
    }

    fn browser_subheader(&self, browser: corvo_browser_tabs::BrowserId) -> Div {
        div()
            .flex_none()
            .px_4()
            .pb_1()
            .text_size(px(12.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(COLOR_TEXT_DIM))
            .child(SharedString::from(browser.spec().name.to_uppercase()))
    }

    fn refresh_ports(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == "kill-process")
            .cloned()
        else {
            self.results.clear();
            self.rebuild_root_flat_items();
            cx.notify();
            return;
        };
        let filter = self.query.clone();
        let store = self.store.clone();
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 200,
                store: Some(store),
            };
            let results = command
                .search(&format!("kill-process-page:ports:{filter}"), &ctx)
                .await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Ports {
                    launcher.results = results;
                    launcher.rebuild_root_flat_items();
                    launcher.selected = launcher
                        .selected
                        .min(launcher.results.len().saturating_sub(1));
                    launcher.results_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_processes(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == "kill-process")
            .cloned()
        else {
            self.results.clear();
            self.rebuild_root_flat_items();
            cx.notify();
            return;
        };
        let filter = self.query.clone();
        let store = self.store.clone();
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext {
                max_results: 200,
                store: Some(store),
            };
            let results = command
                .search(&format!("kill-process-page:processes:{filter}"), &ctx)
                .await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Processes {
                    launcher.results = results;
                    launcher.rebuild_root_flat_items();
                    launcher.selected = launcher
                        .selected
                        .min(launcher.results.len().saturating_sub(1));
                    launcher.results_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Opens the actions menu for the selected result, or closes it.
    fn toggle_actions(&mut self, cx: &mut Context<Self>) {
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        if self.actions_open {
            self.close_actions(cx);
            return;
        }
        if self.page == LauncherPage::Uninstaller {
            let UninstallerState::Ready(ready) = &self.uninstaller else {
                return;
            };
            let focused = ready.files.get(ready.focused);
            let selected = ready
                .selected_files
                .get(ready.focused)
                .copied()
                .unwrap_or(false);
            let target_name = ready.target.name.clone();
            let mut actions = Vec::new();
            if focused.is_some() {
                actions.extend([
                    CommandAction {
                        id: "uninstaller:toggle-file".into(),
                        label: if selected {
                            "Unselect File"
                        } else {
                            "Select File"
                        }
                        .into(),
                        action: Action::CloseWindow,
                        icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::CIRCLE),
                        group: corvo_core::ActionGroup::Standard,
                        hotkey: Some("cmd+enter"),
                    },
                    CommandAction {
                        id: "uninstaller:copy-path".into(),
                        label: "Copy Path".into(),
                        action: Action::CloseWindow,
                        icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                        group: corvo_core::ActionGroup::Standard,
                        hotkey: Some("alt+cmd+c"),
                    },
                    CommandAction {
                        id: "uninstaller:show-in-finder".into(),
                        label: file_manager_label("Show in"),
                        action: Action::CloseWindow,
                        icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::FOLDER),
                        group: corvo_core::ActionGroup::Standard,
                        hotkey: Some("shift+cmd+o"),
                    },
                    CommandAction {
                        id: "uninstaller:show-info".into(),
                        label: file_manager_label("Show Info in"),
                        action: Action::CloseWindow,
                        icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::INFO),
                        group: corvo_core::ActionGroup::Standard,
                        hotkey: Some("shift+cmd+i"),
                    },
                ]);
            }
            self.actions = actions;
            self.actions_title = target_name;
            self.actions_filter.clear();
            self.action_selected = 0;
            self.actions_open = true;
            cx.notify();
            return;
        }
        let (result_id, title, command_id) = match self.selected_result() {
            Some(result) => (
                result.id.clone(),
                result.title.clone(),
                result.id.split(':').next().unwrap_or_default().to_string(),
            ),
            None => return,
        };
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_id)
            .cloned()
        else {
            return;
        };
        let mut actions = command.actions(&result_id);
        let settings = corvo_config::Settings::load();
        let config = settings
            .result_item(&result_id, &title)
            .cloned()
            .unwrap_or_default();
        let favorite = settings.result_is_favorite(&result_id);
        actions.push(CommandAction {
            id: "launcher:favorite".into(),
            label: if favorite {
                "Remove from Favorites".into()
            } else {
                "Add to Favorites".into()
            },
            action: Action::SetResultFavorite {
                result_id: result_id.clone(),
                title: title.clone(),
                favorite: !favorite,
            },
            icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::STAR),
            group: corvo_core::ActionGroup::Standard,
            hotkey: Some("shift+cmd+f"),
        });
        actions.push(CommandAction {
            id: "launcher:hide".into(),
            label: if config.hidden {
                "Show in Search".into()
            } else {
                "Hide from Search".into()
            },
            action: Action::SetResultHidden {
                result_id: result_id.clone(),
                title: title.clone(),
                hidden: !config.hidden,
            },
            icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::EYE_SLASH),
            group: corvo_core::ActionGroup::Standard,
            hotkey: Some("shift+cmd+h"),
        });
        actions.sort_by_key(|action| match action.group {
            corvo_core::ActionGroup::Primary => 0,
            corvo_core::ActionGroup::Standard => 1,
            corvo_core::ActionGroup::Destructive => 2,
        });
        let target = self.previous_app_name.as_deref().unwrap_or("Active App");
        for act in &mut actions {
            if act.label == "Paste to Active App" {
                act.label = format!("Paste to {target}");
            }
        }
        if actions.is_empty() {
            return;
        }
        self.actions = actions;
        self.actions_title = title;
        self.actions_filter = String::new();
        self.action_selected = 0;
        self.actions_open = true;
        cx.notify();
    }

    fn close_actions(&mut self, cx: &mut Context<Self>) {
        self.actions_open = false;
        self.actions = Vec::new();
        self.actions_filter = String::new();
        self.action_selected = 0;
        cx.notify();
    }

    fn toggle_burger_menu(&mut self, cx: &mut Context<Self>) {
        if self.burger_menu_open {
            self.burger_menu_open = false;
        } else {
            self.actions_open = false;
            self.filter_dropdown_open = false;
            self.burger_menu_open = true;
        }
        cx.notify();
    }

    fn close_burger_menu(&mut self, cx: &mut Context<Self>) {
        self.burger_menu_open = false;
        cx.notify();
    }

    fn filter_dropdown_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let (items, active_idx): (&[&str], usize) = match self.page {
            LauncherPage::Emoji => (EMOJI_CATEGORIES, self.emoji_category_index),
            LauncherPage::Clipboard => (CLIPBOARD_FILTERS, self.clipboard_filter_index),
            _ => (&[], 0),
        };

        div()
            .id("filter-dropdown-menu")
            .occlude()
            .absolute()
            .top(px(46.0))
            .right(px(14.0))
            .w(px(210.0))
            .max_h(px(320.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(rgb(COLOR_DIVIDER))
            .bg(rgba(COLOR_MENU))
            .p_1()
            .shadow_xl()
            .children(items.iter().enumerate().map(|(idx, &label)| {
                let is_selected = idx == self.filter_dropdown_selected;
                let is_active = idx == active_idx;
                div()
                    .id(SharedString::from(format!("filter-item-{idx}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .h(px(32.0))
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_selected, |style| style.bg(rgb(COLOR_ROW_SELECTED)))
                    .hover(|style| style.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(move |launcher, _: &ClickEvent, _window, cx| {
                        launcher.apply_filter_selection(idx, cx);
                    }))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(if is_active {
                                rgb(COLOR_ACCENT)
                            } else {
                                rgb(COLOR_TEXT)
                            })
                            .font_weight(if is_active {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(label),
                    )
                    .when(is_active, |row| {
                        row.child(icons::render_phosphor_svg(
                            phosphor_svgs::style::regular::CHECK,
                            rgb(COLOR_ACCENT),
                            14.0,
                        ))
                    })
            }))
    }

    fn burger_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        // Shortcuts are written once in macOS terms and translated for the
        // running platform when drawn, so Windows and Linux read `Ctrl+,`
        // and `Ctrl+Q` instead of a command glyph.
        let items: [(&str, Option<&str>); 4] = [
            ("Preferences...", Some("cmd+,")),
            ("About Corvo", None),
            ("Check for Updates...", None),
            ("Quit Corvo", Some("cmd+q")),
        ];
        div()
            .id("burger-menu")
            .occlude()
            .absolute()
            .left(px(12.0))
            .bottom(px(10.0))
            .w(px(200.0))
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(rgb(COLOR_DIVIDER))
            .bg(rgba(COLOR_MENU))
            .p_1()
            .children(items.iter().enumerate().map(|(idx, (label, hotkey))| {
                let label_str = label.to_string();
                let is_quit = *label == "Quit Corvo";
                let is_preferences = *label == "Preferences...";
                let is_about = *label == "About Corvo";
                let is_check_updates = *label == "Check for Updates...";
                div()
                    .id(SharedString::from(format!("burger-item-{idx}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                        launcher.close_burger_menu(cx);
                        if is_preferences {
                            #[cfg(target_os = "windows")]
                            launcher.dismiss(window);
                            open_settings(cx);
                        } else if is_about {
                            #[cfg(target_os = "windows")]
                            launcher.dismiss(window);
                            open_settings_tab(SettingsTab::About, cx);
                        } else if is_check_updates {
                            #[cfg(target_os = "windows")]
                            launcher.dismiss(window);
                            open_settings_tab_with_update_check(SettingsTab::About, cx);
                        } else if is_quit {
                            launcher.dismiss(window);
                            std::process::exit(0);
                        }
                    }))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .when(is_quit, |l| l.text_color(rgb(COLOR_DESTRUCTIVE)))
                            .child(label_str),
                    )
                    .when_some(*hotkey, |row, hotkey| {
                        row.child(div().flex().gap_1().children(shortcut_keycaps(hotkey)))
                    })
            }))
    }

    /// Indices into `actions` after the menu filter, in menu order.
    fn filtered_actions(&self) -> Vec<usize> {
        let filter = self.actions_filter.to_lowercase();
        self.actions
            .iter()
            .enumerate()
            .filter(|(_, entry)| filter.is_empty() || entry.label.to_lowercase().contains(&filter))
            .map(|(index, _)| index)
            .collect()
    }

    fn selected_action(&self) -> Option<&CommandAction> {
        self.filtered_actions()
            .get(self.action_selected)
            .and_then(|index| self.actions.get(*index))
    }

    fn run_selected_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filtered = self.filtered_actions();
        let Some(position) = filtered.get(self.action_selected) else {
            return;
        };
        let Some(entry) = self.actions.get(*position) else {
            return;
        };
        let entry_id = entry.id.clone();
        if entry_id.starts_with("uninstaller:") {
            self.actions_open = false;
            self.actions.clear();
            self.actions_filter.clear();
            match entry_id.as_str() {
                "uninstaller:toggle-file" => self.toggle_uninstaller_focus(cx),
                "uninstaller:copy-path" => {
                    let path = match &self.uninstaller {
                        UninstallerState::Ready(ready) => ready
                            .files
                            .get(ready.focused)
                            .map(|file| file.path.display().to_string()),
                        _ => None,
                    };
                    if let Some(path) = path {
                        let result = corvo_platform::platform_ops()
                            .copy_text(&path)
                            .map_err(|error| error.to_string());
                        let notice = match result {
                            Ok(()) => ToastNotice::success(
                                ToastCategory::Uninstallation,
                                "Path copied",
                                path,
                            ),
                            Err(error) => ToastNotice::failure(
                                ToastCategory::Uninstallation,
                                "Could not copy path",
                                error,
                            ),
                        };
                        show_action_toast(notice, cx);
                    }
                    cx.notify();
                }
                "uninstaller:show-in-finder" | "uninstaller:show-info" => {
                    let path = match &self.uninstaller {
                        UninstallerState::Ready(ready) => {
                            ready.files.get(ready.focused).map(|file| file.path.clone())
                        }
                        _ => None,
                    };
                    if let Some(path) = path {
                        let show_in_finder = entry_id == "uninstaller:show-in-finder";
                        let result = if show_in_finder {
                            std::process::Command::new("open")
                                .arg("-R")
                                .arg(&path)
                                .status()
                        } else {
                            std::process::Command::new("osascript")
                                .arg("-e")
                                .arg("on run argv\n tell application \"Finder\" to open information window of (POSIX file (item 1 of argv))\nend run")
                                .arg(path.as_os_str())
                                .status()
                        };
                        let notice = match result {
                            Ok(status) if status.success() => ToastNotice::success(
                                ToastCategory::Uninstallation,
                                if show_in_finder {
                                    "Showing in Finder"
                                } else {
                                    "Showing file info"
                                },
                                path.file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("Selected file"),
                            ),
                            Ok(status) => ToastNotice::failure(
                                ToastCategory::Uninstallation,
                                "Could not open Finder action",
                                format!("Exit code {}", status.code().unwrap_or(-1)),
                            ),
                            Err(error) => ToastNotice::failure(
                                ToastCategory::Uninstallation,
                                "Could not open Finder action",
                                error.to_string(),
                            ),
                        };
                        show_action_toast(notice, cx);
                    }
                    cx.notify();
                }
                _ => {}
            }
            return;
        }
        let action = Ok(entry.action.clone());
        let is_repeatable = entry_id.contains("volume-up")
            || entry_id.contains("volume-down")
            || entry_id.contains("brightness-up")
            || entry_id.contains("brightness-down")
            || matches!(
                entry.action,
                Action::AdjustBrightness(_) | Action::AdjustVolume(_)
            );
        if !is_repeatable {
            self.actions_open = false;
            self.actions = Vec::new();
            self.actions_filter = String::new();
        }
        if let Some(result) = self.selected_result() {
            let query_str = if self.query.is_empty() {
                None
            } else {
                Some(self.query.as_str())
            };
            if let Ok(mut store) = corvo_config::ranking::FrecencyStore::global().lock() {
                store.record_visit(&result.id, query_str);
            }
        }
        self.perform(action, window, cx);
        // Notes refresh after a create or delete mutates the list.
        if self.page == LauncherPage::Notes
            && self.selected_result().is_some_and(|result| {
                result.id.starts_with("notes:create:") || result.id.starts_with("notes:delete:")
            })
        {
            self.refresh_notes(cx);
        }
    }

    fn execute_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == LauncherPage::Uninstaller {
            self.begin_uninstall(cx);
            return;
        }
        if self.page == LauncherPage::Emoji {
            self.execute_selected_emoji(window, cx);
            return;
        }
        if self.page == LauncherPage::Clipboard {
            self.execute_selected_clipboard(window, cx);
            return;
        }
        if self.page == LauncherPage::Files {
            if let Some(path) = self
                .selected_result()
                .and_then(|result| result.id.strip_prefix("file-search:"))
                .filter(|path| *path != "open")
                .map(std::path::PathBuf::from)
            {
                self.perform(Ok(Action::Open(path)), window, cx);
            }
            return;
        }
        let selected_opt = self.selected_result().cloned().or_else(|| {
            if self.is_compact_collapsed() {
                let mut initial = cached_initial_results();
                let settings = corvo_config::Settings::load();
                apply_result_preferences(&mut initial, &settings, "");
                initial.into_iter().next()
            } else {
                None
            }
        });
        let Some(result) = selected_opt else {
            return;
        };
        let query_str = if self.query.is_empty() {
            None
        } else {
            Some(self.query.as_str())
        };
        if let Ok(mut store) = corvo_config::ranking::FrecencyStore::global().lock() {
            store.record_visit(&result.id, query_str);
        }
        if result.id == "emoji-picker:open" {
            self.open_emoji_page(window, cx);
            return;
        }
        if result.id == "clipboard-manager:open" {
            self.open_clipboard_page(window, cx);
            return;
        }
        if result.id == "kill-process:open-ports" {
            self.open_ports_page(window, cx);
            return;
        }
        if let Some(input) = result.id.strip_prefix("text-utilities:open") {
            let input = input.strip_prefix(':').unwrap_or_default();
            self.open_text_page(input, window, cx);
            return;
        }
        if result.id == "pomodoro:open" {
            self.open_pomodoro_page(window, cx);
            return;
        }
        if let Some(city) = result.id.strip_prefix("weather:open") {
            let city = city.strip_prefix(':').unwrap_or_default();
            self.open_weather_page(city, window, cx);
            return;
        }
        if result.id == "notes:open" {
            self.open_notes_page(window, cx);
            return;
        }
        // Notes open in corvo's own editor window, not the system's.
        if let Some(text) = result.id.strip_prefix("notes:create:") {
            self.dismiss(window);
            note_editor::open_note_editor(None, Some(text.to_owned()), cx);
            return;
        }
        if let Some(id) = result
            .id
            .strip_prefix("notes:")
            .filter(|id| !id.is_empty() && !id.contains(':'))
        {
            if let Some(note) = corvo_notes::list_notes()
                .into_iter()
                .find(|note| note.id == id)
            {
                self.dismiss(window);
                note_editor::open_note_editor(Some(note), None, cx);
                return;
            }
        }
        if result.id == "media:open" {
            self.open_media_page(window, cx);
            return;
        }
        // Every browser extension opens its own page.
        for browser in corvo_browser_tabs::BrowserId::ALL {
            if result.id == format!("{}:open", browser.spec().id) {
                self.open_browser_page(browser, window, cx);
                return;
            }
        }
        if let Some(filter) = result.id.strip_prefix("kill-process:open-processes") {
            let filter = filter.strip_prefix(':').unwrap_or_default();
            self.open_processes_page(filter, window, cx);
            return;
        }
        match result.id.as_str() {
            "brew:show-installed" => {
                self.open_brew_page(BrewPageMode::Installed, window, cx);
                return;
            }
            "brew:show-upgrades" => {
                self.open_brew_page(BrewPageMode::Upgrades, window, cx);
                return;
            }
            "brew:manage-services" => {
                self.open_brew_page(BrewPageMode::Services, window, cx);
                return;
            }
            "brew:search" => {
                self.open_brew_page(BrewPageMode::Search, window, cx);
                return;
            }
            _ => {}
        }
        if result.id == "system-actions:check-for-updates" || result.id == "check-for-updates" {
            self.dismiss(window);
            open_settings_tab_with_update_check(SettingsTab::About, cx);
            return;
        }
        // Generic extension pages: a command whose manifest declares a
        // View command opens its declarative page from
        // `{id}:open[:{args}]`. Special cases above keep priority.
        if let Some((prefix, args)) = result.id.split_once(":open") {
            if args.is_empty() || args.starts_with(':') {
                let candidate = prefix.to_owned();
                if let Some(command) = self
                    .registry
                    .commands()
                    .iter()
                    .find(|command| command.id() == candidate)
                    .cloned()
                {
                    let declares_view = command
                        .manifest()
                        .commands
                        .iter()
                        .any(|spec| spec.mode == corvo_core::CommandMode::View);
                    if declares_view {
                        let command_id = command.id();
                        let argument = args.strip_prefix(':').unwrap_or_default();
                        self.open_extension_page(command_id, argument, window, cx);
                        return;
                    }
                }
            }
        }
        let Some(command_id) = result.id.split(':').next() else {
            return;
        };
        let Some(command) = self
            .registry
            .commands()
            .iter()
            .find(|command| command.id() == command_id)
            .cloned()
        else {
            return;
        };
        let result_id = result.id.clone();
        let ctx = ExecutionContext {
            store: Some(self.store.clone()),
        };
        let actions = command.actions(&result_id);
        if let Some(primary) = actions
            .iter()
            .find(|a| a.group == ActionGroup::Primary)
            .or_else(|| actions.first())
        {
            if command_id == "app-launcher" {
                corvo_app_launcher::record_launch(&result_id);
            }
            let primary_action = primary.action.clone();
            self.perform(Ok(primary_action), window, cx);
            return;
        }

        cx.spawn_in(window, async move |this, cx| {
            let action = command.execute(&result_id, &ctx).await;
            let _ = this.update_in(cx, |launcher, window, cx| {
                launcher.perform(action, window, cx);
            });
        })
        .detach();
    }

    fn execute_selected_emoji(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(result) = self.results.get(self.selected) else {
            return;
        };
        let raw_glyph = result
            .id
            .strip_prefix("emoji-picker:")
            .unwrap_or(&result.title);
        let glyph = apply_emoji_skin_tone(raw_glyph, self.emoji_skin_tone);
        let action = Ok(Action::PasteText(glyph));
        self.perform(action, window, cx);
    }

    fn copy_selected_emoji(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(result) = self.results.get(self.selected) else {
            return;
        };
        let raw_glyph = result
            .id
            .strip_prefix("emoji-picker:")
            .unwrap_or(&result.title);
        let glyph = apply_emoji_skin_tone(raw_glyph, self.emoji_skin_tone);
        let action = Ok(Action::Copy(glyph));
        self.perform(action, window, cx);
    }

    fn execute_selected_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(result) = self.selected_result() else {
            return;
        };
        let Some(id) = result.id.strip_prefix("clipboard-manager:entry:") else {
            return;
        };
        let Some(entry) = corvo_clipboard_manager::get_entry(id) else {
            return;
        };
        let action = if let Some(path) = entry.image_path() {
            Ok(Action::PasteImage(path))
        } else {
            Ok(Action::PasteText(entry.text))
        };
        self.perform(action, window, cx);
    }

    fn copy_selected_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(result) = self.selected_result() else {
            return;
        };
        let Some(id) = result.id.strip_prefix("clipboard-manager:entry:") else {
            return;
        };
        let Some(entry) = corvo_clipboard_manager::get_entry(id) else {
            return;
        };
        let action = if let Some(path) = entry.image_path() {
            Ok(Action::CopyImage(path))
        } else {
            Ok(Action::Copy(entry.text))
        };
        self.perform(action, window, cx);
    }

    fn perform(
        &mut self,
        action: Result<Action, CommandError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ops = corvo_platform::platform_ops();
        match action {
            Ok(Action::Open(path)) => {
                let category = self
                    .selected_result()
                    .map(|result| category_for_result(&result.id))
                    .unwrap_or(ToastCategory::General);
                let item_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("item")
                    .to_string();
                // dismiss() takes previous_app and hands activation back
                // to it, which is wrong here: the app we are about to open
                // must get the focus instead.
                self.previous_app = None;
                self.dismiss(window);
                // open_path reaches the OS shell. Keep it off the main
                // thread so a slow or hung shell call cannot freeze
                // rendering, and so the panel is already gone by the time
                // the toast needs a window.
                cx.spawn(async move |_this, cx| {
                    let result = smol::unblock(move || ops.open_path(&path)).await;
                    cx.update(|cx| {
                        if let Err(error) = result {
                            show_action_toast(
                                ToastNotice::failure(
                                    category,
                                    "Could not open item",
                                    format!("{item_name} · {error}"),
                                ),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            Ok(Action::OpenAppUninstaller { name, path }) => {
                self.open_uninstaller_page(name, path, window, cx);
            }
            Ok(Action::OpenFileSearch) => self.open_files_page(window, cx),
            Ok(Action::SetResultFavorite {
                result_id,
                title,
                favorite,
            }) => {
                let saved = self.save_result_flag(&result_id, &title, None, Some(favorite));
                let notice = if saved {
                    ToastNotice::success(
                        ToastCategory::General,
                        if favorite { "Pinned" } else { "Unpinned" },
                        title,
                    )
                } else {
                    ToastNotice::failure(ToastCategory::General, "Could not update favorite", title)
                };
                show_action_toast(notice, cx);
                self.refresh(cx);
            }
            Ok(Action::SetResultHidden {
                result_id,
                title,
                hidden,
            }) => {
                let saved = self.save_result_flag(&result_id, &title, Some(hidden), None);
                let notice = if saved {
                    ToastNotice::success(
                        ToastCategory::General,
                        if hidden { "Hidden" } else { "Shown" },
                        title,
                    )
                } else {
                    ToastNotice::failure(
                        ToastCategory::General,
                        "Could not update visibility",
                        title,
                    )
                };
                show_action_toast(notice, cx);
                self.refresh(cx);
            }
            Ok(Action::OpenUrl(url)) => {
                let (category, action_title, is_system_setting) = self
                    .selected_result()
                    .map(|result| {
                        (
                            category_for_result(&result.id),
                            result.title.clone(),
                            result.id.starts_with("system-actions:"),
                        )
                    })
                    .unwrap_or((ToastCategory::General, "Link".into(), false));
                self.previous_app = None;
                corvo_platform::forget_launcher_panel();
                window.remove_window();
                match corvo_platform::open_url(&url) {
                    Ok(()) if is_system_setting => show_action_toast(
                        ToastNotice::success(
                            ToastCategory::System,
                            "Opening setting",
                            action_title,
                        ),
                        cx,
                    ),
                    Err(error) => show_action_toast(
                        ToastNotice::failure(category, "Could not open link", error.to_string()),
                        cx,
                    ),
                    _ => {}
                }
            }
            Ok(Action::RunShell(cmd)) => {
                let (category, action_title, action_id) = self
                    .selected_result()
                    .map(|result| {
                        (
                            category_for_result(&result.id),
                            result.title.clone(),
                            result.id.clone(),
                        )
                    })
                    .unwrap_or((ToastCategory::General, "Action".into(), String::new()));
                let volume_direction = match action_id.as_str() {
                    "system-actions:action:volume-up" => Some(true),
                    "system-actions:action:volume-down" => Some(false),
                    _ => None,
                };
                let is_repeatable = self.selected_result().is_some_and(|r| {
                    r.id.contains("volume-up")
                        || r.id.contains("volume-down")
                        || r.id.contains("brightness-up")
                        || r.id.contains("brightness-down")
                }) || cmd.contains("DisplayServices")
                    || cmd.contains("brightness")
                    || cmd.contains("volume settings")
                    || cmd.contains("wpctl set-volume");

                if is_repeatable {
                    cx.spawn(async move |_this, cx| {
                        let notice =
                            run_shell_toast(cmd, category, action_title, volume_direction).await;
                        cx.update(|cx| show_action_toast(notice, cx));
                    })
                    .detach();
                    cx.notify();
                } else {
                    self.previous_app = None;
                    corvo_platform::forget_launcher_panel();
                    window.remove_window();
                    cx.spawn(async move |_this, cx| {
                        let notice =
                            run_shell_toast(cmd, category, action_title, volume_direction).await;
                        cx.update(|cx| show_action_toast(notice, cx));
                    })
                    .detach();
                }
            }
            Ok(Action::RunNative(native)) => {
                let (category, action_title) = self
                    .selected_result()
                    .map(|result| (category_for_result(&result.id), result.title.clone()))
                    .unwrap_or((ToastCategory::System, "System action".into()));
                self.previous_app = None;
                corvo_platform::forget_launcher_panel();
                self.dismiss(window);
                // Some of these walk the device tree or the window list,
                // so keep them off the main thread.
                cx.spawn(async move |_this, cx| {
                    let result =
                        smol::unblock(move || corvo_platform::run_native_action(native)).await;
                    let notice = match result {
                        Ok(detail) => ToastNotice::success(category, action_title, detail),
                        Err(error) => {
                            ToastNotice::failure(category, action_title, error.to_string())
                        }
                    };
                    cx.update(|cx| show_action_toast(notice, cx));
                })
                .detach();
            }
            Ok(Action::ConfirmProcessTermination { pid, start_time }) => {
                self.actions_title = format!("Confirm force quit for PID {pid}");
                self.actions_filter.clear();
                self.action_selected = 0;
                self.actions = vec![CommandAction {
                    id: "kill-process:confirm-force-terminate".into(),
                    label: format!("Force Quit Process {pid}"),
                    action: Action::TerminateProcess {
                        pid,
                        start_time,
                        force: true,
                    },
                    icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::WARNING),
                    group: ActionGroup::Destructive,
                    hotkey: Some("enter"),
                }];
                self.actions_open = true;
                cx.notify();
            }
            Ok(Action::TerminateProcess {
                pid,
                start_time,
                force,
            }) => {
                cx.spawn(async move |_this, cx| {
                    let identity = corvo_platform::ProcessIdentity { pid, start_time };
                    let mode = if force {
                        corvo_platform::TerminationMode::Force
                    } else {
                        corvo_platform::TerminationMode::Graceful
                    };
                    let result =
                        smol::unblock(move || corvo_platform::terminate_process(identity, mode))
                            .await;
                    let notice = match result {
                        Ok(()) => ToastNotice::success(
                            ToastCategory::Process,
                            "Quit requested",
                            format!("PID {pid}"),
                        ),
                        Err(error) => ToastNotice::failure(
                            ToastCategory::Process,
                            "Could not stop process",
                            format!("PID {pid} · {error}"),
                        ),
                    };
                    cx.update(|cx| show_action_toast(notice, cx));
                })
                .detach();
            }
            Ok(Action::RunProcess {
                program,
                args,
                title,
            }) => {
                let is_brew = program.ends_with("/brew") || program == "brew";
                let notice_context = process_toast_context(&title, &args, is_brew);
                cx.spawn(async move |_this, cx| {
                    // Installs can compile from source, so the cap is
                    // generous — but finite, so a hung child cannot park
                    // the toast forever.
                    const PROCESS_TIMEOUT: std::time::Duration =
                        std::time::Duration::from_secs(15 * 60);
                    let output = smol::unblock(move || {
                        corvo_platform::run_process_with_timeout(&program, &args, PROCESS_TIMEOUT)
                    })
                    .await;
                    let notice = match output {
                        Ok(output) if output.status.success() => ToastNotice::success(
                            notice_context.category,
                            notice_context.success_title,
                            notice_context.success_detail,
                        ),
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let detail = if stderr.trim().is_empty() {
                                format!("Exit code {}", output.status.code().unwrap_or(-1))
                            } else {
                                process_failure_detail(&stderr)
                            };
                            ToastNotice::failure(
                                notice_context.category,
                                notice_context.failure_title,
                                detail,
                            )
                        }
                        Err(error) => ToastNotice::failure(
                            notice_context.category,
                            notice_context.failure_title,
                            error.to_string(),
                        ),
                    };
                    if is_brew {
                        corvo_brew::invalidate_cache();
                    }
                    cx.update(|cx| show_action_toast(notice, cx));
                })
                .detach();
            }
            Ok(Action::Copy(text)) => {
                let result_id = self
                    .selected_result()
                    .map(|result| result.id.clone())
                    .unwrap_or_default();
                let result = ops.copy_text(&text).map_err(|error| error.to_string());
                let notice = copy_notice(&result_id, result, "text");
                self.dismiss(window);
                show_action_toast(notice, cx);
            }
            Ok(Action::PasteText(text)) => {
                let result_id = self
                    .selected_result()
                    .map(|result| result.id.clone())
                    .unwrap_or_default();
                let previous_pid = self
                    .previous_app
                    .take()
                    .or_else(corvo_platform::frontmost_app_pid);
                corvo_platform::forget_launcher_panel();
                window.remove_window();
                if let Some(pid) = previous_pid {
                    cx.spawn(async move |_this, cx| {
                        let result = corvo_platform::auto_paste(pid, &text).await;
                        let notice =
                            paste_notice(result.map_err(|error| error.to_string()), "Text");
                        cx.update(|cx| show_action_toast(notice, cx));
                    })
                    .detach();
                } else {
                    let result = ops.copy_text(&text).map_err(|error| error.to_string());
                    show_action_toast(copy_notice(&result_id, result, "text"), cx);
                }
            }
            Ok(Action::CopyImage(path)) => {
                let result_id = self
                    .selected_result()
                    .map(|result| result.id.clone())
                    .unwrap_or_default();
                let notice = match std::fs::read(&path) {
                    Ok(bytes) => {
                        let result = corvo_platform::copy_image_to_pasteboard(&bytes)
                            .map_err(|error| error.to_string());
                        copy_notice(&result_id, result, "image")
                    }
                    Err(error) => copy_notice(&result_id, Err(error.to_string()), "image"),
                };
                self.dismiss(window);
                show_action_toast(notice, cx);
            }
            Ok(Action::PasteImage(path)) => {
                let previous_pid = self
                    .previous_app
                    .take()
                    .or_else(corvo_platform::frontmost_app_pid);
                corvo_platform::forget_launcher_panel();
                window.remove_window();
                cx.spawn(async move |_this, cx| {
                    let result = if let Ok(bytes) = std::fs::read(&path) {
                        if let Some(pid) = previous_pid {
                            corvo_platform::auto_paste_image(pid, &bytes)
                                .await
                                .map_err(|error| error.to_string())
                        } else {
                            corvo_platform::copy_image_to_pasteboard(&bytes)
                                .map_err(|error| error.to_string())
                        }
                    } else {
                        Err("Could not read image file".to_string())
                    };
                    let notice = match (previous_pid.is_some(), result) {
                        (true, result) => paste_notice(result, "Image"),
                        (false, result) => copy_notice("clipboard-manager", result, "image"),
                    };
                    cx.update(|cx| show_action_toast(notice, cx));
                })
                .detach();
            }
            Ok(Action::ShowToast(msg)) => {
                if let Some(text) = msg.strip_prefix("note-editor:") {
                    let note = corvo_notes::list_notes()
                        .into_iter()
                        .find(|note| note.id == text);
                    if let Some(note) = note {
                        self.dismiss(window);
                        note_editor::open_note_editor(Some(note), None, cx);
                    }
                } else if let Some(text) = msg.strip_prefix("copy:") {
                    let result_id = self
                        .selected_result()
                        .map(|result| result.id.clone())
                        .unwrap_or_default();
                    let result = ops.copy_text(text).map_err(|error| error.to_string());
                    let notice = copy_notice(&result_id, result, "text");
                    self.dismiss(window);
                    show_action_toast(notice, cx);
                } else if let Some(id) = msg.strip_prefix("note-delete:") {
                    // Notes namespace, checked before the clipboard's
                    // `delete:` so the two never collide.
                    let deleted = corvo_notes::delete_note(id);
                    if self.page == LauncherPage::Notes {
                        self.refresh_notes(cx);
                    }
                    let notice = if deleted {
                        ToastNotice::success(
                            ToastCategory::General,
                            "Note deleted",
                            "Removed from your notes",
                        )
                    } else {
                        ToastNotice::failure(
                            ToastCategory::General,
                            "Could not delete note",
                            "The note file went missing",
                        )
                    };
                    show_action_toast(notice, cx);
                } else if let Some(id) = msg.strip_prefix("delete:") {
                    let result = corvo_clipboard_manager::delete_entry(id);
                    self.refresh_clipboard(cx);
                    let notice = match result {
                        Ok(()) => ToastNotice::success(
                            ToastCategory::Clipboard,
                            "Item removed",
                            "Removed from clipboard history",
                        ),
                        Err(error) => ToastNotice::failure(
                            ToastCategory::Clipboard,
                            "Could not remove item",
                            error,
                        ),
                    };
                    show_action_toast(notice, cx);
                } else if msg == "clear" {
                    let result = corvo_clipboard_manager::clear_history();
                    self.refresh_clipboard(cx);
                    let notice = match result {
                        Ok(()) => ToastNotice::success(
                            ToastCategory::Clipboard,
                            "History cleared",
                            "Clipboard history is empty",
                        ),
                        Err(error) => ToastNotice::failure(
                            ToastCategory::Clipboard,
                            "Could not clear history",
                            error,
                        ),
                    };
                    show_action_toast(notice, cx);
                } else if msg == "Clipboard History" {
                    self.open_clipboard_page(window, cx);
                } else if msg == "Search Emoji & Symbols" {
                    self.open_emoji_page(window, cx);
                } else {
                    show_action_toast(
                        ToastNotice::success(ToastCategory::General, "Action complete", msg),
                        cx,
                    );
                }
            }
            Ok(Action::AdjustBrightness(delta)) => {
                let direction = if delta >= 0.0 {
                    "Brightness increased"
                } else {
                    "Brightness reduced"
                };
                cx.spawn(async move |_this, cx| {
                    let result =
                        smol::unblock(move || corvo_platform::adjust_brightness_with_level(delta))
                            .await;
                    let notice = match result {
                        Ok(Some(percent)) => ToastNotice::success(
                            ToastCategory::Brightness,
                            "Display updated",
                            direction,
                        )
                        .with_progress(percent),
                        Ok(None) => ToastNotice::success(
                            ToastCategory::Brightness,
                            "Display updated",
                            direction,
                        ),
                        Err(error) => ToastNotice::failure(
                            ToastCategory::Brightness,
                            "Could not change brightness",
                            error.to_string(),
                        ),
                    };
                    cx.update(|cx| show_action_toast(notice, cx));
                })
                .detach();
            }
            Ok(Action::AdjustVolume(delta)) => {
                cx.spawn(async move |_this, cx| {
                    let result = smol::unblock(move || {
                        corvo_platform::adjust_audio_output_with_level(delta)
                    })
                    .await;
                    let notice = match result {
                        Ok(Some(percent)) => {
                            ToastNotice::success(ToastCategory::System, "Output volume updated", "")
                                .with_progress(percent)
                        }
                        Ok(None) => ToastNotice::success(
                            ToastCategory::System,
                            "Output volume updated",
                            if delta >= 0.0 {
                                "Volume increased"
                            } else {
                                "Volume reduced"
                            },
                        ),
                        Err(error) => ToastNotice::failure(
                            ToastCategory::System,
                            "Could not change output volume",
                            error.to_string(),
                        ),
                    };
                    cx.update(|cx| show_action_toast(notice, cx));
                })
                .detach();
            }
            Ok(Action::TileWindow(action_id)) => {
                let previous_pid = self.previous_app.take();
                self.dismiss(window);
                cx.spawn(async move |_this, cx| {
                    smol::Timer::after(std::time::Duration::from_millis(50)).await;
                    // Saved layouts tile every app they name, unlike the
                    // plain tile actions, which move one window.
                    let result = if let Some(layout_id) = action_id.strip_prefix("layout:") {
                        let settings = corvo_config::Settings::load();
                        settings
                            .window_management
                            .layouts
                            .iter()
                            .find(|layout| layout.id == layout_id)
                            .map(|layout| {
                                let placements: Vec<(String, String)> = layout
                                    .placements
                                    .iter()
                                    .map(|placement| {
                                        (placement.app_name.clone(), placement.position.clone())
                                    })
                                    .collect();
                                corvo_platform::apply_window_layout(&placements)
                            })
                            .unwrap_or_else(|| {
                                Err(corvo_platform::PlatformError::Os(format!(
                                    "unknown layout {layout_id}"
                                )))
                            })
                    } else {
                        corvo_platform::tile_window(previous_pid, &action_id)
                    };
                    let notice = match result {
                        Ok(()) => ToastNotice::success(
                            ToastCategory::Window,
                            "Window arranged",
                            action_id
                                .strip_prefix("layout:")
                                .unwrap_or(&action_id)
                                .replace('-', " "),
                        ),
                        Err(error) => ToastNotice::failure(
                            ToastCategory::Window,
                            "Could not arrange window",
                            error.to_string(),
                        ),
                    };
                    cx.update(|cx| show_action_toast(notice, cx));
                })
                .detach();
            }
            Ok(Action::CloseWindow) => self.dismiss(window),
            Err(error) => {
                let category = self
                    .selected_result()
                    .map(|result| category_for_result(&result.id))
                    .unwrap_or(ToastCategory::General);
                show_action_toast(
                    ToastNotice::failure(category, "Action failed", error.to_string()),
                    cx,
                );
            }
        }
    }

    fn save_result_flag(
        &self,
        result_id: &str,
        title: &str,
        hidden: Option<bool>,
        favorite: Option<bool>,
    ) -> bool {
        let mut saved = true;
        let mut settings = corvo_config::Settings::load();
        if settings.set_result_item_flags(result_id, title, hidden, favorite) {
            if let Err(error) = settings.save() {
                corvo_platform::diagnostics::record_error(
                    "launcher",
                    "item_preferences_save_failed",
                );
                eprintln!("corvo: could not save item preferences: {error}");
                saved = false;
            }
        }
        if let (Some(hidden), Some(_index)) = (hidden, result_id.strip_prefix("quicklinks:")) {
            let mut quicklinks = corvo_config::QuicklinksFile::load();
            if let Some(link) = quicklinks
                .quicklinks
                .iter_mut()
                .find(|link| link.name == title)
            {
                link.hidden = hidden;
                if let Err(error) = quicklinks.save() {
                    corvo_platform::diagnostics::record_error(
                        "launcher",
                        "quicklink_visibility_save_failed",
                    );
                    eprintln!("corvo: could not save quicklink visibility: {error}");
                    saved = false;
                }
            }
        }
        saved
    }

    /// Closes the panel and hands activation back to the app that was
    /// frontmost when it opened.
    fn dismiss(&mut self, window: &mut Window) {
        self.force_expanded = false;
        corvo_platform::forget_launcher_panel();
        #[cfg(target_os = "windows")]
        {
            self.visible = false;
            set_windows_launcher_visible(window, false);
        }
        #[cfg(not(target_os = "windows"))]
        window.remove_window();
        if let Some(pid) = self.previous_app.take() {
            corvo_platform::activate_app(pid);
        }
    }

    #[cfg(target_os = "windows")]
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        corvo_app_launcher::reload_corpus();
        let (pid, name) = match corvo_platform::frontmost_app_info() {
            Some((pid, name)) => (Some(pid), Some(name)),
            None => (corvo_platform::frontmost_app_pid(), None),
        };
        self.previous_app = pid;
        self.previous_app_name = name;
        self.visible = true;
        self.page = LauncherPage::Root;
        self.query.clear();
        self.cursor_idx = 0;
        self.selected = 0;
        self.results = cached_initial_results();
        self.rebuild_root_flat_items();
        self.results_scroll_handle.scroll_to_item(0);
        window.focus(&self.focus_handle, cx);
        // The resident HWND keeps the height the last session left behind.
        // Collapse to the compact height before showing, so the panel never
        // appears at the stale size and then shrinks a frame later.
        self.sync_palette_size(window, cx);
        set_windows_launcher_visible(window, true);
        self.refresh(cx);
        cx.notify();
    }

    fn search_row(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let font_size = px(20.0);
        let cursor_x = cursor_offset_for_query(&self.query, self.cursor_idx, font_size, window);
        let port_input_visible =
            self.page == LauncherPage::Root && self.selected_result_is_port_action();
        let query_width =
            cursor_offset_for_query(&self.query, self.query.chars().count(), font_size, window);
        let port_before = self
            .port_argument
            .chars()
            .take(self.port_cursor_idx)
            .collect::<String>();
        let port_after = self
            .port_argument
            .chars()
            .skip(self.port_cursor_idx)
            .collect::<String>();
        let placeholder: SharedString = if let LauncherPage::Browser(browser) = self.page {
            SharedString::from(format!(
                "Search {} tabs and bookmarks...",
                browser.spec().name
            ))
        } else if let LauncherPage::Extension(_) = self.page {
            self.extension_view
                .as_ref()
                .map(|view| match view {
                    corvo_core::PageView::Blocks(blocks) => blocks.placeholder,
                    corvo_core::PageView::Grid { placeholder, .. } => placeholder,
                    _ => "",
                })
                .filter(|hint| !hint.is_empty())
                .map(SharedString::from)
                .unwrap_or_else(|| SharedString::from("Filter..."))
        } else if self.page == LauncherPage::Brew {
            SharedString::from(match self.brew_page_mode {
                BrewPageMode::Installed => "Filter installed packages...",
                BrewPageMode::Upgrades => "Filter available upgrades...",
                BrewPageMode::Services => "Filter Homebrew services...",
                BrewPageMode::Search => "Search formulae and casks...",
            })
        } else if self.page == LauncherPage::Text {
            "Type text to transform, or leave empty to use the clipboard...".into()
        } else if self.page == LauncherPage::Notes {
            "Search notes, or type a new one and press Enter...".into()
        } else if self.page == LauncherPage::Ports {
            "Filter or enter a port (e.g. 3000)...".into()
        } else if self.page == LauncherPage::Processes {
            "Filter processes by name or PID...".into()
        } else if self.page == LauncherPage::Files {
            "Search files and folders...".into()
        } else if let Some(hint) = self.declared_argument_placeholder() {
            hint
        } else {
            "Search for apps and commands...".into()
        };

        let input_view = if self.query.is_empty() {
            div()
                .id("search-input-content")
                .relative()
                .flex_1()
                .h(px(28.0))
                .flex()
                .items_center()
                .child(render_cursor(
                    px(0.0),
                    px(24.0),
                    self.cursor_visible && !self.port_input_active,
                ))
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child(placeholder),
                )
        } else {
            div()
                .id("search-input-content")
                .relative()
                .flex_1()
                .h(px(28.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT))
                        .child(self.query.clone()),
                )
                .child(render_cursor(
                    cursor_x,
                    px(24.0),
                    self.cursor_visible && !self.port_input_active,
                ))
        };
        let input_view = input_view.when(port_input_visible, |input| {
            input.flex_none().w(query_width + px(8.0))
        });

        div()
            .id("search-row")
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .text_size(font_size)
            .cursor_pointer()
            .on_click(cx.listener(|launcher, event: &ClickEvent, window, cx| {
                let local_x = (event.position().x - px(49.0)).max(px(0.0));
                launcher.cursor_idx =
                    cursor_index_from_click(&launcher.query, local_x, px(20.0), window);
                launcher.cursor_visible = true;
                cx.notify();
            }))
            .child(search_icon())
            .child(input_view)
            .when(port_input_visible, |view| {
                view.child(
                    div()
                        .id("port-argument")
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(COLOR_ACCENT))
                        .text_size(px(13.0))
                        .text_color(rgb(COLOR_TEXT))
                        .when(self.port_input_active, |field| {
                            field.border_color(rgb(COLOR_ACCENT))
                        })
                        .child(
                            div()
                                .min_w(px(44.0))
                                .flex()
                                .items_center()
                                .child(
                                    if self.port_argument.is_empty() && !self.port_input_active {
                                        "Port".to_string()
                                    } else {
                                        port_before
                                    },
                                )
                                .when(self.port_input_active, |field| {
                                    field.child(
                                        div()
                                            .w(px(1.0))
                                            .h(px(16.0))
                                            .when(self.cursor_visible, |cursor| {
                                                cursor.bg(rgb(COLOR_TEXT))
                                            }),
                                    )
                                })
                                .child(port_after),
                        ),
                )
            })
    }

    fn results_list(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.results.is_empty() {
            div().id("no-results").flex_1()
        } else {
            let mut row_views: Vec<Stateful<Div>> = Vec::new();
            for item in &self.root_flat_items {
                match item {
                    RootFlatItem::Header(title) => {
                        row_views.push(
                            div()
                                .id(SharedString::from(format!("section-header-{title}")))
                                .w_full()
                                .h(px(self.row_height))
                                .px(px(6.0))
                                .flex()
                                .items_end()
                                .pb_1()
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(COLOR_TEXT_DIM))
                                        .child(title.clone()),
                                ),
                        );
                    }
                    RootFlatItem::Row(result_idx) => {
                        if let Some(result) = self.results.get(*result_idx) {
                            row_views.push(self.result_row(*result_idx, result, cx));
                        }
                    }
                }
            }

            div()
                .id("results")
                .flex_1()
                .w_full()
                .flex()
                .flex_col()
                .px(px(6.0))
                .pb(px(56.0))
                .overflow_y_scroll()
                .track_scroll(&self.results_scroll_handle)
                .children(row_views)
        }
    }

    fn brew_subheader(&self) -> Div {
        div()
            .flex_none()
            .px_4()
            .pb_1()
            .text_size(px(12.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(COLOR_TEXT_DIM))
            .child(self.brew_page_mode.title().to_uppercase())
    }

    fn ports_subheader(&self) -> Div {
        div()
            .flex_none()
            .px_4()
            .pb_1()
            .text_size(px(12.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(COLOR_TEXT_DIM))
            .child("OPEN PORTS")
    }

    fn processes_subheader(&self) -> Div {
        div()
            .flex_none()
            .px_4()
            .pb_1()
            .text_size(px(12.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(COLOR_TEXT_DIM))
            .child("PROCESSES")
    }

    fn text_subheader(&self) -> Div {
        div()
            .flex_none()
            .px_4()
            .pb_1()
            .text_size(px(12.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(COLOR_TEXT_DIM))
            .child("TEXT TRANSFORMS")
    }

    fn emoji_search_row(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let font_size = px(18.0);
        let cursor_x = cursor_offset_for_query(&self.query, self.cursor_idx, font_size, window);
        let cat_label = EMOJI_CATEGORIES
            .get(self.emoji_category_index)
            .copied()
            .unwrap_or("All Categories");
        let is_filtered = self.emoji_category_index > 0;

        let input_view = if self.query.is_empty() {
            div()
                .id("emoji-input-content")
                .relative()
                .flex_1()
                .h(px(26.0))
                .flex()
                .items_center()
                .child(render_cursor(px(0.0), px(22.0), self.cursor_visible))
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child("Search emoji and symbols..."),
                )
        } else {
            div()
                .id("emoji-input-content")
                .relative()
                .flex_1()
                .h(px(26.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT))
                        .child(self.query.clone()),
                )
                .child(render_cursor(cursor_x, px(22.0), self.cursor_visible))
        };

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2p5()
            .text_size(font_size)
            .child(
                div()
                    .id("emoji-back")
                    .cursor_pointer()
                    .px_1p5()
                    .py_0p5()
                    .rounded_md()
                    .hover(|s| s.bg(rgb(COLOR_PILL)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, window, cx| {
                        launcher.open_root_page(window, cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::CARET_LEFT,
                        rgb(COLOR_TEXT_DIM),
                        20.0,
                    )),
            )
            .child(
                div()
                    .id("emoji-input-area")
                    .flex_1()
                    .cursor_pointer()
                    .on_click(cx.listener(|launcher, event: &ClickEvent, window, cx| {
                        let local_x = (event.position().x - px(53.0)).max(px(0.0));
                        launcher.cursor_idx =
                            cursor_index_from_click(&launcher.query, local_x, px(18.0), window);
                        launcher.cursor_visible = true;
                        cx.notify();
                    }))
                    .child(input_view),
            )
            .child(
                div()
                    .id("emoji-category-button")
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px_2p5()
                    .py_1()
                    .rounded_full()
                    .when(self.filter_dropdown_open || is_filtered, |s| {
                        s.bg(rgb(COLOR_ROW_SELECTED))
                    })
                    .when(!self.filter_dropdown_open && !is_filtered, |s| {
                        s.bg(rgb(COLOR_PILL))
                    })
                    .hover(|s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.toggle_filter_dropdown(cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::SQUARES_FOUR,
                        if is_filtered {
                            rgb(COLOR_ACCENT)
                        } else {
                            rgb(COLOR_TEXT_DIM)
                        },
                        14.0,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if is_filtered {
                                rgb(COLOR_ACCENT)
                            } else {
                                rgb(COLOR_TEXT)
                            })
                            .child(cat_label),
                    )
                    .child(icons::render_phosphor_svg(
                        if self.filter_dropdown_open {
                            phosphor_svgs::style::regular::CARET_UP
                        } else {
                            phosphor_svgs::style::regular::CARET_DOWN
                        },
                        rgb(COLOR_TEXT_DIM),
                        12.0,
                    )),
            )
    }

    fn emoji_subheader(&self, _cx: &mut Context<Self>) -> Div {
        if !self.query.is_empty() {
            div()
                .flex_none()
                .flex()
                .items_baseline()
                .gap_2()
                .px_4()
                .pt_1()
                .pb_2()
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(COLOR_TEXT))
                        .child("Search Results"),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child(format!("{}", self.results.len())),
                )
        } else {
            div().flex_none().h(px(0.0))
        }
    }

    fn emoji_grid_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.results.is_empty() {
            return div().id("no-emoji-results").flex_1();
        }

        let column_count = self.emoji_column_count;
        let num_rows = self.results.len().div_ceil(column_count);
        let mut rows: Vec<Stateful<Div>> = Vec::with_capacity(num_rows);

        for row_idx in 0..num_rows {
            let start = row_idx * column_count;
            let end = (start + column_count).min(self.results.len());
            let mut cells: Vec<Stateful<Div>> = Vec::with_capacity(column_count);

            for idx in start..end {
                let is_selected = idx == self.selected;
                let raw_glyph = match &self.results[idx].icon {
                    Icon::Glyph(g) => (*g).to_string(),
                    _ => self.results[idx]
                        .id
                        .strip_prefix("emoji-picker:")
                        .unwrap_or(&self.results[idx].title)
                        .to_string(),
                };
                let glyph = apply_emoji_skin_tone(&raw_glyph, self.emoji_skin_tone);
                cells.push(
                    div()
                        .id(SharedString::from(format!("emoji-cell-{idx}")))
                        .cursor_pointer()
                        .flex_1()
                        .h(px(62.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_lg()
                        .border_2()
                        .border_color(if is_selected {
                            rgb(COLOR_ACCENT)
                        } else {
                            rgb(0x232529)
                        })
                        .bg(if is_selected {
                            rgb(COLOR_ROW_SELECTED)
                        } else {
                            rgb(0x1d1f22)
                        })
                        .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                            launcher.selected = idx;
                            launcher.execute_selected_emoji(window, cx);
                        }))
                        .child(div().text_size(px(28.0)).child(glyph)),
                );
            }

            for dummy_idx in (end - start)..column_count {
                cells.push(
                    div()
                        .id(SharedString::from(format!(
                            "emoji-dummy-{row_idx}-{dummy_idx}"
                        )))
                        .flex_1()
                        .h(px(62.0)),
                );
            }

            rows.push(
                div()
                    .id(SharedString::from(format!("emoji-row-{row_idx}")))
                    .w_full()
                    .h(px(70.0))
                    .flex()
                    .gap_2()
                    .pb_2()
                    .children(cells),
            );
        }

        div()
            .id("emoji-grid")
            .flex_1()
            .w_full()
            .px_4()
            .pb(px(56.0))
            .overflow_y_scroll()
            .track_scroll(&self.emoji_scroll_handle)
            .children(rows)
    }

    fn clipboard_search_row(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let font_size = px(18.0);
        let cursor_x = cursor_offset_for_query(&self.query, self.cursor_idx, font_size, window);

        let input_view = if self.query.is_empty() {
            div()
                .id("clipboard-input-content")
                .relative()
                .flex_1()
                .h(px(26.0))
                .flex()
                .items_center()
                .child(render_cursor(px(0.0), px(22.0), self.cursor_visible))
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child("Type to filter entries..."),
                )
        } else {
            div()
                .id("clipboard-input-content")
                .relative()
                .flex_1()
                .h(px(26.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT))
                        .child(self.query.clone()),
                )
                .child(render_cursor(cursor_x, px(22.0), self.cursor_visible))
        };

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2p5()
            .text_size(font_size)
            .child(
                div()
                    .id("clipboard-back")
                    .cursor_pointer()
                    .px_1p5()
                    .py_0p5()
                    .rounded_md()
                    .hover(|s| s.bg(rgb(COLOR_PILL)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, window, cx| {
                        launcher.open_root_page(window, cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::CARET_LEFT,
                        rgb(COLOR_TEXT_DIM),
                        20.0,
                    )),
            )
            .child(
                div()
                    .id("clipboard-input-area")
                    .flex_1()
                    .cursor_pointer()
                    .on_click(cx.listener(|launcher, event: &ClickEvent, window, cx| {
                        let local_x = (event.position().x - px(53.0)).max(px(0.0));
                        launcher.cursor_idx =
                            cursor_index_from_click(&launcher.query, local_x, px(18.0), window);
                        launcher.cursor_visible = true;
                        cx.notify();
                    }))
                    .child(input_view),
            )
            .child({
                let current_filter_label = CLIPBOARD_FILTERS
                    .get(self.clipboard_filter_index)
                    .copied()
                    .unwrap_or("All Types");
                let is_filtered = self.clipboard_filter_index > 0;
                div()
                    .id("clipboard-type-filter")
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px_2p5()
                    .py_1()
                    .rounded_full()
                    .when(self.filter_dropdown_open || is_filtered, |s| {
                        s.bg(rgb(COLOR_ROW_SELECTED))
                    })
                    .when(!self.filter_dropdown_open && !is_filtered, |s| {
                        s.bg(rgb(COLOR_PILL))
                    })
                    .hover(|s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.toggle_filter_dropdown(cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::FUNNEL,
                        if is_filtered {
                            rgb(COLOR_ACCENT)
                        } else {
                            rgb(COLOR_TEXT_DIM)
                        },
                        13.0,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if is_filtered {
                                rgb(COLOR_ACCENT)
                            } else {
                                rgb(COLOR_TEXT)
                            })
                            .child(current_filter_label),
                    )
                    .child(icons::render_phosphor_svg(
                        if self.filter_dropdown_open {
                            phosphor_svgs::style::regular::CARET_UP
                        } else {
                            phosphor_svgs::style::regular::CARET_DOWN
                        },
                        rgb(COLOR_TEXT_DIM),
                        12.0,
                    ))
            })
    }

    fn uninstaller_search_row(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let font_size = px(18.0);
        let cursor_x = cursor_offset_for_query(&self.query, self.cursor_idx, font_size, window);
        let target_name = match &self.uninstaller {
            UninstallerState::Removing { target }
            | UninstallerState::Ready(UninstallerReady { target, .. }) => target.name.as_str(),
            UninstallerState::Closed => "Uninstall Application",
        };
        let input = if self.query.is_empty() {
            div()
                .id("uninstaller-input-content")
                .relative()
                .flex_1()
                .h(px(26.0))
                .flex()
                .items_center()
                .child(render_cursor(px(0.0), px(22.0), self.cursor_visible))
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child("Filter files and folders..."),
                )
        } else {
            div()
                .id("uninstaller-input-content")
                .relative()
                .flex_1()
                .h(px(26.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .relative()
                        .top(px(2.0))
                        .text_color(rgb(COLOR_TEXT))
                        .child(self.query.clone()),
                )
                .child(render_cursor(cursor_x, px(22.0), self.cursor_visible))
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2p5()
            .child(
                div()
                    .id("uninstaller-back")
                    .cursor_pointer()
                    .px_1p5()
                    .py_0p5()
                    .rounded_md()
                    .hover(|style| style.bg(rgb(COLOR_PILL)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, window, cx| {
                        launcher.open_root_page(window, cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::CARET_LEFT,
                        rgb(COLOR_TEXT_DIM),
                        20.0,
                    )),
            )
            .child(
                div()
                    .id("uninstaller-input-area")
                    .flex_1()
                    .cursor_pointer()
                    .on_click(cx.listener(|launcher, event: &ClickEvent, window, cx| {
                        let local_x = (event.position().x - px(53.0)).max(px(0.0));
                        launcher.cursor_idx =
                            cursor_index_from_click(&launcher.query, local_x, px(18.0), window);
                        launcher.cursor_visible = true;
                        cx.notify();
                    }))
                    .child(input),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(target_name.to_string()),
            )
    }

    fn uninstaller_list(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let visible = self.uninstaller_visible_indices();
        match &self.uninstaller {
            UninstallerState::Removing { .. } => div()
                .id("uninstaller-removing")
                .flex_1()
                .px_4()
                .py_3()
                .text_color(rgb(COLOR_TEXT_DIM))
                .child(if cfg!(target_os = "macos") {
                    "Moving selected files to Trash..."
                } else {
                    "Uninstalling application..."
                }),
            UninstallerState::Closed => div().id("uninstaller-closed").flex_1(),
            UninstallerState::Ready(ready) => {
                let selected_count = ready
                    .selected_files
                    .iter()
                    .filter(|selected| **selected)
                    .count();
                let selected_size = ready
                    .files
                    .iter()
                    .zip(&ready.selected_files)
                    .filter(|(_, selected)| **selected)
                    .map(|(file, _)| file.size_bytes)
                    .fold(0u64, u64::saturating_add);
                let rows = visible.iter().map(|index| {
                    let index = *index;
                    let file = &ready.files[index];
                    let selected = ready.selected_files.get(index).copied().unwrap_or(false);
                    let focused = ready.focused == index;
                    let label = file
                        .path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Unknown file");
                    let size = format_file_size(file.size_bytes);
                    let path = file.location.clone();
                    let type_icon = if file.is_application {
                        if let Some(ref icon_path) = ready.target.icon_png {
                            img(icon_path.clone())
                                .size(px(18.0))
                                .rounded_sm()
                                .into_any_element()
                        } else {
                            icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::APP_WINDOW,
                                rgb(COLOR_TEXT_DIM),
                                18.0,
                            )
                            .into_any_element()
                        }
                    } else if file.path.is_dir() {
                        icons::render_phosphor_svg(
                            phosphor_svgs::style::fill::FOLDER,
                            rgb(0x38bdf8),
                            18.0,
                        )
                        .into_any_element()
                    } else {
                        icons::render_phosphor_svg(
                            phosphor_svgs::style::regular::FILE_TEXT,
                            rgb(0xe2e8f0),
                            18.0,
                        )
                        .into_any_element()
                    };
                    div()
                        .id(SharedString::from(format!("uninstaller-row-{index}")))
                        .w_full()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_4()
                        .h(px(40.0))
                        .rounded_lg()
                        .cursor_pointer()
                        .when(focused, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
                        .when(!focused, |row| row.hover(|style| style.bg(rgb(0x181b1e))))
                        .on_click(cx.listener(move |launcher, _: &ClickEvent, _window, cx| {
                            if let UninstallerState::Ready(ready) = &mut launcher.uninstaller {
                                ready.focused = index;
                                ready.confirming = false;
                                if let Some(checked) = ready.selected_files.get_mut(index) {
                                    *checked = !*checked;
                                }
                            }
                            cx.notify();
                        }))
                        .child(icons::render_phosphor_svg(
                            if selected {
                                phosphor_svgs::style::regular::CHECK_SQUARE
                            } else {
                                phosphor_svgs::style::regular::SQUARE
                            },
                            if selected {
                                rgb(COLOR_ACCENT)
                            } else {
                                rgb(COLOR_TEXT_DIM)
                            },
                            18.0,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(COLOR_TEXT))
                                        .whitespace_nowrap()
                                        .child(label.to_string()),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(COLOR_TEXT_DIM))
                                        .whitespace_nowrap()
                                        .text_overflow(TextOverflow::Truncate(
                                            SharedString::new_static("..."),
                                        ))
                                        .child(path),
                                )
                                .when(file.matched_by_name, |d| {
                                    d.child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(0x9ca3af))
                                            .whitespace_nowrap()
                                            .child("matched by name"),
                                    )
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(size),
                        )
                        .child(
                            div()
                                .flex_none()
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(type_icon),
                        )
                });
                div()
                    .id("uninstaller-content")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .px_3()
                    .pb(px(56.0))
                    .child(
                        div()
                            .flex_none()
                            .px_1()
                            .py_2()
                            .text_size(px(12.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child(format!(
                                "{selected_count} of {} files selected · {}",
                                ready.files.len(),
                                format_file_size(selected_size)
                            )),
                    )
                    .when(ready.scan_limited, |view| {
                        view.child(
                            div()
                                .flex_none()
                                .px_1()
                                .pb_2()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_DESTRUCTIVE))
                                .child("Scan limit reached. Some related files may not appear."),
                        )
                    })
                    .when_some(ready.message.clone(), |view, message| {
                        view.child(
                            div()
                                .flex_none()
                                .px_1()
                                .pb_2()
                                .text_size(px(12.0))
                                .text_color(if ready.confirming {
                                    rgb(COLOR_DESTRUCTIVE)
                                } else {
                                    rgb(COLOR_TEXT_DIM)
                                })
                                .child(message),
                        )
                    })
                    .child(if ready.files.is_empty() {
                        div()
                            .id("uninstaller-empty")
                            .flex_1()
                            .px_1()
                            .py_2()
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("No related files found")
                    } else if visible.is_empty() {
                        div()
                            .id("uninstaller-filter-empty")
                            .flex_1()
                            .px_1()
                            .py_2()
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("No matching files")
                    } else {
                        div()
                            .id("uninstaller-rows")
                            .flex_1()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .overflow_y_scroll()
                            .children(rows)
                    })
            }
        }
    }

    fn clipboard_split_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("clipboard-split-view")
            .flex_1()
            .flex()
            .overflow_hidden()
            .child(self.clipboard_list(cx))
            .child(div().w(px(1.0)).self_stretch().bg(rgb(COLOR_DIVIDER)))
            .child(self.clipboard_preview_pane(cx))
    }

    fn clipboard_list(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.results.is_empty() {
            return div()
                .id("no-clipboard-results")
                .w(px(250.0))
                .flex_none()
                .flex()
                .flex_col()
                .px(px(6.0))
                .child(div().flex_1());
        }

        div()
            .id("clipboard-list")
            .w(px(250.0))
            .flex_none()
            .flex()
            .flex_col()
            .px(px(6.0))
            .child(
                uniform_list(
                    "clipboard-uniform-list",
                    self.clipboard_flat_items.len(),
                    cx.processor(
                        |launcher: &mut Self, range: std::ops::Range<usize>, _window, cx| {
                            range
                                .map(|flat_idx| match &launcher.clipboard_flat_items[flat_idx] {
                                    ClipboardFlatItem::Header(title) => div()
                                        .id(SharedString::from(format!(
                                            "clipboard-section-{flat_idx}"
                                        )))
                                        .h(px(launcher.row_height))
                                        .px(px(6.0))
                                        .flex()
                                        .items_end()
                                        .pb_1()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(COLOR_TEXT_DIM))
                                                .child(title.clone()),
                                        ),
                                    ClipboardFlatItem::Row(result_idx) => {
                                        let result = &launcher.results[*result_idx];
                                        launcher.clipboard_row(*result_idx, result, cx)
                                    }
                                })
                                .collect::<Vec<_>>()
                        },
                    ),
                )
                .track_scroll(&self.clipboard_scroll_handle)
                .flex_1(),
            )
    }

    fn clipboard_row(
        &self,
        index: usize,
        result: &SearchResult,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = index == self.selected;
        div()
            .id(SharedString::from(format!("clip-row-{index}")))
            .flex_none()
            .flex()
            .items_center()
            .px(px(6.0))
            .mb_0p5()
            .h(px(self.row_height))
            .rounded_lg()
            .when(selected, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
            .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                launcher.select(index, cx);
                launcher.execute_selected(window, cx);
            }))
            .child(
                div()
                    .flex_none()
                    .mr(px(8.0))
                    .size(px(ICON_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(
                        match &result.icon {
                            Icon::Image(path) if path.exists() => Some(path.clone()),
                            _ => None,
                        },
                        |el, path| el.child(img(path).size(px(ICON_SIZE))),
                    )
                    .when(
                        !matches!(&result.icon, Icon::Image(path) if path.exists()),
                        |el| {
                            el.child(icons::render_phosphor_svg(
                                icons::icon_svg_data(&result.icon)
                                    .unwrap_or(phosphor_svgs::style::regular::CLIPBOARD_TEXT),
                                rgb(COLOR_TEXT_ICON),
                                16.0,
                            ))
                        },
                    ),
            )
            .child(
                div()
                    .w(px(192.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .text_color(rgb(COLOR_TEXT))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_overflow(TextOverflow::Truncate(SharedString::new_static("...")))
                    .child(result.title.clone()),
            )
    }

    fn clipboard_preview_pane(&self, _cx: &mut Context<Self>) -> Stateful<Div> {
        let entry = self
            .results
            .get(self.selected)
            .and_then(|r| r.id.strip_prefix("clipboard-manager:entry:"))
            .and_then(corvo_clipboard_manager::get_entry);

        let Some(entry) = entry else {
            return div().id("clipboard-preview-empty").flex_1();
        };

        let trimmed = entry.text.trim();
        let is_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");
        let is_image = entry.is_image();
        let entry_type = if is_image {
            "PNG Image"
        } else if is_url {
            "Link"
        } else {
            "Text"
        };

        let content_view = if let Some(img_path) = entry.image_path() {
            // Natural display size, never upscaled; the card is a fixed
            // block like the text cards, and the image is additionally
            // bounded by it so wide or tall shots shrink instead of
            // stretching the card.
            let (disp_w, disp_h) =
                if let (Some(w), Some(h)) = (entry.image_width, entry.image_height) {
                    if w > 0 && h > 0 {
                        let max_w = 420.0f32;
                        let max_h = 240.0f32;
                        let scale = (max_w / w as f32).min(max_h / h as f32).min(1.0);
                        (
                            (w as f32 * scale).round().max(32.0),
                            (h as f32 * scale).round().max(32.0),
                        )
                    } else {
                        (180.0, 180.0)
                    }
                } else {
                    (180.0, 180.0)
                };

            div()
                .id("clip-preview-image-box")
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .p_3()
                .rounded_lg()
                .bg(rgb(0x121315))
                .border_1()
                .border_color(rgb(0x27272a))
                .child(
                    img(img_path)
                        .w(px(disp_w))
                        .h(px(disp_h))
                        .max_w_full()
                        .max_h_full()
                        .rounded_md(),
                )
        } else {
            Self::render_clipboard_text_preview(&entry.text)
        };

        let mut info_list = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child("Information"),
            )
            .child(info_row("Source", &entry.source_app))
            .child(divider_line())
            .child(info_row("Type", entry_type))
            .child(divider_line());

        if is_image {
            let dims = format!(
                "{} × {} px",
                entry.image_width.unwrap_or(0),
                entry.image_height.unwrap_or(0)
            );
            let size_kb = format!("{:.1} KB", (entry.image_bytes.unwrap_or(0) as f64) / 1024.0);
            info_list = info_list
                .child(info_row("Dimensions", &dims))
                .child(divider_line())
                .child(info_row("File Size", &size_kb))
                .child(divider_line())
                .child(info_row("Copied", &entry.copied_at_str));
        } else {
            info_list = info_list
                .child(info_row("Characters", &format!("{}", entry.char_count)))
                .child(divider_line())
                .child(info_row("Words", &format!("{}", entry.word_count)))
                .child(divider_line())
                .child(info_row("Copied", &entry.copied_at_str));
        }

        div()
            .id("clipboard-preview-pane")
            .flex_1()
            .min_w(px(0.0))
            .max_w_full()
            .overflow_x_hidden()
            .flex()
            .flex_col()
            // Tight gap: the information section hugs the preview card.
            .gap_2()
            .px_4()
            // No top padding: the preview card sits flush at the top and
            // takes the whole height above the information section. The
            // bottom clears the 48px floating footer plus breathing room
            // so the last info row ("Copied") stays visible.
            .pt_0()
            .pb(px(64.0))
            // Only the card's interior scrolls, never this pane.
            .overflow_hidden()
            .child(content_view)
            .child(info_list)
    }

    fn file_search_split_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("file-search-split-view")
            .flex_1()
            .flex()
            .overflow_hidden()
            .child(self.file_search_list(cx))
            .child(div().w(px(1.0)).self_stretch().bg(rgb(COLOR_DIVIDER)))
            .child(self.file_search_preview_pane())
    }

    fn file_search_list(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let rows = self
            .results
            .iter()
            .enumerate()
            .map(|(index, result)| self.file_search_row(index, result, cx));
        let list_content = if self.results.is_empty() {
            div()
                .id("file-search-empty")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px_3()
                .text_size(px(12.0))
                .text_color(rgb(COLOR_TEXT_DIM))
                .child("No matching files or folders")
        } else {
            div()
                .id("file-search-rows")
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .overflow_y_scroll()
                .track_scroll(&self.file_scroll_handle)
                .children(rows)
        };
        div()
            .id("file-search-list")
            .w(px(290.0))
            .flex_none()
            .flex()
            .flex_col()
            .px(px(6.0))
            .child(list_content)
    }

    fn file_search_row(
        &self,
        index: usize,
        result: &SearchResult,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = index == self.selected;
        let is_folder = result.accessory.as_deref() == Some("Folder");
        let path = result.subtitle.clone().unwrap_or_default();
        div()
            .id(SharedString::from(format!("file-search-row-{index}")))
            .flex_none()
            .w_full()
            .h(px(48.0))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .rounded_lg()
            .cursor_pointer()
            .when(selected, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
            .when(!selected, |row| row.hover(|style| style.bg(rgb(0x202226))))
            .on_click(cx.listener(move |launcher, _: &ClickEvent, _window, cx| {
                launcher.select(index, cx);
            }))
            .child(icons::render_phosphor_svg(
                if is_folder {
                    phosphor_svgs::style::fill::FOLDER
                } else {
                    phosphor_svgs::style::regular::FILE_TEXT
                },
                if is_folder {
                    rgb(0x38bdf8)
                } else {
                    rgb(COLOR_TEXT_ICON)
                },
                18.0,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgb(COLOR_TEXT))
                            .whitespace_nowrap()
                            .text_overflow(TextOverflow::Truncate(SharedString::new_static("...")))
                            .child(result.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .whitespace_nowrap()
                            .text_overflow(TextOverflow::Truncate(SharedString::new_static("...")))
                            .child(path),
                    ),
            )
    }

    fn file_search_preview_pane(&self) -> Stateful<Div> {
        let content = match &self.file_preview {
            FilePreviewState::Empty => div()
                .id("file-preview-empty")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(rgb(COLOR_TEXT_DIM))
                .child("Select a file or folder"),
            FilePreviewState::Loading(_) => div()
                .id("file-preview-loading")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(rgb(COLOR_TEXT_DIM))
                .child("Loading preview..."),
            FilePreviewState::Ready { path, preview } => match preview {
                FilePreview::Folder => div()
                    .id("file-preview-folder")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::fill::FOLDER,
                        rgb(0x38bdf8),
                        64.0,
                    ))
                    .child(
                        div().text_size(px(15.0)).text_color(rgb(COLOR_TEXT)).child(
                            path.file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Folder")
                                .to_string(),
                        ),
                    )
                    .child(
                        div()
                            .max_w(px(400.0))
                            .text_size(px(11.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child(path.display().to_string()),
                    ),
                FilePreview::Image => div()
                    .id("file-preview-image")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_3()
                    .child(file_preview_image(path.clone())),
                FilePreview::Text(text) => div()
                    .id("file-preview-text")
                    .flex_1()
                    .min_w(px(0.0))
                    .m_3()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(COLOR_DIVIDER))
                    .bg(rgb(0x121315))
                    .overflow_y_scroll()
                    .text_size(px(11.0))
                    .text_color(rgb(COLOR_TEXT))
                    .font_family("Menlo")
                    .child(text.clone()),
                FilePreview::Unsupported => div()
                    .id("file-preview-unsupported")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::FILE_TEXT,
                        rgb(COLOR_TEXT_ICON),
                        48.0,
                    ))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("Preview is not available for this file"),
                    ),
                FilePreview::Error(message) => div()
                    .id("file-preview-error")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child(message.clone()),
            },
        };
        div()
            .id("file-search-preview-pane")
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(content)
    }

    fn render_clipboard_text_preview(raw_text: &str) -> Stateful<Div> {
        // Bound the preview before any detection or wrapping runs; huge
        // entries would otherwise rebuild thousands of rows per render.
        let raw_text = cap_preview_text(raw_text);
        let raw_text = raw_text.as_str();
        let trimmed = raw_text.trim();
        let is_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");

        let is_latex = trimmed.contains("$$")
            || trimmed.starts_with("\\[")
            || trimmed.contains("\\begin{")
            || trimmed.contains("\\frac")
            || trimmed.contains("\\sum")
            || trimmed.contains("\\int")
            || trimmed.contains("\\prod")
            || trimmed.contains("\\sqrt")
            || trimmed.contains("\\partial")
            || trimmed.contains("\\alpha")
            || trimmed.contains("\\beta")
            || trimmed.contains("\\gamma")
            || trimmed.contains("\\mathbf")
            || trimmed.contains("\\times")
            || trimmed.contains("\\infty")
            || trimmed.contains("\\theta")
            || trimmed.contains("\\rightarrow")
            || trimmed.contains("\\in");

        let parsed_json = if (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        {
            serde_json::from_str::<serde_json::Value>(trimmed)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok())
        } else {
            None
        };

        let is_code = parsed_json.is_some()
            || trimmed.contains("fn ")
            || trimmed.contains("pub fn ")
            || trimmed.contains("def ")
            || trimmed.contains("class ")
            || trimmed.contains("import ")
            || trimmed.contains("export ")
            || trimmed.contains("struct ")
            || trimmed.contains("const ")
            || trimmed.contains("let mut ")
            || trimmed.contains("async fn ")
            || trimmed.contains("func ")
            || trimmed.contains("SELECT ")
            || trimmed.contains("FROM ")
            || trimmed.contains("<div>")
            || trimmed.contains("<!DOCTYPE html>");

        let is_markdown = !is_code
            && !is_latex
            && (trimmed.lines().any(|l| {
                let l = l.trim_start();
                l.starts_with("# ")
                    || l.starts_with("## ")
                    || l.starts_with("### ")
                    || l.starts_with("> ")
                    || l.starts_with("- ")
                    || l.starts_with("* ")
                    || l.starts_with("1. ")
            }) || trimmed.contains("```")
                || trimmed.contains("**")
                || (trimmed.contains('[') && trimmed.contains("](")));

        if is_url {
            let rows = format_wrapped_preview_lines(trimmed, 50);

            div()
                .id("clip-preview-link-card")
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .min_w(px(0.0))
                .max_w_full()
                .overflow_x_hidden()
                .p_4()
                .rounded_lg()
                .bg(rgb(0x141517))
                .border_1()
                .border_color(rgb(0x27272a))
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div().flex().items_center().gap_2().child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(rgb(0x113c30))
                            .text_size(px(11.0))
                            .text_color(rgb(COLOR_ACCENT))
                            .font_weight(FontWeight::BOLD)
                            .child("↗ Link"),
                    ),
                )
                .child(
                    clip_preview_uniform_list(
                        "clip-preview-link-list",
                        ClipPreviewKind::Link,
                        rows,
                    )
                    .flex_1()
                    .min_h(px(0.0)),
                )
        } else if is_latex {
            let rows = format_wrapped_preview_lines(raw_text, 50);

            div()
                .id("clip-preview-latex-box")
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .min_w(px(0.0))
                .max_w_full()
                .overflow_x_hidden()
                .p_3p5()
                .rounded_lg()
                .bg(rgb(0x15161a))
                .border_1()
                .border_color(rgb(0x312e81))
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pb_2()
                        .border_b_1()
                        .border_color(rgb(0x1f2937))
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(rgb(0x2e1065))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xc084fc))
                                .child("Σ LaTeX Formula"),
                        ),
                )
                .child(
                    clip_preview_uniform_list(
                        "clip-preview-latex-list",
                        ClipPreviewKind::Latex,
                        rows,
                    )
                    .flex_1()
                    .min_h(px(0.0)),
                )
        } else if let Some(json_str) = parsed_json {
            let rows = format_wrapped_preview_lines(&json_str, 50);

            div()
                .id("clip-preview-json-box")
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .min_w(px(0.0))
                .max_w_full()
                .overflow_x_hidden()
                .p_3p5()
                .rounded_lg()
                .bg(rgb(0x111214))
                .border_1()
                .border_color(rgb(0x27272a))
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pb_2()
                        .border_b_1()
                        .border_color(rgb(0x1f2937))
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(rgb(0x451a03))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xfbbf24))
                                .child("{ } JSON"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(format!("{} lines", rows.len())),
                        ),
                )
                .child(
                    clip_preview_uniform_list(
                        "clip-preview-json-list",
                        ClipPreviewKind::Json,
                        rows,
                    )
                    .flex_1()
                    .min_h(px(0.0)),
                )
        } else if is_code {
            let rows = format_wrapped_preview_lines(raw_text, 50);

            div()
                .id("clip-preview-code-box")
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .min_w(px(0.0))
                .max_w_full()
                .overflow_x_hidden()
                .p_3p5()
                .rounded_lg()
                .bg(rgb(0x111214))
                .border_1()
                .border_color(rgb(0x27272a))
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pb_2()
                        .border_b_1()
                        .border_color(rgb(0x1f2937))
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(rgb(0x0c4a6e))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0x38bdf8))
                                .child("</> Code"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(format!("{} lines", rows.len())),
                        ),
                )
                .child(
                    clip_preview_uniform_list(
                        "clip-preview-code-list",
                        ClipPreviewKind::Code,
                        rows,
                    )
                    .flex_1()
                    .min_h(px(0.0)),
                )
        } else if is_markdown {
            let mut in_code_block = false;
            let mut md_elements: Vec<Div> = Vec::new();

            for line in raw_text.lines() {
                let t = line.trim();
                if t.starts_with("```") {
                    in_code_block = !in_code_block;
                    continue;
                }
                if in_code_block {
                    for wrapped_l in format_wrapped_preview_lines(line, 50) {
                        md_elements.push(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .font_family("Menlo")
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(0x111214))
                                .text_size(px(12.5))
                                .text_color(rgb(0x38bdf8))
                                .child(wrapped_l),
                        );
                    }
                } else if let Some(h1) = t.strip_prefix("# ") {
                    for wrapped_h1 in format_wrapped_preview_lines(h1, 45) {
                        md_elements.push(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .text_size(px(18.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(COLOR_TEXT))
                                .pb_1()
                                .border_b_1()
                                .border_color(rgb(COLOR_DIVIDER))
                                .child(wrapped_h1),
                        );
                    }
                } else if let Some(h2) = t.strip_prefix("## ") {
                    for wrapped_h2 in format_wrapped_preview_lines(h2, 45) {
                        md_elements.push(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(COLOR_TEXT))
                                .pt_1()
                                .child(wrapped_h2),
                        );
                    }
                } else if let Some(h3) = t.strip_prefix("### ") {
                    for wrapped_h3 in format_wrapped_preview_lines(h3, 45) {
                        md_elements.push(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .text_size(px(14.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(COLOR_TEXT))
                                .child(wrapped_h3),
                        );
                    }
                } else if let Some(quote) = t.strip_prefix("> ") {
                    for wrapped_quote in format_wrapped_preview_lines(quote, 48) {
                        md_elements.push(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .border_l_2()
                                .border_color(rgb(COLOR_ACCENT))
                                .pl_2()
                                .py_0p5()
                                .text_size(px(13.5))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(wrapped_quote),
                        );
                    }
                } else if let Some(bullet) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                    for (b_idx, wrapped_bullet) in format_wrapped_preview_lines(bullet, 48)
                        .into_iter()
                        .enumerate()
                    {
                        md_elements.push(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .flex()
                                .gap_2()
                                .items_start()
                                .child(
                                    div()
                                        .text_color(rgb(COLOR_ACCENT))
                                        .text_size(px(13.5))
                                        .child(if b_idx == 0 { "•" } else { " " }),
                                )
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .max_w_full()
                                        .text_size(px(13.5))
                                        .text_color(rgb(COLOR_TEXT))
                                        .child(wrapped_bullet),
                                ),
                        );
                    }
                } else if t.is_empty() {
                    md_elements.push(div().h(px(8.0)));
                } else {
                    for wrapped_l in format_wrapped_preview_lines(line, 50) {
                        md_elements.push(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .text_size(px(13.5))
                                .text_color(rgb(COLOR_TEXT))
                                .child(wrapped_l),
                        );
                    }
                }
            }

            div()
                .id("clip-preview-markdown-box")
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .min_w(px(0.0))
                .max_w_full()
                .overflow_x_hidden()
                .p_3p5()
                .rounded_lg()
                .bg(rgb(0x141517))
                .border_1()
                .border_color(rgb(0x27272a))
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pb_2()
                        .border_b_1()
                        .border_color(rgb(0x1f2937))
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(rgb(0x064e3b))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(COLOR_ACCENT))
                                .child("M↓ Markdown"),
                        ),
                )
                .child(
                    div()
                        .id("clip-preview-markdown-scroll")
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .children(md_elements),
                )
        } else {
            let rows = format_wrapped_preview_lines(raw_text, 50);

            div()
                .id("clip-preview-text-box")
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .min_w(px(0.0))
                .max_w_full()
                .overflow_x_hidden()
                .p_3p5()
                .rounded_lg()
                .bg(rgb(0x141517))
                .border_1()
                .border_color(rgb(0x27272a))
                .flex()
                .flex_col()
                .child(
                    clip_preview_uniform_list(
                        "clip-preview-text-list",
                        ClipPreviewKind::Text,
                        rows,
                    )
                    .flex_1()
                    .min_h(px(0.0)),
                )
        }
    }

    /// The actions surface, floating over the list and anchored above the
    /// footer: item title, grouped action rows with hotkey hints, and
    /// an actions filter at the bottom.
    fn actions_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let filtered = self.filtered_actions();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut previous_group = None;
        for (position, &index) in filtered.iter().enumerate() {
            let Some(entry) = self.actions.get(index) else {
                continue;
            };
            let selected = position == self.action_selected;
            if previous_group.is_some_and(|group| group != entry.group) {
                rows.push(
                    div()
                        .flex_none()
                        .mx_1()
                        .my_1()
                        .h(px(1.0))
                        .bg(rgb(COLOR_DIVIDER))
                        .into_any_element(),
                );
            }
            previous_group = Some(entry.group);
            rows.push(
                div()
                    .id(SharedString::from(format!("action-{position}")))
                    .flex_none()
                    .w_full()
                    .flex()
                    .items_center()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
                    .when(!selected, |row| {
                        row.hover(|style| style.bg(rgb(COLOR_ROW_SELECTED)))
                    })
                    .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                        launcher.action_selected = position;
                        launcher.run_selected_action(window, cx);
                    }))
                    .child(action_icon(
                        &entry.icon,
                        entry.group == corvo_core::ActionGroup::Destructive,
                    ))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .when(
                                entry.group == corvo_core::ActionGroup::Destructive,
                                |label| label.text_color(rgb(COLOR_DESTRUCTIVE)),
                            )
                            .child(entry.label.clone()),
                    )
                    .child(div().flex_1())
                    .when_some(entry.hotkey, |row, hotkey| {
                        row.child(div().flex().gap_1().children(shortcut_keycaps(hotkey)))
                    })
                    .into_any_element(),
            );
        }
        div()
            .id("actions-menu")
            .occlude()
            .absolute()
            .right(px(12.0))
            .bottom(px(10.0))
            .w(px(280.0))
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(rgb(COLOR_DIVIDER))
            .bg(rgba(COLOR_MENU))
            .p_1()
            .child(
                div().px_2().pt_1().pb_1().child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child(self.actions_title.clone()),
                ),
            )
            .child(
                div()
                    .id("actions-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(260.0))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(div().flex_none().mx_1().h(px(1.0)).bg(rgb(COLOR_DIVIDER)))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_2()
                    .py_2()
                    .text_size(px(13.0))
                    .child(if self.actions_filter.is_empty() {
                        div()
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("Search for actions...")
                    } else {
                        div().child(self.actions_filter.clone())
                    }),
            )
    }

    fn primary_action_label(&self) -> (&'static str, &'static str) {
        if self.page == LauncherPage::Uninstaller {
            let confirming =
                matches!(&self.uninstaller, UninstallerState::Ready(ready) if ready.confirming);
            return (
                if confirming && cfg!(target_os = "macos") {
                    "Confirm Move to Trash"
                } else if confirming {
                    "Confirm Uninstall"
                } else {
                    "Uninstall Application"
                },
                "↵",
            );
        }
        if self.page == LauncherPage::Clipboard {
            return ("Paste to Active App", "↵");
        }
        if let Some(result) = self.selected_result() {
            if result.id == "brew:upgrade-all" {
                return ("Upgrade All", "↵");
            }
            if result.id == "kill-process:open-ports" {
                return ("Open Port Manager", "↵");
            }
            if result.id.starts_with("kill-process:open-processes") {
                return ("Open Process List", "↵");
            }
            if matches!(
                result.id.as_str(),
                "brew:show-installed"
                    | "brew:show-upgrades"
                    | "brew:manage-services"
                    | "brew:search"
            ) {
                return ("Open Brew Page", "↵");
            }
            if result.id == "emoji-picker:open" || result.id == "clipboard-manager:open" {
                return ("Open Command", "↵");
            }
            if result.id.starts_with("system-actions:setting:") {
                return ("Open Setting", "↵");
            }
            if result.id.starts_with("system-actions:action:") {
                return ("Run Action", "↵");
            }
            if result.id.starts_with("window-management:") {
                return ("Tile Window", "↵");
            }
            let prefix = result.id.split(':').next().unwrap_or("");
            match prefix {
                "app-launcher" => ("Open Application", "↵"),
                "emoji-picker" => ("Paste to Active App", "↵"),
                "clipboard-manager" | "snippets" => ("Paste to Active App", "↵"),
                "calculator" => ("Copy Answer", "↵"),
                "quicklinks" | "web-search-fallback" => ("Open in Browser", "↵"),
                "system-actions" | "window-management" => ("Run Action", "↵"),
                "file-search" => ("Open File", "↵"),
                "brew" => ("Open Homebrew Page", "↵"),
                _ => ("Open", "↵"),
            }
        } else {
            ("Open", "↵")
        }
    }

    fn footer(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let (action_name, action_key, action_is_destructive) = if self.actions_open {
            self.selected_action()
                .map(|action| {
                    (
                        action.label.clone(),
                        "↵",
                        action.group == corvo_core::ActionGroup::Destructive,
                    )
                })
                .unwrap_or_else(|| {
                    let (name, key) = self.primary_action_label();
                    (name.to_string(), key, false)
                })
        } else if self.page == LauncherPage::Uninstaller {
            let (name, key) = self.primary_action_label();
            (name.to_string(), key, true)
        } else if self.page == LauncherPage::Emoji || self.page == LauncherPage::Clipboard {
            let target = self.previous_app_name.as_deref().unwrap_or("Active App");
            (format!("Paste to {target}"), "↵", false)
        } else {
            let (name, key) = self.primary_action_label();
            if name == "Paste to Active App" {
                let target = self.previous_app_name.as_deref().unwrap_or("Active App");
                (format!("Paste to {target}"), key, false)
            } else {
                (name.to_string(), key, false)
            }
        };

        let bg = self.background_color;
        let fade_bg = linear_gradient(
            180.0,
            linear_color_stop(rgba(bg & 0xffff_ff00), 0.0),
            linear_color_stop(rgba((bg & 0xffff_ff00) | 0x38), 1.0),
        );

        // The visual pages have no result rows and no actions menu:
        // only the burger stays in their footer.
        let visual_only = matches!(self.page, LauncherPage::Extension(_));
        let show_primary = !visual_only
            && (self.page != LauncherPage::Uninstaller
                || matches!(&self.uninstaller, UninstallerState::Ready(ready) if !ready.scan_in_progress));
        let show_actions = !visual_only
            && (self.page != LauncherPage::Uninstaller
                || matches!(&self.uninstaller, UninstallerState::Ready(ready) if !ready.scan_in_progress));

        div()
            .id("footer-overlay")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(48.0))
            .child(div().id("footer-fade").absolute().inset_0().bg(fade_bg))
            .child(
                div()
                    .id("footer-controls")
                    .relative()
                    .size_full()
                    .flex()
                    .items_end()
                    .justify_between()
                    .px(px(14.0))
                    .pb(px(8.0))
                    .child(
                        // Left: Circular floating burger button
                        div()
                            .id("burger-button")
                            .occlude()
                            .cursor_pointer()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_full()
                            .bg(rgba(0x23262df6))
                            .border_1()
                            .border_color(rgba(0xffffff30))
                            .shadow_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(rgba(0x323640fb)).border_color(rgba(0xffffff48)))
                            .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                launcher.toggle_burger_menu(cx);
                            }))
                            .child(icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::LIST,
                                rgb(0xffffff),
                                14.0,
                            )),
                    )
                    .when_some(
                        self.results
                            .get(self.selected)
                            .filter(|_| self.page == LauncherPage::Emoji),
                        |controls, res| {
                            controls.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(rgb(COLOR_TEXT))
                                            .child(res.title.clone()),
                                    )
                                    .when_some(res.subtitle.clone(), |info, cat| {
                                        info.child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(rgb(COLOR_TEXT_DIM))
                                                .child(format!("· {cat}")),
                                        )
                                    }),
                            )
                        },
                    )
                    .child(div().flex_1())
                    .when(show_primary, |controls| {
                        controls.child(
                            // Right: Single combined pill capsule
                            div()
                                .id("footer-pill")
                                .occlude()
                                .h(px(28.0))
                                .px_3()
                                .rounded_full()
                                .bg(rgba(0x23262df6))
                                .border_1()
                                .border_color(rgba(0xffffff30))
                                .shadow_md()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .id("footer-open")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap_1p5()
                                        .hover(|s| s.opacity(0.85))
                                        .on_click(cx.listener(
                                            |launcher, _: &ClickEvent, window, cx| {
                                                cx.stop_propagation();
                                                if launcher.actions_open {
                                                    launcher.run_selected_action(window, cx);
                                                } else if launcher.page == LauncherPage::Uninstaller
                                                {
                                                    launcher.begin_uninstall(cx);
                                                } else {
                                                    launcher.execute_selected(window, cx);
                                                }
                                            },
                                        ))
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(if action_is_destructive {
                                                    rgb(COLOR_DESTRUCTIVE)
                                                } else {
                                                    rgb(0xffffff)
                                                })
                                                .child(action_name),
                                        )
                                        .child(action_keycap(action_key)),
                                )
                                .when(show_actions, |pill| {
                                    pill.child(
                                        div()
                                            .id("footer-actions")
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .hover(|s| s.opacity(0.85))
                                            .on_click(cx.listener(
                                                |launcher, _: &ClickEvent, _window, cx| {
                                                    cx.stop_propagation();
                                                    launcher.toggle_actions(cx);
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xd1d5db))
                                                    .child("Actions"),
                                            )
                                            .child(
                                                div().flex().items_center().gap_1().children(
                                                    corvo_core::shortcut::keycaps("cmd+k")
                                                        .iter()
                                                        .map(|label| action_keycap(label)),
                                                ),
                                            ),
                                    )
                                }),
                        )
                    }),
            )
    }

    fn calculator_card(
        &self,
        index: usize,
        result: &SearchResult,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = index == self.selected;
        let expr = result
            .subtitle
            .as_deref()
            .and_then(|s| s.strip_prefix("= "))
            .unwrap_or(&self.query);
        let result_val = &result.title;
        let is_error = result.accessory.as_deref() == Some("Error");
        let is_conversion = result.accessory.as_deref() == Some("Unit conversion");
        let result_pill = if is_error {
            "Error"
        } else if is_conversion {
            "Conversion"
        } else {
            "Result"
        };

        div()
            .id(SharedString::from(format!("calculator-card-{index}")))
            .w_full()
            .flex_none()
            .h(px(94.0))
            .mb_2()
            .rounded_xl()
            .border_1()
            .border_color(if is_error {
                rgb(COLOR_DESTRUCTIVE)
            } else if selected {
                rgb(COLOR_ACCENT)
            } else {
                rgb(COLOR_DIVIDER)
            })
            .bg(if is_error {
                rgb(0x221515)
            } else if selected {
                rgb(0x192822)
            } else {
                rgb(0x1c1e22)
            })
            .when(!selected, |card| card.hover(|s| s.bg(rgb(0x202327))))
            .cursor_pointer()
            .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                launcher.select(index, cx);
                launcher.execute_selected(window, cx);
            }))
            .flex()
            .items_center()
            .justify_between()
            .px_6()
            .py_3()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child(expr.to_string()),
                    )
                    .child(
                        div()
                            .px_2p5()
                            .py_0p5()
                            .rounded_full()
                            .bg(rgb(COLOR_KEYCAP))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("Expression"),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::ARROW_RIGHT,
                        rgb(COLOR_TEXT_DIM),
                        18.0,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(if is_error { 18.0 } else { 24.0 }))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if is_error {
                                rgb(COLOR_DESTRUCTIVE)
                            } else if selected {
                                rgb(COLOR_ACCENT)
                            } else {
                                rgb(0xe2e8f0)
                            })
                            .child(result_val.to_string()),
                    )
                    .child(
                        div()
                            .px_2p5()
                            .py_0p5()
                            .rounded_full()
                            .bg(if is_error {
                                rgb(0x381313)
                            } else if selected {
                                rgb(COLOR_ROW_SELECTED)
                            } else {
                                rgb(0x13382c)
                            })
                            .text_size(px(10.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if is_error {
                                rgb(COLOR_DESTRUCTIVE)
                            } else {
                                rgb(COLOR_ACCENT)
                            })
                            .child(result_pill),
                    ),
            )
    }

    fn result_row(
        &self,
        index: usize,
        result: &SearchResult,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if result.id.starts_with("calculator:") {
            return self.calculator_card(index, result, cx);
        }
        let selected = index == self.selected;
        let subtitle = if is_category_subtitle(result) {
            None
        } else {
            result.subtitle.clone()
        };
        div()
            .id(SharedString::from(format!("result-{index}")))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .cursor_pointer()
            .px(px(8.0))
            .mb_0p5()
            .h(px(self.row_height))
            .rounded_lg()
            .when(selected, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
            .when(!selected, |row| row.hover(|s| s.bg(rgb(0x181b1e))))
            .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                launcher.select_result_index(index, cx);
                launcher.execute_selected(window, cx);
            }))
            .child(icon(
                result.icon.clone(),
                result.id.starts_with("app-launcher:"),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .overflow_hidden()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_none()
                            .text_size(px(14.0))
                            .text_color(if selected {
                                rgb(0xffffff)
                            } else {
                                rgb(COLOR_TEXT)
                            })
                            .font_weight(if selected {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_overflow(TextOverflow::Truncate(SharedString::new_static("…")))
                            .child(result.title.clone()),
                    )
                    .when_some(subtitle, |row, sub| {
                        row.child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_overflow(TextOverflow::Truncate(SharedString::new_static(
                                    "…",
                                )))
                                .child(sub),
                        )
                    }),
            )
            .child(div().flex_none())
            .when_some(result.accessory.clone(), |row, accessory| {
                // An accessory can be a shortcut, written in token form
                // by the command crate. Render it with this platform's
                // labels so a Windows row reads "Ctrl Alt C" and not the
                // raw string "cmd+alt+c". Anything that is not a shortcut
                // passes through unchanged.
                let text = shortcut_text(&accessory);
                row.child(
                    div()
                        .flex_none()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .text_size(px(12.0))
                        .text_color(if selected {
                            rgb(0x8cb8a3)
                        } else {
                            rgb(COLOR_TEXT_DIM)
                        })
                        .child(text),
                )
            })
    }
}

/// The magnifier icon at the left of the search row.
fn search_icon() -> Div {
    div().flex_none().child(icons::render_phosphor_svg(
        phosphor_svgs::style::regular::MAGNIFYING_GLASS,
        rgb(COLOR_TEXT_ICON),
        20.0,
    ))
}

/// One keycap badge with subtle border, as in the Raycast footer.
fn action_keycap(key: &str) -> Div {
    if key == "↵" {
        div()
            .flex_none()
            .min_w(px(18.0))
            .h(px(18.0))
            .px_1()
            .rounded_xs()
            .bg(rgba(0xffffff28))
            .border_1()
            .border_color(rgba(0xffffff38))
            .flex()
            .items_center()
            .justify_center()
            .child(icons::render_phosphor_svg(
                phosphor_svgs::style::regular::ARROW_ELBOW_DOWN_LEFT,
                rgb(0xffffff),
                10.0,
            ))
    } else {
        div()
            .flex_none()
            .min_w(px(18.0))
            .h(px(18.0))
            .px_1()
            .rounded_xs()
            .bg(rgba(0xffffff24))
            .border_1()
            .border_color(rgba(0xffffff33))
            .text_size(px(11.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(0xffffff))
            .flex()
            .items_center()
            .justify_center()
            .child(key.to_string())
    }
}

/// The modifier state of one keystroke, named for what the launcher
/// means by each flag.
///
/// GPUI's `Modifiers::platform` is the Command key on macOS, the Windows
/// key on Windows, and the Super key on Linux, so a shortcut bound to it
/// never fires on Windows or Linux. This type resolves the platform's
/// primary modifier once and exposes the two questions the key handlers
/// actually ask.
#[derive(Clone, Copy)]
struct Shortcuts {
    /// The platform's primary shortcut modifier: Command on macOS,
    /// Control on Windows and Linux.
    command: bool,
    /// The physical Control key.
    ctrl: bool,
    alt: bool,
    shift: bool,
}

impl Shortcuts {
    fn from_modifiers(mods: &gpui::Modifiers) -> Self {
        Self {
            command: match corvo_core::Primary::current() {
                corvo_core::Primary::Command => mods.platform,
                corvo_core::Primary::Control => mods.control,
            },
            ctrl: mods.control,
            alt: mods.alt,
            shift: mods.shift,
        }
    }

    /// True when no shortcut modifier is held, so the keystroke is plain
    /// text. Use this to gate typing and unmodified key handling.
    ///
    /// This excludes the primary modifier and the physical Control key
    /// separately, because on Windows and Linux they are the same key and
    /// either check alone would let a shortcut through as text.
    fn is_unmodified(&self) -> bool {
        !self.command && !self.ctrl && !self.alt
    }

    /// True when Control is held on its own, with no other shortcut
    /// modifier. Covers the readline bindings such as Ctrl+P and Ctrl+W.
    ///
    /// Off macOS the primary modifier is Control, so a chord carrying
    /// both is one key, not two.
    fn is_ctrl_alone(&self) -> bool {
        self.ctrl && !self.alt && !(self.command && !self.is_ctrl_primary())
    }

    /// True when the primary modifier is Control, so `command` and `ctrl`
    /// are the same physical key on this platform.
    fn is_ctrl_primary(&self) -> bool {
        matches!(corvo_core::Primary::current(), corvo_core::Primary::Control)
    }

    /// True when the primary modifier is held and Control is not a
    /// separate key. This is the family that opens the actions menu and
    /// the settings window.
    fn is_command_alone(&self) -> bool {
        self.command && !self.alt && !self.shift && (!self.ctrl || self.is_ctrl_primary())
    }
}

/// Names the platform's file manager, for example "Finder".
///
/// Windows and Linux have no Finder, so a label that reads "Show in
/// Finder" there points at an application that does not exist.
fn file_manager() -> &'static str {
    if cfg!(target_os = "macos") {
        "Finder"
    } else if cfg!(target_os = "windows") {
        "Explorer"
    } else {
        "Files"
    }
}

/// Builds a label such as "Show in Finder" for the running platform.
fn file_manager_label(prefix: &str) -> String {
    format!("{prefix} {}", file_manager())
}

/// The image element for the file preview pane.
///
/// On Windows the plain `img` path uploads RGBA bytes into a BGRA texture,
/// which shows every preview with red and blue exchanged. The application
/// icon path already swaps the channels; reuse it here so both surfaces
/// decode the same way.
fn file_preview_image(path: std::path::PathBuf) -> impl IntoElement {
    // A Windows icon is decoded on a worker thread, so the first frame
    // after a cache miss has no pixels yet. Draw the cached one when it
    // is there, and ask for a decode on every miss.
    #[cfg(target_os = "windows")]
    if let Some(decoded) = windows_cached_render_icon(&path) {
        return img(decoded)
            .max_w_full()
            .max_h(px(300.0))
            .rounded_lg()
            .into_any_element();
    } else {
        windows_request_render_icon(&path);
    }

    img(path)
        .max_w_full()
        .max_h(px(300.0))
        .rounded_lg()
        .into_any_element()
}

/// Renders a result accessory for display.
///
/// Most accessories are plain words such as "Application". A few carry a
/// shortcut, which the command crate writes in token form. This returns
/// the accessory with a shortcut translated to this platform's labels,
/// and every other string untouched.
fn shortcut_text(accessory: &str) -> String {
    let looks_like_shortcut = corvo_core::shortcut::tokens(accessory)
        .iter()
        .any(|token| corvo_core::shortcut::primary_from_token(token).is_some());
    if !looks_like_shortcut {
        return accessory.to_string();
    }
    corvo_core::shortcut::keycaps(accessory).join("+")
}

/// One key of a shortcut hint, drawn as a keycap.
///
/// Takes a label rather than a character: Windows and Linux spell the
/// primary modifier `Ctrl`, which is four characters in one key.
fn keycap(label: &str) -> Div {
    let container = div()
        .flex_none()
        .min_w(px(18.0))
        .h(px(18.0))
        .px_1()
        .rounded_sm()
        .bg(rgb(COLOR_KEYCAP))
        .flex()
        .items_center()
        .justify_center();

    if label == "↵" {
        container.child(icons::render_phosphor_svg(
            phosphor_svgs::style::regular::ARROW_ELBOW_DOWN_LEFT,
            rgb(COLOR_TEXT_DIM),
            11.0,
        ))
    } else {
        container
            .text_size(px(11.0))
            .text_color(rgb(COLOR_TEXT_DIM))
            .child(label.to_string())
    }
}

/// The keycaps for a shortcut string, one per key.
fn shortcut_keycaps(shortcut: &str) -> Vec<Div> {
    corvo_core::shortcut::keycaps(shortcut)
        .iter()
        .map(|label| keycap(label))
        .collect()
}

/// The icon slot of an actions-menu row.
fn action_icon(icon: &Icon, destructive: bool) -> Div {
    let icon_color = if destructive {
        rgb(COLOR_DESTRUCTIVE)
    } else {
        rgb(COLOR_TEXT_ICON)
    };
    let slot = div()
        .flex_none()
        .mr(px(8.0))
        .w(px(20.0))
        .flex()
        .items_center()
        .justify_center();

    if let Some(svg_data) = icons::icon_svg_data(icon) {
        slot.child(icons::render_phosphor_svg(svg_data, icon_color, 17.0))
    } else if let Icon::Glyph(glyph) = icon {
        slot.text_size(px(16.0))
            .text_color(icon_color)
            .child(*glyph)
    } else {
        slot.child(icons::render_phosphor_svg(
            phosphor_svgs::style::regular::APP_WINDOW,
            icon_color,
            17.0,
        ))
    }
}

/// The icon slot at the left of a row: a decoded PNG when the command
/// extracted one, a vector SVG or glyph otherwise.
#[cfg(target_os = "windows")]
fn windows_render_icons() -> &'static std::sync::RwLock<
    std::collections::HashMap<std::path::PathBuf, std::sync::Arc<gpui::RenderImage>>,
> {
    static ICONS: std::sync::OnceLock<
        std::sync::RwLock<
            std::collections::HashMap<std::path::PathBuf, std::sync::Arc<gpui::RenderImage>>,
        >,
    > = std::sync::OnceLock::new();
    ICONS.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()))
}

#[cfg(target_os = "windows")]
fn windows_cached_render_icon(path: &std::path::Path) -> Option<std::sync::Arc<gpui::RenderImage>> {
    windows_render_icons().read().ok()?.get(path).cloned()
}

#[cfg(target_os = "windows")]
fn windows_render_icon(path: &std::path::Path) -> Option<std::sync::Arc<gpui::RenderImage>> {
    let icons = windows_render_icons();
    if let Some(icon) = icons.read().ok()?.get(path).cloned() {
        return Some(icon);
    }
    let mut pixels = image::open(path).ok()?.into_rgba8();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let frame = image::Frame::new(pixels);
    let icon = std::sync::Arc::new(gpui::RenderImage::new(smallvec::SmallVec::from_elem(
        frame, 1,
    )));
    icons.write().ok()?.insert(path.to_path_buf(), icon.clone());
    Some(icon)
}

#[cfg(target_os = "windows")]
fn windows_request_render_icon(path: &std::path::Path) {
    type Requests = (
        std::collections::HashSet<std::path::PathBuf>,
        std::collections::HashMap<std::path::PathBuf, std::time::Instant>,
    );
    static REQUESTS: std::sync::OnceLock<std::sync::Mutex<Requests>> = std::sync::OnceLock::new();
    let requests = REQUESTS.get_or_init(|| std::sync::Mutex::new(Default::default()));
    let Ok(mut state) = requests.lock() else {
        return;
    };
    if state.0.contains(path)
        || state
            .1
            .get(path)
            .is_some_and(|failed_at| failed_at.elapsed() < std::time::Duration::from_secs(2))
    {
        return;
    }
    state.1.remove(path);
    state.0.insert(path.to_path_buf());
    drop(state);
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let mut decoded = false;
        for delay_ms in [0, 100, 500] {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
            if windows_render_icon(&path).is_some() {
                decoded = true;
                break;
            }
        }
        if let Ok(mut state) = requests.lock() {
            state.0.remove(&path);
            if !decoded {
                state.1.insert(path, std::time::Instant::now());
            }
        }
        if decoded {
            let _ = windows_icon_ready_channel().0.try_send(());
        }
    });
}

fn icon(icon: Icon, is_application: bool) -> Div {
    let icon_size = if is_application { ICON_SIZE } else { 18.0 };
    let slot = div()
        .flex_none()
        .mr(px(8.0))
        .size(px(ICON_SIZE))
        .flex()
        .items_center()
        .justify_center();
    match icon {
        Icon::Image(path) => {
            #[cfg(target_os = "windows")]
            {
                if is_application {
                    match windows_cached_render_icon(&path) {
                        Some(decoded) => slot.child(img(decoded).size(px(icon_size))),
                        None => {
                            windows_request_render_icon(&path);
                            slot.child(icons::render_phosphor_svg(
                                phosphor_svgs::style::regular::APP_WINDOW,
                                rgb(COLOR_TEXT_ICON),
                                icon_size,
                            ))
                        }
                    }
                } else {
                    slot.child(img(path).size(px(icon_size)))
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                slot.child(img(path).size(px(icon_size)))
            }
        }
        Icon::Glyph(glyph) if !glyph.is_empty() => slot.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(icon_size))
                .text_color(rgb(COLOR_TEXT_ICON))
                .child(glyph),
        ),
        other => {
            let svg =
                icons::icon_svg_data(&other).unwrap_or(phosphor_svgs::style::regular::APP_WINDOW);
            slot.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::render_phosphor_svg(
                        svg,
                        rgb(COLOR_TEXT_ICON),
                        icon_size,
                    )),
            )
        }
    }
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_palette_size(window, cx);
        let is_collapsed = self.is_compact_collapsed();
        let is_root = self.page == LauncherPage::Root;
        let is_emoji = self.page == LauncherPage::Emoji;
        let is_clipboard = self.page == LauncherPage::Clipboard;
        let is_files = self.page == LauncherPage::Files;
        let is_brew = self.page == LauncherPage::Brew;
        let is_text = self.page == LauncherPage::Text;
        let is_notes = self.page == LauncherPage::Notes;
        let is_browser = matches!(self.page, LauncherPage::Browser(_));
        let is_extension = matches!(self.page, LauncherPage::Extension(_));
        let is_ports = self.page == LauncherPage::Ports;
        let is_processes = self.page == LauncherPage::Processes;
        let is_uninstaller = self.page == LauncherPage::Uninstaller;
        div()
            .track_focus(&self.focus_handle)
            .key_context("Launcher")
            .on_action(cx.listener(
                |launcher: &mut Self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>| {
                    launcher.handle_escape(window, cx);
                },
            ))
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .pt_1()
            .rounded(px(launcher_corner_radius()))
            .border_1()
            .border_color(rgb(COLOR_DIVIDER))
            .overflow_hidden()
            .bg(rgba(self.background_color))
            .text_color(rgb(COLOR_TEXT))
            .font_family("Helvetica")
            .when(is_root, |view| {
                view.child(self.search_row(window, cx))
                    .when(!is_collapsed, |v| v.child(self.results_list(cx)))
            })
            .when(is_emoji, |view| {
                view.child(self.emoji_search_row(window, cx))
                    .child(self.emoji_subheader(cx))
                    .child(self.emoji_grid_view(cx))
            })
            .when(is_clipboard, |view| {
                view.child(self.clipboard_search_row(window, cx))
                    .child(self.clipboard_split_view(cx))
            })
            .when(is_files, |view| {
                view.child(self.search_row(window, cx))
                    .child(self.file_search_split_view(cx))
            })
            .when(is_brew, |view| {
                view.child(self.search_row(window, cx))
                    .child(self.brew_subheader())
                    .child(self.results_list(cx))
            })
            .when(is_text, |view| {
                view.child(self.search_row(window, cx))
                    .child(self.text_subheader())
                    .child(self.results_list(cx))
            })
            .when(is_notes, |view| {
                view.child(self.search_row(window, cx))
                    .child(self.notes_subheader())
                    .child(self.results_list(cx))
            })
            .when(is_browser, |view| {
                let browser = match self.page {
                    LauncherPage::Browser(browser) => browser,
                    _ => return view,
                };
                view.child(self.search_row(window, cx))
                    .child(self.browser_subheader(browser))
                    .child(self.results_list(cx))
            })
            .when(is_extension, |view| {
                let _command_id = match self.page {
                    LauncherPage::Extension(id) => id,
                    _ => return view,
                };
                view.child(self.search_row(window, cx))
                    .child(self.extension_page_view(cx))
            })
            .when(is_ports, |view| {
                view.child(self.search_row(window, cx))
                    .child(self.ports_subheader())
                    .child(self.results_list(cx))
            })
            .when(is_processes, |view| {
                view.child(self.search_row(window, cx))
                    .child(self.processes_subheader())
                    .child(self.results_list(cx))
            })
            .when(is_uninstaller, |view| {
                view.child(self.uninstaller_search_row(window, cx))
                    .child(self.uninstaller_list(cx))
            })
            .when(!is_collapsed, |view| view.child(self.footer(cx)))
            .when(self.actions_open, |view| {
                view.child(
                    div()
                        .id("actions-backdrop")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                            launcher.close_actions(cx);
                        })),
                )
                .child(self.actions_menu(cx))
            })
            .when(self.burger_menu_open, |view| {
                view.child(
                    div()
                        .id("burger-backdrop")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                            launcher.close_burger_menu(cx);
                        })),
                )
                .child(self.burger_menu(cx))
            })
            .when(self.filter_dropdown_open, |view| {
                view.child(
                    div()
                        .id("filter-dropdown-backdrop")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                            launcher.close_filter_dropdown(cx);
                        })),
                )
                .child(self.filter_dropdown_menu(cx))
            })
    }
}

fn divider_line() -> Div {
    div().flex_none().h(px(1.0)).bg(rgb(COLOR_DIVIDER))
}

fn info_row(label: &str, value: &str) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(13.0))
        .child(
            div()
                .text_color(rgb(COLOR_TEXT_DIM))
                .child(label.to_string()),
        )
        .child(div().text_color(rgb(COLOR_TEXT)).child(value.to_string()))
}

struct HotkeyManagerGlobal(std::rc::Rc<std::cell::RefCell<corvo_platform::hotkey::HotkeyManager>>);
impl gpui::Global for HotkeyManagerGlobal {}

/// Collects hotkey bindings configured across all settings tabs and files.
pub fn collect_hotkey_bindings(
    settings: &corvo_config::Settings,
) -> Vec<(String, corvo_platform::HotkeyIntent)> {
    let mut bindings = Vec::new();

    // 1. Applications
    if settings.applications.enabled {
        for (app_key, cfg) in &settings.applications.app_configs {
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::LaunchApp(std::path::PathBuf::from(app_key)),
                    ));
                }
            }
        }
    }

    // 2. Window Management
    if settings.window_management.enabled {
        for action in corvo_window_management::WINDOW_ACTIONS {
            let user_cfg = settings.window_management.command_items.get(action.id);
            let is_hidden = user_cfg.is_some_and(|c| c.hidden);
            if is_hidden {
                continue;
            }
            let hotkey = match user_cfg.and_then(|c| c.hotkey.as_ref()) {
                Some(h) if h.is_empty() => None,
                Some(h) => Some(h.clone()),
                None => action.hotkey.map(|h| h.to_string()),
            };

            if let Some(hk) = hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk,
                        corvo_platform::HotkeyIntent::TileWindow(action.id.to_string()),
                    ));
                }
            }
        }
        for (action_id, cfg) in &settings.window_management.command_items {
            if corvo_window_management::WINDOW_ACTIONS
                .iter()
                .any(|a| a.id == action_id)
            {
                continue;
            }
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    if action_id == "Create Window Layout"
                        || action_id == "Create Layout from Current Windows"
                    {
                        bindings.push((
                            hk.clone(),
                            corvo_platform::HotkeyIntent::Command(action_id.clone()),
                        ));
                    } else {
                        bindings.push((
                            hk.clone(),
                            corvo_platform::HotkeyIntent::TileWindow(action_id.clone()),
                        ));
                    }
                }
            }
        }
        for layout in &settings.window_management.layouts {
            if let Some(ref hk) = layout.hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::TileWindow(format!("layout:{}", layout.id)),
                    ));
                }
            }
        }
    }

    // 3. System Actions
    if settings.system_actions.enabled {
        for (action_id, cfg) in &settings.system_actions.items {
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::RunSystemAction(action_id.clone()),
                    ));
                }
            }
        }
    }

    // 4. System Settings
    if settings.system_settings.enabled {
        for (setting_id, cfg) in &settings.system_settings.items {
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::OpenSystemSetting(setting_id.clone()),
                    ));
                }
            }
        }
    }

    // 5. Commands
    for (cmd_id, cfg) in &settings.commands.items {
        if let Some(ref hk) = cfg.hotkey {
            if !hk.is_empty() {
                bindings.push((
                    hk.clone(),
                    corvo_platform::HotkeyIntent::Command(cmd_id.clone()),
                ));
            }
        }
    }

    // 6. Clipboard
    if settings.clipboard.enabled {
        for (item_name, cfg) in &settings.clipboard.command_items {
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    if item_name == "Clipboard History" {
                        bindings.push((hk.clone(), corvo_platform::HotkeyIntent::ClipboardHistory));
                    } else {
                        bindings.push((
                            hk.clone(),
                            corvo_platform::HotkeyIntent::Command(item_name.clone()),
                        ));
                    }
                }
            }
        }
    }

    // 7. Emojis
    for (item_name, cfg) in &settings.emojis.command_items {
        if let Some(ref hk) = cfg.hotkey {
            if !hk.is_empty() {
                if item_name == "Search Emoji & Symbols" {
                    bindings.push((hk.clone(), corvo_platform::HotkeyIntent::EmojiPicker));
                } else {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::Command(item_name.clone()),
                    ));
                }
            }
        }
    }

    // 8. Snippets
    if settings.snippets.enabled {
        for (item_name, cfg) in &settings.snippets.command_items {
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::Command(item_name.clone()),
                    ));
                }
            }
        }
    }

    // 9. File Search
    if settings.file_search.enabled {
        for (item_name, cfg) in &settings.file_search.command_items {
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::Command(item_name.clone()),
                    ));
                }
            }
        }
    }

    // 10. Quicklinks
    let quicklinks_file = corvo_config::QuicklinksFile::load();
    for q in &quicklinks_file.quicklinks {
        if let Some(ref hk) = q.hotkey {
            if !hk.is_empty() && !q.url.is_empty() {
                bindings.push((
                    hk.clone(),
                    corvo_platform::HotkeyIntent::OpenUrl(q.url.clone()),
                ));
            }
        }
    }
    for (name, cfg) in &settings.quicklinks.command_items {
        if let Some(ref hk) = cfg.hotkey {
            if !hk.is_empty() && !quicklinks_file.quicklinks.iter().any(|q| &q.name == name) {
                bindings.push((
                    hk.clone(),
                    corvo_platform::HotkeyIntent::OpenUrl(name.clone()),
                ));
            }
        }
    }

    bindings
}

/// Dynamically updates the global hotkey engine with the current configuration.
pub fn reload_active_hotkeys(settings: &corvo_config::Settings, cx: &mut App) {
    if let Some(handle) = cx.try_global::<HotkeyManagerGlobal>() {
        let bindings = collect_hotkey_bindings(settings);
        handle
            .0
            .borrow_mut()
            .update_bindings(&settings.hotkey, bindings);
    }
}

/// Starts the GPUI app. Every `HotkeyIntent` on `intent_rx` triggers the
/// corresponding launcher action or headless execution.
pub fn run(
    registry: CommandRegistry,
    store: std::sync::Arc<dyn DataStore>,
    intent_tx: smol::channel::Sender<corvo_platform::HotkeyIntent>,
    intent_rx: Receiver<corvo_platform::HotkeyIntent>,
) {
    let app_updates = corvo_app_launcher::subscribe_corpus_changes();
    let refresh_registry = registry.clone();
    let refresh_store = store.clone();
    gpui_platform::application().run(|cx: &mut App| {
        cx.set_quit_mode(gpui::QuitMode::Explicit);
        // Runs after GPUI sets the regular policy, so the launcher
        // stays out of the Dock and Cmd+Tab (SPEC §6).
        corvo_platform::run_as_agent();
        cx.set_global(RegistryGlobal(registry));
        cx.set_global(StoreGlobal(store));
        cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("Launcher"))]);

        cx.spawn(async move |cx: &mut AsyncApp| {
            while app_updates.recv().await.is_ok() {
                let registry = refresh_registry.clone();
                let store = refresh_store.clone();
                smol::unblock(move || preload_initial_results(&registry, store)).await;
                cx.update(|cx| {
                    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|global| global.0) {
                        let _ = handle.update(cx, |launcher, _window, cx| {
                            if launcher.page == LauncherPage::Root {
                                launcher.refresh(cx);
                            }
                        });
                    }
                });
            }
        })
        .detach();

        #[cfg(target_os = "windows")]
        cx.spawn(async move |cx: &mut AsyncApp| {
            let icon_ready = windows_icon_ready_channel().1.clone();
            while icon_ready.recv().await.is_ok() {
                let _ = cx.update(|cx| {
                    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|global| global.0) {
                        let _ = handle.update(cx, |launcher, _, cx| {
                            if launcher.page == LauncherPage::Root {
                                cx.notify();
                            }
                        });
                    }
                });
            }
        })
        .detach();

        // Pre-create the settings window hidden so the first open is instant.
        // 400ms delay lets the launcher render its first frame first.
        #[cfg(target_os = "windows")]
        cx.spawn(async move |cx: &mut AsyncApp| {
            smol::Timer::after(std::time::Duration::from_millis(400)).await;
            let _ = cx.update(|cx| {
                crate::settings::prewarm_settings_window(cx);
            });
        })
        .detach();

        // Initialize dynamic global hotkey subsystem
        let hotkey_mgr = corvo_platform::hotkey::HotkeyManager::new(intent_tx);
        let hotkey_holder = std::rc::Rc::new(std::cell::RefCell::new(hotkey_mgr));
        cx.set_global(HotkeyManagerGlobal(hotkey_holder));

        let (reload_tx, reload_rx) = smol::channel::unbounded::<()>();
        corvo_platform::hotkey::set_reload_sender(reload_tx);

        let initial_settings = corvo_config::Settings::load();
        corvo_platform::set_window_gap(initial_settings.window_management.gap_between_windows);
        reload_active_hotkeys(&initial_settings, cx);

        // Listen for reload notifications when user updates hotkeys in settings
        cx.spawn(async move |cx: &mut AsyncApp| {
            while reload_rx.recv().await.is_ok() {
                cx.update(|cx| {
                    let settings = corvo_config::Settings::load();
                    corvo_platform::set_window_gap(settings.window_management.gap_between_windows);
                    reload_active_hotkeys(&settings, cx);
                });
            }
        })
        .detach();

        // Background update pump (Tinycast model): 30s initial delay, 24h interval, 2h backoff.
        // When a newer release is found, the About tab opens with its
        // changelog plus Skip / Download buttons so the update is visible
        // instead of silently dropped.
        cx.spawn(async move |cx: &mut AsyncApp| {
            smol::Timer::after(std::time::Duration::from_secs(30)).await;
            loop {
                let settings = corvo_config::Settings::load();
                if !settings.updates.check_updates {
                    smol::Timer::after(std::time::Duration::from_secs(3600)).await;
                    continue;
                }
                let channel = corvo_platform::UpdateChannel::parse(&settings.updates.channel);
                let check_res =
                    smol::unblock(move || corvo_platform::check_for_updates(channel, false)).await;

                match check_res {
                    Ok(Some(release)) => {
                        if settings.updates.auto_download {
                            let rel = release.clone();
                            let dl_res = smol::unblock(move || {
                                let cancel_flag = std::sync::atomic::AtomicBool::new(false);
                                corvo_platform::download_and_verify(&rel, &cancel_flag, None)
                                    .map(|path| (rel, path))
                            })
                            .await;
                            match dl_res {
                                Ok((rel, path)) => {
                                    cx.update(|cx| {
                                        crate::open_settings_with_ready_update(rel, path, cx);
                                    });
                                }
                                Err(_) => {
                                    let rel = release.clone();
                                    cx.update(|cx| {
                                        crate::open_settings_with_available_update(rel, cx);
                                    });
                                }
                            }
                        } else {
                            let rel = release.clone();
                            cx.update(|cx| {
                                crate::open_settings_with_available_update(rel, cx);
                            });
                        }
                        smol::Timer::after(std::time::Duration::from_secs(24 * 3600)).await;
                    }
                    Ok(None) => {
                        smol::Timer::after(std::time::Duration::from_secs(24 * 3600)).await;
                    }
                    Err(_) => {
                        smol::Timer::after(std::time::Duration::from_secs(2 * 3600)).await;
                    }
                }
            }
        })
        .detach();

        // Listen for incoming hotkey intents
        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Ok(intent) = intent_rx.recv().await {
                match intent {
                    corvo_platform::HotkeyIntent::ToggleLauncher => {
                        cx.update(toggle);
                    }
                    corvo_platform::HotkeyIntent::TileWindow(action_id) => {
                        if let Some(layout_id) = action_id.strip_prefix("layout:") {
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
                                smol::spawn(async move {
                                    let _ = corvo_platform::apply_window_layout(&placements);
                                })
                                .detach();
                            }
                        } else {
                            smol::spawn(async move {
                                if let Err(err) = corvo_platform::tile_window(None, &action_id) {
                                    corvo_platform::diagnostics::record_error(
                                        "launcher",
                                        "headless_tile_failed",
                                    );
                                    eprintln!("corvo: headless tile window error: {err}");
                                }
                            })
                            .detach();
                        }
                    }
                    corvo_platform::HotkeyIntent::LaunchApp(path) => {
                        let path_str = path.to_string_lossy().to_string();
                        smol::spawn(async move {
                            if let Err(err) = corvo_platform::open_app(&path_str) {
                                corvo_platform::diagnostics::record_error(
                                    "launcher",
                                    "app_open_failed",
                                );
                                eprintln!("corvo: open app error: {err}");
                            }
                        })
                        .detach();
                    }
                    corvo_platform::HotkeyIntent::OpenUrl(url) => {
                        let _ = corvo_platform::open_url(&url);
                    }
                    corvo_platform::HotkeyIntent::RunSystemAction(action_id) => {
                        cx.update(|cx| {
                            execute_system_action_intent(&action_id, cx);
                        });
                    }
                    corvo_platform::HotkeyIntent::OpenSystemSetting(setting_id) => {
                        cx.update(|cx| {
                            execute_system_setting_intent(&setting_id, cx);
                        });
                    }
                    corvo_platform::HotkeyIntent::ClipboardHistory => {
                        cx.update(|cx| {
                            open_launcher_with_page(LauncherPage::Clipboard, cx);
                        });
                    }
                    corvo_platform::HotkeyIntent::EmojiPicker => {
                        cx.update(|cx| {
                            open_launcher_with_page(LauncherPage::Emoji, cx);
                        });
                    }
                    corvo_platform::HotkeyIntent::Command(cmd_id) => {
                        // A declared NoView quick command runs straight
                        // from its hotkey: no launcher window, the
                        // command's own toast confirms.
                        let quick_id = declared_no_view_result_id(&cmd_id);
                        if let Some(result_id) = quick_id {
                            cx.update(|cx| {
                                if let Some(handle) = cx.try_global::<LauncherWindow>().map(|g| g.0) {
                                    let _ = handle.update(cx, |launcher, _window, cx| {
                                        launcher.execute_declared_quick(&result_id, cx);
                                    });
                                }
                            });
                            return;
                        }
                        cx.update(|cx| {
                            execute_command_intent(&cmd_id, cx);
                        });
                    }
                }
            }
        })
        .detach();

        // First launch: guide instead of summoning the launcher. The marker
        // is written at show-time, so the wizard stays one-time even if the
        // user quits mid-flow.
        let first_run = !initial_settings.onboarding.shown;
        if first_run {
            // Marked only after the window exists: a failed open must leave
            // the wizard pending instead of burning the one-time flow.
            if onboarding::open_welcome(cx) {
                onboarding::mark_shown();
            }
        }
        if !first_run {
            open_launcher(cx);
        }
    });
}

fn execute_system_action_intent(action_id: &str, cx: &mut App) {
    let registry = cx.global::<RegistryGlobal>().0.clone();
    if let Some(cmd) = registry
        .commands()
        .iter()
        .find(|c| c.id() == "system-actions")
    {
        let mut target_id = action_id.to_string();
        for action in corvo_system_actions::get_system_actions() {
            if action.id == action_id || action.title.eq_ignore_ascii_case(action_id) {
                target_id = action.id.to_string();
                break;
            }
        }
        let result_id = format!("system-actions:action:{target_id}");
        let ctx = ExecutionContext::default();
        if let Ok(action) = smol::block_on(cmd.execute(&result_id, &ctx)) {
            match action {
                Action::RunShell(shell_cmd) => match target_id.as_str() {
                    "volume-up" | "volume-down" => {
                        let title = if target_id == "volume-up" {
                            "Volume Up"
                        } else {
                            "Volume Down"
                        }
                        .to_string();
                        let direction = target_id == "volume-up";
                        cx.spawn(async move |cx| {
                            let notice = run_shell_toast(
                                shell_cmd,
                                ToastCategory::System,
                                title,
                                Some(direction),
                            )
                            .await;
                            cx.update(|cx| show_action_toast(notice, cx));
                        })
                        .detach();
                    }
                    _ => {
                        // run_shell waits for the child process, so a
                        // blocking action from a hotkey would freeze the
                        // launcher while it ran.
                        let title = target_id.clone();
                        cx.spawn(async move |cx| {
                            let result =
                                smol::unblock(move || corvo_platform::run_shell(&shell_cmd)).await;
                            if let Err(error) = result {
                                let notice = ToastNotice::failure(
                                    ToastCategory::System,
                                    title,
                                    error.to_string(),
                                );
                                cx.update(|cx| show_action_toast(notice, cx));
                            }
                        })
                        .detach();
                    }
                },
                Action::OpenUrl(url) => {
                    let _ = corvo_platform::open_url(&url);
                }
                Action::Open(path) => {
                    let _ = corvo_platform::platform_ops().open_path(&path);
                }
                Action::AdjustBrightness(delta) => {
                    cx.spawn(async move |cx| {
                        let result = smol::unblock(move || {
                            corvo_platform::adjust_brightness_with_level(delta)
                        })
                        .await;
                        let notice = match result {
                            Ok(Some(percent)) => ToastNotice::success(
                                ToastCategory::Brightness,
                                "Display updated",
                                if delta >= 0.0 {
                                    "Brightness increased"
                                } else {
                                    "Brightness reduced"
                                },
                            )
                            .with_progress(percent),
                            Ok(None) => ToastNotice::success(
                                ToastCategory::Brightness,
                                "Display updated",
                                if delta >= 0.0 {
                                    "Brightness increased"
                                } else {
                                    "Brightness reduced"
                                },
                            ),
                            Err(error) => ToastNotice::failure(
                                ToastCategory::Brightness,
                                "Could not change brightness",
                                error.to_string(),
                            ),
                        };
                        cx.update(|cx| show_action_toast(notice, cx));
                    })
                    .detach();
                }
                Action::AdjustVolume(delta) => {
                    cx.spawn(async move |cx| {
                        let result = smol::unblock(move || {
                            corvo_platform::adjust_audio_output_with_level(delta)
                        })
                        .await;
                        let notice = match result {
                            Ok(Some(percent)) => ToastNotice::success(
                                ToastCategory::System,
                                "Output volume updated",
                                "",
                            )
                            .with_progress(percent),
                            Ok(None) => ToastNotice::success(
                                ToastCategory::System,
                                "Output volume updated",
                                if delta >= 0.0 {
                                    "Volume increased"
                                } else {
                                    "Volume reduced"
                                },
                            ),
                            Err(error) => ToastNotice::failure(
                                ToastCategory::System,
                                "Could not change output volume",
                                error.to_string(),
                            ),
                        };
                        cx.update(|cx| show_action_toast(notice, cx));
                    })
                    .detach();
                }
                _ => {}
            }
        }
    }
}

fn execute_system_setting_intent(setting_id: &str, cx: &mut App) {
    let registry = cx.global::<RegistryGlobal>().0.clone();
    if let Some(cmd) = registry
        .commands()
        .iter()
        .find(|c| c.id() == "system-actions")
    {
        let mut target_id = setting_id.to_string();
        for setting in corvo_system_actions::get_system_settings() {
            if setting.id == setting_id || setting.title.eq_ignore_ascii_case(setting_id) {
                target_id = setting.id.to_string();
                break;
            }
        }
        let result_id = format!("system-actions:setting:{target_id}");
        let ctx = ExecutionContext::default();
        if let Ok(action) = smol::block_on(cmd.execute(&result_id, &ctx)) {
            match action {
                Action::RunShell(shell_cmd) => {
                    // Off the main thread: run_shell waits for the child
                    // process to exit.
                    smol::unblock(move || corvo_platform::run_shell(&shell_cmd)).detach();
                }
                Action::RunNative(native) => {
                    smol::unblock(move || corvo_platform::run_native_action(native)).detach();
                }
                Action::OpenUrl(url) => {
                    let _ = corvo_platform::open_url(&url);
                }
                Action::Open(path) => {
                    let _ = corvo_platform::platform_ops().open_path(&path);
                }
                _ => {}
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn hide_launcher_before_settings(cx: &mut App) {
    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|global| global.0) {
        let _ = handle.update(cx, |launcher, window, _| {
            if launcher.visible {
                launcher.dismiss(window);
            }
        });
    }
}

/// The execute id of a declared `NoView` command matching this
/// hotkey target (`command:<extension>:<name>` or the declared
/// title), or `None` when the intent is not a declared command.
fn declared_no_view_result_id(cmd_id: &str) -> Option<String> {
    let registry = corvo_core::CommandRegistry::from_inventory();
    for command in registry.commands() {
        let manifest = command.manifest();
        for spec in &manifest.commands {
            let matches_target = cmd_id
                .strip_prefix("command:")
                .and_then(|rest| rest.split_once(':'))
                .map(|(extension, name)| extension == manifest.name && name == spec.name)
                .unwrap_or_else(|| spec.title == cmd_id);
            if matches_target && spec.mode == corvo_core::CommandMode::NoView {
                return Some(format!("{}:{}", manifest.name, spec.name));
            }
        }
    }
    None
}

fn execute_command_intent(cmd_id: &str, cx: &mut App) {
    #[cfg(target_os = "windows")]
    if matches!(
        cmd_id,
        "open-settings"
            | "Open Settings"
            | "check-for-updates"
            | "Check for Updates"
            | "about-corvo"
            | "About Corvo"
            | "export-backup"
            | "Export Backup"
            | "import-backup"
            | "Import Backup"
            | "import-from-raycast"
            | "Import from Raycast"
            | "Create Snippet"
            | "create-snippet"
    ) {
        hide_launcher_before_settings(cx);
    }
    match cmd_id {
        "open-settings" | "Open Settings" => {
            open_settings(cx);
        }
        "quit-corvo" | "Quit Corvo" => {
            corvo_platform::clear_screen_recording_relaunch_marker();
            cx.quit();
        }
        "check-for-updates" | "Check for Updates" => {
            open_settings_tab_with_update_check(SettingsTab::About, cx);
        }
        "about-corvo" | "About Corvo" => {
            open_settings_tab(SettingsTab::About, cx);
        }
        "export-backup"
        | "Export Backup"
        | "import-backup"
        | "Import Backup"
        | "import-from-raycast"
        | "Import from Raycast" => {
            open_settings_tab(SettingsTab::Backup, cx);
        }
        "Create Window Layout" | "create-window-layout" => {
            open_settings_tab(SettingsTab::WindowManagement, cx);
        }
        "Create Layout from Current Windows" | "create-layout-from-current-windows" => {
            let captured = corvo_platform::capture_current_window_layout();
            if !captured.is_empty() {
                let mut settings = corvo_config::Settings::load();
                let next_idx = settings.window_management.layouts.len() + 1;
                let new_layout = corvo_config::WindowLayoutTemplate {
                    id: format!(
                        "layout-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    ),
                    name: format!("Layout {next_idx}"),
                    hotkey: None,
                    placements: captured
                        .into_iter()
                        .map(|(app, pos)| corvo_config::WindowPlacement {
                            app_name: app,
                            position: pos,
                        })
                        .collect(),
                };
                settings.window_management.layouts.push(new_layout);
                let _ = settings.save();
                reload_active_hotkeys(&settings, cx);
            }
        }
        "reload-applications" | "Reload Applications" => {
            corvo_app_launcher::reload_corpus();
        }
        "toggle-system-appearance" | "Toggle System Appearance" => {
            let _ = corvo_platform::run_shell(
                "osascript -e 'tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode'",
            );
        }
        "open-camera" | "Open Camera" => {
            let _ = corvo_platform::open_app("Photo Booth");
        }
        "clipboard-history" | "Clipboard History" => {
            open_launcher_with_page(LauncherPage::Clipboard, cx);
        }
        "emojis" | "Search Emoji & Symbols" => {
            open_launcher_with_page(LauncherPage::Emoji, cx);
        }
        "Search Snippets" | "search-snippets" => {
            open_launcher_with_query("snippet:", cx);
        }
        "Create Snippet" | "create-snippet" => {
            open_settings_tab(SettingsTab::Snippets, cx);
        }
        "Search Files" | "search-files" => {
            open_launcher_with_page(LauncherPage::Files, cx);
        }
        "define-word" | "Define Word" => {
            open_launcher_with_query("define ", cx);
        }
        "calculator-history" | "Calculator History" => {
            open_launcher_with_query("calculator", cx);
        }
        other => {
            if let Some(layout_id) = other.strip_prefix("apply-layout:") {
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
                    smol::spawn(async move {
                        let _ = corvo_platform::apply_window_layout(&placements);
                    })
                    .detach();
                    return;
                }
            }
            open_launcher_with_query(other, cx);
        }
    }
}

fn open_launcher_with_page(page: LauncherPage, cx: &mut App) {
    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |launcher, window, cx| {
                #[cfg(target_os = "windows")]
                if !launcher.visible {
                    launcher.show(window, cx);
                }
                match page {
                    LauncherPage::Clipboard => launcher.open_clipboard_page(window, cx),
                    LauncherPage::Files => launcher.open_files_page(window, cx),
                    LauncherPage::Emoji => launcher.open_emoji_page(window, cx),
                    LauncherPage::Brew => launcher.open_brew_page(BrewPageMode::Search, window, cx),
                    LauncherPage::Text => launcher.open_text_page("", window, cx),
                    LauncherPage::Extension("pomodoro") => {
                        launcher.open_extension_page("pomodoro", "", window, cx)
                    }
                    LauncherPage::Extension("weather") => {
                        launcher.open_extension_page("weather", "", window, cx)
                    }
                    LauncherPage::Notes => launcher.open_notes_page(window, cx),
                    LauncherPage::Extension("media-control") => {
                        launcher.open_extension_page("media-control", "", window, cx)
                    }
                    LauncherPage::Browser(browser) => {
                        launcher.open_browser_page(browser, window, cx)
                    }
                    LauncherPage::Ports => launcher.open_ports_page(window, cx),
                    LauncherPage::Processes => launcher.open_processes_page("", window, cx),
                    _ => {
                        launcher.page = page;
                        launcher.refresh(cx);
                        launcher.sync_palette_size(window, cx);
                        cx.notify();
                    }
                }
            });
            #[cfg(target_os = "windows")]
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            cx.activate(true);
            return;
        }
    }
    open_launcher_for(page, String::new(), cx);
}

fn open_launcher_with_query(query: &str, cx: &mut App) {
    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |launcher, window, cx| {
                #[cfg(target_os = "windows")]
                if !launcher.visible {
                    launcher.show(window, cx);
                }
                launcher.page = LauncherPage::Root;
                launcher.query = query.to_string();
                launcher.force_expanded = false;
                launcher.cursor_idx = query.chars().count();
                launcher.refresh(cx);
                launcher.sync_palette_size(window, cx);
                cx.notify();
            });
            #[cfg(target_os = "windows")]
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            cx.activate(true);
            return;
        }
    }
    open_launcher_for(LauncherPage::Root, query.to_string(), cx);
}

fn toggle(cx: &mut App) {
    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |launcher, window, cx| {
                #[cfg(target_os = "windows")]
                if !launcher.visible {
                    launcher.show(window, cx);
                    window.activate_window();
                    return;
                }
                let _ = cx;
                launcher.dismiss(window);
            });
            return;
        }
    }
    open_launcher(cx);
}

pub(crate) fn active_display_id() -> Option<gpui::DisplayId> {
    corvo_platform::active_display_id().map(|id| gpui::DisplayId::new(id as u64))
}

#[cfg(target_os = "windows")]
fn set_windows_launcher_visible(window: &Window, visible: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            corvo_platform::set_launcher_window_visible(handle.hwnd.get(), visible);
        }
    }
}

/// Apply the region outside an App update. SetWindowRgn sends window messages.
#[cfg(target_os = "windows")]
fn apply_windows_launcher_region(window: &Window, cx: &mut App) {
    let hwnd = windows_hwnd(window);
    let radius = launcher_corner_radius() * window.scale_factor();
    cx.spawn(async move |_| {
        corvo_platform::set_launcher_window_region(hwnd, radius);
    })
    .detach();
}

/// The Win32 handle behind a GPUI window, or 0 when the platform window
/// is not a Win32 one.
#[cfg(target_os = "windows")]
fn windows_hwnd(window: &Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return 0;
    };
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get(),
        _ => 0,
    }
}

fn open_launcher(cx: &mut App) {
    open_launcher_for(LauncherPage::Root, String::new(), cx);
}

/// Brings the launcher on screen after onboarding, on every platform:
/// Windows keeps its panel resident and hidden, while macOS and Linux
/// remove the window on dismiss.
pub(crate) fn present_launcher(cx: &mut App) {
    #[cfg(target_os = "windows")]
    {
        open_launcher_with_query("", cx);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let alive = cx
            .try_global::<LauncherWindow>()
            .map(|g| g.0)
            .is_some_and(|handle| cx.windows().contains(&handle.into()));
        if !alive {
            open_launcher(cx);
        }
    }
}

fn sync_launcher_preferences(cx: &mut App) {
    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|global| global.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |launcher, window, cx| {
                launcher.sync_palette_size(window, cx);
                cx.notify();
            });
        }
    }
}

fn open_launcher_for(page: LauncherPage, query: String, cx: &mut App) {
    corvo_app_launcher::reload_corpus();
    let store = cx.global::<StoreGlobal>().0.clone();
    let (previous_pid, previous_name) = match corvo_platform::frontmost_app_info() {
        Some((pid, name)) => (Some(pid), Some(name)),
        None => (corvo_platform::frontmost_app_pid(), None),
    };
    let size_scale = match store.interface_size_option() {
        0 => 0.9,
        2 => 1.1,
        _ => 1.0,
    };
    let window_width = WINDOW_WIDTH * size_scale;
    let initial_height = palette_size(
        store.compact_mode(),
        false,
        page,
        &query,
        false,
        false,
        false,
    )
    .height(size_scale);
    // Center on the height the window will actually open at. Centering on
    // the full height and then overwriting the height leaves a compact
    // window sitting well above the middle of the screen.
    let initial_bounds = launcher_bounds(size(px(window_width), px(initial_height)), cx);
    let display_id = active_display_id();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(initial_bounds)),
        display_id,
        // No titlebar: the launcher is a floating panel, not a window.
        // macOS draws no traffic lights; Linux gets client decorations.
        titlebar: None,
        kind: window_kind(),
        is_resizable: false,
        is_movable: true,
        focus: true,
        show: !cfg!(target_os = "macos"),
        window_background: if cfg!(target_os = "windows") {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Blurred
        },
        ..Default::default()
    };
    let opened: Result<WindowHandle<Launcher>, _> = cx.open_window(options, move |window, cx| {
        cx.new(|cx| Launcher::new(window, cx, page, query))
    });
    let window = match opened {
        Ok(handle) => handle,
        Err(err) => {
            corvo_platform::diagnostics::record_error("launcher", "launcher_open_failed");
            eprintln!("corvo: cannot open launcher window: {err}");
            return;
        }
    };
    cx.set_global(LauncherWindow(window));
    #[cfg(target_os = "windows")]
    let _ = window.update(cx, |_, window, cx| {
        apply_windows_launcher_region(window, cx);
        set_windows_launcher_visible(window, true);
        window.activate_window();
    });
    cx.activate(true);
    corvo_platform::update_screens_cache();
    corvo_platform::make_panel_instant(window_width as f64, initial_height as f64);
    let clipboard_source = previous_name.clone();
    let _ = window.update(cx, |launcher, _window, cx| {
        launcher.previous_app = previous_pid;
        launcher.previous_app_name = previous_name;
        if launcher.page == LauncherPage::Root && launcher.results.is_empty() {
            launcher.results = cached_initial_results();
        }
        launcher.rebuild_root_flat_items();
        launcher.results_scroll_handle.scroll_to_item(0);
        if launcher.page == LauncherPage::Clipboard {
            launcher.rebuild_clipboard_flat_items();
            launcher.refresh_clipboard(cx);
        }
    });
    corvo_platform::order_panel_front(window_width as f64, initial_height as f64);
    cx.spawn(async move |cx: &mut AsyncApp| {
        smol::unblock(move || {
            corvo_clipboard_manager::poll_clipboard_with_source(clipboard_source.as_deref());
        })
        .await;
        cx.update(|cx| {
            if let Some(handle) = cx.try_global::<LauncherWindow>().map(|global| global.0) {
                let _ = handle.update(cx, |launcher, _window, cx| {
                    if launcher.page == LauncherPage::Clipboard {
                        launcher.refresh_clipboard(cx);
                    }
                });
            }
        });
    })
    .detach();
}

fn launcher_background(transparency_level: usize) -> u32 {
    // 0: ~93% (238), 1: ~90% (230), 2 (default): ~87% (222), 3: ~84% (214), 4: ~81% (206)
    let alpha = 238u32.saturating_sub(transparency_level.min(4) as u32 * 8);
    (COLOR_BACKGROUND & 0xffff_ff00) | alpha
}

fn centered_bounds(window_size: Size<Pixels>, cx: &App) -> Bounds<Pixels> {
    Bounds::centered(active_display_id(), window_size, cx)
}

/// Where the middle of the launcher panel sits, as a fraction from the top
/// of the usable display area. Half is dead center, which puts the panel
/// low and forces the eye down to read it. A third lands it in the upper
/// half with the first result rows near eye level.
const LAUNCHER_CENTER_FRACTION: f32 = 0.30;

/// Smallest gap kept between the launcher panel and the top of the usable
/// display area, so a tall panel on a short screen never sits off-screen.
const LAUNCHER_TOP_MARGIN: f32 = 24.0;

/// Where the launcher panel opens.
///
/// The panel changes height between compact and expanded, so the position
/// is derived from where the middle of the panel should sit rather than
/// from a fixed offset. A fixed offset puts a short compact panel near the
/// top and leaves a tall expanded panel low on the screen.
fn launcher_bounds(window_size: Size<Pixels>, cx: &App) -> Bounds<Pixels> {
    let display_id = active_display_id();
    let mut bounds = Bounds::centered(display_id, window_size, cx);
    let display = cx
        .displays()
        .into_iter()
        .find(|display| Some(display.id()) == display_id)
        .or_else(|| cx.primary_display());
    if let Some(display) = display {
        // `Bounds::centered` works from the visible bounds, which leave
        // out the menu bar, so this has to use the same rect.
        let visible = display.visible_bounds();
        let target_center = visible.origin.y + visible.size.height * LAUNCHER_CENTER_FRACTION;
        bounds.origin.y = (target_center - window_size.height / 2.0)
            .max(visible.origin.y + px(LAUNCHER_TOP_MARGIN));
    }
    bounds
}

/// Wayland compositors get a centered overlay through layer shell (SPEC
/// §6). macOS uses a floating panel so GPUI applies no utility fade; the
/// popup level returns through `corvo_platform::make_panel_instant`.
/// X11 and Windows use a borderless popup.
#[cfg(target_os = "linux")]
fn window_kind() -> WindowKind {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions::default())
    } else {
        WindowKind::PopUp
    }
}

#[cfg(target_os = "macos")]
fn window_kind() -> WindowKind {
    WindowKind::Floating
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn window_kind() -> WindowKind {
    WindowKind::PopUp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn clipboard_preview_card_keeps_a_constant_height(cx: &mut gpui::TestAppContext) {
        struct ProbeView {
            line_count: usize,
            card_height: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
        }

        impl Render for ProbeView {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut Context<Self>,
            ) -> impl IntoElement {
                let probe = self.card_height.clone();
                let rows = (0..self.line_count)
                    .map(|index| format!("line {index}"))
                    .collect();
                // The exact clipboard pane structure: a bounded pane, a
                // flex_1 card, and the virtualized row list inside it.
                div().size_full().flex().flex_col().child(
                    div()
                        .id("probe-pane")
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("probe-card")
                                .flex_1()
                                .min_h(px(0.0))
                                .w_full()
                                .relative()
                                .flex()
                                .flex_col()
                                .child(
                                    clip_preview_uniform_list(
                                        "probe-list",
                                        ClipPreviewKind::Text,
                                        rows,
                                    )
                                    .flex_1()
                                    .min_h(px(0.0)),
                                )
                                .child(
                                    gpui::canvas(
                                        |_bounds, _window, _cx| {},
                                        move |bounds, _, _, _| probe.set(bounds.size.height),
                                    )
                                    .absolute()
                                    .size_full(),
                                ),
                        )
                        .child(div().id("probe-info").flex_none().h(px(60.0))),
                )
            }
        }

        let card_height = std::rc::Rc::new(std::cell::Cell::new(gpui::Pixels::ZERO));
        let window = cx.add_window({
            let card_height = card_height.clone();
            move |_, _| ProbeView {
                line_count: 5,
                card_height,
            }
        });
        let handle = gpui::AnyWindowHandle::from(window);
        cx.update_window(handle, |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let short = card_height.get();
        assert!(short > px(100.0), "card should be tall, got {short:?}");

        window
            .update(cx, |view: &mut ProbeView, _window, _cx| {
                view.line_count = 5_000;
            })
            .unwrap();
        cx.update_window(handle, |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let long = card_height.get();

        assert_eq!(short, long, "the card must not grow with content length");
    }

    #[test]
    fn cap_preview_text_passes_small_texts_through() {
        assert_eq!(cap_preview_text("hello\nworld"), "hello\nworld");
        assert_eq!(cap_preview_text(""), "");
    }

    #[test]
    fn cap_preview_text_caps_line_count_with_a_marker() {
        let many_lines = (0..(CLIP_PREVIEW_MAX_LINES + 50))
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let capped = cap_preview_text(&many_lines);
        assert_eq!(capped.lines().count(), CLIP_PREVIEW_MAX_LINES + 1);
        assert!(capped.ends_with("… 50 more lines not shown"));
    }

    #[test]
    fn cap_preview_text_shows_part_of_a_single_huge_line() {
        let huge = "a".repeat(CLIP_PREVIEW_MAX_CHARS * 3);
        let capped = cap_preview_text(&huge);
        assert!(capped.starts_with('a'));
        assert!(capped.chars().count() <= CLIP_PREVIEW_MAX_CHARS + 64);
        assert!(capped.ends_with("… 1 more lines not shown"));
    }

    fn size(compact: bool, force: bool, page: LauncherPage, query: &str) -> PaletteSize {
        palette_size(compact, force, page, query, false, false, false)
    }

    #[test]
    fn compact_mode_collapses_on_an_empty_root_query() {
        assert_eq!(
            size(true, false, LauncherPage::Root, ""),
            PaletteSize::Compact
        );
    }

    #[test]
    fn compact_mode_expands_once_the_user_types() {
        assert_eq!(
            size(true, false, LauncherPage::Root, "note"),
            PaletteSize::Extended
        );
    }

    #[test]
    fn compact_mode_stays_collapsed_on_whitespace() {
        // A query of only spaces means the user has typed nothing, so the
        // panel should stay short rather than jumping to full height.
        assert_eq!(
            size(true, false, LauncherPage::Root, "   "),
            PaletteSize::Compact
        );
    }

    #[test]
    fn compact_mode_is_off_when_disabled() {
        assert_eq!(
            size(false, false, LauncherPage::Root, ""),
            PaletteSize::Extended
        );
    }

    #[test]
    fn an_open_overlay_expands_the_panel() {
        // The actions menu and the burger menu need room to draw.
        assert_eq!(
            palette_size(true, false, LauncherPage::Root, "", true, false, false),
            PaletteSize::Extended
        );
        assert_eq!(
            palette_size(true, false, LauncherPage::Root, "", false, true, false),
            PaletteSize::Extended
        );
        assert_eq!(
            palette_size(true, false, LauncherPage::Root, "", false, false, true),
            PaletteSize::Extended
        );
    }

    #[test]
    fn an_extension_page_stays_extended() {
        // Declarative pages render inside the standard extended
        // window: their layout must fit it, scrolling when it does
        // not. No special height.
        assert_eq!(
            palette_size(true, false, LauncherPage::Extension("countdown"), "", false, false, false),
            PaletteSize::Extended
        );
        assert_eq!(
            palette_size(true, false, LauncherPage::Extension("countdown"), "2026-12-25", false, false, false),
            PaletteSize::Extended
        );
    }

    #[test]
    fn a_sub_page_is_never_compact() {
        assert_eq!(
            size(true, false, LauncherPage::Clipboard, ""),
            PaletteSize::Extended
        );
        assert_eq!(
            size(true, false, LauncherPage::Files, ""),
            PaletteSize::Extended
        );
    }

    #[test]
    fn force_expanded_overrides_compact() {
        assert_eq!(
            size(true, true, LauncherPage::Root, ""),
            PaletteSize::Extended
        );
    }

    #[test]
    fn the_compact_height_is_much_smaller_than_the_extended_one() {
        // Guards the constant itself: a regression here would make the
        // panel look collapsed while it is still tall.
        const { assert!(COMPACT_WINDOW_HEIGHT < WINDOW_HEIGHT / 4.0) };
    }

    #[test]
    fn the_interface_size_scale_applies_to_both_heights() {
        for scale in [0.9, 1.0, 1.1] {
            assert_eq!(
                PaletteSize::Compact.height(scale),
                COMPACT_WINDOW_HEIGHT * scale
            );
            assert_eq!(PaletteSize::Extended.height(scale), WINDOW_HEIGHT * scale);
        }
    }

    #[test]
    fn the_reveal_label_names_the_platform_file_manager() {
        // "Show in Finder" points at an application that does not exist on
        // Windows or Linux.
        let label = file_manager_label("Show in");
        if cfg!(target_os = "macos") {
            assert_eq!(label, "Show in Finder");
        } else if cfg!(target_os = "windows") {
            assert_eq!(label, "Show in Explorer");
        } else {
            assert_eq!(label, "Show in Files");
        }
    }

    #[test]
    fn a_shortcut_accessory_is_translated_and_a_word_is_not() {
        // Accessories that carry a shortcut must render with this
        // platform's labels; plain words must pass through untouched.
        let rendered = shortcut_text("cmd+alt+c");
        assert!(!rendered.contains("cmd"), "got {rendered}");
        assert!(rendered.contains(corvo_core::Primary::current().keycap_label()));
        assert_eq!(shortcut_text("Application"), "Application");
        assert_eq!(shortcut_text("Folder"), "Folder");
    }
}


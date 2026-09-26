//! Launcher window (GPUI). Phase 1: typed queries reach the command
//! registry, results render as rows, Enter launches and dismisses.

mod icons;
mod settings;

pub use settings::{
    open_settings, open_settings_tab, open_settings_tab_with_update_check, SettingsTab,
};

use corvo_core::{
    Action, ActionGroup, CommandAction, CommandError, CommandRegistry, DataStore, ExecutionContext,
    Icon, SearchContext, SearchResult,
};
use gpui::{
    actions, div, font, img, prelude::*, px, rgb, rgba, size, uniform_list, AnyElement, App,
    AppContext, AsyncApp, Bounds, ClickEvent, Context, Div, FocusHandle, FontWeight, Global,
    InteractiveElement, IntoElement, KeyBinding, KeyDownEvent, ParentElement, Pixels, Render,
    ScrollHandle, ScrollStrategy, SharedString, Size, Stateful, Styled, Subscription, TextOverflow,
    TextRun, UniformListScrollHandle, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, linear_color_stop, linear_gradient,
};
use smol::channel::Receiver;

const WINDOW_WIDTH: f32 = 750.0;
const WINDOW_HEIGHT: f32 = 475.0;
const COMPACT_WINDOW_HEIGHT: f32 = 58.0;
const ROW_HEIGHT: f32 = 38.0;
const ICON_SIZE: f32 = 26.0;
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
        right_calc
            .cmp(&left_calc)
            .then_with(|| {
                right
                    .score
                    .cmp(&left.score)
                    .then_with(|| corvo_core::search::natural_cmp(&left.title, &right.title))
                    .then_with(|| left.id.cmp(&right.id))
            })
    });
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
        right_calc
            .cmp(&left_calc)
            .then_with(|| {
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

pub fn preload_initial_results(registry: &CommandRegistry, store: std::sync::Arc<dyn DataStore>) {
    let ctx = SearchContext {
        max_results: 2000,
        store: Some(store),
    };
    let mut results = Vec::new();
    for command in registry.commands() {
        if ctx.store.as_ref().is_some_and(|store| {
            store.command_enabled(command.id()) && store.show_command_in_launcher(command.id())
        }) {
            results.extend(smol::block_on(command.search("", &ctx)));
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(2000);
    let cell = INITIAL_RESULTS.get_or_init(|| std::sync::RwLock::new(Vec::new()));
    if let Ok(mut lock) = cell.write() {
        *lock = results;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherPage {
    Root,
    Emoji,
    Clipboard,
    Uninstaller,
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

#[derive(Clone, Debug)]
enum ClipboardFlatItem {
    Header(SharedString),
    Row(usize),
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
    cursor_visible: bool,
    results: Vec<SearchResult>,
    selected: usize,
    search_seq: u64,
    results_scroll_handle: ScrollHandle,
    emoji_scroll_handle: ScrollHandle,
    clipboard_scroll_handle: UniformListScrollHandle,
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
    emoji_category_index: usize,
    emoji_column_count: usize,
    emoji_skin_tone: usize,
    background_color: u32,
    row_height: f32,
    clipboard_filter_index: usize,
    registry: CommandRegistry,
    store: std::sync::Arc<dyn DataStore>,
    current_window_height: f32,
    force_expanded: bool,
    _activation_sub: Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultSection {
    Calculator,
    Recent,
    Applications,
    SystemSettings,
    SystemActions,
    WindowManagement,
    Commands,
}

impl ResultSection {
    fn title(self) -> &'static str {
        match self {
            Self::Calculator => "CALCULATOR",
            Self::Recent => "RECENT",
            Self::Applications => "APPLICATIONS",
            Self::SystemSettings => "SYSTEM SETTINGS",
            Self::SystemActions => "SYSTEM ACTIONS",
            Self::WindowManagement => "WINDOW MANAGEMENT",
            Self::Commands => "COMMANDS",
        }
    }

    fn from_result(result: &SearchResult) -> Self {
        if result.id.starts_with("calculator:") {
            Self::Calculator
        } else if result.accessory.as_deref() == Some("Recent") {
            Self::Recent
        } else if result.id.starts_with("app-launcher:") {
            Self::Applications
        } else if result.accessory.as_deref() == Some("System Setting") {
            Self::SystemSettings
        } else if result.accessory.as_deref() == Some("System Action") {
            Self::SystemActions
        } else if result.id.starts_with("window-management:") {
            Self::WindowManagement
        } else {
            Self::Commands
        }
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
        let size_scale = match store.interface_size_option() {
            0 => 0.9,
            2 => 1.1,
            _ => 1.0,
        };
        let current_window_height = palette_size(
            store.compact_mode(), false, page, &query, false, false, false,
        ).height(size_scale);
        let cursor_idx = query.chars().count();
        let mut launcher = Self {
            focus_handle,
            page,
            query,
            cursor_idx,
            cursor_visible: true,
            results: cached_initial_results(),
            selected: 0,
            search_seq: 0,
            results_scroll_handle: ScrollHandle::new(),
            emoji_scroll_handle: ScrollHandle::new(),
            clipboard_scroll_handle: UniformListScrollHandle::new(),
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
            emoji_category_index: 0,
            emoji_column_count,
            emoji_skin_tone,
            background_color,
            row_height,
            clipboard_filter_index: 0,
            registry: cx.global::<RegistryGlobal>().0.clone(),
            store,
            current_window_height,
            force_expanded: false,
            _activation_sub,
        };
        launcher.rebuild_root_flat_items();
        launcher.start_cursor_blink(cx);
        match page {
            LauncherPage::Root => launcher.refresh(cx),
            LauncherPage::Emoji => launcher.refresh_emoji(cx),
            LauncherPage::Clipboard => launcher.refresh_clipboard(cx),
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

    fn sync_palette_size(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let target_w = self.desired_window_width();
        let target_h = self.desired_window_height();
        if (self.current_window_height - target_h).abs() > 0.5 {
            // Update current_window_height immediately so that render() paints the
            // correct content size on this frame. The actual NSWindow resize happens
            // on the next event-loop tick via spawn, but because CATransaction with
            // disableActions is used in resize_launcher_panel, Core Animation will
            // not interpolate between the old and new frames.
            self.current_window_height = target_h;
            #[cfg(target_os = "macos")]
            {
                cx.spawn(async move |_, _| {
                    corvo_platform::resize_launcher_panel(target_w as f64, target_h as f64);
                })
                .detach();
            }
            #[cfg(not(target_os = "macos"))]
            {
                _window.resize(size(px(target_w), px(target_h)));
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
        if self.filter_dropdown_open {
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
        } else if self.page == LauncherPage::Emoji || self.page == LauncherPage::Clipboard {
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
        let cmd = keystroke.modifiers.platform;
        let ctrl = keystroke.modifiers.control;
        let alt = keystroke.modifiers.alt;

        // ⌘1..⌘9: Direct action execution
        if cmd && !ctrl && !alt {
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

        // ⌘↵: Execute secondary action
        if cmd && !ctrl && !alt && key == "enter" {
            if self.filtered_actions().len() > 1 {
                self.action_selected = 1;
                self.run_selected_action(window, cx);
            }
            return;
        }

        // ⌘V: Paste into actions filter
        if cmd && !ctrl && !alt && key == "v" {
            self.paste_from_clipboard(cx);
            return;
        }

        // Enter: Execute selected action
        if !cmd && !ctrl && !alt && key == "enter" {
            self.run_selected_action(window, cx);
            return;
        }

        // Up / ⌃P: Move selection up
        if (!cmd && !ctrl && !alt && key == "up") || (!cmd && ctrl && !alt && key == "p") {
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
        if (!cmd && !ctrl && !alt && key == "down") || (!cmd && ctrl && !alt && key == "n") {
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

        // ⌥⌫ / ⌃W: Delete word backward in actions filter
        if (!cmd && !ctrl && alt && key == "backspace") || (!cmd && ctrl && !alt && key == "w") {
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

        // ⌘⌫ / ⌃U: Clear actions filter
        if (cmd && !ctrl && !alt && key == "backspace") || (!cmd && ctrl && !alt && key == "u") {
            if !self.actions_filter.is_empty() {
                self.actions_filter.clear();
                self.action_selected = 0;
                cx.notify();
            }
            return;
        }

        // Backspace: Delete character
        if !cmd && !ctrl && !alt && key == "backspace" {
            if self.actions_filter.pop().is_some() {
                self.action_selected = 0;
                cx.notify();
            }
            return;
        }

        if !cmd
            && !ctrl
            && !alt
            && matches!(
                key,
                "tab" | "left" | "right" | "home" | "end" | "pageup" | "pagedown"
            )
        {
            return;
        }

        if cmd || ctrl || alt {
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
        let mods = &keystroke.modifiers;

        let cmd = mods.platform;
        let ctrl = mods.control;
        let alt = mods.alt;
        let shift = mods.shift;

        // Stage 0: Global App & Window Lifecycle
        if cmd && !ctrl && !alt && !shift {
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
                    open_settings(cx);
                    cx.stop_propagation();
                    return;
                }
                "k" => {
                    if self.page != LauncherPage::Uninstaller {
                        self.toggle_actions(cx);
                        cx.stop_propagation();
                        return;
                    }
                }
                _ => {}
            }
        }

        // ⌃⌘Space: Switch to Emoji picker
        if cmd && ctrl && !alt && key == "space" {
            self.open_emoji_page(window, cx);
            cx.stop_propagation();
            return;
        }

        // ⌥⌘C: Switch to Clipboard manager
        if cmd && alt && !ctrl && key == "c" {
            self.open_clipboard_page(window, cx);
            cx.stop_propagation();
            return;
        }

        // Stage 1: Escape Modal Layer Unwinding
        if !cmd && !ctrl && !alt && key == "escape" {
            self.handle_escape(window, cx);
            cx.stop_propagation();
            return;
        }

        // Stage 2: Modal Overlays (Dropdown, Burger, Actions)
        if self.filter_dropdown_open {
            self.handle_filter_dropdown_key(key, ctrl, cx);
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
            if cmd && !ctrl && !alt && key == "enter" {
                self.begin_uninstall(cx);
                cx.stop_propagation();
                return;
            }
            if !cmd && !ctrl && !alt && key == "space" && self.query.is_empty() {
                self.toggle_uninstaller_focus(cx);
                cx.stop_propagation();
                return;
            }
        }

        // Stage 3: Contextual List Actions
        if self.page == LauncherPage::Clipboard {
            // ⌘⌫ / ⌘⌦: Delete selected clipboard entry from history
            if cmd && !ctrl && !alt && (key == "backspace" || key == "delete") {
                if let Some(res) = self.selected_result() {
                    if let Some(id) = res.id.strip_prefix("clipboard-manager:entry:") {
                        corvo_clipboard_manager::delete_entry(id);
                        self.refresh_clipboard(cx);
                        cx.stop_propagation();
                        return;
                    }
                }
            }
            // ⌘C or ⌘↵: Copy entry to clipboard without pasting
            if cmd && !ctrl && !alt && (key == "c" || key == "enter") {
                self.execute_secondary_selected(window, cx);
                cx.stop_propagation();
                return;
            }
        }

        // ⌘↵: Secondary Action on selected row (Root / Main List)
        if cmd && !ctrl && !alt && key == "enter" {
            self.execute_secondary_selected(window, cx);
            cx.stop_propagation();
            return;
        }

        // ⌘C: Copy selected item in Root if query is empty or on non-text selection
        if cmd && !ctrl && !alt && key == "c" {
            if self.page != LauncherPage::Uninstaller && self.execute_copy_action(window, cx) {
                cx.stop_propagation();
                return;
            }
        }

        // Stage 4: List Navigation & Activation
        // Up / ⌃P: Move selection up
        if (!cmd && !ctrl && !alt && key == "up") || (!cmd && ctrl && !alt && key == "p") {
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
        if (!cmd && !ctrl && !alt && key == "down") || (!cmd && ctrl && !alt && key == "n") {
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
        if !cmd && !ctrl && !alt && key == "pageup" {
            let delta = if self.page == LauncherPage::Emoji {
                self.emoji_column_count * 3
            } else {
                8
            };
            self.select(self.selected_position().saturating_sub(delta), cx);
            cx.stop_propagation();
            return;
        }
        if !cmd && !ctrl && !alt && key == "pagedown" {
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
        if !cmd && !ctrl && !alt && key == "enter" {
            self.execute_selected(window, cx);
            cx.stop_propagation();
            return;
        }

        // Tab: Toggle filter dropdown on Emoji/Clipboard
        if !cmd && !ctrl && !alt && key == "tab" {
            if self.page == LauncherPage::Emoji || self.page == LauncherPage::Clipboard {
                self.toggle_filter_dropdown(cx);
                cx.stop_propagation();
                return;
            }
        }

        // Emoji grid lateral movement
        if self.page == LauncherPage::Emoji && !cmd && !ctrl && !alt {
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

        // Stage 5: macOS Text Editing Primitives
        // ⌘V: Paste
        if cmd && !ctrl && !alt && key == "v" {
            self.paste_from_clipboard(cx);
            self.ensure_window_size(window, cx);
            cx.stop_propagation();
            return;
        }

        // ⌥←: Word backward
        if !cmd && !ctrl && alt && key == "left" {
            self.cursor_word_left();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // ⌥→: Word forward
        if !cmd && !ctrl && alt && key == "right" {
            self.cursor_word_right();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // ⌘← / ⌃A: Line start
        if (cmd && !ctrl && !alt && key == "left") || (!cmd && ctrl && !alt && key == "a") {
            self.cursor_idx = 0;
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // ⌘→ / ⌃E: Line end
        if (cmd && !ctrl && !alt && key == "right") || (!cmd && ctrl && !alt && key == "e") {
            self.cursor_idx = self.query.chars().count();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Left / Right: Single character movement
        if !cmd && !ctrl && !alt && key == "left" && self.page != LauncherPage::Emoji {
            self.cursor_idx = self.cursor_idx.saturating_sub(1);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if !cmd && !ctrl && !alt && key == "right" && self.page != LauncherPage::Emoji {
            self.cursor_idx = (self.cursor_idx + 1).min(self.query.chars().count());
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Home / End
        if !cmd && !ctrl && !alt && key == "home" {
            if self.query.is_empty() {
                self.select(0, cx);
            } else {
                self.cursor_idx = 0;
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if !cmd && !ctrl && !alt && key == "end" {
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

        // ⌥⌫ / ⌃W: Delete word backward
        if (!cmd && !ctrl && alt && key == "backspace") || (!cmd && ctrl && !alt && key == "w") {
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

        // ⌘⌫ / ⌃U: Delete to beginning of line (in Root / Emoji pages)
        if (cmd && !ctrl && !alt && key == "backspace") || (!cmd && ctrl && !alt && key == "u") {
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

        // ⌃K: Delete to end of line (kill line)
        if !cmd && ctrl && !alt && key == "k" {
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
        if !cmd && !ctrl && !alt && key == "backspace" {
            if self.backspace_char() {
                if self.query.trim().is_empty() {
                    self.force_expanded = false;
                }
                self.ensure_window_size(window, cx);
                self.refresh_current_page(cx);
            } else if self.page == LauncherPage::Emoji
                || self.page == LauncherPage::Clipboard
                || self.page == LauncherPage::Uninstaller
            {
                self.open_root_page(window, cx);
            }
            cx.stop_propagation();
            return;
        }

        // Delete: Delete character forward
        if !cmd && !ctrl && !alt && key == "delete" {
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
        if !cmd && !ctrl && !alt && key == "space" {
            self.insert_str(" ");
            self.ensure_window_size(window, cx);
            self.refresh_current_page(cx);
            cx.stop_propagation();
            return;
        }

        // Stage 6: Character Typing Guard
        if cmd || ctrl || alt {
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

    fn open_uninstaller_page(
        &mut self,
        name: String,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = path.canonicalize().unwrap_or(path);
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

        let mut current_sec = None;
        for (res_idx, res) in self.results.iter().enumerate() {
            let sec = ResultSection::from_result(res);
            if current_sec != Some(sec) {
                current_sec = Some(sec);
                self.root_flat_items
                    .push(RootFlatItem::Header(sec.title().into()));
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
        if self.page == LauncherPage::Emoji {
            self.selected = index.min(self.results.len().saturating_sub(1));
            let row = self.selected / self.emoji_column_count;
            self.emoji_scroll_handle.scroll_to_item(row);
            cx.notify();
            return;
        }

        self.selected = index.min(self.results.len().saturating_sub(1));
        let flat_idx = self.root_to_flat.get(self.selected).copied().unwrap_or(0);
        let scroll_target = if self.selected > 0 {
            (flat_idx + 1).min(self.root_flat_items.len().saturating_sub(1))
        } else {
            0
        };
        self.results_scroll_handle.scroll_to_item(scroll_target);
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
            ready.message = Some("Select at least one item to move to Trash".into());
            ready.confirming = false;
            cx.notify();
            return;
        }
        if !ready.confirming {
            ready.confirming = true;
            ready.message = Some(format!("Move {selected_count} selected items to Trash?"));
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
            let (files, reached_scan_limit, message) = match result {
                Ok(moved_paths) => {
                    let moved_paths: std::collections::HashSet<_> =
                        moved_paths.into_iter().collect();
                    let remaining: Vec<_> = files
                        .into_iter()
                        .filter(|file| !moved_paths.contains(&file.path))
                        .collect();
                    (
                        remaining,
                        scan_limited,
                        Some(format!("Moved {} items to Trash", moved_paths.len())),
                    )
                }
                Err(error) => {
                    let scan_target = target.clone();
                    let current_scan = smol::unblock(move || {
                        corvo_platform::associated_app_files(&scan_target.path)
                    })
                    .await;
                    match current_scan {
                        Ok(scan) => (scan.files, scan.reached_scan_limit, Some(error.to_string())),
                        Err(_) => (files, scan_limited, Some(error.to_string())),
                    }
                }
            };
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

    fn selected_result(&self) -> Option<&SearchResult> {
        self.results.get(self.selected)
    }

    /// Runs every command's search on the background executor and swaps
    /// the result list when they answer.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.query.clone();
        let commands = self.registry.commands().to_vec();
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
            let fast = cached_query_results(&query, max_results, fallback_enabled);
            self.results = fast;
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
                let query = query.clone();
                let ctx = ctx.clone();
                smol::spawn(async move {
                    let results = command.search(&query, &ctx).await;
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
                if fallback_enabled {
                    fast_results.extend(corvo_web_search_fallback::search_results(&query));
                }
                apply_result_preferences(&mut fast_results, &search_settings, &query);
            } else {
                fast_results.truncate(max_results);
            }
            if query.is_empty() {
                if let Some(cell) = INITIAL_RESULTS.get() {
                    if let Ok(mut lock) = cell.write() {
                        *lock = fast_results.clone();
                    }
                }
            }
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq {
                    let prev_selected = launcher.selected;
                    let prev_id = if prev_selected > 0 {
                        launcher.selected_result().map(|r| r.id.clone())
                    } else {
                        None
                    };
                    launcher.results = fast_results;
                    launcher.rebuild_root_flat_items();
                    if let Some(ref id) = prev_id {
                        launcher.selected = launcher.results.iter().position(|r| &r.id == id).unwrap_or(0);
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
                    if fallback_enabled {
                        results.extend(corvo_web_search_fallback::search_results(&query));
                    }
                    apply_result_preferences(&mut results, &search_settings, &query);
                } else {
                    results.truncate(max_results);
                }
                let _ = this.update(cx, |launcher, cx| {
                    if launcher.search_seq == seq {
                        let prev_selected = launcher.selected;
                        let prev_id = if prev_selected > 0 {
                            launcher.selected_result().map(|r| r.id.clone())
                        } else {
                            None
                        };
                        launcher.results = results;
                        launcher.rebuild_root_flat_items();
                        if let Some(ref id) = prev_id {
                            launcher.selected = launcher.results.iter().position(|r| &r.id == id).unwrap_or(0);
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

    /// Opens the ⌘K actions menu for the selected result, or closes it.
    fn toggle_actions(&mut self, cx: &mut Context<Self>) {
        self.burger_menu_open = false;
        self.filter_dropdown_open = false;
        if self.actions_open {
            self.close_actions(cx);
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
        let config = settings.result_item(&result_id, &title).cloned().unwrap_or_default();
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
            hotkey: Some("⇧⌘F"),
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
            hotkey: Some("⇧⌘H"),
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
        let items: [(&str, Option<&str>); 4] = [
            ("Preferences...", Some("⌘,")),
            ("About Corvo", None),
            ("Check for Updates...", None),
            ("Quit Corvo", Some("⌘Q")),
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
                            open_settings(cx);
                        } else if is_about {
                            open_settings_tab(SettingsTab::About, cx);
                        } else if is_check_updates {
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
                        row.child(div().flex().gap_1().children(hotkey.chars().map(keycap)))
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

    fn run_selected_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filtered = self.filtered_actions();
        let Some(position) = filtered.get(self.action_selected) else {
            return;
        };
        let Some(entry) = self.actions.get(*position) else {
            return;
        };
        let entry_id = entry.id.clone();
        let action = Ok(entry.action.clone());
        let is_repeatable = entry_id.contains("volume-up")
            || entry_id.contains("volume-down")
            || entry_id.contains("brightness-up")
            || entry_id.contains("brightness-down")
            || matches!(entry.action, Action::AdjustBrightness(_));
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
    }

    fn execute_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == LauncherPage::Uninstaller {
            self.toggle_uninstaller_focus(cx);
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
        if result.id == "system-actions:check-for-updates" || result.id == "check-for-updates" {
            self.dismiss(window);
            open_settings_tab_with_update_check(SettingsTab::About, cx);
            return;
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
            let primary_action = primary.action.clone();
            cx.spawn(async move |_this, _cx| {
                let _ = command.execute(&result_id, &ctx).await;
            })
            .detach();
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
                self.previous_app = None;
                corvo_platform::forget_launcher_panel();
                window.remove_window();
                let _ = ops.open_path(&path);
            }
            Ok(Action::OpenAppUninstaller { name, path }) => {
                self.open_uninstaller_page(name, path, window, cx);
            }
            Ok(Action::SetResultFavorite {
                result_id,
                title,
                favorite,
            }) => {
                self.save_result_flag(&result_id, &title, None, Some(favorite));
                self.refresh(cx);
            }
            Ok(Action::SetResultHidden {
                result_id,
                title,
                hidden,
            }) => {
                self.save_result_flag(&result_id, &title, Some(hidden), None);
                self.refresh(cx);
            }
            Ok(Action::OpenUrl(url)) => {
                self.previous_app = None;
                corvo_platform::forget_launcher_panel();
                window.remove_window();
                let _ = corvo_platform::open_url(&url);
            }
            Ok(Action::RunShell(cmd)) => {
                let is_repeatable = self.selected_result().is_some_and(|r| {
                    r.id.contains("volume-up")
                        || r.id.contains("volume-down")
                        || r.id.contains("brightness-up")
                        || r.id.contains("brightness-down")
                }) || cmd.contains("DisplayServices")
                    || cmd.contains("brightness")
                    || cmd.contains("volume settings")
                    || cmd.contains("wpctl set-volume")
                    || cmd.contains("[char]175")
                    || cmd.contains("[char]174");

                if is_repeatable {
                    cx.spawn(async move |_this, _cx| {
                        let _ = corvo_platform::run_shell(&cmd);
                    })
                    .detach();
                    cx.notify();
                } else {
                    self.previous_app = None;
                    corvo_platform::forget_launcher_panel();
                    window.remove_window();
                    let _ = corvo_platform::run_shell(&cmd);
                }
            }
            Ok(Action::Copy(text)) => {
                let _ = ops.copy_text(&text);
                self.dismiss(window);
            }
            Ok(Action::PasteText(text)) => {
                let previous_pid = self.previous_app.take().or_else(corvo_platform::frontmost_app_pid);
                let _ = ops.copy_text(&text);
                corvo_platform::forget_launcher_panel();
                window.remove_window();
                if let Some(pid) = previous_pid {
                    cx.spawn(async move |_this, _cx| {
                        if let Err(err) = corvo_platform::auto_paste(pid, &text).await {
                            eprintln!("corvo: auto-paste error: {err}");
                        }
                    })
                    .detach();
                }
            }
            Ok(Action::CopyImage(path)) => {
                if let Ok(bytes) = std::fs::read(&path) {
                    corvo_platform::copy_image_to_pasteboard(&bytes);
                }
                self.dismiss(window);
            }
            Ok(Action::PasteImage(path)) => {
                let previous_pid = self.previous_app.take().or_else(corvo_platform::frontmost_app_pid);
                corvo_platform::forget_launcher_panel();
                window.remove_window();
                cx.spawn(async move |_this, _cx| {
                    if let Ok(bytes) = std::fs::read(&path) {
                        if let Some(pid) = previous_pid {
                            if let Err(err) = corvo_platform::auto_paste_image(pid, &bytes).await {
                                eprintln!("corvo: auto-paste image error: {err}");
                            }
                        } else {
                            corvo_platform::copy_image_to_pasteboard(&bytes);
                        }
                    }
                })
                .detach();
            }
            Ok(Action::ShowToast(msg)) => {
                if let Some(text) = msg.strip_prefix("copy:") {
                    let _ = ops.copy_text(text);
                    self.dismiss(window);
                } else if let Some(id) = msg.strip_prefix("delete:") {
                    corvo_clipboard_manager::delete_entry(id);
                    self.refresh_clipboard(cx);
                } else if msg == "clear" {
                    corvo_clipboard_manager::clear_history();
                    self.refresh_clipboard(cx);
                }
            }
            Ok(Action::AdjustBrightness(delta)) => {
                cx.spawn(async move |_this, _cx| {
                    if let Err(err) = corvo_platform::adjust_brightness(delta) {
                        eprintln!("corvo: adjust brightness error: {err}");
                    }
                })
                .detach();
                cx.notify();
            }
            Ok(Action::TileWindow(action_id)) => {
                let previous_pid = self.previous_app.take();
                self.dismiss(window);
                cx.spawn(async move |_this, _cx| {
                    smol::Timer::after(std::time::Duration::from_millis(50)).await;
                    if let Err(err) = corvo_platform::tile_window(previous_pid, &action_id) {
                        eprintln!("corvo: tile window error: {err}");
                    }
                })
                .detach();
            }
            Ok(Action::CloseWindow) => self.dismiss(window),
            _ => {}
        }
    }

    fn save_result_flag(
        &self,
        result_id: &str,
        title: &str,
        hidden: Option<bool>,
        favorite: Option<bool>,
    ) {
        let mut settings = corvo_config::Settings::load();
        if settings.set_result_item_flags(result_id, title, hidden, favorite) {
            if let Err(error) = settings.save() {
                eprintln!("corvo: could not save item preferences: {error}");
            }
        }
        if let (Some(hidden), Some(_index)) = (hidden, result_id.strip_prefix("quicklinks:")) {
            let mut quicklinks = corvo_config::QuicklinksFile::load();
            if let Some(link) = quicklinks.quicklinks.iter_mut().find(|link| link.name == title) {
                link.hidden = hidden;
                if let Err(error) = quicklinks.save() {
                    eprintln!("corvo: could not save quicklink visibility: {error}");
                }
            }
        }
    }

    /// Closes the panel and hands activation back to the app that was
    /// frontmost when it opened.
    fn dismiss(&mut self, window: &mut Window) {
        self.force_expanded = false;
        corvo_platform::forget_launcher_panel();
        window.remove_window();
        if let Some(pid) = self.previous_app.take() {
            corvo_platform::activate_app(pid);
        }
    }

    fn search_row(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let font_size = px(20.0);
        let cursor_x = cursor_offset_for_query(&self.query, self.cursor_idx, font_size, window);

        let input_view = if self.query.is_empty() {
            div()
                .id("search-input-content")
                .relative()
                .flex_1()
                .h(px(28.0))
                .flex()
                .items_center()
                .child(render_cursor(px(0.0), px(24.0), self.cursor_visible))
                .child(
                    div()
                        .text_color(rgb(COLOR_TEXT_DIM))
                        .child("Search for apps and commands..."),
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
                        .text_color(rgb(COLOR_TEXT))
                        .child(self.query.clone()),
                )
                .child(render_cursor(cursor_x, px(24.0), self.cursor_visible))
        };

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
        let num_rows = (self.results.len() + column_count - 1) / column_count;
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
                .child("Moving selected files to Trash..."),
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
                        match &result.icon {
                            Icon::Image(path) if path.exists() => false,
                            _ => true,
                        },
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
                .flex_none()
                .w_full()
                .min_h(px(220.0))
                .max_h(px(280.0))
                .flex()
                .items_center()
                .justify_center()
                .p_3()
                .rounded_lg()
                .bg(rgb(0x121315))
                .border_1()
                .border_color(rgb(0x27272a))
                .child(img(img_path).w(px(disp_w)).h(px(disp_h)).rounded_md())
        } else {
            Self::render_clipboard_text_preview(&entry.text)
        };

        let mut info_list = div()
            .flex_none()
            .pt_3()
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
            .gap_4()
            .p_4()
            .overflow_y_scroll()
            .child(content_view)
            .child(info_list)
    }

    fn render_clipboard_text_preview(raw_text: &str) -> Stateful<Div> {
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
            let url_lines: Vec<Div> = format_wrapped_preview_lines(trimmed, 50)
                .into_iter()
                .map(|l| {
                    div()
                        .min_w(px(0.0))
                        .max_w_full()
                        .text_size(px(14.0))
                        .text_color(rgb(COLOR_ACCENT))
                        .child(l)
                })
                .collect();

            div()
                .id("clip-preview-link-card")
                .flex_none()
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
                .child(div().flex().flex_col().gap_0p5().children(url_lines))
        } else if is_latex {
            let lines: Vec<Div> = format_wrapped_preview_lines(raw_text, 50)
                .into_iter()
                .map(|l| {
                    if l.trim().is_empty() {
                        div().h(px(8.0))
                    } else {
                        div()
                            .min_w(px(0.0))
                            .max_w_full()
                            .text_size(px(13.0))
                            .text_color(rgb(0xf1f5f9))
                            .child(l)
                    }
                })
                .collect();

            div()
                .id("clip-preview-latex-box")
                .flex_none()
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
                    div()
                        .font_family("Menlo")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(lines),
                )
        } else if let Some(json_str) = parsed_json {
            let lines: Vec<Div> = format_wrapped_preview_lines(&json_str, 50)
                .into_iter()
                .enumerate()
                .map(|(idx, l)| {
                    div()
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
                                .child(format!("{}", idx + 1)),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .text_size(px(12.5))
                                .text_color(rgb(0xe4e4e7))
                                .child(l),
                        )
                })
                .collect();

            div()
                .id("clip-preview-json-box")
                .flex_none()
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
                                .child(format!("{} lines", lines.len())),
                        ),
                )
                .child(
                    div()
                        .font_family("Menlo")
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .children(lines),
                )
        } else if is_code {
            let lines: Vec<Div> = format_wrapped_preview_lines(raw_text, 50)
                .into_iter()
                .enumerate()
                .map(|(idx, l)| {
                    div()
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
                                .child(format!("{}", idx + 1)),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .max_w_full()
                                .text_size(px(12.5))
                                .text_color(rgb(0xe4e4e7))
                                .child(if l.is_empty() { " ".to_string() } else { l }),
                        )
                })
                .collect();

            div()
                .id("clip-preview-code-box")
                .flex_none()
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
                                .child(format!("{} lines", lines.len())),
                        ),
                )
                .child(
                    div()
                        .font_family("Menlo")
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .children(lines),
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
                .flex_none()
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
                .child(div().flex().flex_col().gap_1p5().children(md_elements))
        } else {
            let lines: Vec<Div> = format_wrapped_preview_lines(raw_text, 50)
                .into_iter()
                .map(|l| {
                    if l.trim().is_empty() {
                        div().h(px(8.0))
                    } else {
                        div()
                            .min_w(px(0.0))
                            .max_w_full()
                            .text_size(px(13.5))
                            .text_color(rgb(COLOR_TEXT))
                            .child(l)
                    }
                })
                .collect();

            div()
                .id("clip-preview-text-box")
                .flex_none()
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
                .gap_1()
                .children(lines)
        }
    }

    /// The ⌘K surface, floating over the list and anchored above the
    /// footer: item title, grouped action rows with hotkey hints, and
    /// an actions filter at the bottom.
    fn actions_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let filtered = self.filtered_actions();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut previous_group = None;
        for position in 0..filtered.len() {
            let index = filtered[position];
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
                    .when(!selected, |row| row.hover(|style| style.bg(rgb(COLOR_ROW_SELECTED))))
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
                        row.child(div().flex().gap_1().children(hotkey.chars().map(keycap)))
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
                div()
                    .px_2()
                    .pt_1()
                    .pb_1()
                    .child(
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
            .child(
                div()
                    .flex_none()
                    .mx_1()
                    .h(px(1.0))
                    .bg(rgb(COLOR_DIVIDER)),
            )
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
                if confirming {
                    "Confirm Move to Trash"
                } else {
                    "Move to Trash"
                },
                "↵",
            );
        }
        if self.page == LauncherPage::Clipboard {
            return ("Paste to Active App", "↵");
        }
        if let Some(result) = self.selected_result() {
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
                _ => ("Open", "↵"),
            }
        } else {
            ("Open", "↵")
        }
    }

    fn footer(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let (action_name, action_key) =
            if self.page == LauncherPage::Emoji || self.page == LauncherPage::Clipboard {
                let target = self.previous_app_name.as_deref().unwrap_or("Active App");
                (format!("Paste to {target}"), "↵")
            } else {
                let (name, key) = self.primary_action_label();
                if name == "Paste to Active App" {
                    let target = self.previous_app_name.as_deref().unwrap_or("Active App");
                    (format!("Paste to {target}"), key)
                } else {
                    (name.to_string(), key)
                }
            };

        let bg = self.background_color;
        let fade_bg = linear_gradient(
            180.0,
            linear_color_stop(rgba(bg & 0xffff_ff00), 0.0),
            linear_color_stop(rgba((bg & 0xffff_ff00) | 0x38), 1.0),
        );

        let show_primary = self.page != LauncherPage::Uninstaller
            || matches!(&self.uninstaller, UninstallerState::Ready(ready) if !ready.scan_in_progress);
        let show_actions = self.page != LauncherPage::Uninstaller;

        div()
            .id("footer-overlay")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(48.0))
            .child(
                div()
                    .id("footer-fade")
                    .absolute()
                    .inset_0()
                    .bg(fade_bg),
            )
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
                                        .on_click(cx.listener(|launcher, _: &ClickEvent, window, cx| {
                                            cx.stop_propagation();
                                            if launcher.page == LauncherPage::Uninstaller {
                                                launcher.begin_uninstall(cx);
                                            } else {
                                                launcher.execute_selected(window, cx);
                                            }
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(0xffffff))
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
                                            .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                                                cx.stop_propagation();
                                                launcher.toggle_actions(cx);
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xd1d5db))
                                                    .child("Actions"),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(action_keycap("⌘"))
                                                    .child(action_keycap("K")),
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
                launcher.select(index, cx);
                launcher.execute_selected(window, cx);
            }))
            .child(icon(
                result.icon.clone(),
                result.id.starts_with("app-launcher:"),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
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
                            .child(result.title.clone()),
                    )
                    .when_some(result.subtitle.clone(), |row, sub| {
                        row.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(sub),
                        )
                    }),
            )
            .child(div().flex_1())
            .when_some(result.accessory.clone(), |row, accessory| {
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
                        .child(accessory),
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

/// One character of an action hotkey hint.
fn keycap(glyph: char) -> Div {
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

    if glyph == '↵' {
        container.child(icons::render_phosphor_svg(
            phosphor_svgs::style::regular::ARROW_ELBOW_DOWN_LEFT,
            rgb(COLOR_TEXT_DIM),
            11.0,
        ))
    } else {
        container
            .text_size(px(11.0))
            .text_color(rgb(COLOR_TEXT_DIM))
            .child(glyph.to_string())
    }
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
        slot.child(icons::render_phosphor_svg(
            svg_data,
            icon_color,
            17.0,
        ))
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
        Icon::Image(path) => slot.child(img(path).size(px(icon_size))),
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
            .rounded_xl()
            .border_1()
            .border_color(rgb(COLOR_DIVIDER))
            .overflow_hidden()
            .bg(rgba(if is_collapsed { self.background_color | 0xff } else { self.background_color }))
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
        for (app_path, cfg) in &settings.applications.app_configs {
            if let Some(ref hk) = cfg.hotkey {
                if !hk.is_empty() {
                    bindings.push((
                        hk.clone(),
                        corvo_platform::HotkeyIntent::LaunchApp(std::path::PathBuf::from(app_path)),
                    ));
                }
            }
        }
    }

    // 2. Window Management
    for (action_id, cfg) in &settings.window_management.command_items {
        if let Some(ref hk) = cfg.hotkey {
            if !hk.is_empty() {
                bindings.push((
                    hk.clone(),
                    corvo_platform::HotkeyIntent::TileWindow(action_id.clone()),
                ));
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
                }
            }
        }
    }

    // 8. Quicklinks
    let quicklinks_file = corvo_config::QuicklinksFile::load();
    for q in &quicklinks_file.quicklinks {
        if let Some(ref hk) = q.hotkey {
            if !hk.is_empty() && !q.url.is_empty() {
                bindings.push((hk.clone(), corvo_platform::HotkeyIntent::OpenUrl(q.url.clone())));
            }
        }
    }
    for (name, cfg) in &settings.quicklinks.command_items {
        if let Some(ref hk) = cfg.hotkey {
            if !hk.is_empty() {
                if !quicklinks_file.quicklinks.iter().any(|q| &q.name == name) {
                    bindings.push((hk.clone(), corvo_platform::HotkeyIntent::OpenUrl(name.clone())));
                }
            }
        }
    }

    bindings
}

/// Dynamically updates the global hotkey engine with the current configuration.
pub fn reload_active_hotkeys(settings: &corvo_config::Settings, cx: &mut App) {
    if let Some(handle) = cx.try_global::<HotkeyManagerGlobal>() {
        let bindings = collect_hotkey_bindings(settings);
        handle.0.borrow_mut().update_bindings(&settings.hotkey, bindings);
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
    gpui_platform::application().run(|cx: &mut App| {
        // Runs after GPUI sets the regular policy, so the launcher
        // stays out of the Dock and Cmd+Tab (SPEC §6).
        corvo_platform::run_as_agent();
        cx.set_global(RegistryGlobal(registry));
        cx.set_global(StoreGlobal(store));
        cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("Launcher"))]);

        // Initialize dynamic global hotkey subsystem
        let hotkey_mgr = corvo_platform::hotkey::HotkeyManager::new(intent_tx);
        let hotkey_holder = std::rc::Rc::new(std::cell::RefCell::new(hotkey_mgr));
        cx.set_global(HotkeyManagerGlobal(hotkey_holder));

        let (reload_tx, reload_rx) = smol::channel::unbounded::<()>();
        corvo_platform::hotkey::set_reload_sender(reload_tx);

        let initial_settings = corvo_config::Settings::load();
        reload_active_hotkeys(&initial_settings, cx);

        // Listen for reload notifications when user updates hotkeys in settings
        cx.spawn(async move |cx: &mut AsyncApp| {
            while reload_rx.recv().await.is_ok() {
                let _ = cx.update(|cx| {
                    let settings = corvo_config::Settings::load();
                    reload_active_hotkeys(&settings, cx);
                });
            }
        })
        .detach();

        // Background update pump (Tinycast model): 30s initial delay, 24h interval, 2h backoff
        cx.spawn(async move |_cx: &mut AsyncApp| {
            smol::Timer::after(std::time::Duration::from_secs(30)).await;
            loop {
                let settings = corvo_config::Settings::load();
                if !settings.updates.check_updates {
                    smol::Timer::after(std::time::Duration::from_secs(3600)).await;
                    continue;
                }
                let channel = corvo_platform::UpdateChannel::parse(&settings.updates.channel);
                let check_res = smol::unblock(move || {
                    corvo_platform::check_for_updates(channel, false)
                })
                .await;

                match check_res {
                    Ok(Some(release)) => {
                        if settings.updates.auto_download {
                            let cancel_flag = std::sync::atomic::AtomicBool::new(false);
                            let rel = release.clone();
                            let _ = smol::unblock(move || {
                                corvo_platform::download_and_verify(&rel, &cancel_flag, None)
                            })
                            .await;
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
                        smol::spawn(async move {
                            if let Err(err) = corvo_platform::tile_window(None, &action_id) {
                                eprintln!("corvo: headless tile window error: {err}");
                            }
                        })
                        .detach();
                    }
                    corvo_platform::HotkeyIntent::LaunchApp(path) => {
                        let ops = corvo_platform::platform_ops();
                        let _ = ops.open_path(&path);
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
                        cx.update(|cx| {
                            open_launcher_with_command_or_query(&cmd_id, cx);
                        });
                    }
                }
            }
        })
        .detach();

        open_launcher(cx);
    });
}

fn execute_system_action_intent(action_id: &str, cx: &mut App) {
    let registry = cx.global::<RegistryGlobal>().0.clone();
    if let Some(cmd) = registry.commands().iter().find(|c| c.id() == "system-actions") {
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
                Action::RunShell(shell_cmd) => {
                    let _ = corvo_platform::run_shell(&shell_cmd);
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

fn execute_system_setting_intent(setting_id: &str, cx: &mut App) {
    let registry = cx.global::<RegistryGlobal>().0.clone();
    if let Some(cmd) = registry.commands().iter().find(|c| c.id() == "system-actions") {
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
                    let _ = corvo_platform::run_shell(&shell_cmd);
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

fn open_launcher_with_command_or_query(cmd_id: &str, cx: &mut App) {
    match cmd_id {
        "clipboard-history" | "Clipboard History" => {
            open_launcher_with_page(LauncherPage::Clipboard, cx);
        }
        "emojis" | "Search Emoji & Symbols" => {
            open_launcher_with_page(LauncherPage::Emoji, cx);
        }
        query => {
            open_launcher_with_query(query, cx);
        }
    }
}

fn open_launcher_with_page(page: LauncherPage, cx: &mut App) {
    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |launcher, window, cx| match page {
                LauncherPage::Clipboard => launcher.open_clipboard_page(window, cx),
                LauncherPage::Emoji => launcher.open_emoji_page(window, cx),
                _ => {
                    launcher.page = page;
                    launcher.refresh(cx);
                    launcher.sync_palette_size(window, cx);
                    cx.notify();
                }
            });
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
                launcher.page = LauncherPage::Root;
                launcher.query = query.to_string();
                launcher.force_expanded = false;
                launcher.cursor_idx = query.chars().count();
                launcher.refresh(cx);
                launcher.sync_palette_size(window, cx);
                cx.notify();
            });
            cx.activate(true);
            return;
        }
    }
    open_launcher_for(LauncherPage::Root, query.to_string(), cx);
}

fn toggle(cx: &mut App) {
    if let Some(handle) = cx.try_global::<LauncherWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |launcher, window, _cx| launcher.dismiss(window));
            return;
        }
    }
    open_launcher(cx);
}

fn active_display_id() -> Option<gpui::DisplayId> {
    corvo_platform::active_display_id().map(|id| gpui::DisplayId::new(id as u64))
}

fn open_launcher(cx: &mut App) {
    open_launcher_for(LauncherPage::Root, String::new(), cx);
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
    let store = cx.global::<StoreGlobal>().0.clone();
    let size_scale = match store.interface_size_option() {
        0 => 0.9,
        2 => 1.1,
        _ => 1.0,
    };
    let window_width = WINDOW_WIDTH * size_scale;
    let full_height = WINDOW_HEIGHT * size_scale;
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
    let mut initial_bounds = centered_bounds(size(px(window_width), px(full_height)), cx);
    initial_bounds.size.height = px(initial_height);
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
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    };
    let opened: Result<WindowHandle<Launcher>, _> = cx.open_window(options, move |window, cx| {
        cx.new(|cx| Launcher::new(window, cx, page, query))
    });
    let window = match opened {
        Ok(handle) => handle,
        Err(err) => {
            eprintln!("corvo: cannot open launcher window: {err}");
            return;
        }
    };
    cx.set_global(LauncherWindow(window));
    // Whoever is frontmost now loses activation; the pid and name
    // are recorded before Corvo activates so dismissal or auto-paste
    // can hand focus back.
    let (previous_pid, previous_name) = match corvo_platform::frontmost_app_info() {
        Some((pid, name)) => (Some(pid), Some(name)),
        None => (corvo_platform::frontmost_app_pid(), None),
    };
    cx.activate(true);
    corvo_platform::update_screens_cache();
    corvo_platform::make_panel_instant(window_width as f64, initial_height as f64);
    let _ = window.update(cx, |launcher, _window, cx| {
        launcher.previous_app = previous_pid;
        launcher.previous_app_name = previous_name;
        if launcher.page == LauncherPage::Root && launcher.results.is_empty() {
            launcher.results = cached_initial_results();
        }
        launcher.rebuild_root_flat_items();
        launcher.results_scroll_handle.scroll_to_item(0);
        corvo_clipboard_manager::poll_clipboard_with_source(launcher.previous_app_name.as_deref());
        if launcher.page == LauncherPage::Clipboard {
            launcher.rebuild_clipboard_flat_items();
            launcher.refresh_clipboard(cx);
        }
    });
    corvo_platform::order_panel_front(window_width as f64, initial_height as f64);
}

fn launcher_background(transparency_level: usize) -> u32 {
    // 0: ~93% (238), 1: ~90% (230), 2 (default): ~87% (222), 3: ~84% (214), 4: ~81% (206)
    let alpha = 238u32.saturating_sub(transparency_level.min(4) as u32 * 8);
    (COLOR_BACKGROUND & 0xffff_ff00) | alpha
}

fn centered_bounds(window_size: Size<Pixels>, cx: &App) -> Bounds<Pixels> {
    Bounds::centered(active_display_id(), window_size, cx)
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

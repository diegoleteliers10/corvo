//! Launcher window (GPUI). Phase 1: typed queries reach the command
//! registry, results render as rows, Enter launches and dismisses.

mod icons;
mod settings;

pub use settings::open_settings;

use corvo_core::{
    Action, CommandAction, CommandError, CommandRegistry, ExecutionContext, Icon,
    SearchContext, SearchResult,
};
use gpui::{
    actions, div, font, img, prelude::*, px, rgb, rgba, size, uniform_list, App, AppContext, AsyncApp, Bounds,
    ClickEvent, Context, Div, FocusHandle, FontWeight, Global, InteractiveElement, IntoElement,
    KeyBinding, KeyDownEvent, ParentElement, Pixels, Render, ScrollHandle, ScrollStrategy, SharedString, Size,
    Stateful, Styled, Subscription, TextOverflow, TextRun, UniformListScrollHandle, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use smol::channel::Receiver;

const WINDOW_WIDTH: f32 = 750.0;
const WINDOW_HEIGHT: f32 = 475.0;
const ROW_HEIGHT: f32 = 38.0;
const ICON_SIZE: f32 = 26.0;

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

actions!(corvo_ui, [Dismiss]);

struct RegistryGlobal(CommandRegistry);

impl Global for RegistryGlobal {}

static INITIAL_RESULTS: std::sync::OnceLock<std::sync::RwLock<Vec<SearchResult>>> = std::sync::OnceLock::new();

pub fn preload_initial_results(registry: &CommandRegistry) {
    let ctx = SearchContext { max_results: 2000, store: None };
    let mut results = Vec::new();
    for command in registry.commands() {
        results.extend(smol::block_on(command.search("", &ctx)));
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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

const CLIPBOARD_FILTERS: &[&str] = &[
    "All Types",
    "Text",
    "Links",
    "Images",
    "JSON",
];

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

pub struct Launcher {
    focus_handle: FocusHandle,
    page: LauncherPage,
    query: String,
    cursor_idx: usize,
    cursor_visible: bool,
    is_loading: bool,
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
    clipboard_filter_index: usize,
    registry: CommandRegistry,
    _activation_sub: Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultSection {
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
            Self::Recent => "RECENT",
            Self::Applications => "APPLICATIONS",
            Self::SystemSettings => "SYSTEM SETTINGS",
            Self::SystemActions => "SYSTEM ACTIONS",
            Self::WindowManagement => "WINDOW MANAGEMENT",
            Self::Commands => "COMMANDS",
        }
    }

    fn from_result(result: &SearchResult) -> Self {
        if result.accessory.as_deref() == Some("Recent") {
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
        .child(
            div()
                .w(px(2.0))
                .h(height)
                .rounded_xs()
                .bg(color),
        )
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

impl Launcher {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let _activation_sub = cx.observe_window_activation(window, |launcher, window, _cx| {
            if !window.is_window_active() {
                launcher.dismiss(window);
            }
        });
        let mut launcher = Self {
            focus_handle,
            page: LauncherPage::Root,
            query: String::new(),
            cursor_idx: 0,
            cursor_visible: true,
            is_loading: false,
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
            actions_open: false,
            actions: Vec::new(),
            actions_title: String::new(),
            actions_filter: String::new(),
            action_selected: 0,
            burger_menu_open: false,
            previous_app: None,
            previous_app_name: None,
            emoji_category_index: 0,
            clipboard_filter_index: 0,
            registry: cx.global::<RegistryGlobal>().0.clone(),
            _activation_sub,
        };
        launcher.rebuild_root_flat_items();
        launcher.start_cursor_blink(cx);
        launcher
    }

    fn start_cursor_blink(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                smol::Timer::after(std::time::Duration::from_millis(530)).await;
                let res = this.update(cx, |launcher, cx| {
                    launcher.cursor_visible = !launcher.cursor_visible;
                    cx.notify();
                });
                if res.is_err() {
                    break;
                }
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

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        let keystroke = &event.keystroke;
        let key = keystroke.key.as_str();
        if keystroke.modifiers.platform && key == "k" {
            self.toggle_actions(cx);
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.platform && key == "enter" && self.actions_open {
            // The ⌘↵ hint marks the second action, as in Raycast.
            if self.filtered_actions().len() > 1 {
                self.action_selected = 1;
                self.run_selected_action(window, cx);
            }
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.platform && keystroke.modifiers.control && key == "space" {
            self.open_emoji_page(cx);
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.platform && key == "c" && keystroke.modifiers.alt {
            self.open_clipboard_page(cx);
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.platform && key == "," {
            self.close_burger_menu(cx);
            self.close_actions(cx);
            open_settings(cx);
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.platform && key == "left" {
            self.cursor_idx = 0;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.platform && key == "right" {
            self.cursor_idx = self.query.chars().count();
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.platform || keystroke.modifiers.control || keystroke.modifiers.alt {
            return;
        }
        if self.filter_dropdown_open {
            let max_idx = match self.page {
                LauncherPage::Emoji => EMOJI_CATEGORIES.len().saturating_sub(1),
                LauncherPage::Clipboard => CLIPBOARD_FILTERS.len().saturating_sub(1),
                _ => 0,
            };
            match key {
                "escape" | "tab" => {
                    self.close_filter_dropdown(cx);
                    cx.stop_propagation();
                }
                "enter" => {
                    self.apply_filter_dropdown_selection(cx);
                    cx.stop_propagation();
                }
                "up" => {
                    self.filter_dropdown_selected = self.filter_dropdown_selected.saturating_sub(1);
                    cx.notify();
                    cx.stop_propagation();
                }
                "down" => {
                    self.filter_dropdown_selected = (self.filter_dropdown_selected + 1).min(max_idx);
                    cx.notify();
                    cx.stop_propagation();
                }
                _ => {
                    cx.stop_propagation();
                }
            }
            return;
        }
        if self.burger_menu_open && key == "escape" {
            self.close_burger_menu(cx);
            cx.stop_propagation();
            return;
        }
        if self.actions_open {
            match key {
                "escape" => {
                    self.close_actions(cx);
                    cx.stop_propagation();
                }
                "enter" => {
                    self.run_selected_action(window, cx);
                    cx.stop_propagation();
                }
                "up" => {
                    self.action_selected = self.action_selected.saturating_sub(1);
                    cx.notify();
                    cx.stop_propagation();
                }
                "down" => {
                    let last = self.filtered_actions().len().saturating_sub(1);
                    self.action_selected = (self.action_selected + 1).min(last);
                    cx.notify();
                    cx.stop_propagation();
                }
                "backspace" => {
                    if self.actions_filter.pop().is_some() {
                        self.action_selected = 0;
                        cx.notify();
                    }
                    cx.stop_propagation();
                }
                "tab" | "left" | "right" | "home" | "end" | "pageup" | "pagedown" => {}
                other => {
                    let typed = keystroke
                        .key_char
                        .clone()
                        .or_else(|| (other.chars().count() == 1).then(|| other.to_string()));
                    if let Some(text) = typed {
                        self.actions_filter.push_str(&text);
                        self.action_selected = 0;
                        cx.notify();
                        cx.stop_propagation();
                    }
                }
            }
            return;
        }

        if self.page == LauncherPage::Emoji {
            match key {
                "escape" => {
                    if !self.query.is_empty() {
                        self.query.clear();
                        self.cursor_idx = 0;
                        self.refresh_emoji(cx);
                    } else {
                        self.open_root_page(cx);
                    }
                    cx.stop_propagation();
                }
                "backspace" => {
                    if self.backspace_char() {
                        self.refresh_emoji(cx);
                    } else {
                        self.open_root_page(cx);
                    }
                    cx.stop_propagation();
                }
                "delete" => {
                    if self.delete_char() {
                        self.refresh_emoji(cx);
                    }
                    cx.stop_propagation();
                }
                "enter" => {
                    self.execute_selected_emoji(window, cx);
                    cx.stop_propagation();
                }
                "left" => {
                    self.select_emoji_delta(-1, cx);
                    cx.stop_propagation();
                }
                "right" => {
                    self.select_emoji_delta(1, cx);
                    cx.stop_propagation();
                }
                "up" => {
                    self.select_emoji_delta(-8, cx);
                    cx.stop_propagation();
                }
                "down" => {
                    self.select_emoji_delta(8, cx);
                    cx.stop_propagation();
                }
                "pageup" => {
                    self.select_emoji_delta(-24, cx);
                    cx.stop_propagation();
                }
                "pagedown" => {
                    self.select_emoji_delta(24, cx);
                    cx.stop_propagation();
                }
                "home" => {
                    self.select(0, cx);
                    cx.stop_propagation();
                }
                "end" => {
                    let last = self.results.len().saturating_sub(1);
                    self.select(last, cx);
                    cx.stop_propagation();
                }
                "space" => {
                    self.insert_str(" ");
                    self.refresh_emoji(cx);
                    cx.stop_propagation();
                }
                "tab" => {
                    self.toggle_filter_dropdown(cx);
                    cx.stop_propagation();
                }
                key => {
                    let typed = keystroke
                        .key_char
                        .clone()
                        .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                    if let Some(text) = typed {
                        self.insert_str(&text);
                        self.refresh_emoji(cx);
                        cx.stop_propagation();
                    }
                }
            }
            return;
        }

        if self.page == LauncherPage::Clipboard {
            if keystroke.modifiers.platform && key == "backspace" {
                if let Some(res) = self.selected_result() {
                    if let Some(id) = res.id.strip_prefix("clipboard-manager:entry:") {
                        corvo_clipboard_manager::delete_entry(id);
                        self.refresh_clipboard(cx);
                        cx.stop_propagation();
                        return;
                    }
                }
            }
            if keystroke.modifiers.platform && (key == "enter" || key == "c") && !self.actions_open {
                if let Some(res) = self.selected_result() {
                    if let Some(id) = res.id.strip_prefix("clipboard-manager:entry:") {
                        if let Some(entry) = corvo_clipboard_manager::get_entry(id) {
                            if let Some(path) = entry.image_path() {
                                if let Ok(bytes) = std::fs::read(&path) {
                                    corvo_platform::copy_image_to_pasteboard(&bytes);
                                }
                            } else {
                                let _ = corvo_platform::platform_ops().copy_text(&entry.text);
                            }
                            self.dismiss(window);
                            cx.stop_propagation();
                            return;
                        }
                    }
                }
            }
            if keystroke.modifiers.platform || keystroke.modifiers.control || keystroke.modifiers.alt {
                return;
            }
            match key {
                "escape" => {
                    if !self.query.is_empty() {
                        self.query.clear();
                        self.cursor_idx = 0;
                        self.refresh_clipboard(cx);
                    } else {
                        self.open_root_page(cx);
                    }
                    cx.stop_propagation();
                }
                "backspace" => {
                    if self.backspace_char() {
                        self.refresh_clipboard(cx);
                    } else {
                        self.open_root_page(cx);
                    }
                    cx.stop_propagation();
                }
                "delete" => {
                    if self.delete_char() {
                        self.refresh_clipboard(cx);
                    }
                    cx.stop_propagation();
                }
                "enter" => {
                    self.execute_selected(window, cx);
                    cx.stop_propagation();
                }
                "up" => {
                    self.select(self.selected.saturating_sub(1), cx);
                    cx.stop_propagation();
                }
                "down" => {
                    let last = self.results.len().saturating_sub(1);
                    self.select((self.selected + 1).min(last), cx);
                    cx.stop_propagation();
                }
                "pageup" => {
                    self.select(self.selected.saturating_sub(8), cx);
                    cx.stop_propagation();
                }
                "pagedown" => {
                    let last = self.results.len().saturating_sub(1);
                    self.select((self.selected + 8).min(last), cx);
                    cx.stop_propagation();
                }
                "home" => {
                    if self.query.is_empty() {
                        self.select(0, cx);
                    } else {
                        self.cursor_idx = 0;
                        cx.notify();
                    }
                    cx.stop_propagation();
                }
                "end" => {
                    if self.query.is_empty() {
                        let last = self.results.len().saturating_sub(1);
                        self.select(last, cx);
                    } else {
                        self.cursor_idx = self.query.chars().count();
                        cx.notify();
                    }
                    cx.stop_propagation();
                }
                "left" => {
                    self.cursor_idx = self.cursor_idx.saturating_sub(1);
                    cx.notify();
                    cx.stop_propagation();
                }
                "right" => {
                    self.cursor_idx = (self.cursor_idx + 1).min(self.query.chars().count());
                    cx.notify();
                    cx.stop_propagation();
                }
                "space" => {
                    self.insert_str(" ");
                    self.refresh_clipboard(cx);
                    cx.stop_propagation();
                }
                "tab" => {
                    self.toggle_filter_dropdown(cx);
                    cx.stop_propagation();
                }
                key => {
                    let typed = keystroke
                        .key_char
                        .clone()
                        .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                    if let Some(text) = typed {
                        self.insert_str(&text);
                        self.refresh_clipboard(cx);
                        cx.stop_propagation();
                    }
                }
            }
            return;
        }

        match key {
            "backspace" => {
                if self.backspace_char() {
                    self.refresh(cx);
                }
                cx.stop_propagation();
            }
            "delete" => {
                if self.delete_char() {
                    self.refresh(cx);
                }
                cx.stop_propagation();
            }
            "enter" => {
                self.execute_selected(window, cx);
                cx.stop_propagation();
            }
            "up" => {
                self.select(self.selected.saturating_sub(1), cx);
                cx.stop_propagation();
            }
            "down" => {
                let last = self.results.len().saturating_sub(1);
                self.select((self.selected + 1).min(last), cx);
                cx.stop_propagation();
            }
            "pageup" => {
                self.select(self.selected.saturating_sub(6), cx);
                cx.stop_propagation();
            }
            "pagedown" => {
                let last = self.results.len().saturating_sub(1);
                self.select((self.selected + 6).min(last), cx);
                cx.stop_propagation();
            }
            "home" => {
                if self.query.is_empty() {
                    self.select(0, cx);
                } else {
                    self.cursor_idx = 0;
                    cx.notify();
                }
                cx.stop_propagation();
            }
            "end" => {
                if self.query.is_empty() {
                    let last = self.results.len().saturating_sub(1);
                    self.select(last, cx);
                } else {
                    self.cursor_idx = self.query.chars().count();
                    cx.notify();
                }
                cx.stop_propagation();
            }
            "left" => {
                self.cursor_idx = self.cursor_idx.saturating_sub(1);
                cx.notify();
                cx.stop_propagation();
            }
            "right" => {
                self.cursor_idx = (self.cursor_idx + 1).min(self.query.chars().count());
                cx.notify();
                cx.stop_propagation();
            }
            "space" => {
                self.insert_str(" ");
                self.refresh(cx);
                cx.stop_propagation();
            }
            "tab" | "escape" => {}
            key => {
                let typed = keystroke
                    .key_char
                    .clone()
                    .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                if let Some(text) = typed {
                    self.insert_str(&text);
                    self.refresh(cx);
                    cx.stop_propagation();
                }
            }
        }
    }

    fn open_emoji_page(&mut self, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    fn open_root_page(&mut self, cx: &mut Context<Self>) {
        self.page = LauncherPage::Root;
        self.query.clear();
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
        cx.notify();
    }

    fn open_clipboard_page(&mut self, cx: &mut Context<Self>) {
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
        self.clipboard_scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
        corvo_clipboard_manager::poll_clipboard_with_source(self.previous_app_name.as_deref());
        self.refresh_clipboard(cx);
        cx.notify();
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
            if !res.id.starts_with("calculator:") {
                let sec = ResultSection::from_result(res);
                if current_sec != Some(sec) {
                    current_sec = Some(sec);
                    self.root_flat_items.push(RootFlatItem::Header(sec.title().into()));
                }
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
                self.clipboard_flat_items.push(ClipboardFlatItem::Header(section.into()));
            }
            self.clipboard_to_flat[res_idx] = self.clipboard_flat_items.len();
            self.clipboard_flat_items.push(ClipboardFlatItem::Row(res_idx));
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
        if self.results.is_empty() {
            self.selected = 0;
            cx.notify();
            return;
        }
        if self.page == LauncherPage::Clipboard {
            self.selected = index.min(self.results.len().saturating_sub(1));
            let flat_idx = self.clipboard_to_flat.get(self.selected).copied().unwrap_or(0);
            self.clipboard_scroll_handle.scroll_to_item(flat_idx, ScrollStrategy::Nearest);
            cx.notify();
            return;
        }
        if self.page == LauncherPage::Emoji {
            self.selected = index.min(self.results.len().saturating_sub(1));
            let row = self.selected / 8;
            self.emoji_scroll_handle.scroll_to_item(row);
            cx.notify();
            return;
        }

        self.selected = index.min(self.results.len().saturating_sub(1));
        let flat_idx = self.root_to_flat.get(self.selected).copied().unwrap_or(0);
        self.results_scroll_handle.scroll_to_item(flat_idx);
        cx.notify();
    }

    fn selected_result(&self) -> Option<&SearchResult> {
        self.results.get(self.selected)
    }

    /// Runs every command's search on the background executor and swaps
    /// the result list when they answer.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
        cx.notify();
        let query = self.query.clone();
        let commands = self.registry.commands().to_vec();
        let max_results = if query.is_empty() { 2000 } else { 100 };
        // Only the latest refresh may write results; a stale task from
        // an older query never overwrites the current list.
        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext { max_results, store: None };
            let mut results = Vec::new();
            for command in &commands {
                results.extend(command.search(&query, &ctx).await);
            }
            results
                .sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(max_results);
            if query.is_empty() {
                if let Some(cell) = INITIAL_RESULTS.get() {
                    if let Ok(mut lock) = cell.write() {
                        *lock = results.clone();
                    }
                }
            }
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq {
                    launcher.results = results;
                    launcher.rebuild_root_flat_items();
                    launcher.selected = 0;
                    launcher.results_scroll_handle.scroll_to_item(0);
                    launcher.is_loading = false;
                    launcher.actions_open = false;
                    launcher.actions = Vec::new();
                    launcher.actions_filter = String::new();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_emoji(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
        cx.notify();
        let query = self.query.clone();
        let selected_category = if self.emoji_category_index > 0 {
            EMOJI_CATEGORIES.get(self.emoji_category_index).copied()
        } else {
            None
        };
        let commands = self.registry.commands().to_vec();
        let Some(command) = commands.iter().find(|c| c.id() == "emoji-picker").cloned() else {
            self.is_loading = false;
            cx.notify();
            return;
        };

        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext { max_results: 1071, store: None };
            let raw_query = format!("emoji-page:{query}");
            let mut results = command.search(&raw_query, &ctx).await;
            if let Some(cat) = selected_category {
                results.retain(|r| r.subtitle.as_deref() == Some(cat));
            }
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Emoji {
                    launcher.results = results;
                    launcher.selected = 0;
                    launcher.is_loading = false;
                    launcher.emoji_scroll_handle.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_clipboard(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
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
        let Some(command) = commands.iter().find(|c| c.id() == "clipboard-manager").cloned() else {
            self.is_loading = false;
            cx.notify();
            return;
        };

        self.search_seq += 1;
        let seq = self.search_seq;
        cx.spawn(async move |this, cx| {
            let ctx = SearchContext { max_results: 100, store: None };
            let raw_query = format!("clipboard-page:filter={filter_name}:{query}");
            let results = command.search(&raw_query, &ctx).await;
            let _ = this.update(cx, |launcher, cx| {
                if launcher.search_seq == seq && launcher.page == LauncherPage::Clipboard {
                    launcher.results = results;
                    launcher.rebuild_clipboard_flat_items();
                    launcher.selected = launcher.selected.min(launcher.results.len().saturating_sub(1));
                    let flat_idx = launcher.clipboard_to_flat.get(launcher.selected).copied().unwrap_or(0);
                    launcher.clipboard_scroll_handle.scroll_to_item(flat_idx, ScrollStrategy::Nearest);
                    launcher.is_loading = false;
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
                            .text_color(if is_active { rgb(COLOR_ACCENT) } else { rgb(COLOR_TEXT) })
                            .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
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
                    .when_some(*hotkey, |row, key| {
                        row.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .child(key),
                        )
                    })
            }))
    }

    /// Indices into `actions` after the menu filter, in menu order.
    fn filtered_actions(&self) -> Vec<usize> {
        let filter = self.actions_filter.to_lowercase();
        self.actions
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                filter.is_empty() || entry.label.to_lowercase().contains(&filter)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn run_selected_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filtered = self.filtered_actions();
        let Some(position) = filtered.get(self.action_selected) else { return };
        let Some(entry) = self.actions.get(*position) else { return };
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
        self.perform(action, window, cx);
    }

    fn execute_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == LauncherPage::Emoji {
            self.execute_selected_emoji(window, cx);
            return;
        }
        let Some(result) = self.selected_result().cloned() else { return };
        if result.id == "emoji-picker:open" {
            self.open_emoji_page(cx);
            return;
        }
        if result.id == "clipboard-manager:open" {
            self.open_clipboard_page(cx);
            return;
        }
        let Some(command_id) = result.id.split(':').next() else { return };
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
        let ctx = ExecutionContext::default();
        cx.spawn_in(window, async move |this, cx| {
            let action = command.execute(&result_id, &ctx).await;
            let _ = this.update_in(cx, |launcher, window, cx| {
                launcher.perform(action, window, cx);
            });
        })
        .detach();
    }

    fn execute_selected_emoji(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(result) = self.results.get(self.selected) else { return };
        let glyph = result.id.strip_prefix("emoji-picker:").unwrap_or(&result.title);
        let action = Ok(Action::Copy(glyph.to_owned()));
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
                let _ = ops.open_path(&path);
                self.dismiss(window);
            }
            Ok(Action::OpenUrl(url)) => {
                let _ = corvo_platform::open_url(&url);
                self.dismiss(window);
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
                    self.dismiss(window);
                    cx.spawn(async move |_this, _cx| {
                        smol::Timer::after(std::time::Duration::from_millis(50)).await;
                        let _ = corvo_platform::run_shell(&cmd);
                    })
                    .detach();
                }
            }
            Ok(Action::Copy(text)) => {
                let previous_pid = self.previous_app.take();
                window.remove_window();
                if let Some(pid) = previous_pid {
                    cx.spawn(async move |_this, _cx| {
                        if let Err(err) = corvo_platform::auto_paste(pid, &text).await {
                            eprintln!("corvo: auto-paste error: {err}");
                        }
                    })
                    .detach();
                } else {
                    let _ = ops.copy_text(&text);
                }
            }
            Ok(Action::CopyImage(path)) => {
                if let Ok(bytes) = std::fs::read(&path) {
                    corvo_platform::copy_image_to_pasteboard(&bytes);
                }
                self.dismiss(window);
            }
            Ok(Action::PasteImage(path)) => {
                let previous_pid = self.previous_app.take();
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

    /// Closes the panel and hands activation back to the app that was
    /// frontmost when it opened.
    fn dismiss(&mut self, window: &mut Window) {
        self.is_loading = false;
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
                        .pt(px(2.0))
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
                        .pt(px(2.0))
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
                                .h(px(ROW_HEIGHT))
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
                        .pt(px(1.5))
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
                        .pt(px(1.5))
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
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.open_root_page(cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::CARET_LEFT,
                        rgb(COLOR_TEXT_DIM),
                        16.0,
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
                    .when(self.filter_dropdown_open || is_filtered, |s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .when(!self.filter_dropdown_open && !is_filtered, |s| s.bg(rgb(COLOR_PILL)))
                    .hover(|s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.toggle_filter_dropdown(cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::SQUARES_FOUR,
                        if is_filtered { rgb(COLOR_ACCENT) } else { rgb(COLOR_TEXT_DIM) },
                        14.0,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if is_filtered { rgb(COLOR_ACCENT) } else { rgb(COLOR_TEXT) })
                            .child(cat_label),
                    )
                    .child(icons::render_phosphor_svg(
                        if self.filter_dropdown_open { phosphor_svgs::style::regular::CARET_UP } else { phosphor_svgs::style::regular::CARET_DOWN },
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

        let num_rows = (self.results.len() + 7) / 8;
        let mut rows: Vec<Stateful<Div>> = Vec::with_capacity(num_rows);

        for row_idx in 0..num_rows {
            let start = row_idx * 8;
            let end = (start + 8).min(self.results.len());
            let mut cells: Vec<Stateful<Div>> = Vec::with_capacity(8);

            for idx in start..end {
                let is_selected = idx == self.selected;
                let glyph = match &self.results[idx].icon {
                    Icon::Glyph(g) => (*g).to_string(),
                    _ => self.results[idx].id.strip_prefix("emoji-picker:").unwrap_or(&self.results[idx].title).to_string(),
                };
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
                        .child(
                            div()
                                .text_size(px(28.0))
                                .child(glyph),
                        ),
                );
            }

            for dummy_idx in (end - start)..8 {
                cells.push(
                    div()
                        .id(SharedString::from(format!("emoji-dummy-{row_idx}-{dummy_idx}")))
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
                        .pt(px(1.5))
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
                        .pt(px(1.5))
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
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.open_root_page(cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::CARET_LEFT,
                        rgb(COLOR_TEXT_DIM),
                        16.0,
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
                    .when(self.filter_dropdown_open || is_filtered, |s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .when(!self.filter_dropdown_open && !is_filtered, |s| s.bg(rgb(COLOR_PILL)))
                    .hover(|s| s.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.toggle_filter_dropdown(cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::FUNNEL,
                        if is_filtered { rgb(COLOR_ACCENT) } else { rgb(COLOR_TEXT_DIM) },
                        13.0,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if is_filtered { rgb(COLOR_ACCENT) } else { rgb(COLOR_TEXT) })
                            .child(current_filter_label),
                    )
                    .child(icons::render_phosphor_svg(
                        if self.filter_dropdown_open { phosphor_svgs::style::regular::CARET_UP } else { phosphor_svgs::style::regular::CARET_DOWN },
                        rgb(COLOR_TEXT_DIM),
                        12.0,
                    ))
            })
    }

    fn clipboard_split_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("clipboard-split-view")
            .flex_1()
            .flex()
            .overflow_hidden()
            .child(self.clipboard_list(cx))
            .child(
                div()
                    .w(px(1.0))
                    .self_stretch()
                    .bg(rgb(COLOR_DIVIDER)),
            )
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
                    cx.processor(|launcher: &mut Self, range: std::ops::Range<usize>, _window, cx| {
                        range.map(|flat_idx| match &launcher.clipboard_flat_items[flat_idx] {
                            ClipboardFlatItem::Header(title) => div()
                                .id(SharedString::from(format!("clipboard-section-{flat_idx}")))
                                .h(px(ROW_HEIGHT))
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
                        }).collect::<Vec<_>>()
                    }),
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
            .h(px(ROW_HEIGHT))
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
                        |el, path| el.child(img(path).size(px(ICON_SIZE)).rounded_md()),
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
            let (disp_w, disp_h) = if let (Some(w), Some(h)) = (entry.image_width, entry.image_height) {
                if w > 0 && h > 0 {
                    let max_w = 420.0f32;
                    let max_h = 240.0f32;
                    let scale = (max_w / w as f32).min(max_h / h as f32).min(1.0);
                    ((w as f32 * scale).round().max(32.0), (h as f32 * scale).round().max(32.0))
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
                .child(
                    img(img_path)
                        .w(px(disp_w))
                        .h(px(disp_h))
                        .rounded_md(),
                )
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
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
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
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .children(url_lines),
                )
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
                    for (b_idx, wrapped_bullet) in format_wrapped_preview_lines(bullet, 48).into_iter().enumerate() {
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
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .children(md_elements),
                )
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
        let rows = (0..filtered.len()).filter_map(|position| {
            let index = filtered[position];
            let entry = self.actions.get(index)?;
            let selected = position == self.action_selected;
            Some(
                div()
                    .id(SharedString::from(format!("action-{position}")))
                    .flex()
                    .items_center()
                    .px(px(6.0))
                    .mb_0p5()
                    .h(px(ROW_HEIGHT))
                    .rounded_lg()
                    .when(selected, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
                    .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                        launcher.action_selected = position;
                        launcher.run_selected_action(window, cx);
                    }))
                    .child(action_icon(&entry.icon))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .when(entry.group == corvo_core::ActionGroup::Destructive, |label| {
                                label.text_color(rgb(COLOR_DESTRUCTIVE))
                            })
                            .child(entry.label.clone()),
                    )
                    .child(div().flex_1())
                    .when_some(entry.hotkey, |row, hotkey| {
                        row.child(div().flex().gap_1().children(hotkey.chars().map(keycap)))
                    }),
            )
        });
        div()
            .id("actions-menu")
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
                    .text_size(px(15.0))
                    .child(if self.actions_filter.is_empty() {
                        div().text_color(rgb(COLOR_TEXT_DIM)).child("Search for actions...")
                    } else {
                        div().child(self.actions_filter.clone())
                    }),
            )
    }

    fn primary_action_label(&self) -> (&'static str, &'static str) {
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
                "calculator" => ("Copy Result", "↵"),
                "quicklinks" | "web-search-fallback" => ("Open in Browser", "↵"),
                "system-actions" | "window-management" => ("Run Action", "↵"),
                "file-search" => ("Open File", "↵"),
                _ => ("Open", "↵"),
            }
        } else {
            ("Open", "↵")
        }
    }

    fn footer(&self, cx: &mut Context<Self>) -> Div {
        let (action_name, action_key) = if self.page == LauncherPage::Emoji || self.page == LauncherPage::Clipboard {
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
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .child(
                div()
                    .id("burger-button")
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .hover(|style| style.bg(rgb(COLOR_PILL)))
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.toggle_burger_menu(cx);
                    }))
                    .child(icons::render_phosphor_svg(
                        phosphor_svgs::style::regular::LIST,
                        rgb(COLOR_TEXT_DIM),
                        16.0,
                    )),
            )
            .when_some(
                self.results.get(self.selected).filter(|_| self.page == LauncherPage::Emoji),
                |footer, res| {
                    footer.child(
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
            .child(
                div()
                    .id("footer-open")
                    .on_click(cx.listener(|launcher, _: &ClickEvent, window, cx| {
                        launcher.execute_selected(window, cx);
                    }))
                    .child(action_pill(&action_name, action_key)),
            )
            .child(
                div()
                    .id("footer-actions")
                    .on_click(cx.listener(|launcher, _: &ClickEvent, _window, cx| {
                        launcher.toggle_actions(cx);
                    }))
                    .child(action_pill("Actions", "⌘K")),
            )
    }

    fn result_row(
        &self,
        index: usize,
        result: &SearchResult,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
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
            .h(px(ROW_HEIGHT))
            .rounded_lg()
            .when(selected, |row| row.bg(rgb(COLOR_ROW_SELECTED)))
            .when(!selected, |row| row.hover(|s| s.bg(rgb(0x181b1e))))
            .on_click(cx.listener(move |launcher, _: &ClickEvent, window, cx| {
                launcher.select(index, cx);
                launcher.execute_selected(window, cx);
            }))
            .child(icon(result.icon.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(if selected { rgb(0xffffff) } else { rgb(COLOR_TEXT) })
                            .font_weight(if selected { FontWeight::MEDIUM } else { FontWeight::NORMAL })
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
                        .text_color(if selected { rgb(0x8cb8a3) } else { rgb(COLOR_TEXT_DIM) })
                        .child(accessory),
                )
            })
    }

}

/// The magnifier icon at the left of the search row.
fn search_icon() -> Div {
    div()
        .flex_none()
        .child(icons::render_phosphor_svg(
            phosphor_svgs::style::regular::MAGNIFYING_GLASS,
            rgb(COLOR_TEXT_ICON),
            20.0,
        ))
}

/// A hint pill: action label plus a keycap, as in the Raycast footer.
fn action_pill(label: &str, key: &str) -> Div {
    let key_node = if key == "↵" {
        div()
            .flex_none()
            .min_w(px(18.0))
            .h(px(18.0))
            .px_1()
            .rounded_sm()
            .bg(rgb(COLOR_KEYCAP))
            .flex()
            .items_center()
            .justify_center()
            .child(icons::render_phosphor_svg(
                phosphor_svgs::style::regular::ARROW_ELBOW_DOWN_LEFT,
                rgb(COLOR_TEXT_DIM),
                11.0,
            ))
    } else {
        div()
            .px_1p5()
            .rounded_sm()
            .bg(rgb(COLOR_KEYCAP))
            .text_size(px(12.0))
            .text_color(rgb(COLOR_TEXT_DIM))
            .child(key.to_string())
    };

    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .rounded_full()
        .bg(rgb(COLOR_PILL))
        .child(div().text_size(px(13.0)).child(label.to_string()))
        .child(key_node)
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
fn action_icon(icon: &Icon) -> Div {
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
            rgb(COLOR_TEXT_ICON),
            18.0,
        ))
    } else if let Icon::Glyph(glyph) = icon {
        slot.text_size(px(16.0))
            .text_color(rgb(COLOR_TEXT_ICON))
            .child(*glyph)
    } else {
        slot.child(icons::render_phosphor_svg(
            phosphor_svgs::style::regular::APP_WINDOW,
            rgb(COLOR_TEXT_ICON),
            18.0,
        ))
    }
}

/// The icon slot at the left of a row: a decoded PNG when the command
/// extracted one, a vector SVG or glyph otherwise.
fn icon(icon: Icon) -> Div {
    let slot = div().flex_none().mr(px(8.0)).size(px(ICON_SIZE));
    match icon {
        Icon::Image(path) => {
            slot.child(img(path).size(px(ICON_SIZE)).rounded_md())
        }
        Icon::Glyph(glyph) if !glyph.is_empty() => slot.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(ICON_SIZE - 4.0))
                .text_color(rgb(COLOR_TEXT_ICON))
                .child(glyph),
        ),
        other => {
            let svg = icons::icon_svg_data(&other)
                .unwrap_or(phosphor_svgs::style::regular::APP_WINDOW);
            slot.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::render_phosphor_svg(
                        svg,
                        rgb(COLOR_TEXT_ICON),
                        ICON_SIZE,
                    )),
            )
        }
    }
}


impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_root = self.page == LauncherPage::Root;
        let is_emoji = self.page == LauncherPage::Emoji;
        let is_clipboard = self.page == LauncherPage::Clipboard;
        div()
            .track_focus(&self.focus_handle)
            .key_context("Launcher")
            .on_action(cx.listener(
                |launcher: &mut Self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>| {
                    if launcher.filter_dropdown_open {
                        launcher.close_filter_dropdown(cx);
                    } else if launcher.burger_menu_open {
                        launcher.close_burger_menu(cx);
                    } else if launcher.actions_open {
                        launcher.close_actions(cx);
                    } else if launcher.page == LauncherPage::Emoji || launcher.page == LauncherPage::Clipboard {
                        launcher.open_root_page(cx);
                    } else {
                        launcher.dismiss(window);
                    }
                },
            ))
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .pt_2()
            .rounded_xl()
            .border_1()
            .border_color(rgb(COLOR_DIVIDER))
            .overflow_hidden()
            .bg(rgba(COLOR_BACKGROUND))
            .text_color(rgb(COLOR_TEXT))
            .font_family("Helvetica")
            .when(is_root, |view| {
                view.child(self.search_row(window, cx))
                    .child(self.results_list(cx))
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
            .child(self.footer(cx))
            .children(self.actions_open.then(|| self.actions_menu(cx)))
            .children(self.burger_menu_open.then(|| self.burger_menu(cx)))
            .when(self.filter_dropdown_open, |view| {
                view.child(
                    div()
                        .id("filter-dropdown-backdrop")
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
        .child(
            div()
                .text_color(rgb(COLOR_TEXT))
                .child(value.to_string()),
        )
}

/// Starts the GPUI app. Every `()` on `toggle_rx` shows or hides the
/// launcher window.
pub fn run(registry: CommandRegistry, toggle_rx: Receiver<()>) {
    gpui_platform::application().run(|cx: &mut App| {
        // Runs after GPUI sets the regular policy, so the launcher
        // stays out of the Dock and Cmd+Tab (SPEC §6).
        corvo_platform::run_as_agent();
        cx.set_global(RegistryGlobal(registry));
        cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("Launcher"))]);

        cx.spawn(async move |cx: &mut AsyncApp| {
            while toggle_rx.recv().await.is_ok() {
                cx.update(toggle);
            }
        })
        .detach();

        open_launcher(cx);
    });
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
    let window_size = size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT));
    let display_id = active_display_id();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(centered_bounds(window_size, cx))),
        display_id,
        // No titlebar: the launcher is a floating panel, not a window.
        // macOS draws no traffic lights; Linux gets client decorations.
        titlebar: None,
        kind: window_kind(),
        is_resizable: false,
        is_movable: true,
        focus: true,
        show: true,
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    };
    let opened: Result<WindowHandle<Launcher>, _> =
        cx.open_window(options, |window, cx| cx.new(|cx| Launcher::new(window, cx)));
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
    corvo_platform::make_panel_instant(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64);
    // The empty query lists the whole corpus, so the first paint shows
    // apps while the corpus scan is still settling. Always start at Home.
    let _ = window.update(cx, |launcher, window, cx| {
        launcher.previous_app = previous_pid;
        launcher.previous_app_name = previous_name;
        launcher.page = LauncherPage::Root;
        launcher.query.clear();
        if launcher.results.is_empty() {
            launcher.results = cached_initial_results();
        }
        launcher.rebuild_root_flat_items();
        launcher.selected = 0;
        launcher.actions_open = false;
        launcher.burger_menu_open = false;
        launcher.filter_dropdown_open = false;
        launcher.results_scroll_handle.scroll_to_item(0);
        corvo_clipboard_manager::poll_clipboard_with_source(launcher.previous_app_name.as_deref());
        launcher.refresh(cx);
        window.activate_window();
    });
    corvo_platform::order_panel_front(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64);
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

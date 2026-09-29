//! First-launch welcome wizard. Opens once on macOS when
//! `[onboarding] shown` is still false in settings.toml, and re-runs from
//! Settings › General. The wizard is its own normal window; the launcher
//! stays out of the way until "Get Started".

use gpui::{
    div, img, prelude::*, px, rgb, size, AnyElement, App, AppContext, Bounds, ClickEvent, Context,
    Div, FocusHandle, FontWeight, Global, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Render, Stateful, Styled, Window, WindowBounds,
    WindowControlArea, WindowHandle, WindowKind, WindowOptions,
};

use crate::settings::{
    format_hotkey_keycaps, COLOR_ACCENT, COLOR_BG, COLOR_BORDER_SUBTLE, COLOR_CARD_BG,
    COLOR_CONTROL_BG, COLOR_CONTROL_BORDER, COLOR_CONTROL_HOVER, COLOR_TEXT, COLOR_TEXT_DIM,
    COLOR_TEXT_MUTED, COLOR_TOGGLE_OFF,
};

/// Accessibility is the only permission the wizard asks for, like Tinycast:
/// it is what window tiling needs, and it never kills the app. The rest
/// (Screen Recording, Calendars, Full Disk) live in Settings › Permissions
/// (SPEC §9).
const WELCOME_PERMISSION: corvo_platform::permissions::PermissionKind =
    corvo_platform::permissions::PermissionKind::Accessibility;

const HOTKEY_RECORDING_COLOR: u32 = 0x3b82f6;

/// App logo for the welcome hero, embedded so release builds need no assets
/// directory on disk.
fn app_logo() -> std::sync::Arc<gpui::Image> {
    static LOGO: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| {
        std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            include_bytes!("../../../assets/corvoIcon.png").to_vec(),
        ))
    })
    .clone()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WelcomeStep {
    Shortcut,
    Permissions,
    Ready,
}

/// macOS asks for Accessibility mid-wizard; other platforms have no
/// equivalent system grant, so their wizard is two steps.
#[cfg(target_os = "macos")]
const WELCOME_STEPS: [WelcomeStep; 3] = [
    WelcomeStep::Shortcut,
    WelcomeStep::Permissions,
    WelcomeStep::Ready,
];
#[cfg(not(target_os = "macos"))]
const WELCOME_STEPS: [WelcomeStep; 2] = [WelcomeStep::Shortcut, WelcomeStep::Ready];

impl WelcomeStep {
    fn index(self) -> usize {
        WELCOME_STEPS
            .iter()
            .position(|step| *step == self)
            .unwrap_or(0)
    }

    fn from_index(index: usize) -> Self {
        WELCOME_STEPS
            .get(index)
            .copied()
            .unwrap_or(WELCOME_STEPS[WELCOME_STEPS.len() - 1])
    }

    fn dots() -> usize {
        WELCOME_STEPS.len()
    }

    fn title(self) -> &'static str {
        match self {
            Self::Shortcut => "Welcome to Corvo",
            Self::Permissions => "Grant Access",
            Self::Ready => "You're all set",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::Shortcut => "Set the shortcut that summons Corvo from anywhere.",
            Self::Permissions => {
                "Let Corvo arrange windows with the window management commands."
            }
            Self::Ready => "Corvo sits in the background until you call it.",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Shortcut => phosphor_svgs::style::regular::COMMAND,
            Self::Permissions => phosphor_svgs::style::regular::SHIELD_CHECK,
            Self::Ready => phosphor_svgs::style::fill::CHECK_CIRCLE,
        }
    }
}

struct WelcomeWindow(WindowHandle<WelcomeView>);
impl Global for WelcomeWindow {}

/// Writes the first-run marker so the wizard is a one-time flow even if the
/// user quits mid-way. Safe to call twice.
pub fn mark_shown() {
    let mut settings = corvo_config::Settings::load();
    if settings.onboarding.shown {
        return;
    }
    settings.onboarding.shown = true;
    if let Err(error) = settings.save() {
        eprintln!("corvo: could not save onboarding state: {error}");
    }
}

/// Opens the welcome window, or focuses it if it is already open. Returns
/// whether a window is on screen.
pub fn open_welcome(cx: &mut App) -> bool {
    if let Some(handle) = cx.try_global::<WelcomeWindow>().map(|g| g.0) {
        if cx.windows().contains(&handle.into()) {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            cx.activate(true);
            return true;
        }
    }

    let display_id = crate::active_display_id();
    let window_size = size(px(560.0), px(500.0));
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
        is_resizable: false,
        is_movable: true,
        focus: true,
        show: true,
        ..Default::default()
    };

    let opened: Result<WindowHandle<WelcomeView>, _> =
        cx.open_window(options, move |window, cx| {
            let view = cx.new(WelcomeView::new);
            let focus = view.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
            view
        });

    match opened {
        Ok(handle) => {
            cx.set_global(WelcomeWindow(handle));
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            #[cfg(not(target_os = "windows"))]
            {
                corvo_platform::activate_app(std::process::id() as i32);
                corvo_platform::order_window_front(560.0, 500.0);
            }
            cx.activate(true);
            true
        }
        Err(error) => {
            eprintln!("corvo: cannot open welcome window: {error}");
            false
        }
    }
}

struct WelcomeView {
    step: WelcomeStep,
    recording_hotkey: bool,
    settings: corvo_config::Settings,
    focus_handle: FocusHandle,
}

impl WelcomeView {
    fn new(cx: &mut Context<Self>) -> Self {
        // A recording started and the window died before it finished (traffic
        // light close, quit): never leave the global hotkeys swallowed.
        cx.on_release(|_, cx| {
            if let Some(global) = cx.try_global::<crate::HotkeyManagerGlobal>() {
                global.0.borrow_mut().set_suppressed(false);
            }
        })
        .detach();
        Self {
            step: WelcomeStep::Shortcut,
            recording_hotkey: false,
            settings: corvo_config::Settings::load(),
            focus_handle: cx.focus_handle(),
        }
    }

    fn set_recording(&mut self, active: bool, cx: &mut Context<Self>) {
        self.recording_hotkey = active;
        if let Some(global) = cx.try_global::<crate::HotkeyManagerGlobal>() {
            global.0.borrow_mut().set_suppressed(active);
        }
    }

    fn accessibility_granted(&self) -> bool {
        corvo_platform::permissions::is_granted(WELCOME_PERMISSION)
    }

    fn set_step(&mut self, step: WelcomeStep, cx: &mut Context<Self>) {
        self.step = step;
        if step == WelcomeStep::Permissions {
            self.start_permission_polling(cx);
        }
        cx.notify();
    }

    /// Permission state can flip while System Settings has focus, so keep
    /// refreshing while the step is on screen. Spawned once per entry into
    /// the step; the loop self-terminates on leave or window close.
    fn start_permission_polling(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            smol::Timer::after(std::time::Duration::from_millis(500)).await;
            let mut still_on_step = false;
            let res = this.update(cx, |view, cx| {
                if view.step == WelcomeStep::Permissions {
                    still_on_step = true;
                    cx.notify();
                }
            });
            if res.is_err() || !still_on_step {
                break;
            }
        })
        .detach();
    }

    /// Narrow writer: merges only the fields this window edits into a
    /// fresh load, so a stale SettingsView snapshot cannot clobber them
    /// (and vice versa) when both write settings.toml.
    fn save_settings(&self) {
        let mut settings = corvo_config::Settings::load();
        settings.hotkey = self.settings.hotkey.clone();
        settings.launch_at_login = self.settings.launch_at_login;
        if let Err(error) = settings.save() {
            eprintln!("corvo: could not save settings: {error}");
        }
        corvo_platform::hotkey::notify_hotkeys_changed();
    }

    fn request_permission(&self, kind: corvo_platform::permissions::PermissionKind) {
        corvo_platform::permissions::request(kind);
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        mark_shown();
        window.remove_window();
        cx.spawn(async move |_this, cx| {
            smol::Timer::after(std::time::Duration::from_millis(100)).await;
            cx.update(crate::present_launcher);
        })
        .detach();
    }

    fn primary_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            WelcomeStep::Shortcut => {
                let next = WelcomeStep::from_index(self.step.index() + 1);
                self.set_step(next, cx);
            }
            WelcomeStep::Permissions => {
                if self.accessibility_granted() {
                    let next = WelcomeStep::from_index(self.step.index() + 1);
                    self.set_step(next, cx);
                } else {
                    self.request_permission(WELCOME_PERMISSION);
                }
            }
            WelcomeStep::Ready => self.finish(window, cx),
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;

        if self.recording_hotkey {
            cx.stop_propagation();
            if keystroke.key == "escape" {
                self.set_recording(false, cx);
                cx.notify();
                return;
            }
            if matches!(
                keystroke.key.as_str(),
                "cmd" | "command" | "control" | "ctrl" | "alt" | "shift" | "meta"
            ) {
                return;
            }
            let mut parts: Vec<&str> = Vec::new();
            if keystroke.modifiers.control {
                parts.push("ctrl");
            }
            if keystroke.modifiers.alt {
                parts.push("alt");
            }
            if keystroke.modifiers.platform {
                parts.push("cmd");
            }
            if keystroke.modifiers.shift {
                parts.push("shift");
            }
            let key = match keystroke.key.as_str() {
                " " | "space" => "space",
                k => k,
            };
            parts.push(key);
            self.settings.hotkey = parts.join("+");
            self.save_settings();
            self.set_recording(false, cx);
            cx.notify();
            return;
        }

        match keystroke.key.as_str() {
            "enter" => {
                cx.stop_propagation();
                self.primary_action(window, cx);
            }
            "escape" => {
                cx.stop_propagation();
                match self.step.index() {
                    0 => window.remove_window(),
                    previous => self.set_step(WelcomeStep::from_index(previous - 1), cx),
                }
            }
            _ => {}
        }
    }

    // MARK: - Rendering

    fn render_header(&self) -> Div {
        div()
            .h(px(28.0))
            .flex()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, |_ev, window, _cx| {
                window.start_window_move();
            })
    }

    fn render_hero(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(self.render_hero_mark())
            .child(
                div()
                    .text_size(px(20.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child(self.step.title()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child(self.step.subtitle()),
            )
    }

    fn render_hero_mark(&self) -> AnyElement {
        match self.step {
            WelcomeStep::Shortcut => img(app_logo())
                .size(px(64.0))
                .into_any_element(),
            _ => div()
                .size(px(56.0))
                .rounded_full()
                .bg(rgb(COLOR_CONTROL_BG))
                .border_1()
                .border_color(rgb(COLOR_BORDER_SUBTLE))
                .flex()
                .items_center()
                .justify_center()
                .child(crate::icons::render_phosphor_svg(
                    self.step.icon(),
                    rgb(COLOR_ACCENT),
                    26.0,
                ))
                .into_any_element(),
        }
    }

    fn welcome_card(&self, rows: Vec<Div>) -> Div {
        let mut row_children = Vec::new();
        for (i, row) in rows.into_iter().enumerate() {
            if i > 0 {
                row_children.push(div().h(px(1.0)).bg(rgb(COLOR_BORDER_SUBTLE)));
            }
            row_children.push(row);
        }
        div()
            .w_full()
            .rounded_lg()
            .bg(rgb(COLOR_CARD_BG))
            .border_1()
            .border_color(rgb(COLOR_BORDER_SUBTLE))
            .px_4()
            .py_2()
            .flex()
            .flex_col()
            .children(row_children)
    }

    fn caption(&self, text: &str) -> Div {
        div()
            .w_full()
            .flex()
            .justify_center()
            .text_size(px(12.0))
            .text_color(rgb(COLOR_TEXT_DIM))
            .child(text.to_string())
    }

    fn render_hotkey_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.recording_hotkey {
            return div()
                .id("welcome-hotkey-recording")
                .h(px(26.0))
                .px_2p5()
                .rounded_md()
                .bg(rgb(COLOR_CONTROL_BG))
                .border_1()
                .border_color(rgb(HOTKEY_RECORDING_COLOR))
                .flex()
                .items_center()
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.set_recording(false, cx);
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(HOTKEY_RECORDING_COLOR))
                        .child("Press keys..."),
                );
        }

        let keycaps = format_hotkey_keycaps(&self.settings.hotkey);
        div()
            .id("welcome-hotkey-button")
            .h(px(26.0))
            .px(px(6.0))
            .rounded_md()
            .bg(rgb(COLOR_CONTROL_BG))
            .border_1()
            .border_color(rgb(COLOR_CONTROL_BORDER))
            .flex()
            .items_center()
            .gap(px(4.0))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)).border_color(rgb(0x44474e)))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.set_recording(true, cx);
                cx.notify();
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
                    .text_size(px(11.0))
                    .text_color(rgb(COLOR_TEXT_DIM))
                    .child("Edit"),
            )
    }

    fn render_shortcut_step(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.welcome_card(vec![
                div()
                    .h(px(44.0))
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
                                    .child("Toggle Corvo"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_TEXT_DIM))
                                    .child("Press this shortcut to open the launcher."),
                            ),
                    )
                    .child(self.render_hotkey_button(cx)),
                self.launch_at_login_row(cx),
            ]))
            .child(self.caption("You can change these anytime in Settings."))
    }

    fn launch_at_login_row(&self, cx: &mut Context<Self>) -> Div {
        let active = self.settings.launch_at_login;
        div()
            .h(px(44.0))
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
                            .child("Launch at login"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(COLOR_TEXT_DIM))
                            .child("Start Corvo automatically when you log in."),
                    ),
            )
            .child(
                div()
                    .id("welcome-toggle-launch-at-login")
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
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        let enabled = !this.settings.launch_at_login;
                        this.settings.launch_at_login = enabled;
                        this.save_settings();
                        std::thread::spawn(move || {
                            let _ = corvo_platform::set_launch_at_login(enabled);
                        });
                        cx.notify();
                    }))
                    .child(div().size(px(18.0)).rounded_full().bg(rgb(0xffffff))),
            )
    }

    /// macOS-only: other platforms have no system grant to reflect here.
    #[cfg(target_os = "macos")]
    fn render_permissions_step(&self, cx: &mut Context<Self>) -> Div {
        let granted = self.accessibility_granted();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.welcome_card(vec![div()
                .min_h(px(40.0))
                .py_1p5()
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
                                .child(WELCOME_PERMISSION.title()),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(COLOR_TEXT_DIM))
                                .truncate()
                                .child(WELCOME_PERMISSION.description()),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(self.permission_status_badge(granted))
                        .children(if granted {
                            None
                        } else {
                            Some(self.permission_grant_button(cx))
                        }),
                )]))
            .child(self.caption(
                "Optional. You can grant this later in Settings › Permissions.",
            ))
    }

    #[cfg(target_os = "macos")]
    fn permission_status_badge(&self, granted: bool) -> Div {
        div()
            .flex()
            .items_center()
            .gap_1p5()
            .child(crate::icons::render_phosphor_svg(
                if granted {
                    phosphor_svgs::style::fill::CHECK_CIRCLE
                } else {
                    phosphor_svgs::style::regular::X_CIRCLE
                },
                if granted {
                    rgb(COLOR_ACCENT)
                } else {
                    rgb(COLOR_TOGGLE_OFF)
                },
                15.0,
            ))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if granted {
                        rgb(COLOR_ACCENT)
                    } else {
                        rgb(COLOR_TEXT_DIM)
                    })
                    .child(if granted { "Granted" } else { "Not granted" }),
            )
    }

    #[cfg(target_os = "macos")]
    fn permission_grant_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("welcome-permission-grant")
            .cursor_pointer()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(rgb(COLOR_ACCENT))
            .hover(|s| s.bg(rgb(0x2fc28a)))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(0x0b0c0d))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, _cx| {
                this.request_permission(WELCOME_PERMISSION);
            }))
            .child("Grant")
    }

    fn render_ready_step(&self) -> Div {
        let keycaps = format_hotkey_keycaps(&self.settings.hotkey);
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .children(keycaps.into_iter().map(|cap| {
                        div()
                            .h(px(24.0))
                            .px(px(7.0))
                            .rounded_sm()
                            .bg(rgb(0x32353c))
                            .border_1()
                            .border_color(rgb(COLOR_CONTROL_BORDER))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xd1d5db))
                                    .child(cap),
                            )
                    })),
            )
            .child(self.caption("Press this shortcut anytime to open Corvo."))
            .child(self.caption("Everything else lives in Settings."))
    }

    fn render_dots(&self) -> Div {
        let current = self.step.index();
        div()
            .flex()
            .items_center()
            .justify_center()
            .gap_1p5()
            .children((0..WelcomeStep::dots()).map(|index| {
                div()
                    .size(px(7.0))
                    .rounded_full()
                    .bg(if index == current {
                        rgb(COLOR_TEXT)
                    } else {
                        rgb(COLOR_TOGGLE_OFF)
                    })
            }))
    }

    fn render_back_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("welcome-back")
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
            .text_size(px(12.5))
            .text_color(rgb(COLOR_TEXT_MUTED))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                let previous = WelcomeStep::from_index(this.step.index() - 1);
                this.set_step(previous, cx);
            }))
            .child(crate::icons::render_phosphor_svg(
                phosphor_svgs::style::regular::CARET_LEFT,
                rgb(COLOR_TEXT_MUTED),
                10.0,
            ))
            .child("Back")
    }

    fn render_skip_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("welcome-skip")
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .hover(|s| s.bg(rgb(COLOR_CONTROL_HOVER)))
            .text_size(px(12.5))
            .text_color(rgb(COLOR_TEXT_MUTED))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.set_step(WelcomeStep::Ready, cx);
            }))
            .child("Skip")
    }

    fn render_primary_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let label = match self.step {
            WelcomeStep::Shortcut => "Continue".to_string(),
            WelcomeStep::Permissions => {
                if self.accessibility_granted() {
                    "Continue".to_string()
                } else {
                    "Grant Access".to_string()
                }
            }
            WelcomeStep::Ready => "Get Started".to_string(),
        };
        div()
            .id("welcome-primary")
            .px_4()
            .py_1p5()
            .rounded_md()
            .bg(rgb(COLOR_ACCENT))
            .hover(|s| s.bg(rgb(0x2fc28a)))
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0x0b0c0d))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.primary_action(window, cx);
            }))
            .child(label)
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> Div {
        let step = self.step;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_4()
            .px_10()
            .pb_6()
            .child(self.render_dots())
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(if step.index() > 0 {
                        self.render_back_button(cx).into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .children(if step == WelcomeStep::Permissions
                                && !self.accessibility_granted()
                            {
                                Some(self.render_skip_button(cx).into_any_element())
                            } else {
                                None
                            })
                            .child(self.render_primary_button(cx).into_any_element()),
                    ),
            )
    }
}

impl Render for WelcomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let step = self.step;
        div()
            .key_context("WelcomeView")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(COLOR_BG))
            .text_color(rgb(COLOR_TEXT))
            .child(self.render_header())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_5()
                    .px_10()
                    .pt_2()
                    .child(self.render_hero())
                    .child({
                        #[cfg(target_os = "macos")]
                        {
                            match step {
                                WelcomeStep::Shortcut => self.render_shortcut_step(cx),
                                WelcomeStep::Permissions => self.render_permissions_step(cx),
                                WelcomeStep::Ready => self.render_ready_step(),
                            }
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            match step {
                                WelcomeStep::Shortcut => self.render_shortcut_step(cx),
                                _ => self.render_ready_step(),
                            }
                        }
                    }),
            )
            .child(self.render_footer(cx))
    }
}

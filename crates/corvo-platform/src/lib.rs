//! OS integration boundary. All `#[cfg(target_os = ...)]` in the
//! workspace lives under this crate. Commands call these traits, never
//! raw OS APIs.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

pub mod hotkey;
pub mod ipc;
pub mod permissions;
pub mod updates;
pub use hotkey::HotkeyIntent;
pub use permissions::PermissionKind;
pub use updates::{
    cached_update_archive, check_for_updates, cleanup_old_installations, clear_pending_release,
    dismiss_version, download_and_verify, install_and_restart, is_version_dismissed,
    load_pending_release, save_pending_release, take_install_error, UpdateChannel, UpdateError,
    UpdateRelease,
};

mod processes;
pub use processes::{
    listening_port_snapshot, process_snapshot, terminate_process, ListeningPortSnapshot,
    ProcessIdentity, ProcessSnapshot, TerminationMode,
};

#[cfg(target_os = "macos")]
mod app_uninstall;
#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// A window on the desktop, as `window-management` sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowHandle {
    pub id: u64,
    pub title: String,
    pub app_name: Option<String>,
}

/// One installed application, as `app-launcher` sees it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub path: PathBuf,
    /// PNG extracted from the app's icon, ready to render. `None` when
    /// the OS keeps its icons in a format this platform cannot read.
    pub icon_png: Option<PathBuf>,
}

/// One app bundle or app-scoped data item that can move to the system Trash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppFileEntry {
    pub path: PathBuf,
    pub location: String,
    pub size_bytes: u64,
    pub is_application: bool,
    pub matched_by_name: bool,
}

/// Results from a bounded scan of common app-data locations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppFileScan {
    pub files: Vec<AppFileEntry>,
    pub reached_scan_limit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformError {
    Unsupported(String),
    Os(String),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::Os(msg) => write!(f, "os error: {msg}"),
        }
    }
}

impl std::error::Error for PlatformError {}

pub type PlatformResult<T> = Result<T, PlatformError>;

/// Session and window control. One implementation per OS.
pub trait PlatformOps: Send + Sync {
    fn lock(&self) -> PlatformResult<()>;
    fn sleep(&self) -> PlatformResult<()>;
    fn shutdown(&self) -> PlatformResult<()>;
    fn list_windows(&self) -> PlatformResult<Vec<WindowHandle>>;
    fn focus_window(&self, handle: &WindowHandle) -> PlatformResult<()>;
    /// Installed applications for the launcher corpus.
    fn list_apps(&self) -> PlatformResult<Vec<AppEntry>>;
    /// Opens a file, directory, or application bundle with the OS default.
    fn open_path(&self, path: &std::path::Path) -> PlatformResult<()>;
    /// Writes text to the system clipboard, replacing its contents.
    fn copy_text(&self, text: &str) -> PlatformResult<()>;
}

/// Runs the process as an OS agent: no Dock icon on macOS, no taskbar
/// entry on Windows, no launcher entry on Linux. Call after the GPUI
/// platform is up, because GPUI sets the regular policy on macOS during
/// launch. Raycast runs the same way (SPEC §6).
pub fn run_as_agent() {
    #[cfg(target_os = "macos")]
    macos::run_as_agent();
}

/// Strips the show/hide animation from the launcher panel and gives it
/// the popup level. No-op where the platform draws no window animation.
pub fn make_panel_instant(_width: f64, _height: f64) {
    #[cfg(target_os = "macos")]
    macos::make_panel_instant(_width, _height);
}

/// Removes the native shadow from Corvo's temporary action toast on macOS.
pub fn remove_action_toast_shadow(_width: f64, _height: f64) {
    #[cfg(target_os = "macos")]
    macos::remove_action_toast_shadow(_width, _height);
}

/// Orders the launcher panel front and makes it key, synchronously.
/// No-op outside macOS.
pub fn order_panel_front(_width: f64, _height: f64) {
    #[cfg(target_os = "macos")]
    macos::order_panel_front(_width, _height);
}

/// Resizes the launcher panel to the specified dimensions keeping the top edge anchored.
pub fn resize_launcher_panel(_width: f64, _height: f64) -> bool {
    #[cfg(target_os = "macos")]
    return macos::resize_launcher_panel(_width, _height);
    #[cfg(not(target_os = "macos"))]
    return false;
}

pub fn forget_launcher_panel() {
    #[cfg(target_os = "macos")]
    macos::forget_launcher_panel();
}

/// Orders a window matching dimensions front and makes it key, synchronously.
/// No-op outside macOS.
pub fn order_window_front(_width: f64, _height: f64) {
    #[cfg(target_os = "macos")]
    macos::order_window_front(_width, _height);
}

/// The pid of the frontmost app, unless it is this process. No-op
/// outside macOS.
pub fn frontmost_app_pid() -> Option<i32> {
    #[cfg(target_os = "macos")]
    return macos::frontmost_app_pid();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    return frontmost_app_info().map(|(pid, _)| pid);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return None;
}

/// The pid and display name of the frontmost app, unless it is this process.
pub fn frontmost_app_info() -> Option<(i32, String)> {
    #[cfg(target_os = "macos")]
    return macos::frontmost_app_info();
    #[cfg(target_os = "windows")]
    return windows::frontmost_app_info();
    #[cfg(target_os = "linux")]
    return linux::frontmost_app_info();
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return None;
}

/// Reads the current string from the system clipboard.
pub fn read_clipboard_text() -> Option<String> {
    #[cfg(target_os = "macos")]
    return macos::read_clipboard_text();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    return portable_clipboard(|clipboard| clipboard.get_text().ok()).flatten();
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return None;
}

/// Returns the current change count of the system clipboard.
pub fn clipboard_change_count() -> isize {
    #[cfg(target_os = "macos")]
    return macos::clipboard_change_count();
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetClipboardSequenceNumber() -> u32;
        }
        return unsafe { GetClipboardSequenceNumber() as isize };
    }
    #[cfg(target_os = "linux")]
    return portable_clipboard(|clipboard| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Ok(image) = clipboard.get_image() {
            1u8.hash(&mut hasher);
            image.width.hash(&mut hasher);
            image.height.hash(&mut hasher);
            image.bytes.hash(&mut hasher);
            return hasher.finish() as isize;
        }
        if let Ok(text) = clipboard.get_text() {
            2u8.hash(&mut hasher);
            text.hash(&mut hasher);
            return hasher.finish() as isize;
        }
        0
    })
    .unwrap_or(0);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return 0;
}

/// Checks whether the system clipboard contains concealed or sensitive data.
pub fn clipboard_is_concealed() -> bool {
    #[cfg(target_os = "macos")]
    return macos::clipboard_is_concealed();
    #[cfg(not(target_os = "macos"))]
    return false;
}

/// Hands activation to another app by pid. No-op outside macOS.
pub fn activate_app(_pid: i32) {
    #[cfg(target_os = "macos")]
    macos::activate_app(_pid);
    #[cfg(target_os = "windows")]
    let _ = windows::activate_app(_pid);
    #[cfg(target_os = "linux")]
    let _ = linux::activate_app(_pid);
}

/// Reads image bytes (PNG) from the system clipboard.
pub fn read_clipboard_image() -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    return macos::read_clipboard_image();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    return portable_clipboard(|clipboard| {
        use image::ImageEncoder;
        let image = clipboard.get_image().ok()?;
        let width = u32::try_from(image.width).ok()?;
        let height = u32::try_from(image.height).ok()?;
        let mut png_bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png_bytes)
            .write_image(
                image.bytes.as_ref(),
                width,
                height,
                image::ColorType::Rgba8.into(),
            )
            .ok()?;
        Some(png_bytes)
    })
    .flatten();
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return None;
}

/// Writes PNG image bytes to the system clipboard.
pub fn copy_image_to_pasteboard(_png_bytes: &[u8]) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::copy_image_to_pasteboard(_png_bytes);
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    return portable_clipboard(|clipboard| {
        let decoded = image::load_from_memory(_png_bytes)
            .map_err(|error| {
                PlatformError::Os(format!("could not decode clipboard image: {error}"))
            })?
            .to_rgba8();
        let (width, height) = decoded.dimensions();
        clipboard
            .set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: decoded.into_raw().into(),
            })
            .map_err(|error| PlatformError::Os(format!("could not write clipboard image: {error}")))
    })
    .unwrap_or_else(|| Err(PlatformError::Os("clipboard is unavailable".into())));
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err(PlatformError::Unsupported("image clipboard write".into()))
}

/// Parses width and height from PNG bytes.
pub fn parse_png_dimensions(_bytes: &[u8]) -> Option<(u32, u32)> {
    #[cfg(target_os = "macos")]
    return macos::parse_png_dimensions(_bytes);
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    return image::load_from_memory_with_format(_bytes, image::ImageFormat::Png)
        .ok()
        .map(|image| (image.width(), image.height()));
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return None;
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn portable_clipboard<R>(operation: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
    use std::sync::{Mutex, OnceLock};

    static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    let clipboard = CLIPBOARD.get_or_init(|| Mutex::new(None));
    let mut clipboard = clipboard.lock().ok()?;
    if clipboard.is_none() {
        *clipboard = arboard::Clipboard::new().ok();
    }
    Some(operation(clipboard.as_mut()?))
}

/// Automatically copies image, reactivates previous app, and synthesizes Cmd+V.
pub async fn auto_paste_image(_target_pid: i32, _png_bytes: &[u8]) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::auto_paste_image(_target_pid, _png_bytes).await;
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        #[cfg(target_os = "linux")]
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            copy_image_to_pasteboard(_png_bytes)?;
            return Err(PlatformError::Unsupported(
                "automatic paste is not available on this Wayland compositor. The image was copied to the clipboard".into(),
            ));
        }
        copy_image_to_pasteboard(_png_bytes)?;
        #[cfg(target_os = "windows")]
        windows::activate_app(_target_pid)?;
        #[cfg(target_os = "linux")]
        linux::activate_app(_target_pid)?;
        smol::Timer::after(std::time::Duration::from_millis(150)).await;
        #[cfg(target_os = "windows")]
        return windows::send_paste_keystroke();
        #[cfg(target_os = "linux")]
        return linux::send_paste_keystroke();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return Err(PlatformError::Unsupported("auto_paste_image".into()));
}

/// Automatically copies text, reactivates the previous app, and synthesizes Cmd+V.
pub async fn auto_paste(_target_pid: i32, text: &str) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::auto_paste(_target_pid, text).await;
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        #[cfg(target_os = "linux")]
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            platform_ops().copy_text(text)?;
            return Err(PlatformError::Unsupported(
                "automatic paste is not available on this Wayland compositor. The text was copied to the clipboard".into(),
            ));
        }
        platform_ops().copy_text(text)?;
        #[cfg(target_os = "windows")]
        windows::activate_app(_target_pid)?;
        #[cfg(target_os = "linux")]
        linux::activate_app(_target_pid)?;
        smol::Timer::after(std::time::Duration::from_millis(150)).await;
        #[cfg(target_os = "windows")]
        return windows::send_paste_keystroke();
        #[cfg(target_os = "linux")]
        return linux::send_paste_keystroke();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err(PlatformError::Unsupported("auto_paste".into()))
}

/// Queries whether the process has macOS Accessibility permissions.
pub fn is_accessibility_trusted(_prompt: bool) -> bool {
    #[cfg(target_os = "macos")]
    return macos::is_accessibility_trusted(_prompt);
    #[cfg(not(target_os = "macos"))]
    true
}

/// Queries whether the process has macOS Full Disk Access permissions.
pub fn is_full_disk_access_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::fs::read_dir("/Library/Application Support/com.apple.TCC").is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Queries whether the process has macOS Calendar permissions.
pub fn is_calendar_access_granted() -> bool {
    #[cfg(target_os = "macos")]
    return macos::is_calendar_access_granted();
    #[cfg(not(target_os = "macos"))]
    true
}

/// Requests full access to macOS Calendars.
pub fn request_calendar_access() {
    #[cfg(target_os = "macos")]
    macos::request_calendar_access();
}

/// Returns the display ID of the display containing the cursor, or primary display.
pub fn active_display_id() -> Option<u32> {
    #[cfg(target_os = "macos")]
    return macos::active_display_id();
    #[cfg(not(target_os = "macos"))]
    None
}

/// Lists installed applications within custom search scopes.
pub fn list_apps_in_scopes(_scopes: &[String]) -> PlatformResult<Vec<AppEntry>> {
    #[cfg(target_os = "macos")]
    return macos::list_apps_in_scopes(_scopes);
    #[cfg(not(target_os = "macos"))]
    platform_ops().list_apps()
}

#[cfg(target_os = "windows")]
pub fn hydrate_app_icons(apps: &mut [AppEntry]) {
    windows::hydrate_app_icons(apps);
}

#[cfg(target_os = "windows")]
pub fn hydrate_shortcut_icons(apps: &mut [AppEntry]) {
    windows::hydrate_shortcut_icons(apps);
}

#[cfg(target_os = "windows")]
pub fn hydrate_start_app_icons(apps: &mut [AppEntry]) {
    windows::hydrate_start_app_icons(apps);
}

#[cfg(target_os = "windows")]
pub fn append_start_apps(apps: &mut Vec<AppEntry>) {
    windows::append_start_apps(apps);
}

#[cfg(target_os = "windows")]
pub fn set_launcher_window_visible(handle: isize, visible: bool) {
    windows::set_launcher_window_visible(handle, visible);
}

/// Finds an app bundle and data named with its bundle identifier.
pub fn associated_app_files(_app_path: &std::path::Path) -> PlatformResult<AppFileScan> {
    #[cfg(target_os = "macos")]
    return app_uninstall::associated_app_files(_app_path);
    #[cfg(target_os = "windows")]
    return windows::associated_app_files(_app_path);
    #[cfg(target_os = "linux")]
    return linux::associated_app_files(_app_path);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err(PlatformError::Unsupported(
        "app uninstall is not supported on this platform".into(),
    ))
}

/// Moves selected app files to the system Trash after a fresh safety check.
pub fn move_app_files_to_trash(
    _app_path: &std::path::Path,
    _paths: &[PathBuf],
) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return app_uninstall::move_app_files_to_trash(_app_path, _paths);
    #[cfg(target_os = "windows")]
    return windows::move_app_files_to_trash(_app_path, _paths);
    #[cfg(target_os = "linux")]
    return linux::move_app_files_to_trash(_app_path, _paths);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err(PlatformError::Unsupported(
        "app uninstall is not supported on this platform".into(),
    ))
}

/// Whether this host can show the app uninstall action.
pub const fn supports_app_uninstall() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ))
}

/// Returns true when Corvo can use a native package manager to remove this app.
pub fn supports_app_uninstall_path(path: &std::path::Path) -> bool {
    #[cfg(target_os = "macos")]
    return path.extension().is_some_and(|extension| extension == "app")
        && !path.starts_with("/System/");
    #[cfg(target_os = "windows")]
    return windows::supports_app_uninstall_path(path);
    #[cfg(target_os = "linux")]
    return linux::supports_app_uninstall_path(path);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        false
    }
}

/// Renders the app bundle's icon to a cached PNG.
pub fn extract_app_icon(_bundle: &std::path::Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return macos::extract_app_icon(_bundle);
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Queries whether launch at login is currently enabled on the host platform.
pub fn is_launch_at_login_enabled() -> bool {
    #[cfg(target_os = "macos")]
    return macos::is_launch_at_login_enabled();
    #[cfg(target_os = "windows")]
    return windows::is_launch_at_login_enabled();
    #[cfg(target_os = "linux")]
    return linux::is_launch_at_login_enabled();
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return false;
}

/// Enables or disables launch at login on the host platform.
/// Marks a clean exit so a Screen Recording relaunch watchdog stands down
/// instead of bringing Corvo back.
pub fn clear_screen_recording_relaunch_marker() {
    #[cfg(target_os = "macos")]
    macos::clear_screen_recording_relaunch_marker();
}

pub fn set_launch_at_login(enabled: bool) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::set_launch_at_login(enabled);
    #[cfg(target_os = "windows")]
    return windows::set_launch_at_login(enabled);
    #[cfg(target_os = "linux")]
    return linux::set_launch_at_login(enabled);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = enabled;
        Ok(())
    }
}

/// Executes a shell command on the host platform.
pub fn run_shell(cmd: &str) -> PlatformResult<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let status = std::process::Command::new("cmd")
            .args(["/c", cmd])
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|error| {
                PlatformError::Os(format!("could not start shell command: {error}"))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!(
                "shell command exited with status {}",
                status
                    .code()
                    .map_or_else(|| "unknown".into(), |code| code.to_string())
            )))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(cmd)
            .status()
            .map_err(|error| {
                PlatformError::Os(format!("could not start shell command: {error}"))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!(
                "shell command exited with status {}",
                status
                    .code()
                    .map_or_else(|| "unknown".into(), |code| code.to_string())
            )))
        }
    }
}

/// Opens a URL or system URI with the default system handler.
pub fn open_url(url: &str) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open").arg(url).spawn();
        if status.is_ok() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!("failed to open URL {url}")))
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let status = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .creation_flags(0x0800_0000)
            .spawn();
        if status.is_ok() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!("failed to open URL {url}")))
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let status = std::process::Command::new("xdg-open").arg(url).spawn();
        if status.is_ok() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!("failed to open URL {url}")))
        }
    }
}

/// The platform implementation for the OS this binary was built for.
pub fn platform_ops() -> Arc<dyn PlatformOps> {
    #[cfg(target_os = "macos")]
    return Arc::new(macos::MacPlatform);
    #[cfg(all(unix, not(target_os = "macos")))]
    return Arc::new(linux::LinuxPlatform);
    #[cfg(target_os = "windows")]
    return Arc::new(windows::WindowsPlatform);
}

/// Adjusts display brightness by delta (-1.0 to 1.0).
pub fn adjust_brightness(delta: f32) -> PlatformResult<()> {
    adjust_brightness_with_level(delta).map(|_| ())
}

/// Adjusts display brightness and returns the resulting percentage when available.
pub fn adjust_brightness_with_level(delta: f32) -> PlatformResult<Option<f32>> {
    #[cfg(target_os = "macos")]
    return macos::adjust_brightness(delta).map(Some);
    #[cfg(target_os = "windows")]
    return windows::adjust_brightness_with_level(delta).map(Some);
    #[cfg(target_os = "linux")]
    return linux::adjust_brightness_with_level(delta).map(Some);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err(PlatformError::Unsupported(
        "display brightness control is not available on this platform".into(),
    ))
}

/// Adjusts the default audio output by a fraction and returns the new percentage.
pub fn adjust_audio_output_with_level(delta: f32) -> PlatformResult<Option<f32>> {
    #[cfg(target_os = "macos")]
    return macos::adjust_audio_output_with_level(delta).map(Some);
    #[cfg(target_os = "windows")]
    return windows::adjust_audio_output_with_level(delta).map(Some);
    #[cfg(target_os = "linux")]
    return linux::adjust_audio_output_with_level(delta).map(Some);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = delta;
        Err(PlatformError::Unsupported(
            "audio output control is not available on this platform".into(),
        ))
    }
}

/// Reads the current audio output level as a percentage when the platform exposes it.
pub fn audio_output_level() -> Option<f32> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args(["-e", "output volume of (get volume settings)"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f32>()
            .ok()
            .map(|percent| percent.clamp(0.0, 100.0))
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(output) = std::process::Command::new("wpctl")
            .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
            .output()
            .ok()
            .filter(|output| output.status.success())
        {
            let value = String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .find_map(|part| part.parse::<f32>().ok());
            if let Some(value) = value {
                return Some((value * 100.0).clamp(0.0, 100.0));
            }
        }
        if let Some(output) = std::process::Command::new("pactl")
            .args(["get-sink-volume", "@DEFAULT_SINK@"])
            .output()
            .ok()
            .filter(|output| output.status.success())
        {
            let text = String::from_utf8_lossy(&output.stdout);
            return text
                .split_whitespace()
                .find_map(|part| part.strip_suffix('%')?.parse::<f32>().ok())
                .map(|percent| percent.clamp(0.0, 100.0));
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        windows::audio_output_level()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

static WINDOW_GAP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Sets the gap between windows in pixels.
pub fn set_window_gap(gap: usize) {
    WINDOW_GAP.store(gap, std::sync::atomic::Ordering::Relaxed);
}

/// Gets the current gap between windows.
pub fn get_window_gap() -> usize {
    WINDOW_GAP.load(std::sync::atomic::Ordering::Relaxed)
}

/// Opens an application by bundle path or name.
pub fn open_app(name_or_path: &str) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::open_app(name_or_path);
    #[cfg(not(target_os = "macos"))]
    {
        let ops = platform_ops();
        ops.open_path(std::path::Path::new(name_or_path))
    }
}

/// Captures the positions of currently visible application windows.
pub fn capture_current_window_layout() -> Vec<(String, String)> {
    #[cfg(target_os = "macos")]
    return macos::capture_current_window_layout();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// Applies a window layout arrangement for multiple applications.
pub fn apply_window_layout(placements: &[(String, String)]) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::apply_window_layout(placements);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = placements;
        Ok(())
    }
}

/// Tiles the target application window using native OS accessibility APIs.
pub fn tile_window(target_pid: Option<i32>, action: &str) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::tile_window(target_pid, action);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target_pid, action);
        Ok(())
    }
}

/// Updates the display cache for multi-monitor geometry calculation.
pub fn update_screens_cache() {
    #[cfg(target_os = "macos")]
    macos::update_screens_cache();
}

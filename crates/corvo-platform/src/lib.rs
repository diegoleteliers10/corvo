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
    check_for_updates, cleanup_old_installations, dismiss_version, download_and_verify,
    install_and_restart, is_version_dismissed, UpdateChannel, UpdateError, UpdateRelease,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod app_uninstall;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(all(unix, not(target_os = "macos")))]
mod linux;

/// A window on the desktop, as `window-management` sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowHandle {
    pub id: u64,
    pub title: String,
    pub app_name: Option<String>,
}

/// One installed application, as `app-launcher` sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// The pid and display name of the frontmost app, unless it is this process.
pub fn frontmost_app_info() -> Option<(i32, String)> {
    #[cfg(target_os = "macos")]
    return macos::frontmost_app_info();
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Reads the current string from the system clipboard.
pub fn read_clipboard_text() -> Option<String> {
    #[cfg(target_os = "macos")]
    return macos::read_clipboard_text();
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Returns the current change count of the system clipboard.
pub fn clipboard_change_count() -> isize {
    #[cfg(target_os = "macos")]
    return macos::clipboard_change_count();
    #[cfg(not(target_os = "macos"))]
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
}

/// Reads image bytes (PNG) from the system clipboard.
pub fn read_clipboard_image() -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    return macos::read_clipboard_image();
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Writes PNG image bytes to the system clipboard.
pub fn copy_image_to_pasteboard(_png_bytes: &[u8]) {
    #[cfg(target_os = "macos")]
    macos::copy_image_to_pasteboard(_png_bytes);
}

/// Parses width and height from PNG bytes.
pub fn parse_png_dimensions(_bytes: &[u8]) -> Option<(u32, u32)> {
    #[cfg(target_os = "macos")]
    return macos::parse_png_dimensions(_bytes);
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Automatically copies image, reactivates previous app, and synthesizes Cmd+V.
pub async fn auto_paste_image(_target_pid: i32, _png_bytes: &[u8]) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::auto_paste_image(_target_pid, _png_bytes).await;
    #[cfg(not(target_os = "macos"))]
    return Err(PlatformError::Unsupported("auto_paste_image".into()));
}

/// Automatically copies text, reactivates the previous app, and synthesizes Cmd+V.
pub async fn auto_paste(_target_pid: i32, text: &str) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::auto_paste(_target_pid, text).await;
    #[cfg(not(target_os = "macos"))]
    {
        let ops = platform_ops();
        ops.copy_text(text)
    }
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

/// Finds an app bundle and data named with its bundle identifier.
pub fn associated_app_files(_app_path: &std::path::Path) -> PlatformResult<AppFileScan> {
    #[cfg(target_os = "macos")]
    return app_uninstall::associated_app_files(_app_path);
    #[cfg(not(target_os = "macos"))]
    Err(PlatformError::Unsupported("app uninstall is not supported on this platform".into()))
}

/// Moves selected app files to the system Trash after a fresh safety check.
pub fn move_app_files_to_trash(_app_path: &std::path::Path, _paths: &[PathBuf]) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return app_uninstall::move_app_files_to_trash(_app_path, _paths);
    #[cfg(not(target_os = "macos"))]
    Err(PlatformError::Unsupported("app uninstall is not supported on this platform".into()))
}

/// Whether this host can show the app uninstall action.
pub const fn supports_app_uninstall() -> bool {
    cfg!(target_os = "macos")
}

/// Renders the app bundle's icon to a cached PNG.
pub fn extract_app_icon(_bundle: &std::path::Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return macos::extract_app_icon(_bundle);
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Executes a shell command on the host platform.
pub fn run_shell(cmd: &str) -> PlatformResult<()> {
    #[cfg(target_os = "windows")]
    {
        let status = std::process::Command::new("cmd")
            .args(["/c", cmd])
            .spawn();
        if status.is_ok() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!("failed to run shell command: {cmd}")))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(cmd)
            .spawn();
        if status.is_ok() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!("failed to run shell command: {cmd}")))
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
        let status = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
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
    #[cfg(target_os = "macos")]
    return macos::adjust_brightness(delta);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = delta;
        Ok(())
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

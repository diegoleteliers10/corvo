//! OS integration boundary. All `#[cfg(target_os = ...)]` in the
//! workspace lives under this crate. Commands call these traits, never
//! raw OS APIs.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

pub mod hotkey;
pub mod ipc;

#[cfg(target_os = "macos")]
mod macos;
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
pub fn make_panel_instant(width: f64, height: f64) {
    #[cfg(target_os = "macos")]
    macos::make_panel_instant(width, height);
}

/// Orders the launcher panel front and makes it key, synchronously.
/// No-op outside macOS.
pub fn order_panel_front(width: f64, height: f64) {
    #[cfg(target_os = "macos")]
    macos::order_panel_front(width, height);
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
pub fn activate_app(pid: i32) {
    #[cfg(target_os = "macos")]
    macos::activate_app(pid);
}

/// Reads image bytes (PNG) from the system clipboard.
pub fn read_clipboard_image() -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    return macos::read_clipboard_image();
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Writes PNG image bytes to the system clipboard.
pub fn copy_image_to_pasteboard(png_bytes: &[u8]) {
    #[cfg(target_os = "macos")]
    macos::copy_image_to_pasteboard(png_bytes);
}

/// Parses width and height from PNG bytes.
pub fn parse_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    #[cfg(target_os = "macos")]
    return macos::parse_png_dimensions(bytes);
    #[cfg(not(target_os = "macos"))]
    return None;
}

/// Automatically copies image, reactivates previous app, and synthesizes Cmd+V.
pub async fn auto_paste_image(target_pid: i32, png_bytes: &[u8]) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::auto_paste_image(target_pid, png_bytes).await;
    #[cfg(not(target_os = "macos"))]
    return Err(PlatformError::Unsupported("auto_paste_image".into()));
}

/// Automatically copies text, reactivates the previous app, and synthesizes Cmd+V.
pub async fn auto_paste(target_pid: i32, text: &str) -> PlatformResult<()> {
    #[cfg(target_os = "macos")]
    return macos::auto_paste(target_pid, text).await;
    #[cfg(not(target_os = "macos"))]
    {
        let ops = platform_ops();
        ops.copy_text(text)
    }
}

/// Queries whether the process has macOS Accessibility permissions.
pub fn is_accessibility_trusted(prompt: bool) -> bool {
    #[cfg(target_os = "macos")]
    return macos::is_accessibility_trusted(prompt);
    #[cfg(not(target_os = "macos"))]
    true
}

/// Returns the display ID of the display containing the cursor, or primary display.
pub fn active_display_id() -> Option<u32> {
    #[cfg(target_os = "macos")]
    return macos::active_display_id();
    #[cfg(not(target_os = "macos"))]
    None
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
        let status = std::process::Command::new("open").arg(url).status();
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
            .status();
        if status.is_ok() {
            Ok(())
        } else {
            Err(PlatformError::Os(format!("failed to open URL {url}")))
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let status = std::process::Command::new("xdg-open").arg(url).status();
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

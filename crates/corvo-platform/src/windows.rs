//! Windows implementation. Real bodies land in phase 4.

use super::{AppEntry, PlatformError, PlatformOps, PlatformResult, WindowHandle};

pub struct WindowsPlatform;

impl PlatformOps for WindowsPlatform {
    fn lock(&self) -> PlatformResult<()> {
        Err(unimplemented_os("lock"))
    }

    fn sleep(&self) -> PlatformResult<()> {
        Err(unimplemented_os("sleep"))
    }

    fn shutdown(&self) -> PlatformResult<()> {
        Err(unimplemented_os("shutdown"))
    }

    fn list_windows(&self) -> PlatformResult<Vec<WindowHandle>> {
        Err(unimplemented_os("list_windows"))
    }

    fn focus_window(&self, _handle: &WindowHandle) -> PlatformResult<()> {
        Err(unimplemented_os("focus_window"))
    }

    fn list_apps(&self) -> PlatformResult<Vec<AppEntry>> {
        Err(unimplemented_os("list_apps"))
    }

    fn open_path(&self, _path: &std::path::Path) -> PlatformResult<()> {
        Err(unimplemented_os("open_path"))
    }

    fn copy_text(&self, _text: &str) -> PlatformResult<()> {
        Err(unimplemented_os("copy_text"))
    }
}

fn unimplemented_os(what: &str) -> PlatformError {
    PlatformError::Unsupported(format!("windows {what} arrives in phase 4"))
}

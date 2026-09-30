#[derive(Clone, Debug, PartialEq)]
pub enum ActionExecution {
    RunShell(String),
    OpenUrl(String),
    AdjustBrightness(f32),
    AdjustVolume(f32),
    /// A platform API call. Preferred over `RunShell` when the platform
    /// has a real API: a shell one-liner has to survive another layer's
    /// quoting rules, and on Windows it did not.
    Native(corvo_core::NativeAction),
}

pub struct SystemActionDef {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: &'static str,
    pub icon: &'static str,
    pub execution: ActionExecution,
}

pub struct SystemSettingDef {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: &'static str,
    pub icon: &'static str,
    pub execution: ActionExecution,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{get_system_actions, get_system_settings};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{get_system_actions, get_system_settings};

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(all(unix, not(target_os = "macos")))]
pub use linux::{get_system_actions, get_system_settings};

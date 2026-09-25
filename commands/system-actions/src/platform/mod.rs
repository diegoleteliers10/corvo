#[derive(Clone, Debug, PartialEq)]
pub enum ActionExecution {
    RunShell(String),
    OpenUrl(String),
    AdjustBrightness(f32),
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

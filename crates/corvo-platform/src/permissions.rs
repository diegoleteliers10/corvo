//! Unified system permissions subsystem for Corvo.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PermissionKind {
    Accessibility,
    Calendars,
    ScreenRecording,
    FullDiskAccess,
}

impl PermissionKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Accessibility => "Accessibility",
            Self::Calendars => "Calendars",
            Self::ScreenRecording => "Screen Recording",
            Self::FullDiskAccess => "Full Disk Access",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Accessibility => {
                "Arranges application windows and pastes content directly into target apps."
            }
            Self::Calendars => "Reads upcoming meeting schedules and join links.",
            Self::ScreenRecording => {
                "Inspects window titles and captures thumbnails for window switcher."
            }
            Self::FullDiskAccess => "Searches files across protected system directories.",
        }
    }

    pub fn deep_link_url(self) -> &'static str {
        match self {
            Self::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Self::Calendars => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars"
            }
            Self::ScreenRecording => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            Self::FullDiskAccess => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
            }
        }
    }
}

/// Checks whether permission is granted without triggering an OS prompt.
pub fn is_granted(kind: PermissionKind) -> bool {
    #[cfg(target_os = "macos")]
    {
        match kind {
            PermissionKind::Accessibility => super::macos::is_accessibility_trusted(false),
            PermissionKind::Calendars => super::macos::is_calendar_access_granted(),
            PermissionKind::ScreenRecording => super::macos::is_screen_recording_granted(),
            PermissionKind::FullDiskAccess => super::is_full_disk_access_granted(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        true
    }
}

/// Requests the permission, prompting the system dialog if supported and opening System Settings.
pub fn request(kind: PermissionKind) {
    #[cfg(target_os = "macos")]
    {
        match kind {
            PermissionKind::Accessibility => {
                super::macos::is_accessibility_trusted(true);
            }
            PermissionKind::ScreenRecording => {
                super::macos::request_screen_recording();
            }
            PermissionKind::Calendars => {
                let status = super::macos::calendar_authorization_status();
                if status == 0 {
                    super::macos::request_calendar_access();
                    return;
                }
            }
            _ => {}
        }
        let _ = super::open_url(kind.deep_link_url());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
    }
}

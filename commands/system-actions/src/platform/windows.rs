use super::{ActionExecution, SystemActionDef, SystemSettingDef};

pub fn get_system_actions() -> Vec<SystemActionDef> {
    vec![
        SystemActionDef {
            id: "dismiss-notifications",
            title: "Dismiss Notifications",
            keywords: "dismiss notifications close clear banners alerts action center",
            icon: phosphor_svgs::style::regular::BELL_SLASH,
            // There is no Win32 API that clears the Action Center.
            // ToastNotificationManager.History.Clear() only removes the
            // calling app's own toasts, so from an unpackaged desktop app
            // it cannot clear the user's notifications at all. Open the
            // page where they can do it instead.
            execution: ActionExecution::OpenUrl("ms-settings:notifications".into()),
        },
        SystemActionDef {
            id: "eject-all-disks",
            title: "Eject All Disks",
            keywords: "eject all disks unmount usb drives volumes removable safely remove",
            icon: phosphor_svgs::style::regular::EJECT,
            execution: ActionExecution::Native(corvo_core::NativeAction::EjectRemovableDisks),
        },
        SystemActionDef {
            id: "lock",
            title: "Lock Screen",
            keywords: "lock screen display security sleep workstation",
            icon: phosphor_svgs::style::regular::LOCK,
            execution: ActionExecution::Native(corvo_core::NativeAction::LockWorkstation),
        },
        SystemActionDef {
            id: "sleep",
            title: "Sleep",
            keywords: "sleep suspend system power stand by",
            icon: phosphor_svgs::style::regular::MOON,
            execution: ActionExecution::Native(corvo_core::NativeAction::Suspend),
        },
        SystemActionDef {
            id: "restart",
            title: "Restart",
            keywords: "restart reboot system",
            icon: phosphor_svgs::style::regular::ARROW_CLOCKWISE,
            execution: ActionExecution::RunShell("shutdown.exe /r /t 0".into()),
        },
        SystemActionDef {
            id: "shutdown",
            title: "Shut Down",
            keywords: "shut down shutdown power off system",
            icon: phosphor_svgs::style::regular::POWER,
            execution: ActionExecution::RunShell("shutdown.exe /s /t 0".into()),
        },
        SystemActionDef {
            id: "logout",
            title: "Log Out",
            keywords: "log out logout sign out user session",
            icon: phosphor_svgs::style::regular::SIGN_OUT,
            execution: ActionExecution::RunShell("shutdown.exe /l".into()),
        },
        SystemActionDef {
            id: "empty-trash",
            title: "Empty Recycle Bin",
            keywords: "empty trash recycle bin delete cleanup",
            icon: phosphor_svgs::style::regular::TRASH,
            execution: ActionExecution::Native(corvo_core::NativeAction::EmptyRecycleBin),
        },
        SystemActionDef {
            id: "toggle-appearance",
            title: "Toggle System Appearance",
            keywords: "toggle dark mode light mode appearance theme switch personalize",
            icon: phosphor_svgs::style::regular::SUN,
            execution: ActionExecution::Native(corvo_core::NativeAction::ToggleDarkMode),
        },
        SystemActionDef {
            id: "toggle-mute",
            title: "Toggle Mute",
            keywords: "toggle mute audio sound volume silence",
            icon: phosphor_svgs::style::regular::SPEAKER_SLASH,
            execution: ActionExecution::Native(corvo_core::NativeAction::ToggleMute),
        },
        SystemActionDef {
            id: "volume-up",
            title: "Volume Up",
            keywords: "volume up louder increase audio sound",
            icon: phosphor_svgs::style::regular::SPEAKER_HIGH,
            execution: ActionExecution::AdjustVolume(0.05),
        },
        SystemActionDef {
            id: "volume-down",
            title: "Volume Down",
            keywords: "volume down quieter decrease audio sound lower",
            icon: phosphor_svgs::style::regular::SPEAKER_LOW,
            execution: ActionExecution::AdjustVolume(-0.05),
        },
        SystemActionDef {
            id: "brightness-up",
            title: "Increase Brightness",
            keywords: "brightness up increase display screen brighter monitor",
            icon: phosphor_svgs::style::regular::SUN,
            execution: ActionExecution::AdjustBrightness(0.05),
        },
        SystemActionDef {
            id: "brightness-down",
            title: "Decrease Brightness",
            keywords: "brightness down decrease display screen dimmer monitor",
            icon: phosphor_svgs::style::regular::SUN_HORIZON,
            execution: ActionExecution::AdjustBrightness(-0.05),
        },
        SystemActionDef {
            id: "next-track",
            title: "Next Track",
            keywords: "next track song forward music skip player audio",
            icon: phosphor_svgs::style::regular::SKIP_FORWARD,
            execution: ActionExecution::Native(corvo_core::NativeAction::MediaNextTrack),
        },
        SystemActionDef {
            id: "open-trash",
            title: "Open Trash",
            keywords: "open trash recycle bin folder deleted items",
            icon: phosphor_svgs::style::regular::TRASH,
            // explorer.exe exits non-zero even when it opened the
            // window, which surfaced as a false error. The shell
            // namespace moniker reports success honestly.
            execution: ActionExecution::Native(corvo_core::NativeAction::OpenRecycleBin),
        },
        SystemActionDef {
            id: "play-pause",
            title: "Play / Pause",
            keywords: "play pause music audio song player toggle",
            icon: phosphor_svgs::style::regular::PLAY_PAUSE,
            execution: ActionExecution::Native(corvo_core::NativeAction::MediaPlayPause),
        },
        SystemActionDef {
            id: "previous-track",
            title: "Previous Track",
            keywords: "previous track song back music rewind player audio",
            icon: phosphor_svgs::style::regular::SKIP_BACK,
            execution: ActionExecution::Native(corvo_core::NativeAction::MediaPreviousTrack),
        },
        SystemActionDef {
            id: "hide-apps",
            title: "Hide All Applications",
            keywords: "hide all minimize desktop show desktop",
            icon: phosphor_svgs::style::regular::EYE_SLASH,
            execution: ActionExecution::Native(corvo_core::NativeAction::ShowDesktop),
        },
        SystemActionDef {
            id: "quit-all",
            title: "Quit All Applications",
            keywords: "quit all applications apps terminate close everything",
            icon: phosphor_svgs::style::regular::X_CIRCLE,
            execution: ActionExecution::Native(corvo_core::NativeAction::QuitAllApplications),
        },
    ]
}

pub fn get_system_settings() -> Vec<SystemSettingDef> {
    vec![
        SystemSettingDef {
            id: "about",
            title: "About Settings",
            keywords: "about system info specifications device hardware windows version",
            icon: phosphor_svgs::style::regular::LAPTOP,
            execution: ActionExecution::OpenUrl("ms-settings:about".into()),
        },
        SystemSettingDef {
            id: "battery",
            title: "Battery & Power Settings",
            keywords: "battery power energy saver charge percentage sleep screen timeout",
            icon: phosphor_svgs::style::regular::BATTERY_FULL,
            execution: ActionExecution::OpenUrl("ms-settings:batterysaver".into()),
        },
        SystemSettingDef {
            id: "displays",
            title: "Display Settings",
            keywords: "displays monitor screen resolution scaling brightness hdr night light",
            icon: phosphor_svgs::style::regular::MONITOR,
            execution: ActionExecution::OpenUrl("ms-settings:display".into()),
        },
        SystemSettingDef {
            id: "sound",
            title: "Sound Settings",
            keywords: "sound audio volume output input speakers microphone devices",
            icon: phosphor_svgs::style::regular::SPEAKER_HIGH,
            execution: ActionExecution::OpenUrl("ms-settings:sound".into()),
        },
        SystemSettingDef {
            id: "wifi",
            title: "Wi-Fi Settings",
            keywords: "wifi wireless network internet connections ssid router",
            icon: phosphor_svgs::style::regular::WIFI_HIGH,
            execution: ActionExecution::OpenUrl("ms-settings:network-wifi".into()),
        },
        SystemSettingDef {
            id: "network",
            title: "Network Status & Settings",
            keywords: "network ethernet vpn internet dns proxy status adapter",
            icon: phosphor_svgs::style::regular::GLOBE,
            execution: ActionExecution::OpenUrl("ms-settings:network".into()),
        },
        SystemSettingDef {
            id: "bluetooth",
            title: "Bluetooth & Devices",
            keywords: "bluetooth devices connect pair wireless airpods headphones mouse keyboard",
            icon: phosphor_svgs::style::regular::BLUETOOTH,
            execution: ActionExecution::OpenUrl("ms-settings:bluetooth".into()),
        },
        SystemSettingDef {
            id: "appearance",
            title: "Colors & Appearance Settings",
            keywords: "appearance dark mode light mode accent colors personalization theme",
            icon: phosphor_svgs::style::regular::MOON_STARS,
            execution: ActionExecution::OpenUrl("ms-settings:personalization-colors".into()),
        },
        SystemSettingDef {
            id: "wallpaper",
            title: "Background & Wallpaper Settings",
            keywords: "wallpaper background desktop image pictures personalization colors",
            icon: phosphor_svgs::style::regular::IMAGE,
            execution: ActionExecution::OpenUrl("ms-settings:personalization-background".into()),
        },
        SystemSettingDef {
            id: "lock-screen",
            title: "Lock Screen Settings",
            keywords: "lock screen display sleep timeout password spotlight wallpaper",
            icon: phosphor_svgs::style::regular::LOCK,
            execution: ActionExecution::OpenUrl("ms-settings:lockscreen".into()),
        },
        SystemSettingDef {
            id: "notifications",
            title: "Notifications & Actions Settings",
            keywords: "notifications alerts badges focus assist do not disturb sounds",
            icon: phosphor_svgs::style::regular::BELL,
            execution: ActionExecution::OpenUrl("ms-settings:notifications".into()),
        },
        SystemSettingDef {
            id: "accessibility",
            title: "Accessibility / Ease of Access Settings",
            keywords:
                "accessibility vision hearing magnifier contrast narrator captions ease of access",
            icon: phosphor_svgs::style::regular::USER_CIRCLE,
            execution: ActionExecution::OpenUrl("ms-settings:easeofaccess".into()),
        },
        SystemSettingDef {
            id: "control-center",
            title: "Power & Battery Settings",
            keywords: "power sleep battery battery saver screen timeout performance",
            icon: phosphor_svgs::style::regular::BATTERY_FULL,
            execution: ActionExecution::OpenUrl("ms-settings:powersleep".into()),
        },
        SystemSettingDef {
            id: "desktop-dock",
            title: "Taskbar Settings",
            keywords: "taskbar dock taskbar items alignment system tray badges",
            icon: phosphor_svgs::style::regular::DESKTOP,
            execution: ActionExecution::OpenUrl("ms-settings:taskbar".into()),
        },
        SystemSettingDef {
            id: "keyboard",
            title: "Keyboard Settings",
            keywords: "keyboard typing input layout repeat rate shortcuts",
            icon: phosphor_svgs::style::regular::KEYBOARD,
            execution: ActionExecution::OpenUrl("ms-settings:typing".into()),
        },
        SystemSettingDef {
            id: "trackpad",
            title: "Touchpad Settings",
            keywords: "touchpad sensitivity gestures gestures tap click pinch zoom",
            icon: phosphor_svgs::style::regular::HAND_TAP,
            execution: ActionExecution::OpenUrl("ms-settings:devices-touchpad".into()),
        },
        SystemSettingDef {
            id: "mouse",
            title: "Mouse Settings",
            keywords: "mouse speed pointer primary button scrolling natural click",
            icon: phosphor_svgs::style::regular::MOUSE,
            execution: ActionExecution::OpenUrl("ms-settings:mousetouchpad".into()),
        },
        SystemSettingDef {
            id: "date-time",
            title: "Date & Time Settings",
            keywords: "date time clock timezone automatic internet time calendar",
            icon: phosphor_svgs::style::regular::CLOCK,
            execution: ActionExecution::OpenUrl("ms-settings:dateandtime".into()),
        },
        SystemSettingDef {
            id: "software-update",
            title: "Windows Update Settings",
            keywords: "windows update software upgrade patch updates install restart",
            icon: phosphor_svgs::style::regular::ARROW_CLOCKWISE,
            execution: ActionExecution::OpenUrl("ms-settings:windowsupdate".into()),
        },
        SystemSettingDef {
            id: "privacy-security",
            title: "Privacy & Security Settings",
            keywords:
                "privacy security permissions camera microphone location windows defender antivirus",
            icon: phosphor_svgs::style::regular::SHIELD,
            execution: ActionExecution::OpenUrl("ms-settings:privacy".into()),
        },
    ]
}

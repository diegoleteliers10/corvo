use super::{ActionExecution, SystemActionDef, SystemSettingDef};

pub fn get_system_actions() -> Vec<SystemActionDef> {
    vec![
        SystemActionDef {
            id: "lock",
            title: "Lock Screen",
            keywords: "lock screen display security sleep",
            icon: phosphor_svgs::style::regular::LOCK,
            execution: ActionExecution::RunShell(
                "/System/Library/CoreServices/Menu Extras/User.menu/Contents/Resources/CGSession -suspend || pmset displaysleepnow".into(),
            ),
        },
        SystemActionDef {
            id: "sleep",
            title: "Sleep",
            keywords: "sleep suspend system power",
            icon: phosphor_svgs::style::regular::MOON,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to sleep'".into(),
            ),
        },
        SystemActionDef {
            id: "restart",
            title: "Restart",
            keywords: "restart reboot system",
            icon: phosphor_svgs::style::regular::ARROW_CLOCKWISE,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to restart'".into(),
            ),
        },
        SystemActionDef {
            id: "shutdown",
            title: "Shut Down",
            keywords: "shut down shutdown power off system",
            icon: phosphor_svgs::style::regular::POWER,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to shut down'".into(),
            ),
        },
        SystemActionDef {
            id: "logout",
            title: "Log Out",
            keywords: "log out logout sign out user session",
            icon: phosphor_svgs::style::regular::SIGN_OUT,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to log out'".into(),
            ),
        },
        SystemActionDef {
            id: "empty-trash",
            title: "Empty Trash",
            keywords: "empty trash bin delete cleanup",
            icon: phosphor_svgs::style::regular::TRASH,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"Finder\" to empty trash'".into(),
            ),
        },
        SystemActionDef {
            id: "toggle-appearance",
            title: "Toggle System Appearance",
            keywords: "toggle dark mode light mode appearance theme switch",
            icon: phosphor_svgs::style::regular::SUN,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode'".into(),
            ),
        },
        SystemActionDef {
            id: "toggle-mute",
            title: "Toggle Mute",
            keywords: "toggle mute audio sound volume silence",
            icon: phosphor_svgs::style::regular::SPEAKER_SLASH,
            execution: ActionExecution::RunShell(
                "osascript -e 'set volume output muted not (output muted of (get volume settings))'".into(),
            ),
        },
        SystemActionDef {
            id: "volume-up",
            title: "Volume Up",
            keywords: "volume up louder increase audio sound",
            icon: phosphor_svgs::style::regular::SPEAKER_HIGH,
            execution: ActionExecution::RunShell(
                "osascript -e 'set volume output volume ((output volume of (get volume settings)) + 10)'".into(),
            ),
        },
        SystemActionDef {
            id: "volume-down",
            title: "Volume Down",
            keywords: "volume down quieter decrease audio sound lower",
            icon: phosphor_svgs::style::regular::SPEAKER_LOW,
            execution: ActionExecution::RunShell(
                "osascript -e 'set volume output volume ((output volume of (get volume settings)) - 10)'".into(),
            ),
        },
        SystemActionDef {
            id: "brightness-up",
            title: "Increase Brightness",
            keywords: "brightness up increase display screen brighter monitor",
            icon: phosphor_svgs::style::regular::SUN,
            execution: ActionExecution::AdjustBrightness(0.0625),
        },
        SystemActionDef {
            id: "brightness-down",
            title: "Decrease Brightness",
            keywords: "brightness down decrease display screen dimmer monitor",
            icon: phosphor_svgs::style::regular::SUN_HORIZON,
            execution: ActionExecution::AdjustBrightness(-0.0625),
        },
        SystemActionDef {
            id: "hide-apps",
            title: "Hide All Applications",
            keywords: "hide all other applications apps desktop minimize",
            icon: phosphor_svgs::style::regular::EYE_SLASH,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to set visible of every process whose visible is true and name is not \"corvo\" to false'".into(),
            ),
        },
        SystemActionDef {
            id: "quit-all",
            title: "Quit All Applications",
            keywords: "quit all applications apps terminate close everything",
            icon: phosphor_svgs::style::regular::X_CIRCLE,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to set quit_list to (name of every process whose background only is false and name is not \"corvo\" and name is not \"Finder\")' -e 'repeat with a in quit_list' -e 'tell application a to quit' -e 'end repeat'".into(),
            ),
        },
    ]
}

pub fn get_system_settings() -> Vec<SystemSettingDef> {
    vec![
        SystemSettingDef {
            id: "displays",
            title: "Displays Settings",
            keywords: "displays monitor screen resolution brightness arrangement night shift true tone",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Displays-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "sound",
            title: "Sound Settings",
            keywords: "sound audio volume output input alert sound effects speakers microphone",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Sound-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "wifi",
            title: "Wi-Fi Settings",
            keywords: "wifi wireless network internet connections ssid router",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.wifi-settings-extension".into()),
        },
        SystemSettingDef {
            id: "bluetooth",
            title: "Bluetooth Settings",
            keywords: "bluetooth devices connect pair wireless airpods mouse keyboard headphones",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.BluetoothSettings".into()),
        },
        SystemSettingDef {
            id: "network",
            title: "Network Settings",
            keywords: "network ethernet vpn internet dns proxy firewall interface",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Network-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "appearance",
            title: "Appearance Settings",
            keywords: "appearance dark mode light mode accent highlight colors auto theme",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Appearance-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "wallpaper",
            title: "Wallpaper Settings",
            keywords: "wallpaper background desktop image pictures screensaver",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Wallpaper-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "lock-screen",
            title: "Lock Screen Settings",
            keywords: "lock screen display sleep turn off screen timeout password required",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Lock-Screen-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "notifications",
            title: "Notifications Settings",
            keywords: "notifications alerts badges banners sounds do not disturb focus",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Notifications-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "accessibility",
            title: "Accessibility Settings",
            keywords: "accessibility vision hearing motor zoom captions voiceover speech contrast spoken content",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Accessibility-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "control-center",
            title: "Control Center Settings",
            keywords: "control center menu bar status items clock battery sound wifi bluetooth airdrop",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.ControlCenter-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "desktop-dock",
            title: "Desktop & Dock Settings",
            keywords: "desktop dock stage manager mission control spaces hot corners recent apps",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Desktop-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "keyboard",
            title: "Keyboard Settings",
            keywords: "keyboard typing input sources shortcuts text dictation repeat rate illumination",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Keyboard-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "trackpad",
            title: "Trackpad Settings",
            keywords: "trackpad touch gestures tap click scroll natural zoom rotate gestures",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Trackpad-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "mouse",
            title: "Mouse Settings",
            keywords: "mouse tracking speed scrolling natural click primary button secondary",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Mouse-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "privacy-security",
            title: "Privacy & Security Settings",
            keywords: "privacy security permissions camera microphone location full disk filevault gatekeeper",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Privacy-Security-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "date-time",
            title: "Date & Time Settings",
            keywords: "date time clock timezone automatic 24 hour analog digital",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Date-Time-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "software-update",
            title: "Software Update Settings",
            keywords: "software update macos upgrade version patch updates install automatic",
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Software-Update-Settings.extension".into()),
        },
    ]
}

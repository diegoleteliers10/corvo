use super::{ActionExecution, SystemActionDef, SystemSettingDef};

pub fn get_system_actions() -> Vec<SystemActionDef> {
    vec![
        SystemActionDef {
            id: "dismiss-notifications",
            title: "Dismiss Notifications",
            keywords: "dismiss notifications close notification center alert banners clear",
            icon: phosphor_svgs::style::regular::BELL_SLASH,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to tell process \"NotificationCenter\" to click button 1 of every window' 2>/dev/null || true".into(),
            ),
        },
        SystemActionDef {
            id: "eject-all-disks",
            title: "Eject All Disks",
            keywords: "eject all disks unmount usb drives volumes removable",
            icon: phosphor_svgs::style::regular::EJECT,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"Finder\" to eject (every disk whose ejectable is true)'".into(),
            ),
        },
        SystemActionDef {
            id: "empty-trash",
            title: "Empty Trash",
            keywords: "empty trash bin delete cleanup purge",
            icon: phosphor_svgs::style::regular::TRASH,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"Finder\" to empty trash' || find \"${HOME:?HOME is not set}/.Trash\" -mindepth 1 -maxdepth 1 -exec rm -rf {} +".into(),
            ),
        },
        SystemActionDef {
            id: "hide-apps",
            title: "Hide All Apps Except Frontmost",
            keywords: "hide all other applications apps desktop minimize conceal focus",
            icon: phosphor_svgs::style::regular::EYE_SLASH,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to set visible of every process whose visible is true and frontmost is false and name is not \"corvo\" to false'".into(),
            ),
        },
        SystemActionDef {
            id: "lock",
            title: "Lock Screen",
            keywords: "lock screen display security sleep protect",
            icon: phosphor_svgs::style::regular::LOCK,
            execution: ActionExecution::RunShell(
                "/System/Library/CoreServices/Menu Extras/User.menu/Contents/Resources/CGSession -suspend || pmset displaysleepnow".into(),
            ),
        },
        SystemActionDef {
            id: "logout",
            title: "Log Out",
            keywords: "log out logout sign out user session exit",
            icon: phosphor_svgs::style::regular::SIGN_OUT,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to log out'".into(),
            ),
        },
        SystemActionDef {
            id: "next-track",
            title: "Next Track",
            keywords: "next track song forward music skip player audio",
            icon: phosphor_svgs::style::regular::SKIP_FORWARD,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"Music\" to next track' 2>/dev/null || osascript -e 'tell application \"Spotify\" to next track' 2>/dev/null || true".into(),
            ),
        },
        SystemActionDef {
            id: "open-trash",
            title: "Open Trash",
            keywords: "open trash bin folder finder deleted items",
            icon: phosphor_svgs::style::regular::TRASH,
            execution: ActionExecution::RunShell(
                "open ~/.Trash".into(),
            ),
        },
        SystemActionDef {
            id: "play-pause",
            title: "Play / Pause",
            keywords: "play pause music audio song player toggle",
            icon: phosphor_svgs::style::regular::PLAY_PAUSE,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"Music\" to playpause' 2>/dev/null || osascript -e 'tell application \"Spotify\" to playpause' 2>/dev/null || true".into(),
            ),
        },
        SystemActionDef {
            id: "previous-track",
            title: "Previous Track",
            keywords: "previous track song back music rewind player audio",
            icon: phosphor_svgs::style::regular::SKIP_BACK,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"Music\" to previous track' 2>/dev/null || osascript -e 'tell application \"Spotify\" to previous track' 2>/dev/null || true".into(),
            ),
        },
        SystemActionDef {
            id: "quit-all",
            title: "Quit All Applications",
            keywords: "quit all applications apps terminate close everything exit",
            icon: phosphor_svgs::style::regular::X_CIRCLE,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to set quit_list to (name of every process whose background only is false and name is not \"corvo\" and name is not \"Finder\")' -e 'repeat with a in quit_list' -e 'tell application a to quit' -e 'end repeat'".into(),
            ),
        },
        SystemActionDef {
            id: "restart",
            title: "Restart",
            keywords: "restart reboot system machine turn off on",
            icon: phosphor_svgs::style::regular::ARROW_CLOCKWISE,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to restart'".into(),
            ),
        },
        SystemActionDef {
            id: "shutdown",
            title: "Shut Down",
            keywords: "shut down shutdown power off system machine turn off",
            icon: phosphor_svgs::style::regular::POWER,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to shut down'".into(),
            ),
        },
        SystemActionDef {
            id: "sleep",
            title: "Sleep",
            keywords: "sleep suspend system power standby",
            icon: phosphor_svgs::style::regular::MOON,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to sleep'".into(),
            ),
        },
        SystemActionDef {
            id: "toggle-mute",
            title: "Toggle Mute",
            keywords: "toggle mute audio sound volume silence quiet",
            icon: phosphor_svgs::style::regular::SPEAKER_SLASH,
            execution: ActionExecution::RunShell(
                "osascript -e 'set volume output muted not (output muted of (get volume settings))'".into(),
            ),
        },
        SystemActionDef {
            id: "toggle-appearance",
            title: "Toggle System Appearance",
            keywords: "toggle system appearance dark mode light mode theme switch style",
            icon: phosphor_svgs::style::regular::SUN,
            execution: ActionExecution::RunShell(
                "osascript -e 'tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode'".into(),
            ),
        },
        SystemActionDef {
            id: "volume-down",
            title: "Volume Down",
            keywords: "volume down quieter decrease audio sound lower quiet",
            icon: phosphor_svgs::style::regular::SPEAKER_LOW,
            execution: ActionExecution::AdjustVolume(-0.05),
        },
        SystemActionDef {
            id: "volume-up",
            title: "Volume Up",
            keywords: "volume up louder increase audio sound louder raise",
            icon: phosphor_svgs::style::regular::SPEAKER_HIGH,
            execution: ActionExecution::AdjustVolume(0.05),
        },
        SystemActionDef {
            id: "brightness-down",
            title: "Decrease Brightness",
            keywords: "brightness down decrease display screen dimmer monitor",
            icon: phosphor_svgs::style::regular::SUN_HORIZON,
            execution: ActionExecution::AdjustBrightness(-0.05),
        },
        SystemActionDef {
            id: "brightness-up",
            title: "Increase Brightness",
            keywords: "brightness up increase display screen brighter monitor",
            icon: phosphor_svgs::style::regular::SUN,
            execution: ActionExecution::AdjustBrightness(0.05),
        },
    ]
}

pub fn get_system_settings() -> Vec<SystemSettingDef> {
    vec![
        SystemSettingDef {
            id: "about",
            title: "About",
            keywords: "about this mac system info specifications serial hardware model chip memory",
            icon: phosphor_svgs::style::regular::LAPTOP,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.SystemProfiler.AboutExtension".into()),
        },
        SystemSettingDef {
            id: "accessibility",
            title: "Accessibility",
            keywords: "accessibility vision hearing motor zoom captions voiceover speech contrast spoken content",
            icon: phosphor_svgs::style::regular::USER_CIRCLE,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Accessibility-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "airdrop-continuity",
            title: "AirDrop & Continuity",
            keywords: "airdrop continuity handoff airplay receiver universal control share nearby",
            icon: phosphor_svgs::style::regular::BROADCAST,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.AirDrop-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "appearance",
            title: "Appearance",
            keywords: "appearance dark mode light mode accent highlight colors auto theme",
            icon: phosphor_svgs::style::regular::MOON_STARS,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Appearance-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "apple-account",
            title: "Apple Account",
            keywords: "apple account apple id icloud sign in subscription media purchases family",
            icon: phosphor_svgs::style::regular::USER,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.systempreferences.AppleIDSettings".into()),
        },
        SystemSettingDef {
            id: "applecare-warranty",
            title: "AppleCare & Warranty",
            keywords: "applecare warranty coverage support service repairs hardware coverage",
            icon: phosphor_svgs::style::regular::SHIELD,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Coverage-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "background-security",
            title: "Background Security Improvements",
            keywords: "background security improvements system security rapid response patches",
            icon: phosphor_svgs::style::regular::SHIELD_CHECK,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Privacy-Security-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "battery",
            title: "Battery",
            keywords: "battery power energy saver low power mode health charge percentage",
            icon: phosphor_svgs::style::regular::BATTERY_FULL,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Battery-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "bluetooth",
            title: "Bluetooth",
            keywords: "bluetooth devices connect pair wireless airpods mouse keyboard headphones",
            icon: phosphor_svgs::style::regular::BLUETOOTH,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.BluetoothSettings".into()),
        },
        SystemSettingDef {
            id: "control-center",
            title: "Control Center",
            keywords: "control center menu bar status items clock battery sound wifi bluetooth airdrop",
            icon: phosphor_svgs::style::regular::SLIDERS,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.ControlCenter-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "date-time",
            title: "Date & Time",
            keywords: "date time clock timezone automatic 24 hour analog digital calendar",
            icon: phosphor_svgs::style::regular::CLOCK,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Date-Time-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "desktop-dock",
            title: "Desktop & Dock",
            keywords: "desktop dock stage manager mission control spaces hot corners recent apps",
            icon: phosphor_svgs::style::regular::DESKTOP,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Desktop-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "displays",
            title: "Displays",
            keywords: "displays monitor screen resolution brightness arrangement night shift true tone",
            icon: phosphor_svgs::style::regular::MONITOR,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Displays-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "keyboard",
            title: "Keyboard",
            keywords: "keyboard typing input sources shortcuts text dictation repeat rate illumination",
            icon: phosphor_svgs::style::regular::KEYBOARD,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Keyboard-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "lock-screen",
            title: "Lock Screen",
            keywords: "lock screen display sleep turn off screen timeout password required",
            icon: phosphor_svgs::style::regular::LOCK,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Lock-Screen-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "mouse",
            title: "Mouse",
            keywords: "mouse tracking speed scrolling natural click primary button secondary",
            icon: phosphor_svgs::style::regular::MOUSE,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Mouse-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "network",
            title: "Network",
            keywords: "network ethernet vpn internet dns proxy firewall interface",
            icon: phosphor_svgs::style::regular::GLOBE,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Network-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "notifications",
            title: "Notifications",
            keywords: "notifications alerts badges banners sounds do not disturb focus",
            icon: phosphor_svgs::style::regular::BELL,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Notifications-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "privacy-security",
            title: "Privacy & Security",
            keywords: "privacy security permissions camera microphone location full disk filevault gatekeeper",
            icon: phosphor_svgs::style::regular::SHIELD,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Privacy-Security-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "software-update",
            title: "Software Update",
            keywords: "software update macos upgrade version patch updates install automatic",
            icon: phosphor_svgs::style::regular::ARROW_CLOCKWISE,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Software-Update-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "sound",
            title: "Sound",
            keywords: "sound audio volume output input alert sound effects speakers microphone",
            icon: phosphor_svgs::style::regular::SPEAKER_HIGH,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Sound-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "trackpad",
            title: "Trackpad",
            keywords: "trackpad touch gestures tap click scroll natural zoom rotate gestures",
            icon: phosphor_svgs::style::regular::HAND_TAP,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Trackpad-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "wallpaper",
            title: "Wallpaper",
            keywords: "wallpaper background desktop image pictures screensaver",
            icon: phosphor_svgs::style::regular::IMAGE,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.Wallpaper-Settings.extension".into()),
        },
        SystemSettingDef {
            id: "wifi",
            title: "Wi-Fi",
            keywords: "wifi wireless network internet connections ssid router",
            icon: phosphor_svgs::style::regular::WIFI_HIGH,
            execution: ActionExecution::OpenUrl("x-apple.systempreferences:com.apple.wifi-settings-extension".into()),
        },
    ]
}

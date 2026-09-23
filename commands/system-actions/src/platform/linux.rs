use super::{ActionExecution, SystemActionDef, SystemSettingDef};

fn resolve_linux_desktop_setting(gnome_panel: &str, kde_kcm: &str) -> ActionExecution {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    if desktop.contains("kde") {
        ActionExecution::RunShell(format!("systemsettings {kde_kcm} || systemsettings5 {kde_kcm}"))
    } else {
        ActionExecution::RunShell(format!("gnome-control-center {gnome_panel}"))
    }
}

pub fn get_system_actions() -> Vec<SystemActionDef> {
    vec![
        SystemActionDef {
            id: "lock",
            title: "Lock Screen",
            keywords: "lock screen display security sleep session",
            icon: phosphor_svgs::style::regular::LOCK,
            execution: ActionExecution::RunShell(
                "loginctl lock-session || xdg-screensaver lock".into(),
            ),
        },
        SystemActionDef {
            id: "sleep",
            title: "Sleep",
            keywords: "sleep suspend system power",
            icon: phosphor_svgs::style::regular::MOON,
            execution: ActionExecution::RunShell(
                "systemctl suspend || loginctl suspend".into(),
            ),
        },
        SystemActionDef {
            id: "restart",
            title: "Restart",
            keywords: "restart reboot system",
            icon: phosphor_svgs::style::regular::ARROW_CLOCKWISE,
            execution: ActionExecution::RunShell(
                "systemctl reboot || loginctl reboot".into(),
            ),
        },
        SystemActionDef {
            id: "shutdown",
            title: "Shut Down",
            keywords: "shut down shutdown power off system",
            icon: phosphor_svgs::style::regular::POWER,
            execution: ActionExecution::RunShell(
                "systemctl poweroff || loginctl poweroff".into(),
            ),
        },
        SystemActionDef {
            id: "logout",
            title: "Log Out",
            keywords: "log out logout sign out user session",
            icon: phosphor_svgs::style::regular::SIGN_OUT,
            execution: ActionExecution::RunShell(
                "loginctl terminate-session ${XDG_SESSION_ID:-self} || gnome-session-quit --logout --no-prompt".into(),
            ),
        },
        SystemActionDef {
            id: "empty-trash",
            title: "Empty Trash",
            keywords: "empty trash bin delete cleanup",
            icon: phosphor_svgs::style::regular::TRASH,
            execution: ActionExecution::RunShell(
                "gio trash --empty || rm -rf ~/.local/share/Trash/*".into(),
            ),
        },
        SystemActionDef {
            id: "toggle-appearance",
            title: "Toggle System Appearance",
            keywords: "toggle dark mode light mode appearance theme switch",
            icon: phosphor_svgs::style::regular::SUN,
            execution: ActionExecution::RunShell(
                "sh -c 'if [ \"$(gsettings get org.gnome.desktop.interface color-scheme 2>/dev/null)\" = \"\\x27prefer-dark\\x27\" ]; then gsettings set org.gnome.desktop.interface color-scheme \"prefer-light\"; else gsettings set org.gnome.desktop.interface color-scheme \"prefer-dark\"; fi'".into(),
            ),
        },
        SystemActionDef {
            id: "toggle-mute",
            title: "Toggle Mute",
            keywords: "toggle mute audio sound volume silence",
            icon: phosphor_svgs::style::regular::SPEAKER_SLASH,
            execution: ActionExecution::RunShell(
                "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle 2>/dev/null || pactl set-sink-mute @DEFAULT_SINK@ toggle || amixer set Master toggle".into(),
            ),
        },
        SystemActionDef {
            id: "volume-up",
            title: "Volume Up",
            keywords: "volume up louder increase audio sound",
            icon: phosphor_svgs::style::regular::SPEAKER_HIGH,
            execution: ActionExecution::RunShell(
                "wpctl set-volume -l 1.5 @DEFAULT_AUDIO_SINK@ 5%+ 2>/dev/null || pactl set-sink-volume @DEFAULT_SINK@ +5% || amixer set Master 5%+".into(),
            ),
        },
        SystemActionDef {
            id: "volume-down",
            title: "Volume Down",
            keywords: "volume down quieter decrease audio sound lower",
            icon: phosphor_svgs::style::regular::SPEAKER_LOW,
            execution: ActionExecution::RunShell(
                "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- 2>/dev/null || pactl set-sink-volume @DEFAULT_SINK@ -5% || amixer set Master 5%-".into(),
            ),
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
            id: "hide-apps",
            title: "Hide All Applications",
            keywords: "hide all other applications apps desktop minimize show desktop",
            icon: phosphor_svgs::style::regular::EYE_SLASH,
            execution: ActionExecution::RunShell(
                "wmctrl -k on 2>/dev/null || xdotool key Super+d".into(),
            ),
        },
        SystemActionDef {
            id: "quit-all",
            title: "Quit All Applications",
            keywords: "quit all applications apps terminate close everything",
            icon: phosphor_svgs::style::regular::X_CIRCLE,
            execution: ActionExecution::RunShell(
                "sh -c 'wmctrl -l | while read -r id d c t; do [ \"$c\" != \"corvo\" ] && wmctrl -ic \"$id\"; done'".into(),
            ),
        },
    ]
}

pub fn get_system_settings() -> Vec<SystemSettingDef> {
    vec![
        SystemSettingDef {
            id: "displays",
            title: "Displays Settings",
            keywords: "displays monitor screen resolution refresh rate arrangement",
            execution: resolve_linux_desktop_setting("display", "kcm_kscreen"),
        },
        SystemSettingDef {
            id: "sound",
            title: "Sound Settings",
            keywords: "sound audio volume output input speakers microphone pipewire pulseaudio",
            execution: resolve_linux_desktop_setting("sound", "kcm_pulseaudio"),
        },
        SystemSettingDef {
            id: "wifi",
            title: "Wi-Fi Settings",
            keywords: "wifi wireless network internet connections ssid router",
            execution: resolve_linux_desktop_setting("wifi", "kcm_networkmanagement"),
        },
        SystemSettingDef {
            id: "network",
            title: "Network Settings",
            keywords: "network ethernet vpn internet dns proxy firewall interface",
            execution: resolve_linux_desktop_setting("network", "kcm_networkmanagement"),
        },
        SystemSettingDef {
            id: "bluetooth",
            title: "Bluetooth Settings",
            keywords: "bluetooth devices connect pair wireless airpods mouse headphones",
            execution: resolve_linux_desktop_setting("bluetooth", "kcm_bluetooth"),
        },
        SystemSettingDef {
            id: "appearance",
            title: "Appearance Settings",
            keywords: "appearance dark mode light mode accent colors theme style",
            execution: resolve_linux_desktop_setting("appearance", "kcm_lookandfeel"),
        },
        SystemSettingDef {
            id: "wallpaper",
            title: "Wallpaper Settings",
            keywords: "wallpaper background desktop image pictures screensaver theme",
            execution: resolve_linux_desktop_setting("background", "kcm_wallpaper"),
        },
        SystemSettingDef {
            id: "lock-screen",
            title: "Lock Screen Settings",
            keywords: "lock screen display sleep timeout password screenlocker",
            execution: resolve_linux_desktop_setting("screen-lock", "kcm_screenlocker"),
        },
        SystemSettingDef {
            id: "notifications",
            title: "Notifications Settings",
            keywords: "notifications alerts badges banners sounds do not disturb",
            execution: resolve_linux_desktop_setting("notifications", "kcm_notifications"),
        },
        SystemSettingDef {
            id: "accessibility",
            title: "Accessibility Settings",
            keywords: "accessibility vision hearing zoom captions high contrast universal access",
            execution: resolve_linux_desktop_setting("universal-access", "kcm_accessibility"),
        },
        SystemSettingDef {
            id: "control-center",
            title: "Control Center Settings",
            keywords: "control center system settings preferences configuration",
            execution: resolve_linux_desktop_setting("", ""),
        },
        SystemSettingDef {
            id: "desktop-dock",
            title: "Desktop & Dock Settings",
            keywords: "desktop dock panel taskbar dash",
            execution: resolve_linux_desktop_setting("ubuntu-panel", "kcm_kded"),
        },
        SystemSettingDef {
            id: "keyboard",
            title: "Keyboard Settings",
            keywords: "keyboard typing input layout shortcuts repeat rate",
            execution: resolve_linux_desktop_setting("keyboard", "kcm_keyboard"),
        },
        SystemSettingDef {
            id: "trackpad",
            title: "Touchpad Settings",
            keywords: "touchpad touch gestures tap click scroll natural zoom gestures",
            execution: resolve_linux_desktop_setting("mouse", "kcm_touchpad"),
        },
        SystemSettingDef {
            id: "mouse",
            title: "Mouse Settings",
            keywords: "mouse speed pointer primary button scrolling natural click",
            execution: resolve_linux_desktop_setting("mouse", "kcm_mouse"),
        },
        SystemSettingDef {
            id: "date-time",
            title: "Date & Time Settings",
            keywords: "date time clock timezone automatic ntp calendar 24 hour",
            execution: resolve_linux_desktop_setting("datetime", "kcm_clock"),
        },
        SystemSettingDef {
            id: "software-update",
            title: "Software Updates Settings",
            keywords: "software update package manager distro upgrade version patch",
            execution: resolve_linux_desktop_setting("software", "kcm_updates"),
        },
        SystemSettingDef {
            id: "privacy-security",
            title: "Privacy & Security Settings",
            keywords: "privacy security permissions camera microphone location diagnostics",
            execution: resolve_linux_desktop_setting("privacy", "kcm_privacy"),
        },
    ]
}

//! Global hotkey management. The `global-hotkey` crate supports macOS, Windows,
//! and Linux X11. Wayland users bind `corvo --toggle` in the compositor.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyIntent {
    ToggleLauncher,
    TileWindow(String),
    LaunchApp(std::path::PathBuf),
    RunSystemAction(String),
    OpenSystemSetting(String),
    OpenUrl(String),
    ClipboardHistory,
    EmojiPicker,
    Command(String),
}

static RELOAD_NOTIFY: OnceLock<smol::channel::Sender<()>> = OnceLock::new();

/// Sets the channel sender used to notify the UI runtime that hotkey bindings need reloading.
pub fn set_reload_sender(tx: smol::channel::Sender<()>) {
    let _ = RELOAD_NOTIFY.set(tx);
}

/// Triggers a reload of active global hotkeys. Can be called from any thread or handler.
pub fn notify_hotkeys_changed() {
    if let Some(tx) = RELOAD_NOTIFY.get() {
        let _ = tx.try_send(());
    }
}

/// Parses a human-readable or settings hotkey string into `(Modifiers, Code)`.
/// Supports combos like "cmd+space", "ctrl+alt+t", "opt+cmd+left", "ctrl+opt+enter", etc.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn parse_hotkey_string(hotkey: &str) -> Option<(Modifiers, Code)> {
    let mut mods = Modifiers::empty();
    let mut target_code: Option<Code> = None;

    let s = hotkey.trim();
    if s.is_empty() {
        return None;
    }

    // Support unicode symbols or plus-separated tokens
    let tokens: Vec<String> = if s.contains('+') {
        s.split('+').map(|t| t.trim().to_lowercase()).collect()
    } else {
        // Parse symbols like ⌥⌘←, ⌃⌥↵, etc.
        let mut list = Vec::new();
        let mut rest = s;
        while !rest.is_empty() {
            if rest.starts_with('⌃') {
                list.push("ctrl".into());
                rest = &rest['⌃'.len_utf8()..];
            } else if rest.starts_with('⌥') {
                list.push("alt".into());
                rest = &rest['⌥'.len_utf8()..];
            } else if rest.starts_with('⇧') {
                list.push("shift".into());
                rest = &rest['⇧'.len_utf8()..];
            } else if rest.starts_with('⌘') {
                list.push("cmd".into());
                rest = &rest['⌘'.len_utf8()..];
            } else {
                list.push(rest.to_lowercase());
                break;
            }
        }
        list
    };

    for token in tokens {
        match token.as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "opt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            // `cmd` is the primary modifier: Command on macOS, Control
            // elsewhere. `super`, `win`, and `meta` always mean the
            // physical Windows or Super key.
            "cmd" | "command" => {
                if cfg!(target_os = "macos") {
                    mods |= Modifiers::SUPER;
                } else {
                    mods |= Modifiers::CONTROL;
                }
            }
            "super" | "win" | "meta" => mods |= Modifiers::SUPER,

            // Navigation and editing
            "space" | " " => target_code = Some(Code::Space),
            "↵" | "enter" | "return" => target_code = Some(Code::Enter),
            "⌫" | "backspace" => target_code = Some(Code::Backspace),
            // Delete is a separate key from Backspace on Windows and
            // Linux. Collapsing the two made Ctrl+Delete fire the
            // delete-to-start-of-line binding.
            "delete" | "del" | "⌦" | "forwarddelete" => target_code = Some(Code::Delete),
            "⎋" | "esc" | "escape" => target_code = Some(Code::Escape),
            "⇥" | "tab" => target_code = Some(Code::Tab),
            "←" | "left" | "arrowleft" | "arrow_left" => target_code = Some(Code::ArrowLeft),
            "→" | "right" | "arrowright" | "arrow_right" => target_code = Some(Code::ArrowRight),
            "↑" | "up" | "arrowup" | "arrow_up" => target_code = Some(Code::ArrowUp),
            "↓" | "down" | "arrowdown" | "arrow_down" => target_code = Some(Code::ArrowDown),

            // Letters
            "a" => target_code = Some(Code::KeyA),
            "b" => target_code = Some(Code::KeyB),
            "c" => target_code = Some(Code::KeyC),
            "d" => target_code = Some(Code::KeyD),
            "e" => target_code = Some(Code::KeyE),
            "f" => target_code = Some(Code::KeyF),
            "g" => target_code = Some(Code::KeyG),
            "h" => target_code = Some(Code::KeyH),
            "i" => target_code = Some(Code::KeyI),
            "j" => target_code = Some(Code::KeyJ),
            "k" => target_code = Some(Code::KeyK),
            "l" => target_code = Some(Code::KeyL),
            "m" => target_code = Some(Code::KeyM),
            "n" => target_code = Some(Code::KeyN),
            "o" => target_code = Some(Code::KeyO),
            "p" => target_code = Some(Code::KeyP),
            "q" => target_code = Some(Code::KeyQ),
            "r" => target_code = Some(Code::KeyR),
            "s" => target_code = Some(Code::KeyS),
            "t" => target_code = Some(Code::KeyT),
            "u" => target_code = Some(Code::KeyU),
            "v" => target_code = Some(Code::KeyV),
            "w" => target_code = Some(Code::KeyW),
            "x" => target_code = Some(Code::KeyX),
            "y" => target_code = Some(Code::KeyY),
            "z" => target_code = Some(Code::KeyZ),

            // Digits
            "0" => target_code = Some(Code::Digit0),
            "1" => target_code = Some(Code::Digit1),
            "2" => target_code = Some(Code::Digit2),
            "3" => target_code = Some(Code::Digit3),
            "4" => target_code = Some(Code::Digit4),
            "5" => target_code = Some(Code::Digit5),
            "6" => target_code = Some(Code::Digit6),
            "7" => target_code = Some(Code::Digit7),
            "8" => target_code = Some(Code::Digit8),
            "9" => target_code = Some(Code::Digit9),

            // Function keys
            "f1" => target_code = Some(Code::F1),
            "f2" => target_code = Some(Code::F2),
            "f3" => target_code = Some(Code::F3),
            "f4" => target_code = Some(Code::F4),
            "f5" => target_code = Some(Code::F5),
            "f6" => target_code = Some(Code::F6),
            "f7" => target_code = Some(Code::F7),
            "f8" => target_code = Some(Code::F8),
            "f9" => target_code = Some(Code::F9),
            "f10" => target_code = Some(Code::F10),
            "f11" => target_code = Some(Code::F11),
            "f12" => target_code = Some(Code::F12),

            // Punctuation & symbols
            "-" | "minus" => target_code = Some(Code::Minus),
            "=" | "equal" => target_code = Some(Code::Equal),
            "[" | "bracketleft" => target_code = Some(Code::BracketLeft),
            "]" | "bracketright" => target_code = Some(Code::BracketRight),
            "\\" | "backslash" => target_code = Some(Code::Backslash),
            ";" | "semicolon" => target_code = Some(Code::Semicolon),
            "'" | "quote" => target_code = Some(Code::Quote),
            "," | "comma" => target_code = Some(Code::Comma),
            "." | "period" => target_code = Some(Code::Period),
            "/" | "slash" => target_code = Some(Code::Slash),
            "`" | "backquote" => target_code = Some(Code::Backquote),

            _ => {}
        }
    }

    let code = target_code?;
    Some((mods, code))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn parse_hotkey_string(_hotkey: &str) -> Option<()> {
    None
}

/// Hotkey manager handle that registers and dynamically reloads hotkeys.
pub struct HotkeyManager {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    manager: Option<GlobalHotKeyManager>,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    registered: Vec<HotKey>,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    id_map: Arc<Mutex<HashMap<u32, HotkeyIntent>>>,
    last_bindings: Option<(String, Vec<(String, HotkeyIntent)>)>,
    suppressed: bool,
}

impl HotkeyManager {
    pub fn new(tx: smol::channel::Sender<HotkeyIntent>) -> Self {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let manager = if cfg!(target_os = "linux")
                && (std::env::var_os("DISPLAY").is_none()
                    || std::env::var_os("WAYLAND_DISPLAY").is_some())
            {
                eprintln!(
                    "corvo: bind global hotkeys in the Wayland compositor with `corvo --toggle`"
                );
                None
            } else {
                match GlobalHotKeyManager::new() {
                    Ok(mgr) => Some(mgr),
                    Err(err) => {
                        crate::diagnostics::record_error("hotkey", "manager_init_failed");
                        eprintln!("corvo: cannot initialize GlobalHotKeyManager: {err}");
                        None
                    }
                }
            };

            let id_map = Arc::new(Mutex::new(HashMap::new()));
            let worker_map = id_map.clone();

            std::thread::spawn(move || {
                let receiver = GlobalHotKeyEvent::receiver();
                let mut pressed_keys: HashMap<u32, std::time::Instant> = HashMap::new();

                while let Ok(event) = receiver.recv() {
                    match event.state {
                        HotKeyState::Pressed => {
                            let now = std::time::Instant::now();
                            if let Some(&last_press) = pressed_keys.get(&event.id) {
                                if now.duration_since(last_press)
                                    < std::time::Duration::from_millis(50)
                                {
                                    continue;
                                }
                            }
                            pressed_keys.insert(event.id, now);

                            let intent = {
                                let guard = worker_map.lock().unwrap();
                                guard.get(&event.id).cloned()
                            };

                            if let Some(intent) = intent {
                                let _ = tx.try_send(intent);
                            }
                        }
                        HotKeyState::Released => {
                            pressed_keys.remove(&event.id);
                        }
                    }
                }
            });

            Self {
                manager,
                registered: Vec::new(),
                id_map,
                last_bindings: None,
                suppressed: false,
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = tx;
            Self {
                last_bindings: None,
                suppressed: false,
            }
        }
    }

    pub fn update_bindings(
        &mut self,
        launcher_hotkey_str: &str,
        bindings: Vec<(String, HotkeyIntent)>,
    ) {
        self.last_bindings = Some((launcher_hotkey_str.to_string(), bindings.clone()));

        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let Some(ref manager) = self.manager else {
                return;
            };

            // 1. Unregister all existing hotkeys
            if !self.registered.is_empty() {
                let _ = manager.unregister_all(&self.registered);
                self.registered.clear();
            }

            let mut new_id_map: HashMap<u32, HotkeyIntent> = HashMap::new();
            let mut new_registered: Vec<HotKey> = Vec::new();

            // 2. Register main launcher toggle hotkey
            if !launcher_hotkey_str.trim().is_empty() {
                if let Some((launch_mod, launch_code)) = parse_hotkey_string(launcher_hotkey_str) {
                    let launch_hk = HotKey::new(Some(launch_mod), launch_code);
                    if let Err(err) = manager.register(launch_hk) {
                        crate::diagnostics::record_error("hotkey", "launcher_registration_failed");
                        eprintln!("corvo: cannot register main launcher hotkey: {err}");
                    } else {
                        new_id_map.insert(launch_hk.id(), HotkeyIntent::ToggleLauncher);
                        new_registered.push(launch_hk);
                    }
                } else {
                    crate::diagnostics::record_error("hotkey", "launcher_parse_failed");
                    eprintln!("corvo: cannot parse main launcher hotkey: '{launcher_hotkey_str}'");
                }
            }

            // 3. Register custom bindings
            for (hotkey_str, intent) in bindings {
                if let Some((mods, code)) = parse_hotkey_string(&hotkey_str) {
                    let hk = HotKey::new(Some(mods), code);
                    // Avoid duplicate registrations of the same hotkey
                    if new_id_map.contains_key(&hk.id()) {
                        continue;
                    }
                    if let Err(err) = manager.register(hk) {
                        crate::diagnostics::record_error("hotkey", "binding_registration_failed");
                        eprintln!("corvo: cannot register hotkey '{hotkey_str}': {err}");
                    } else {
                        new_id_map.insert(hk.id(), intent);
                        new_registered.push(hk);
                    }
                }
            }

            // 4. Update state
            self.registered = new_registered;
            let mut map_guard = self.id_map.lock().unwrap();
            *map_guard = new_id_map;
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = (launcher_hotkey_str, bindings);
        }
    }

    /// While suppressed, every registered global hotkey is unregistered, so
    /// the OS delivers those keystrokes to the focused window instead of the
    /// hotkey worker. In-app hotkey recorders use this: rebinding the toggle
    /// key cannot fire the launcher mid-recording, and the recorder can
    /// capture even the currently bound combination. Unsuplicing re-registers
    /// the last known binding set.
    pub fn set_suppressed(&mut self, suppressed: bool) {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            if suppressed == self.suppressed {
                return;
            }
            self.suppressed = suppressed;
            let Some(ref manager) = self.manager else {
                return;
            };
            if suppressed {
                if !self.registered.is_empty() {
                    let _ = manager.unregister_all(&self.registered);
                    self.registered.clear();
                }
                if let Ok(mut id_map) = self.id_map.lock() {
                    id_map.clear();
                }
            } else if let Some((launcher, bindings)) = self.last_bindings.clone() {
                self.update_bindings(&launcher, bindings);
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = (self, suppressed);
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        if let Some(ref manager) = self.manager {
            if !self.registered.is_empty() {
                let _ = manager.unregister_all(&self.registered);
            }
        }
    }
}

pub type HotkeyGuard = HotkeyManager;

/// Backward-compatible setup function with initial bindings.
pub fn setup_with_bindings(
    launcher_hotkey_str: &str,
    bindings: Vec<(String, HotkeyIntent)>,
    tx: smol::channel::Sender<HotkeyIntent>,
) -> Option<HotkeyGuard> {
    let mut mgr = HotkeyManager::new(tx);
    mgr.update_bindings(launcher_hotkey_str, bindings);
    Some(mgr)
}

/// Fallback backward-compatible setup function.
pub fn setup(tx: smol::channel::Sender<()>) -> Option<HotkeyGuard> {
    let (intent_tx, intent_rx) = smol::channel::unbounded::<HotkeyIntent>();
    let guard = setup_with_bindings(
        &corvo_config::default_launcher_hotkey(),
        Vec::new(),
        intent_tx,
    )?;
    smol::spawn(async move {
        while let Ok(intent) = intent_rx.recv().await {
            if intent == HotkeyIntent::ToggleLauncher {
                let _ = tx.try_send(());
            }
        }
    })
    .detach();
    Some(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn parses_various_hotkey_formats() {
        let primary = if cfg!(target_os = "macos") {
            Modifiers::SUPER
        } else {
            Modifiers::CONTROL
        };
        let (mods, code) = parse_hotkey_string("cmd+space").expect("cmd+space");
        assert!(mods.contains(primary));
        assert_eq!(code, Code::Space);

        let (mods, code) = parse_hotkey_string("ctrl+alt+t").expect("ctrl+alt+t");
        assert!(mods.contains(Modifiers::CONTROL));
        assert!(mods.contains(Modifiers::ALT));
        assert_eq!(code, Code::KeyT);

        let (mods, code) = parse_hotkey_string("⌥⌘←").expect("unicode symbols");
        assert!(mods.contains(Modifiers::ALT));
        assert!(mods.contains(primary));
        assert_eq!(code, Code::ArrowLeft);

        let (mods, code) = parse_hotkey_string("f12").expect("f12");
        assert!(mods.is_empty());
        assert_eq!(code, Code::F12);
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn cmd_means_the_platform_primary_modifier() {
        // `cmd` is the primary modifier. Mapping it to SUPER everywhere
        // bound the Windows key on Windows and the Super key on Linux, so
        // none of the launcher's shortcuts fired.
        let (mods, _) = parse_hotkey_string("cmd+k").expect("cmd+k");
        if cfg!(target_os = "macos") {
            assert!(mods.contains(Modifiers::SUPER), "macOS uses Command");
        } else {
            assert!(
                mods.contains(Modifiers::CONTROL),
                "Windows and Linux use Control"
            );
        }
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn super_is_always_the_physical_key() {
        let (mods, _) = parse_hotkey_string("super+space").expect("super+space");
        assert!(mods.contains(Modifiers::SUPER));
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn delete_is_not_backspace() {
        // Delete and Backspace are different keys on Windows and Linux.
        // Collapsing them made Ctrl+Delete run delete-to-start-of-line.
        let (_, code) = parse_hotkey_string("ctrl+delete").expect("ctrl+delete");
        assert_eq!(code, Code::Delete);
        let (_, code) = parse_hotkey_string("ctrl+backspace").expect("ctrl+backspace");
        assert_eq!(code, Code::Backspace);
    }
}

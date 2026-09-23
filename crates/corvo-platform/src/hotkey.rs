//! Global hotkey. macOS and Windows use the `global-hotkey` crate. Linux
//! has no app-level global shortcut on Wayland, so users bind
//! `corvo --toggle` in the compositor instead (SPEC §6).

use global_hotkey::{GlobalHotKeyEvent, HotKeyState};

pub struct HotkeyGuard {
    _manager: global_hotkey::GlobalHotKeyManager,
}

/// Cmd+Space on macOS, Alt+Space on Windows. Windows keeps Alt because
/// the OS reserves Win+Space for input-language switching. The combo is
/// configurable in `settings.toml` when that wiring lands in phase 3.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn setup(tx: smol::channel::Sender<()>) -> Option<HotkeyGuard> {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::GlobalHotKeyManager;

    #[cfg(target_os = "macos")]
    const MODIFIER: Modifiers = Modifiers::SUPER;
    #[cfg(target_os = "windows")]
    const MODIFIER: Modifiers = Modifiers::ALT;

    let Ok(manager) = GlobalHotKeyManager::new() else {
        eprintln!("corvo: hotkey manager unavailable");
        return None;
    };
    let hotkey = HotKey::new(Some(MODIFIER), Code::Space);
    if let Err(err) = manager.register(hotkey) {
        eprintln!("corvo: cannot register hotkey: {err}");
        return None;
    }
    std::thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        let mut is_pressed = false;
        let mut last_toggle = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(1))
            .unwrap_or_else(std::time::Instant::now);
        while let Ok(event) = receiver.recv() {
            match event.state {
                HotKeyState::Pressed => {
                    if is_pressed {
                        // OS auto-repeat while key is held down; ignore.
                        continue;
                    }
                    let now = std::time::Instant::now();
                    if now.duration_since(last_toggle) < std::time::Duration::from_millis(50) {
                        continue;
                    }
                    is_pressed = true;
                    last_toggle = now;
                    let _ = tx.try_send(());
                }
                HotKeyState::Released => {
                    is_pressed = false;
                }
            }
        }
    });
    Some(HotkeyGuard { _manager: manager })
}

#[cfg(target_os = "linux")]
pub fn setup(_tx: smol::channel::Sender<()>) -> Option<HotkeyGuard> {
    None
}

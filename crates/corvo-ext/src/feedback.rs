//! The Feedback module: how an extension talks back.
//!
//! One rule covers most cases: return an [`corvo_core::Action`] from
//! `execute`, and let the UI render it. `Action::ShowToast` is the
//! inline confirmation; for work that finishes when no launcher
//! window is in flight (a timer, a download), call
//! `corvo_platform::notify(title, body)` — AppleScript on macOS, a
//! toast via PowerShell on Windows, `notify-send` on Linux. That call
//! is blocking: keep it off the search path.

use corvo_core::Action;

/// An inline confirmation toast. Use it for transport-class feedback
/// ("Media control sent", "Copied") that happens while the launcher
/// window is up. The text is what the user reads — write a sentence,
/// not a debug string.
pub fn toast(message: impl Into<String>) -> Action {
    Action::ShowToast(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_wraps_show_toast() {
        assert_eq!(toast("Saved"), Action::ShowToast("Saved".into()));
    }
}

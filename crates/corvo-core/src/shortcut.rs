//! Platform-aware keyboard shortcuts.
//!
//! One codebase ships for macOS, Windows and Linux, and the three
//! platforms disagree on which key acts as the primary modifier. macOS
//! uses Command, Windows and Linux use Control. A shortcut written as
//! `cmd+k` therefore has to become Ctrl+K on Windows and Linux, both in
//! the key handler and in the label the user reads.
//!
//! [`Primary`] resolves the modifier for the running platform and
//! [`keycaps`] renders a shortcut as the keycaps to draw. Command crates
//! keep writing one string per shortcut and let this module translate.

/// The modifier a platform treats as primary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Primary {
    /// Command on macOS, Control on Windows and Linux.
    Command,
    /// Control everywhere.
    Control,
}

impl Primary {
    /// The primary modifier for the running platform.
    ///
    /// Use this instead of reading `modifiers.platform` directly:
    /// `platform` is the Windows key on Windows and the Super key on
    /// Linux, so a shortcut bound to it never fires.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Command
        } else {
            Self::Control
        }
    }

    /// True on macOS, where the primary modifier is Command.
    pub const fn is_command() -> bool {
        matches!(Self::current(), Self::Command)
    }

    /// The name used in stored hotkey strings, for example `cmd+space`.
    pub const fn token(&self) -> &'static str {
        match self {
            Self::Command => "cmd",
            Self::Control => "ctrl",
        }
    }

    /// The label drawn in a keycap on this platform.
    pub const fn keycap_label(&self) -> &'static str {
        match self {
            Self::Command => "⌘",
            Self::Control => "Ctrl",
        }
    }

    /// The glyph modifier that means the same key as the primary
    /// modifier. Command crates write shortcuts with this glyph.
    pub const fn glyph(&self) -> char {
        match self {
            Self::Command => '⌘',
            Self::Control => '⌃',
        }
    }
}

/// The modifier a shortcut means by its first token.
pub fn primary_from_token(token: &str) -> Option<Primary> {
    match token.trim().to_ascii_lowercase().as_str() {
        "cmd" | "command" | "super" | "win" | "meta" => Some(Primary::Command),
        "ctrl" | "control" => Some(Primary::Control),
        _ => None,
    }
}

/// Splits a shortcut string into its tokens.
///
/// Accepts both the `cmd+space` form and the `⌘Space` glyph form, so a
/// value written by hand and one written by the settings recorder both
/// parse.
pub fn tokens(shortcut: &str) -> Vec<String> {
    if shortcut.contains('+') {
        return shortcut
            .split('+')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
    }
    let mut tokens = Vec::new();
    let mut rest = shortcut.trim();
    while !rest.is_empty() {
        let glyph = rest.chars().next().expect("rest is not empty");
        if !is_modifier_glyph(glyph) {
            break;
        }
        tokens.push(modifier_glyph_token(glyph).to_string());
        rest = &rest[glyph.len_utf8()..];
    }
    if !rest.is_empty() {
        tokens.push(rest.to_string());
    }
    tokens
}

fn is_modifier_glyph(glyph: char) -> bool {
    matches!(glyph, '⌘' | '⌃' | '⌥' | '⇧' | '⌫' | '␣' | '+')
}

fn modifier_glyph_token(glyph: char) -> &'static str {
    match glyph {
        '⌘' => "cmd",
        '⌃' => "ctrl",
        '⌥' => "alt",
        '⇧' => "shift",
        '⌫' => "backspace",
        '␣' => "space",
        '+' => "+",
        _ => "unknown",
    }
}

/// The label for one keycap on this platform.
///
/// macOS uses glyphs, Windows and Linux use words. Windows and Linux
/// users read `Ctrl`, not the control glyph, which is Mac typography.
pub fn keycap_label(token: &str) -> String {
    let token = token.trim();
    if token.is_empty() {
        return String::new();
    }
    // An arrow glyph names the arrow key on every platform. Normalize it
    // to the token the map below already handles.
    let token = match token {
        "←" => "left",
        "→" => "right",
        "↑" => "up",
        "↓" => "down",
        "↵" => "enter",
        "⌫" => "backspace",
        "⌦" => "delete",
        "⌘" => "cmd",
        "⌃" => "ctrl",
        "⌥" => "alt",
        "⇧" => "shift",
        "⎋" => "escape",
        "⇥" => "tab",
        other => other,
    };
    // A single character is a literal key: a letter, a digit, or a
    // punctuation key such as a comma.
    if token.chars().count() == 1 {
        return token.to_uppercase().to_string();
    }
    match token.to_ascii_lowercase().as_str() {
        // `cmd` is the primary modifier, so it follows the platform. A
        // literal `super`, `win`, or `meta` is the physical Windows or
        // Super key on every platform.
        "cmd" | "command" => {
            if Primary::is_command() {
                "⌘".to_string()
            } else {
                "Ctrl".to_string()
            }
        }
        "super" | "win" | "meta" => {
            if Primary::is_command() {
                "⌘".to_string()
            } else {
                "Win".to_string()
            }
        }
        // The control key is the Control key on every platform. Only its
        // label changes: a glyph on macOS, a word elsewhere.
        "ctrl" | "control" => {
            if Primary::is_command() {
                "⌃".to_string()
            } else {
                "Ctrl".to_string()
            }
        }
        "alt" | "opt" | "option" => {
            if Primary::is_command() {
                "⌥".to_string()
            } else {
                "Alt".to_string()
            }
        }
        "shift" => {
            if Primary::is_command() {
                "⇧".to_string()
            } else {
                "Shift".to_string()
            }
        }
        "space" => "Space".to_string(),
        "enter" | "return" => "↵".to_string(),
        "backspace" => "⌫".to_string(),
        "delete" | "del" => "⌦".to_string(),
        "escape" | "esc" => "Esc".to_string(),
        "tab" => "⇥".to_string(),
        "up" => {
            if Primary::is_command() {
                "↑".to_string()
            } else {
                "Up".to_string()
            }
        }
        "down" => {
            if Primary::is_command() {
                "↓".to_string()
            } else {
                "Down".to_string()
            }
        }
        "left" => {
            if Primary::is_command() {
                "←".to_string()
            } else {
                "Left".to_string()
            }
        }
        "right" => {
            if Primary::is_command() {
                "→".to_string()
            } else {
                "Right".to_string()
            }
        }
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// The keycaps to draw for a shortcut, in press order.
///
/// One entry per key, so a caller renders one keycap per element instead
/// of splitting the string by character.
pub fn keycaps(shortcut: &str) -> Vec<String> {
    let mut labels: Vec<String> = tokens(shortcut)
        .iter()
        .map(|token| keycap_label(token))
        .filter(|label| !label.is_empty())
        .collect();
    // Off macOS the primary modifier and Control are the same key, so a
    // shortcut written as `cmd+ctrl+k` would draw `Ctrl` twice. Keep the
    // first and drop the repeat.
    if !Primary::is_command() {
        let mut seen_primary = false;
        labels.retain(|label| {
            if label != "Ctrl" {
                return true;
            }
            if seen_primary {
                return false;
            }
            seen_primary = true;
            true
        });
    }
    labels
}

/// A shortcut with its tokens translated for the running platform, for
/// example `cmd+shift+v` becomes `ctrl+shift+v` on Windows and Linux.
///
/// Use this before storing a shortcut, so a settings file written on one
/// platform still binds the same physical keys on another.
pub fn normalize(shortcut: &str) -> String {
    if Primary::is_command() {
        return shortcut.trim().to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    for token in tokens(shortcut) {
        let lower = token.to_ascii_lowercase();
        // `cmd` means the primary modifier, which is Control off macOS.
        // A literal `super` stays the physical Super key.
        let part = match lower.as_str() {
            "cmd" | "command" | "ctrl" | "control" => Primary::current().token().to_string(),
            _ => lower,
        };
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_is_command_only_on_macos() {
        assert_eq!(Primary::is_command(), cfg!(target_os = "macos"));
    }

    #[test]
    fn parses_plus_form() {
        assert_eq!(tokens("cmd+shift+v"), ["cmd", "shift", "v"]);
        assert_eq!(tokens("ctrl+alt+t"), ["ctrl", "alt", "t"]);
        assert_eq!(tokens("alt+space"), ["alt", "space"]);
    }

    #[test]
    fn parses_glyph_form() {
        assert_eq!(tokens("⌥⌘←"), ["alt", "cmd", "←"]);
        assert_eq!(tokens("⌃⌥K"), ["ctrl", "alt", "K"]);
        assert_eq!(tokens("⇧⌘F"), ["shift", "cmd", "F"]);
        assert_eq!(tokens("⌘↵"), ["cmd", "↵"]);
    }

    #[test]
    fn tokens_ignore_case_and_spacing() {
        assert_eq!(tokens("CMD + Shift + V"), ["cmd", "shift", "v"]);
    }

    #[test]
    fn every_keycap_is_one_label() {
        // A renderer that splits by character needs one entry per key.
        for shortcut in ["cmd+shift+v", "⌃⌥K", "alt+left", "⌘↵"] {
            let caps = keycaps(shortcut);
            assert!(!caps.is_empty(), "{shortcut} produced no keycaps");
            for cap in &caps {
                assert!(!cap.is_empty(), "{shortcut} produced an empty keycap");
            }
        }
    }

    #[test]
    fn normalize_collapses_secondary_onto_the_primary() {
        // On Windows and Linux, Command means Control, so `cmd+shift+v`
        // must not stay as a modifier the platform cannot produce.
        let normalized = normalize("cmd+shift+v");
        if cfg!(target_os = "macos") {
            assert_eq!(normalized, "cmd+shift+v");
        } else {
            assert_eq!(normalized, "ctrl+shift+v");
        }
    }

    #[test]
    fn normalize_keeps_ordinary_keys() {
        assert_eq!(normalize("alt+space"), "alt+space");
        assert_eq!(normalize("f12"), "f12");
    }

    #[test]
    fn keycap_labels_use_the_platform_convention() {
        let command_cap = keycap_label("cmd");
        if cfg!(target_os = "macos") {
            assert_eq!(command_cap, "⌘");
        } else {
            assert_eq!(command_cap, "Ctrl");
        }
    }

    #[test]
    fn punctuation_keys_survive() {
        assert_eq!(keycaps("cmd+,"), vec!["⌘".to_string(), ",".to_string()]);
    }

    #[test]
    fn named_and_glyph_keys_render_alike() {
        // The command crates write tokens, not glyphs, but a hand-edited
        // settings file or an older label can still hold a glyph. Both
        // forms must reach the same keycaps.
        for (named, glyph) in [
            ("enter", "↵"),
            ("backspace", "⌫"),
            ("delete", "⌦"),
            ("left", "←"),
            ("right", "→"),
            ("cmd", "⌘"),
        ] {
            assert_eq!(
                keycap_label(named),
                keycap_label(glyph),
                "{named} and {glyph} must render alike"
            );
        }
    }

    #[test]
    fn enter_renders_as_the_return_glyph() {
        // The keycap renderer special-cases this glyph, so the token form
        // has to produce it.
        assert_eq!(keycap_label("enter"), "↵");
    }

    #[test]
    fn a_repeated_primary_modifier_draws_one_keycap() {
        // Off macOS, `cmd` and `ctrl` are the same key.
        let caps = keycaps("cmd+ctrl+k");
        if cfg!(target_os = "macos") {
            assert_eq!(caps, vec!["⌘", "⌃", "K"]);
        } else {
            assert_eq!(caps, vec!["Ctrl", "K"]);
        }
    }

    #[test]
    fn the_physical_super_key_keeps_its_own_label() {
        // `super` is the Windows key on every platform. It must not be
        // folded into the primary modifier, which is Control off macOS.
        let caps = keycaps("super+space");
        if cfg!(target_os = "macos") {
            assert_eq!(caps, vec!["⌘", "Space"]);
        } else {
            assert_eq!(caps, vec!["Win", "Space"]);
        }
    }

    #[test]
    fn windows_labels_read_as_words() {
        // The whole point of the module: a Windows or Linux user must not
        // be shown Mac typography for a modifier.
        for (token, expected) in [
            ("cmd", "Ctrl"),
            ("ctrl", "Ctrl"),
            ("alt", "Alt"),
            ("shift", "Shift"),
            ("left", "Left"),
            ("right", "Right"),
            ("up", "Up"),
            ("down", "Down"),
        ] {
            if cfg!(target_os = "macos") {
                continue;
            }
            assert_eq!(keycap_label(token), expected, "token {token}");
        }
    }
}

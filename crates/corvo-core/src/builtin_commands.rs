//! The built-in launcher commands that Settings lists in the Commands pane.
//!
//! These live here rather than in the settings UI because two places need
//! them and they have to agree: the Settings pane renders a row per entry,
//! and a command crate emits a search result per entry. When the lists
//! drifted, eleven rows had a Settings entry, a hotkey and an alias, but no
//! search result at all, so typing the alias found nothing.

use phosphor_svgs;

/// One entry in the Commands pane.
pub struct BuiltinCommandDef {
    /// Stable key for this command's settings entry. Also the id the
    /// Commands pane writes, so an alias resolves without a title lookup.
    pub id: &'static str,
    /// The search result id the launcher emits. The launcher resolves a
    /// result to its command by taking the text before the first colon, so
    /// this must start with the command that owns it: `builtin-commands:`
    /// for the entries here, or another command's prefix for the ones that
    /// command emits itself (Homebrew emits `brew:open-page` from the brew
    /// command).
    pub result_id: &'static str,
    pub title: &'static str,
    /// Extra text the fuzzy matcher searches, beyond the title.
    pub keywords: &'static str,
    pub icon: &'static str,
}

/// An entry the builtin-commands command owns: the launcher resolves it
/// back there.
macro_rules! owned {
    ($id:expr, $title:expr, $keywords:expr, $icon:expr) => {
        BuiltinCommandDef {
            id: $id,
            result_id: concat!("builtin-commands:", $id),
            title: $title,
            keywords: $keywords,
            icon: $icon,
        }
    };
}

/// An entry another command owns. That command already emits a result for
/// it, so the builtin-commands command must not emit a duplicate. Listed
/// here because the Commands pane still renders a row, an alias, and a
/// hotkey for it.
macro_rules! owned_elsewhere {
    ($id:expr, $result_id:expr, $title:expr, $keywords:expr, $icon:expr) => {
        BuiltinCommandDef {
            id: $id,
            result_id: $result_id,
            title: $title,
            keywords: $keywords,
            icon: $icon,
        }
    };
}

/// Every command Settings shows in its Commands pane, in display order.
pub const BUILTIN_COMMANDS: &[BuiltinCommandDef] = &[
    owned!(
        "about-corvo",
        "About Corvo",
        "about version info credits",
        phosphor_svgs::style::regular::INFO
    ),
    owned_elsewhere!(
        "brew:open-page",
        "brew:open-page",
        "Homebrew",
        "homebrew brew packages casks install update",
        phosphor_svgs::style::regular::DOWNLOAD_SIMPLE
    ),
    owned_elsewhere!(
        "countdown:open",
        "countdown:open",
        "Countdown",
        "countdown timer alarm stopwatch",
        phosphor_svgs::style::regular::CLOCK
    ),
    owned_elsewhere!(
        "media:open",
        "media:open",
        "Media Control",
        "media music audio play pause next previous volume",
        phosphor_svgs::style::regular::MUSIC_NOTES
    ),
    owned_elsewhere!(
        "notes:open",
        "notes:open",
        "Notes",
        "notes note memo write scratch",
        phosphor_svgs::style::regular::NOTEBOOK
    ),
    owned_elsewhere!(
        "pomodoro:status",
        "pomodoro:status",
        "Pomodoro",
        "pomodoro timer focus break pom",
        phosphor_svgs::style::regular::TIMER
    ),
    owned_elsewhere!(
        "text-utilities:open-page",
        "text-utilities:open-page",
        "Text Utilities",
        "text case upper lower title transform",
        phosphor_svgs::style::regular::TEXT_AA
    ),
    owned_elsewhere!(
        "weather:current",
        "weather:current",
        "Weather",
        "weather forecast temperature rain",
        phosphor_svgs::style::regular::CLOUD_SUN
    ),
    owned!(
        "calculator-history",
        "Calculator History",
        "calculator history past calculations math",
        phosphor_svgs::style::regular::PLUS_MINUS
    ),
    owned!(
        "check-for-updates",
        "Check for Updates",
        "update upgrade version release download",
        phosphor_svgs::style::regular::ARROW_CIRCLE_DOWN
    ),
    owned!(
        "define-word",
        "Define Word",
        "define dictionary word meaning lookup",
        phosphor_svgs::style::regular::BOOK_OPEN
    ),
    owned!(
        "export-backup",
        "Export Backup",
        "backup export settings archive save",
        phosphor_svgs::style::regular::EXPORT
    ),
    owned!(
        "import-backup",
        "Import Backup",
        "backup import settings archive restore",
        phosphor_svgs::style::regular::DOWNLOAD_SIMPLE
    ),
    owned!(
        "import-from-raycast",
        "Import from Raycast",
        "raycast import migrate commands settings",
        phosphor_svgs::style::regular::FILE_ARROW_DOWN
    ),
    owned!(
        "open-camera",
        "Open Camera",
        "camera photo webcam capture",
        phosphor_svgs::style::regular::CAMERA
    ),
    owned!(
        "quit-corvo",
        "Quit Corvo",
        "quit exit close stop launcher",
        phosphor_svgs::style::regular::POWER
    ),
    owned!(
        "reload-applications",
        "Reload Applications",
        "reload refresh applications apps index rescan",
        phosphor_svgs::style::regular::ARROWS_CLOCKWISE
    ),
    owned!(
        "open-settings",
        "Open Settings",
        "settings preferences options configure",
        phosphor_svgs::style::regular::GEAR
    ),
    owned!(
        "toggle-system-appearance",
        "Toggle System Appearance",
        "dark mode light appearance theme toggle",
        phosphor_svgs::style::regular::SUN
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The launcher resolves a result to its command from the text before
    /// the first colon. An entry here must either be emitted by this crate
    /// under `builtin-commands:` or name the section that does emit it.
    #[test]
    fn every_entry_names_the_command_that_emits_it() {
        for def in BUILTIN_COMMANDS {
            let (section, _) = def
                .result_id
                .split_once(':')
                .expect("every result id carries a section prefix");
            if section == "builtin-commands" {
                assert_eq!(
                    def.result_id,
                    format!("builtin-commands:{}", def.id),
                    "entry {} emits under a different id than its settings key",
                    def.id
                );
            } else {
                // An extension entry's settings key IS its result id: the
                // full id is what resolves it, because its split suffix
                // names no settings section.
                assert_eq!(
                    def.result_id, def.id,
                    "entry {} in section {section} must key settings by its full id",
                    def.id
                );
            }
        }
    }

    /// Two commands emitting one row would show it twice.
    #[test]
    fn result_ids_are_unique() {
        let mut ids: Vec<&str> = BUILTIN_COMMANDS.iter().map(|d| d.result_id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BUILTIN_COMMANDS.len(), "duplicate result id");
    }
}

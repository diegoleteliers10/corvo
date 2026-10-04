//! Command contract and registry for corvo.
//!
//! Every launcher feature implements [`Command`]. Command crates depend on
//! this crate only, plus `corvo-platform` when they need OS actions, and
//! register themselves with [`register_command!`] at link time. The binary
//! keeps no central command list.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// Upper bound handed to commands so no command can flood the result list.
pub const DEFAULT_MAX_RESULTS: usize = 20;

pub mod page;
pub mod search;
pub mod shortcut;
pub use page::{
    ArgumentKind, ArgumentSpec, Badge, Block, Blocks, CommandMode, CommandSpec, ExtensionManifest,
    FormField, GridContent, GridItem, Hero, Metadata, PageButton, PageFilter, PageView, Progress,
    Refresh, StripCard, Style, Tone,
};
pub use search::search_match_score;
pub use shortcut::Primary;

pub use phosphor_svgs;

/// Glyph kinds the UI knows how to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Icon {
    App,
    File,
    Clipboard,
    Calculator,
    Snippet,
    Window,
    Emoji,
    Link,
    System,
    Web,
    /// Raw static SVG string (such as from `phosphor_svgs::style::regular::*`).
    Svg(&'static str),
    /// Direct unicode glyph, for emoji items in emoji picker.
    Glyph(&'static str),
    /// A PNG on disk, for example an extracted application icon.
    Image(std::path::PathBuf),
}

/// One row in the result list.
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Icon,
    pub score: i32,
    /// Right-aligned hint, for example a hotkey or a unit.
    pub accessory: Option<String>,
    /// Further right-aligned facts, each with an optional tooltip.
    /// Rendered after `accessory`; keep each one short — a number, a
    /// state, a unit.
    pub accessories: Vec<SearchAccessory>,
    /// Group label for list sections. Consecutive rows sharing a
    /// section render under one header; rows without one land in the
    /// default group.
    pub section: Option<String>,
}

/// One right-aligned fact on a result row.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchAccessory {
    pub text: String,
    /// Shown on hover; explain the number, not repeat it.
    pub tooltip: Option<String>,
}

impl SearchAccessory {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tooltip: None,
        }
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// A system operation performed through the platform's own API.
///
/// Each variant maps to one documented call on at least one platform. A
/// platform with no implementation returns `Unsupported` rather than
/// silently doing nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAction {
    /// Lock the workstation.
    LockWorkstation,
    /// Suspend to memory.
    Suspend,
    /// Empty every Recycle Bin on every drive.
    EmptyRecycleBin,
    /// Open the Recycle Bin window.
    OpenRecycleBin,
    /// Minimize every window, showing the desktop.
    ShowDesktop,
    /// Ask every other application to close.
    QuitAllApplications,
    /// Switch the system between the light and dark theme.
    ToggleDarkMode,
    /// Mute or unmute the default output device.
    ToggleMute,
    /// Skip to the next track in the active media player.
    MediaNextTrack,
    /// Go back to the previous track.
    MediaPreviousTrack,
    /// Play or pause the active media player.
    MediaPlayPause,
    /// Safely eject every removable drive.
    EjectRemovableDisks,
}

/// What the launcher does after the user picks a result.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Open(PathBuf),
    OpenAppUninstaller {
        name: String,
        path: PathBuf,
    },
    OpenFileSearch,
    SetResultFavorite {
        result_id: String,
        title: String,
        favorite: bool,
    },
    SetResultHidden {
        result_id: String,
        title: String,
        hidden: bool,
    },
    OpenUrl(String),
    Copy(String),
    PasteText(String),
    CopyImage(PathBuf),
    PasteImage(PathBuf),
    RunShell(String),
    /// A system operation the platform performs through its own API.
    ///
    /// Commands use this instead of a shell string when the platform has a
    /// real API for the job. A shell one-liner has to survive the quoting
    /// rules of whatever runs it, and the Windows one-liners did not: every
    /// embedded quote was mangled before PowerShell saw it.
    RunNative(NativeAction),
    RunProcess {
        program: String,
        args: Vec<String>,
        title: String,
    },
    ConfirmProcessTermination {
        pid: u32,
        start_time: u64,
    },
    TerminateProcess {
        pid: u32,
        start_time: u64,
        force: bool,
    },
    ShowToast(String),
    CloseWindow,
    TileWindow(String),
    AdjustBrightness(f32),
    AdjustVolume(f32),
}

/// One entry of a result's actions menu, the surface the primary modifier
/// plus K opens. `action` runs as if the command had returned it from
/// `execute`.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandAction {
    pub id: String,
    pub label: String,
    pub action: Action,
    pub icon: Icon,
    /// Menu section the action belongs to; sections render apart with a
    /// divider between them.
    pub group: ActionGroup,
    /// Shortcut that triggers the action, written with `shortcut` tokens,
    /// for example "enter" or "cmd+enter". `cmd` means the platform's
    /// primary modifier, so the same value renders as Command+Enter on
    /// macOS and Ctrl+Enter on Windows and Linux. Pass `None` to draw no
    /// hint.
    pub hotkey: Option<&'static str>,
}

/// The section of the actions menu an action renders in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionGroup {
    /// The action `execute` runs; sits first with the ↵ hint.
    Primary,
    /// Extra actions below the primary section.
    Standard,
    /// Removal-class actions; the UI renders them apart, in red.
    Destructive,
}

/// Error surface for command execution. Commands report failure as a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The command exists but does not work on this platform yet.
    Unsupported,
    /// The result id is unknown or stale.
    NotFound,
    /// The OS rejected the action.
    Platform(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => write!(f, "unsupported on this platform"),
            Self::NotFound => write!(f, "unknown result"),
            Self::Platform(msg) => write!(f, "platform error: {msg}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Runtime configuration and data shared by the config layer and commands.
/// Defined here so command crates do not depend on `corvo-config` directly.
pub trait DataStore: Send + Sync {
    fn snippets(&self) -> Vec<Snippet>;
    fn quicklinks(&self) -> Vec<Quicklink>;
    fn replace_snippets(&self, snippets: Vec<Snippet>);
    fn replace_quicklinks(&self, quicklinks: Vec<Quicklink>);
    fn command_enabled(&self, command_id: &str) -> bool;
    fn show_command_in_launcher(&self, command_id: &str) -> bool;
    fn replace_command_availability(&self, availability: Vec<CommandAvailability>);
    fn emoji_column_count(&self) -> usize;
    fn emoji_skin_tone(&self) -> usize;
    fn replace_emoji_preferences(&self, column_count: usize, skin_tone: usize);
    fn clipboard_auto_paste(&self) -> bool;
    fn replace_clipboard_auto_paste(&self, enabled: bool);
    fn file_search_options(&self) -> FileSearchOptions;
    fn replace_file_search_options(&self, options: FileSearchOptions);
    fn escape_closes_window(&self) -> bool;
    fn replace_escape_behavior(&self, close_window: bool);
    fn interface_size_option(&self) -> usize;
    fn transparency_level(&self) -> usize;
    fn replace_interface_appearance(&self, size_option: usize, transparency_level: usize);
    fn compact_mode(&self) -> bool;
    fn replace_compact_mode(&self, enabled: bool);
    fn update_settings(&self) -> UpdateSettings;
    fn replace_update_settings(&self, settings: UpdateSettings);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateSettings {
    pub check_updates: bool,
    pub channel: String,
    pub auto_download: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_updates: true,
            channel: "stable".to_string(),
            auto_download: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snippet {
    pub name: String,
    pub keyword: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct Quicklink {
    pub name: String,
    pub url: String,
    pub alias: Option<String>,
    pub hotkey: Option<String>,
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub struct CommandAvailability {
    pub command_id: String,
    pub enabled: bool,
    pub show_in_launcher: bool,
}

#[derive(Clone, Debug, Default)]
pub struct FileSearchOptions {
    pub enabled: bool,
    pub search_scopes: Vec<String>,
    pub ignore_patterns: Vec<String>,
}

/// Input handed to `Command::search`.
#[derive(Clone)]
pub struct SearchContext {
    pub max_results: usize,
    pub store: Option<Arc<dyn DataStore>>,
}

impl Default for SearchContext {
    fn default() -> Self {
        Self {
            max_results: DEFAULT_MAX_RESULTS,
            store: None,
        }
    }
}

/// Input handed to `Command::execute`.
#[derive(Clone, Default)]
pub struct ExecutionContext {
    pub store: Option<Arc<dyn DataStore>>,
}

/// The contract every launcher feature implements.
#[async_trait::async_trait]
pub trait Command: Send + Sync {
    /// Stable identifier, also the namespace of result ids.
    fn id(&self) -> &'static str;
    /// Extra words the global search matches against.
    fn keywords(&self) -> &'static [&'static str] {
        &[]
    }
    /// When set, the command only answers queries that start with it.
    fn prefix(&self) -> Option<&'static str> {
        None
    }

    /// Must stay non-blocking. Slow work runs on a background task that
    /// feeds a cache; `search` reads the cache.
    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult>;
    async fn execute(
        &self,
        result_id: &str,
        ctx: &ExecutionContext,
    ) -> Result<Action, CommandError>;
    /// The actions menu for one result. Static and cheap; the UI calls it
    /// when the menu opens.
    fn actions(&self, _result_id: &str) -> Vec<CommandAction> {
        Vec::new()
    }

    /// What the extension declares about itself: title, description,
    /// icon, and its command surface. Root search and Settings render
    /// from this; it never runs I/O.
    fn manifest(&self) -> ExtensionManifest {
        ExtensionManifest {
            name: self.id(),
            title: "Untitled extension",
            description: "",
            icon: Icon::App,
            categories: &[],
            commands: Vec::new(),
        }
    }

    /// The page this command renders for `{id}-page:{filter}`
    /// queries, or `None` to keep the generic results list. Blocking
    /// but bounded: read state and caches only; the UI calls it
    /// through an unblock executor and re-asks at the page's
    /// [`Refresh`] cadence.
    fn page(&self, _query: &str) -> Option<PageView> {
        None
    }

    /// Higher wins when two commands answer the same query. The
    /// web-search fallback sits at the bottom on purpose.
    fn priority(&self) -> u8 {
        50
    }
}

/// Link-time registration record. `factory` builds a default instance.
pub struct CommandDescriptor {
    pub factory: fn() -> Box<dyn Command>,
}
inventory::collect!(CommandDescriptor);

// Re-exported so `register_command!` expands in crates that do not
// depend on `inventory` directly.
pub use inventory;

/// Registers a `Default`-constructible command at link time.
///
/// The registered type must implement `Default`.
#[macro_export]
macro_rules! register_command {
    ($ty:ty) => {
        $crate::inventory::submit! {
            $crate::CommandDescriptor { factory: || Box::new(<$ty>::default()) }
        }
    };
}

/// All commands linked into the binary, highest priority first.
#[derive(Clone)]
pub struct CommandRegistry {
    commands: Vec<Arc<dyn Command>>,
}

impl CommandRegistry {
    /// Collects every `register_command!` submission from the linked crates.
    pub fn from_inventory() -> Self {
        let mut commands: Vec<Arc<dyn Command>> = inventory::iter::<CommandDescriptor>()
            .map(|d| Arc::from((d.factory)()))
            .collect();
        commands.sort_by_key(|command| std::cmp::Reverse(command.priority()));
        Self { commands }
    }

    pub fn commands(&self) -> &[Arc<dyn Command>] {
        &self.commands
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::from_inventory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phosphor_svg_content() {
        use phosphor_svgs::style::regular::*;
        let icons = [
            MAGNIFYING_GLASS,
            LIST,
            CARET_LEFT,
            CARET_DOWN,
            SQUARES_FOUR,
            FUNNEL,
            FILE_TEXT,
            CLIPBOARD_TEXT,
            CALCULATOR,
            SCISSORS,
            APP_WINDOW,
            SMILEY,
            ARROW_UP_RIGHT,
            GEAR,
            GLOBE,
            COPY,
            ARROW_BEND_DOWN_LEFT,
            TRASH,
            LOCK,
            MOON,
            ARROW_CLOCKWISE,
            POWER,
            SIGN_OUT,
            SUN,
            SPEAKER_SLASH,
            SPEAKER_HIGH,
            SPEAKER_LOW,
            EYE_SLASH,
            X_CIRCLE,
            SIDEBAR_SIMPLE,
            ROWS,
            ARROWS_OUT,
            ARROWS_OUT_SIMPLE,
            FRAME_CORNERS,
            FOLDER,
            TERMINAL_WINDOW,
            IMAGE,
        ];
        for svg in icons {
            assert!(svg.starts_with("<svg"));
            assert!(svg.ends_with("</svg>"));
        }
    }
}

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
    pub score: f32,
    /// Right-aligned hint, for example a hotkey or a unit.
    pub accessory: Option<String>,
}

/// What the launcher does after the user picks a result.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Open(PathBuf),
    OpenUrl(String),
    Copy(String),
    CopyImage(PathBuf),
    PasteImage(PathBuf),
    RunShell(String),
    ShowToast(String),
    CloseWindow,
    TileWindow(String),
    AdjustBrightness(f32),
}

/// One entry of a result's actions menu, the ⌘K surface. `action` runs
/// as if the command had returned it from `execute`.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandAction {
    pub id: String,
    pub label: String,
    pub action: Action,
    pub icon: Icon,
    /// Menu section the action belongs to; sections render apart with a
    /// divider between them.
    pub group: ActionGroup,
    /// Display hint for the shortcut that triggers the action, for
    /// example "↵" or "⌘↵".
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

/// Read-only data the config layer loads for commands. Defined here so
/// command crates never depend on `corvo-config` directly. Frecency and
/// clipboard history join this trait in later phases.
pub trait DataStore: Send + Sync {
    fn snippets(&self) -> Vec<Snippet>;
    fn quicklinks(&self) -> Vec<Quicklink>;
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
}

/// Input handed to `Command::search`.
#[derive(Clone)]
pub struct SearchContext {
    pub max_results: usize,
    pub store: Option<Arc<dyn DataStore>>,
}

impl Default for SearchContext {
    fn default() -> Self {
        Self { max_results: DEFAULT_MAX_RESULTS, store: None }
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
    async fn execute(&self, result_id: &str, ctx: &ExecutionContext) -> Result<Action, CommandError>;
    /// The ⌘K actions menu for one result. Static and cheap; the UI
    /// calls it when the menu opens.
    fn actions(&self, _result_id: &str) -> Vec<CommandAction> {
        Vec::new()
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


//! The page model: what an extension can render beyond list rows, and
//! the manifest it declares about itself. Mirrors the view catalog of
//! launcher extension platforms: composed blocks, a markdown Detail,
//! a Grid, and a Form, plus per-command metadata with typed
//! arguments. `corvo-ui` interprets these values generically — an
//! extension never draws.

use std::path::PathBuf;

/// Semantic color roles. The UI maps these onto its palette, so
/// extensions stay theme-independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Positive,
    Warning,
    Destructive,
}

/// How often the UI re-asks the command for its page while the page
/// is open. `Every` generalizes the ticking-clock page: pick the
/// slowest cadence that still looks alive (one second for countdowns).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// Refresh only after an action or a query change.
    Manual,
    /// Re-render every `u64` seconds while the page is visible.
    Every(u64),
}

/// One composed page: a vertical stack of blocks. Covers the
/// big-number pages (countdown, timer), now-playing surfaces, and
/// forecast strips without bespoke UI code.
#[derive(Clone, Debug, PartialEq)]
pub struct Blocks {
    pub blocks: Vec<Block>,
    pub refresh: Refresh,
    /// Search-bar hint while the page is open, e.g. "Minutes for a
    /// custom focus, or leave empty for 25...".
    pub placeholder: &'static str,
}

/// A large status label, the small pill above hero surfaces.
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    pub label: String,
    pub tone: Tone,
}

/// The dominant datum of a page: a small glyph, a headline, one very
/// large value, and a quiet line under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Hero {
    pub glyph: Option<&'static str>,
    pub title: String,
    pub value: String,
    pub subtitle: String,
    pub tone: Tone,
}

/// A determinate progress bar, 0.0 to 1.0.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub fraction: f32,
    pub tone: Tone,
}

/// A row of small stat cards, like a three-day forecast strip.
#[derive(Clone, Debug, PartialEq)]
pub struct StripCard {
    pub title: String,
    pub glyph: Option<&'static str>,
    pub value: String,
    pub subtitle: String,
}

/// One clickable control. `action_id` rides in the execute id
/// `{command-id}:page:{action_id}`; no data may be embedded here —
/// carry state through the command itself.
#[derive(Clone, Debug, PartialEq)]
pub struct PageButton {
    pub action_id: &'static str,
    pub label: String,
    pub tone: Tone,
    /// Shortcut tokens such as "cmd+enter"; the UI translates `cmd`
    /// to the platform's primary modifier.
    pub hotkey: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Badge(Badge),
    Hero(Hero),
    Progress(Progress),
    /// Rendered with the launcher's markdown subset.
    Markdown(String),
    Strip(Vec<StripCard>),
    Buttons(Vec<PageButton>),
}

/// One structured metadata entry under a Detail page.
#[derive(Clone, Debug, PartialEq)]
pub enum Metadata {
    /// A `title: text` line.
    Label { title: String, text: String, tone: Tone },
    /// A `title: text` line that opens `url`.
    Link { title: String, text: String, url: String },
    /// A row of small tags.
    Tags { title: String, tags: Vec<String> },
    Separator,
}

/// One tile of a grid: an emoji glyph, a color swatch, an image, or a
/// letter/word. Clicks route to `execute` as
/// `{command-id}:grid:{id}`.
#[derive(Clone, Debug, PartialEq)]
pub struct GridItem {
    pub id: String,
    pub content: GridContent,
    pub title: String,
    pub subtitle: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GridContent {
    Glyph(String),
    Color(u32),
    Image(PathBuf),
    Text(String),
}

/// A single-line form field. Values travel to `execute` encoded in
/// the submit id; keep them small and secret-free — passwords are
/// masked on screen but not encrypted in transit.
#[derive(Clone, Debug, PartialEq)]
pub enum FormField {
    Text {
        id: &'static str,
        title: String,
        placeholder: &'static str,
        default: String,
        password: bool,
        required: bool,
    },
    Checkbox {
        id: &'static str,
        title: String,
        default: bool,
    },
    Select {
        id: &'static str,
        title: String,
        options: Vec<(String, String)>,
        default: String,
        required: bool,
    },
}

impl FormField {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Text { id, .. } | Self::Checkbox { id, .. } | Self::Select { id, .. } => id,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Text { title, .. } | Self::Checkbox { title, .. } | Self::Select { title, .. } => {
                title
            }
        }
    }

    pub fn required(&self) -> bool {
        match self {
            Self::Text { required, .. } | Self::Select { required, .. } => *required,
            Self::Checkbox { .. } => false,
        }
    }
}

/// The whole page an extension renders for `{id}-page:{filter}`
/// queries. `List` is the implicit default: rows from `search` in the
/// generic results list.
#[derive(Clone, Debug, PartialEq)]
pub enum PageView {
    Blocks(Blocks),
    /// A rendered markdown document with structured metadata on the
    /// side.
    Detail {
        markdown: String,
        metadata: Vec<Metadata>,
        refresh: Refresh,
    },
    Grid {
        items: Vec<GridItem>,
        /// 1..=8; `None` lets the UI pick from the window width.
        columns: Option<u8>,
        placeholder: &'static str,
        refresh: Refresh,
    },
    Form {
        title: String,
        fields: Vec<FormField>,
        /// Label of the submit button.
        submit: String,
        refresh: Refresh,
    },
}

impl PageView {
    pub fn refresh(&self) -> Refresh {
        match self {
            Self::Blocks(blocks) => blocks.refresh,
            Self::Detail { refresh, .. }
            | Self::Grid { refresh, .. }
            | Self::Form { refresh, .. } => *refresh,
        }
    }
}

/// One declared argument of a manifest command: a typed slot the user
/// fills in root search. Up to three per command, required first.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentKind {
    Text,
    Password,
    /// A fixed choice list of `(title, value)`.
    Dropdown(Vec<(String, String)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArgumentSpec {
    pub name: &'static str,
    pub placeholder: &'static str,
    pub kind: ArgumentKind,
    pub required: bool,
}

/// How a declared command presents itself. `NoView` commands act and
/// confirm with a toast; `View` commands open a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandMode {
    View,
    NoView,
}

/// One command inside an extension manifest. The launcher renders
/// these entries in root search and Settings without help from the
/// crate; execution still flows through the crate's `execute` with
/// ids `{command-id}:{name}[:{argument-values}]`.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub mode: CommandMode,
    pub icon: Option<crate::Icon>,
    pub arguments: Vec<ArgumentSpec>,
    pub keywords: &'static [&'static str],
}

/// What an extension declares about itself: the extension-level
/// identity plus its command surface. The manifest drives root
/// search, the Settings command list, and hotkey binding.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionManifest {
    /// The crate's command id — equal to `Command::id`.
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: crate::Icon,
    pub categories: &'static [&'static str],
    pub commands: Vec<CommandSpec>,
}

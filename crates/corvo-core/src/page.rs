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

/// Optional per-block styling. The launcher's components are generic;
/// the design is the extension's: every `Some` field overrides the
/// theme default for that piece, every `None` field keeps it. Colors
/// are sRGB values such as `0xff6600`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    /// Primary text or accent color of the block.
    pub color: Option<u32>,
    /// Block background fill.
    pub background: Option<u32>,
    /// Primary text size in px (the hero value, the badge label, the
    /// card value).
    pub size: Option<u32>,
    /// Glyph size in px, when the block draws one.
    pub glyph_size: Option<u32>,
    /// Fixed width in px (progress track).
    pub width: Option<u32>,
    /// Fixed height in px (progress track).
    pub height: Option<u32>,
    /// Bold primary text.
    pub bold: Option<bool>,
}

impl Style {
    /// The styled color, or the theme default.
    pub fn color_or(self, default: u32) -> u32 {
        self.color.unwrap_or(default)
    }
}

/// One page-level filter: a chip row above the content, the second
/// filter dimension list extensions lean on. The command owns the
/// selection and re-renders when a chip runs
/// `{command-id}:page:filter:{value}`.
#[derive(Clone, Debug, PartialEq)]
pub struct PageFilter {
    pub value: String,
    pub label: String,
}

impl PageFilter {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
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
    /// Filter chips rendered above the content. The selected one is
    /// whatever the command styles as picked.
    pub filters: Vec<PageFilter>,
}

/// A large status label, the small pill above hero surfaces.
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    pub label: String,
    pub tone: Tone,
    pub style: Style,
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
    pub style: Style,
}

/// A determinate progress bar, 0.0 to 1.0.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub fraction: f32,
    pub tone: Tone,
    pub style: Style,
}

/// A row of small stat cards, like a three-day forecast strip.
#[derive(Clone, Debug, PartialEq)]
pub struct StripCard {
    pub title: String,
    pub glyph: Option<&'static str>,
    pub value: String,
    pub subtitle: String,
    pub style: Style,
}

/// One clickable control. `action_id` rides in the execute id
/// `{command-id}:page:{action_id}`; the page rebuilds every render,
/// so the id may carry per-render data such as a chip's value.
#[derive(Clone, Debug, PartialEq)]
pub struct PageButton {
    pub action_id: String,
    pub label: String,
    pub tone: Tone,
    pub style: Style,
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
    Markdown { text: String, style: Style },
    Strip(Vec<StripCard>),
    Buttons(Vec<PageButton>),
}

impl Block {
    /// Merges `style` into this block, `Some` fields winning. The
    /// `PageBuilder::style` chain uses this to restyle the block that
    /// was just pushed.
    pub fn apply_style(&mut self, style: Style) {
        match self {
            Self::Badge(badge) => merge(&mut badge.style, style),
            Self::Hero(hero) => merge(&mut hero.style, style),
            Self::Progress(progress) => merge(&mut progress.style, style),
            Self::Markdown { style: existing, .. } => merge(existing, style),
            Self::Strip(cards) => {
                for card in cards {
                    merge(&mut card.style, style);
                }
            }
            Self::Buttons(buttons) => {
                for button in buttons {
                    merge(&mut button.style, style);
                }
            }
        }
    }
}

fn merge(existing: &mut Style, incoming: Style) {
    if incoming.color.is_some() {
        existing.color = incoming.color;
    }
    if incoming.background.is_some() {
        existing.background = incoming.background;
    }
    if incoming.size.is_some() {
        existing.size = incoming.size;
    }
    if incoming.glyph_size.is_some() {
        existing.glyph_size = incoming.glyph_size;
    }
    if incoming.width.is_some() {
        existing.width = incoming.width;
    }
    if incoming.height.is_some() {
        existing.height = incoming.height;
    }
    if incoming.bold.is_some() {
        existing.bold = incoming.bold;
    }
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
    /// Group label; consecutive items sharing one render under a
    /// section header with the item count, the Unsplash pattern.
    pub section: Option<String>,
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

#[cfg(test)]
mod style_tests {
    use super::*;

    #[test]
    fn apply_style_merges_only_set_fields() {
        let mut block = Block::Hero(Hero {
            glyph: None,
            title: "Focus".into(),
            value: "24:59".into(),
            subtitle: String::new(),
            tone: Tone::Accent,
            style: Style {
                color: Some(0xf97316),
                ..Style::default()
            },
        });
        block.apply_style(Style {
            size: Some(96),
            color: Some(0xfbbf24),
            ..Style::default()
        });
        let Block::Hero(hero) = &block else {
            panic!("expected hero");
        };
        assert_eq!(hero.style.color, Some(0xfbbf24), "the later color wins");
        assert_eq!(hero.style.size, Some(96));
    }

    #[test]
    fn apply_style_cascades_into_strips_and_buttons() {
        let mut block = Block::Strip(vec![StripCard {
            title: "Sun".into(),
            glyph: None,
            value: "18°".into(),
            subtitle: String::new(),
            style: Style::default(),
        }]);
        block.apply_style(Style {
            color: Some(0x38bdf8),
            ..Style::default()
        });
        let Block::Strip(cards) = &block else {
            panic!("expected strip");
        };
        assert_eq!(cards[0].style.color, Some(0x38bdf8));
    }

    #[test]
    fn color_or_falls_back_to_the_theme() {
        let styled = Style {
            color: Some(0xff6600),
            ..Style::default()
        };
        assert_eq!(styled.color_or(0x34d399), 0xff6600);
        assert_eq!(Style::default().color_or(0x34d399), 0x34d399);
    }
}

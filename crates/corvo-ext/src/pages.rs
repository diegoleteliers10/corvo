//! The Pages module: builders for the declarative view catalog —
//! composed Blocks, Detail documents, Grids, and Forms. Extensions
//! return a [`corvo_core::PageView`] from `Command::page` and the
//! launcher renders it; no crate ever draws.

use corvo_core::{
    Badge, Block, Blocks, FormField, GridContent, GridItem, Hero, Metadata, PageButton, PageView,
    Progress, Refresh, StripCard, Tone,
};

/// A composed page under construction.
pub struct PageBuilder {
    blocks: Vec<Block>,
    refresh: Refresh,
    placeholder: &'static str,
}

impl PageBuilder {
    pub fn new(placeholder: &'static str) -> Self {
        Self {
            blocks: Vec::new(),
            refresh: Refresh::Manual,
            placeholder,
        }
    }

    /// The page re-renders every `secs` seconds while visible.
    pub fn ticking(mut self, secs: u64) -> Self {
        self.refresh = Refresh::Every(secs.max(1));
        self
    }

    pub fn badge(mut self, label: impl Into<String>, tone: Tone) -> Self {
        self.blocks.push(Block::Badge(Badge {
            label: label.into(),
            tone,
        }));
        self
    }

    pub fn hero(
        mut self,
        glyph: Option<&'static str>,
        title: impl Into<String>,
        value: impl Into<String>,
        subtitle: impl Into<String>,
        tone: Tone,
    ) -> Self {
        self.blocks.push(Block::Hero(Hero {
            glyph,
            title: title.into(),
            value: value.into(),
            subtitle: subtitle.into(),
            tone,
        }));
        self
    }

    /// Clamped to 0.0..=1.0.
    pub fn progress(mut self, fraction: f32, tone: Tone) -> Self {
        self.blocks.push(Block::Progress(Progress {
            fraction: fraction.clamp(0.0, 1.0),
            tone,
        }));
        self
    }

    pub fn markdown(mut self, markdown: impl Into<String>) -> Self {
        self.blocks.push(Block::Markdown(markdown.into()));
        self
    }

    pub fn strip(mut self, cards: Vec<StripCard>) -> Self {
        self.blocks.push(Block::Strip(cards));
        self
    }

    pub fn buttons(mut self, buttons: Vec<PageButton>) -> Self {
        self.blocks.push(Block::Buttons(buttons));
        self
    }

    pub fn build(self) -> PageView {
        PageView::Blocks(Blocks {
            blocks: self.blocks,
            refresh: self.refresh,
            placeholder: self.placeholder,
        })
    }
}

/// A markdown Detail page with structured metadata lines.
pub fn detail(
    markdown: impl Into<String>,
    metadata: Vec<Metadata>,
    refresh: Refresh,
) -> PageView {
    PageView::Detail {
        markdown: markdown.into(),
        metadata,
        refresh,
    }
}

/// A grid of tiles: emoji walls, color palettes, icon pickers.
pub fn grid(
    items: Vec<GridItem>,
    columns: Option<u8>,
    placeholder: &'static str,
    refresh: Refresh,
) -> PageView {
    PageView::Grid {
        items,
        columns: columns.map(|columns| columns.clamp(1, 8)),
        placeholder,
        refresh,
    }
}

/// One grid tile with an emoji glyph.
pub fn grid_glyph(id: impl Into<String>, glyph: impl Into<String>, title: impl Into<String>) -> GridItem {
    GridItem {
        id: id.into(),
        content: GridContent::Glyph(glyph.into()),
        title: title.into(),
        subtitle: String::new(),
    }
}

/// One grid tile with a color swatch.
pub fn grid_color(
    id: impl Into<String>,
    rgb: u32,
    title: impl Into<String>,
    subtitle: impl Into<String>,
) -> GridItem {
    GridItem {
        id: id.into(),
        content: GridContent::Color(rgb),
        title: title.into(),
        subtitle: subtitle.into(),
    }
}

/// One grid tile with a cached PNG image.
pub fn grid_image(
    id: impl Into<String>,
    png: std::path::PathBuf,
    title: impl Into<String>,
    subtitle: impl Into<String>,
) -> GridItem {
    GridItem {
        id: id.into(),
        content: GridContent::Image(png),
        title: title.into(),
        subtitle: subtitle.into(),
    }
}

/// A data-entry form. Field values arrive at `execute` encoded in the
/// submit id.
pub fn form(title: impl Into<String>, fields: Vec<FormField>, submit: impl Into<String>) -> PageView {
    PageView::Form {
        title: title.into(),
        fields,
        submit: submit.into(),
        refresh: Refresh::Manual,
    }
}

/// A required single-line text field.
pub fn text_field(
    id: &'static str,
    title: impl Into<String>,
    placeholder: &'static str,
    default: impl Into<String>,
    password: bool,
) -> FormField {
    FormField::Text {
        id,
        title: title.into(),
        placeholder,
        default: default.into(),
        password,
        required: true,
    }
}

/// An optional single-line text field.
pub fn optional_text_field(
    id: &'static str,
    title: impl Into<String>,
    placeholder: &'static str,
    default: impl Into<String>,
) -> FormField {
    FormField::Text {
        id,
        title: title.into(),
        placeholder,
        default: default.into(),
        password: false,
        required: false,
    }
}

pub fn checkbox(id: &'static str, title: impl Into<String>, default: bool) -> FormField {
    FormField::Checkbox {
        id,
        title: title.into(),
        default,
    }
}

/// `options` is a list of `(value, title)` pairs.
pub fn select(
    id: &'static str,
    title: impl Into<String>,
    options: Vec<(String, String)>,
    default: impl Into<String>,
) -> FormField {
    FormField::Select {
        id,
        title: title.into(),
        options,
        default: default.into(),
        required: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_composes_blocks_and_clamps() {
        let page = PageBuilder::new("Tick...")
            .ticking(1)
            .badge("FOCUS · PAUSED", Tone::Neutral)
            .hero(None, "Focus", "24:59", "Stop at 15:00", Tone::Accent)
            .progress(1.4, Tone::Accent)
            .buttons(vec![PageButton {
                action_id: "pause",
                label: "Pause".into(),
                tone: Tone::Neutral,
                hotkey: Some("enter"),
            }])
            .build();
        let PageView::Blocks(blocks) = page else {
            panic!("expected blocks");
        };
        assert_eq!(blocks.refresh, Refresh::Every(1));
        assert_eq!(blocks.blocks.len(), 4);
        let Block::Progress(progress) = &blocks.blocks[2] else {
            panic!("expected progress");
        };
        assert_eq!(progress.fraction, 1.0);
    }

    #[test]
    fn grid_helpers_fill_items() {
        let page = grid(
            vec![
                grid_glyph("party", "🥳", "Party"),
                grid_color("sky", 0x38bdf8, "Sky", "#38bdf8"),
            ],
            Some(12), // clamped to 8
            "Pick a glyph...",
            Refresh::Manual,
        );
        let PageView::Grid { items, columns, .. } = page else {
            panic!("expected grid");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(columns, Some(8));
    }

    #[test]
    fn form_fields_report_ids_and_requirements() {
        let fields = [
            text_field("name", "Name", "Ada Lovelace", "", false),
            optional_text_field("note", "Note", "optional", ""),
            checkbox("pin", "Pin to top", false),
            select(
                "tone",
                "Tone",
                vec![("soft".into(), "Soft".into()), ("loud".into(), "Loud".into())],
                "soft",
            ),
        ];
        let ids: Vec<_> = fields.iter().map(|field| field.id()).collect();
        assert_eq!(ids, ["name", "note", "pin", "tone"]);
        let required: Vec<_> = fields.iter().map(|field| field.required()).collect();
        assert_eq!(required, [true, false, false, true]);
    }
}

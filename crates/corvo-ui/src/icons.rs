//! Phosphor Icons SVG renderer.

use corvo_core::Icon;
use gpui::*;

pub fn render_phosphor_svg(svg_data: &'static str, color: impl Into<Hsla>, size_px: f32) -> Svg {
    svg()
        .data(svg_data.as_bytes())
        .text_color(color)
        .size(px(size_px))
        .flex_none()
}

pub fn icon_svg_data(icon: &Icon) -> Option<&'static str> {
    match icon {
        Icon::File => Some(phosphor_svgs::style::regular::FILE_TEXT),
        Icon::Clipboard => Some(phosphor_svgs::style::regular::CLIPBOARD_TEXT),
        Icon::Calculator => Some(phosphor_svgs::style::regular::CALCULATOR),
        Icon::Snippet => Some(phosphor_svgs::style::regular::SCISSORS),
        Icon::Window => Some(phosphor_svgs::style::regular::APP_WINDOW),
        Icon::Emoji => Some(phosphor_svgs::style::regular::SMILEY),
        Icon::Link => Some(phosphor_svgs::style::regular::ARROW_UP_RIGHT),
        Icon::System => Some(phosphor_svgs::style::regular::GEAR),
        Icon::Web => Some(phosphor_svgs::style::regular::GLOBE),
        Icon::App => Some(phosphor_svgs::style::regular::APP_WINDOW),
        Icon::Svg(svg_str) => Some(svg_str),
        Icon::Glyph(_) | Icon::Image(_) => None,
    }
}

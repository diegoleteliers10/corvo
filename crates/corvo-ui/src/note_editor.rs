//! Corvo's native note editor window: the markdown renders as you type
//! (headings, bullets, checklists, quotes, inline styles) over the
//! launcher's design palette. Editing stays line-based underneath the
//! rendering; the file is autosaved and named from the title.

use gpui::{
    div, prelude::*, px, rgb, rgba, App, ClickEvent, Context, Div, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Render, SharedString, Stateful, Styled, StyledText,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};

#[path = "note_syntax.rs"]
mod note_syntax;
use note_syntax::{CodeRowKind, CodeTone};
#[path = "note_scrollbar.rs"]
mod note_scrollbar;
#[path = "note_tables.rs"]
mod note_tables;
use note_tables::{TableAlign, TableRowKind};

const WINDOW_WIDTH: f32 = 680.0;
const WINDOW_HEIGHT: f32 = 520.0;
const FOOTER_BADGE_HEIGHT: f32 = 28.0;

const COLOR_TEXT: u32 = 0xf4f4f5;
const COLOR_DIM: u32 = 0xa1a1aa;
const COLOR_FAINT: u32 = 0x52525b;
const COLOR_DIVIDER: u32 = 0x27272a;
const COLOR_MENU: u32 = 0x111214;
const COLOR_BAR: u32 = 0x16181b;
const COLOR_ACCENT: u32 = 0x34d399;
const COLOR_SELECTION: u32 = 0x1d3b2f;
const COLOR_CODE_BG: u32 = 0x1b1e22;

/// Opens (and focuses) the note editor window for an existing note, or
/// a fresh one seeded with `initial_title`.
pub fn open_note_editor(
    existing: Option<corvo_notes::Note>,
    initial_title: Option<String>,
    cx: &mut App,
) {
    if let Some(note) = existing.as_ref() {
        let open_editor = cx.windows().into_iter().find_map(|window| {
            let handle = window.downcast::<NoteEditor>()?;
            handle
                .read_with(cx, |editor, _| {
                    editor.note_id.as_deref() == Some(note.id.as_str())
                })
                .ok()
                .filter(|matches| *matches)
                .map(|_| handle)
        });
        if let Some(handle) = open_editor {
            let _ = handle.update(cx, |editor, window, cx| {
                editor.focus_handle.focus(window, cx);
                window.activate_window();
            });
            cx.activate(true);
            return;
        }
    }
    let size = gpui::size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT));
    let bounds = gpui::Bounds::centered(None, size, cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("Corvo Notes")),
            appears_transparent: true,
            // Center the lights in the shorter bar (32px).
            traffic_light_position: Some(gpui::point(px(14.0), px(9.0))),
        }),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        is_resizable: true,
        focus: true,
        show: true,
        ..Default::default()
    };
    let opened = cx.open_window(options, |_window, cx| {
        cx.new(|cx| {
            let scroll = gpui::ScrollHandle::new();
            let reveal_generation = std::rc::Rc::new(std::cell::Cell::new(0));
            let scrollbar = cx.new(|_| {
                note_scrollbar::NoteScrollbar::new(scroll.clone(), reveal_generation.clone())
            });
            let focus_handle = cx.focus_handle();
            let mut body: Vec<String> = existing
                .as_ref()
                .map(|note| note.body.split("\n").map(str::to_owned).collect())
                .unwrap_or_default();
            if body.is_empty() {
                body.push(String::new());
            }
            let is_existing = existing.is_some();
            NoteEditor {
                note_id: existing.as_ref().map(|note| note.id.clone()),
                title: existing
                    .map(|note| note.title)
                    .or(initial_title)
                    .unwrap_or_default(),
                title_col: 0,
                body,
                cursor_line: 0,
                cursor_col: 0,
                selection_anchor: None,
                pointer_anchor: None,
                toolbar_menu: None,
                context_menu: None,
                copy_feedback: false,
                body_scroll: scroll,
                scrollbar,
                reveal_generation,
                pending_reveal: None,
                pressed_link: None,
                title_focused: !is_existing,
                status: if is_existing {
                    "Saved"
                } else {
                    "Empty — start typing to save"
                }
                .into(),
                focus_handle,
            }
        })
    });
    // Take focus and come up in front of every other app's windows.
    if let Ok(handle) = opened {
        let _ = handle.update(cx, |editor, window, _cx| {
            editor.focus_handle.focus(window, _cx);
            window.activate_window();
        });
        cx.activate(true);
    }
}

pub struct NoteEditor {
    note_id: Option<String>,
    title: String,
    title_col: usize,
    body: Vec<String>,
    cursor_line: usize,
    cursor_col: usize,
    selection_anchor: Option<(usize, usize)>,
    pointer_anchor: Option<(usize, usize)>,
    toolbar_menu: Option<ToolbarMenu>,
    context_menu: Option<gpui::Point<gpui::Pixels>>,
    copy_feedback: bool,
    body_scroll: gpui::ScrollHandle,
    scrollbar: gpui::Entity<note_scrollbar::NoteScrollbar>,
    reveal_generation: std::rc::Rc<std::cell::Cell<u64>>,
    pending_reveal: Option<u64>,
    pressed_link: Option<(String, gpui::Point<gpui::Pixels>)>,
    title_focused: bool,
    status: String,
    focus_handle: FocusHandle,
}

#[derive(Clone, Copy, PartialEq)]
enum ToolbarMenu {
    Headings,
    Styles,
    Lists,
}

impl ToolbarMenu {
    fn options(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Headings => &[
                ("Paragraph", "paragraph"),
                ("Heading 1", "heading"),
                ("Heading 2", "heading2"),
                ("Heading 3", "heading3"),
            ],
            Self::Styles => &[
                ("Bold", "bold"),
                ("Italic", "italic"),
                ("Inline code", "code"),
            ],
            Self::Lists => &[
                ("Bullet list", "list"),
                ("Numbered list", "numbered"),
                ("Checklist", "checklist"),
                ("Table", "table"),
            ],
        }
    }
}

/// A normalized (start, end) caret pair in (line, col) order.
fn ordered(head: (usize, usize), anchor: (usize, usize)) -> ((usize, usize), (usize, usize)) {
    if (head.0, head.1) < (anchor.0, anchor.1) {
        (head, anchor)
    } else {
        (anchor, head)
    }
}

/// One run of rendered text from the source line, with its styles and
/// where it starts in source columns.
#[derive(Clone)]
struct InlineSeg {
    /// Column offset of this run inside the line's content.
    start: usize,
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    selected: bool,
    color: Option<u32>,
    link: Option<String>,
}

impl InlineSeg {
    fn chars(&self) -> usize {
        self.text.chars().count()
    }

    fn split(self, at: usize) -> (Self, Self) {
        let left: String = self.text.chars().take(at).collect();
        let right: String = self.text.chars().skip(at).collect();
        let mut first = self.clone();
        first.text = left;
        let mut second = self;
        second.start += at;
        second.text = right;
        (first, second)
    }
}

/// Splits every segment at `col` so a selection or caret boundary can
/// be painted exactly.
fn split_segments_at(segments: &mut Vec<InlineSeg>, col: usize) {
    let mut index = 0;
    while index < segments.len() {
        let start = segments[index].start;
        let end = start + segments[index].chars();
        if col > start && col < end {
            let left = segments[index].clone().split(col - start);
            let right = left.1;
            segments[index] = left.0;
            segments.insert(index + 1, right);
        }
        index += 1;
    }
}

/// Marks the source range [from, to) of the content as selected.
fn mark_selection(segments: &mut Vec<InlineSeg>, from: usize, to: usize) {
    split_segments_at(segments, from);
    split_segments_at(segments, to);
    for segment in segments.iter_mut() {
        segment.selected = segment.start >= from && segment.start < to;
    }
}

fn markdown_link(chars: &[char], start: usize) -> Option<(usize, usize, usize, String)> {
    if chars.get(start) != Some(&'[') || (start > 0 && matches!(chars[start - 1], '!' | '\\')) {
        return None;
    }
    let label_end = (start + 1..chars.len()).find(|index| chars[*index] == ']')?;
    if label_end == start + 1 || chars.get(label_end + 1) != Some(&'(') {
        return None;
    }
    let mut depth = 1usize;
    let mut end = label_end + 2;
    while end < chars.len() {
        match chars[end] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        end += 1;
    }
    if depth != 0 {
        return None;
    }
    let url: String = chars[label_end + 2..end].iter().collect();
    let url = url.trim().to_owned();
    if !supported_link(&url) {
        return None;
    }
    Some((start + 1, label_end, end + 1, url))
}

fn supported_link(url: &str) -> bool {
    if url.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return false;
    }
    if let Some(address) = url.strip_prefix("mailto:") {
        return !address.is_empty();
    }
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .is_some_and(|rest| !rest.split('/').next().unwrap_or("").is_empty())
}

/// Parses a content line into styled runs: toggles `**bold**`,
/// `*italic*`, and `` `code` `` markers, which are consumed.
fn parse_inline(content: &str, sel: Option<(usize, usize)>) -> Vec<InlineSeg> {
    let mut segments: Vec<InlineSeg> = Vec::new();
    let mut bold = false;
    let mut italic = false;
    let mut italic_marker: Option<char> = None;
    let mut code = false;
    let mut run_start = 0usize;
    let mut run = String::new();

    fn flush(
        segments: &mut Vec<InlineSeg>,
        run_start: usize,
        run: &mut String,
        bold: bool,
        italic: bool,
        code: bool,
    ) {
        if !run.is_empty() {
            segments.push(InlineSeg {
                start: run_start,
                text: std::mem::take(run),
                bold,
                italic,
                code,
                selected: false,
                color: None,
                link: None,
            });
        }
    }
    let chars: Vec<char> = content.chars().collect();
    let mut col = 0usize;
    let mut index = 0usize;
    while index < chars.len() {
        if !code {
            if let Some((label_start, label_end, end, url)) = markdown_link(&chars, index) {
                flush(&mut segments, run_start, &mut run, bold, italic, code);
                let label: String = chars[label_start..label_end].iter().collect();
                for mut segment in parse_inline(&label, None) {
                    segment.start += label_start;
                    segment.bold |= bold;
                    segment.italic |= italic;
                    segment.link = Some(url.clone());
                    segments.push(segment);
                }
                index = end;
                col = end;
                run_start = col;
                continue;
            }
        }
        let rest: String = chars[index..].iter().collect();
        if !code && rest.starts_with("**") && (bold || rest[2..].contains("**")) {
            flush(&mut segments, run_start, &mut run, bold, italic, code);
            bold = !bold;
            index += 2;
            col += 2;
            run_start = col;
            continue;
        }
        if chars[index] == '`' && (code || chars[index + 1..].contains(&'`')) {
            flush(&mut segments, run_start, &mut run, bold, italic, code);
            code = !code;
            index += 1;
            col += 1;
            run_start = col;
            continue;
        }
        let marker = chars[index];
        let inside_word = marker == '_'
            && index > 0
            && chars[index - 1].is_alphanumeric()
            && chars.get(index + 1).is_some_and(|ch| ch.is_alphanumeric());
        let opens_emphasis = italic_marker.is_none()
            && !inside_word
            && chars.get(index + 1).is_some_and(|ch| !ch.is_whitespace())
            && chars[index + 1..].iter().enumerate().any(|(offset, ch)| {
                *ch == marker && offset > 0 && !chars[index + offset].is_whitespace()
            });
        if !code
            && (marker == '*' || marker == '_')
            && (italic_marker == Some(marker) || opens_emphasis)
        {
            flush(&mut segments, run_start, &mut run, bold, italic, code);
            italic = !italic;
            italic_marker = if italic { Some(marker) } else { None };
            index += 1;
            col += 1;
            run_start = col;
            continue;
        }
        run.push(chars[index]);
        index += 1;
        col += 1;
    }
    flush(&mut segments, run_start, &mut run, bold, italic, code);

    if let Some((from, to)) = sel {
        mark_selection(&mut segments, from, to);
    }
    segments
}

/// How a source line renders.
#[derive(Clone, Copy, PartialEq)]
enum LineKind {
    Heading1,
    Heading2,
    Heading3,
    Bullet,
    Numbered(u64),
    Checkbox(bool),
    Quote,
    Paragraph,
}

struct ParsedLine {
    kind: LineKind,
    /// Source columns the marker consumed.
    prefix_len: usize,
    content: String,
}

fn parse_line(line: &str) -> ParsedLine {
    let trimmed_start = line.trim_start_matches(' ');
    let indent = line.chars().count() - trimmed_start.chars().count();
    let (kind, marker_len) = if let Some(_rest) = trimmed_start.strip_prefix("### ") {
        (LineKind::Heading3, 4)
    } else if let Some(_rest) = trimmed_start.strip_prefix("## ") {
        (LineKind::Heading2, 3)
    } else if let Some(_rest) = trimmed_start.strip_prefix("# ") {
        (LineKind::Heading1, 2)
    } else if trimmed_start.starts_with("- [ ] ") {
        (LineKind::Checkbox(false), 6)
    } else if trimmed_start.starts_with("- [x] ") || trimmed_start.starts_with("- [X] ") {
        (LineKind::Checkbox(true), 6)
    } else if let Some(_rest) = trimmed_start
        .strip_prefix("- ")
        .or_else(|| trimmed_start.strip_prefix("* "))
    {
        (LineKind::Bullet, 2)
    } else if let Some(_rest) = trimmed_start.strip_prefix("> ") {
        (LineKind::Quote, 2)
    } else {
        // A numbered item: digits, dot, space.
        let digits: String = trimmed_start
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() && digits.len() <= 9 {
            let marker = format!("{digits}. ");
            if let Some(_rest) = trimmed_start.strip_prefix(marker.as_str()) {
                let number: u64 = digits.parse().unwrap_or(1);
                (LineKind::Numbered(number), marker.chars().count())
            } else {
                (LineKind::Paragraph, 0)
            }
        } else {
            (LineKind::Paragraph, 0)
        }
    };
    let prefix_len = if marker_len == 0 {
        0
    } else {
        indent + marker_len
    };
    let content: String = line.chars().skip(prefix_len).collect();
    ParsedLine {
        kind,
        prefix_len,
        content,
    }
}

impl NoteEditor {
    fn cancel_reveal(&mut self) {
        self.reveal_generation
            .set(self.reveal_generation.get().wrapping_add(1));
        self.pending_reveal = None;
    }

    fn request_reveal(&mut self) {
        self.cancel_reveal();
        self.pending_reveal = Some(self.reveal_generation.get());
    }

    fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            self.copy_feedback = true;
        }
        self.context_menu = None;
        cx.notify();
    }

    fn start_pointer_selection(&mut self, line: usize, col: usize, extend: bool) {
        self.cancel_reveal();
        self.pressed_link = None;
        let position = (line, col.min(self.body[line].chars().count()));
        let anchor = if extend && !self.title_focused {
            self.selection_anchor
                .unwrap_or((self.cursor_line, self.cursor_col))
        } else {
            position
        };
        self.title_focused = false;
        self.toolbar_menu = None;
        self.context_menu = None;
        self.copy_feedback = false;
        self.cursor_line = position.0;
        self.cursor_col = position.1;
        self.selection_anchor = Some(anchor);
        self.pointer_anchor = Some(anchor);
    }

    fn select_pointer_range(&mut self, line: usize, col: usize, clicks: usize, extend: bool) {
        self.start_pointer_selection(line, col, extend);
        if extend || clicks < 2 {
            return;
        }
        let chars: Vec<char> = self.body[line].chars().collect();
        let (from, to) = if clicks >= 3 {
            (0, chars.len())
        } else {
            word_range(&chars, col)
        };
        self.selection_anchor = Some((line, from));
        self.pointer_anchor = Some((line, from));
        self.cursor_col = to;
    }

    fn extend_pointer_selection(&mut self, line: usize, col: usize) {
        self.copy_feedback = false;
        self.pressed_link = None;
        if let Some(anchor) = self.pointer_anchor {
            self.cursor_line = line;
            self.cursor_col = col.min(self.body[line].chars().count());
            self.selection_anchor = Some(anchor);
        }
    }

    fn has_selection(&self) -> bool {
        self.selection_anchor
            .is_some_and(|anchor| anchor != (self.cursor_line, self.cursor_col))
    }

    fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        self.selection_anchor
            .map(|anchor| ordered((self.cursor_line, self.cursor_col), anchor))
            .filter(|(start, end)| start != end)
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        if start.0 == end.0 {
            Some(char_slice(&self.body[start.0], start.1, end.1).to_owned())
        } else {
            let mut parts = vec![self.body[start.0]
                .split_at_char_boundary(start.1)
                .1
                .to_owned()];
            parts.extend(self.body[start.0 + 1..end.0].iter().cloned());
            parts.push(self.body[end.0].split_at_char_boundary(end.1).0.to_owned());
            Some(parts.join("\n"))
        }
    }

    fn delete_selection(&mut self) {
        let Some((start, end)) = self.selection_range() else {
            return;
        };
        let tail = self.body[end.0].split_at_char_boundary(end.1).1.to_owned();
        let mut merged = self.body[start.0]
            .split_at_char_boundary(start.1)
            .0
            .to_owned();
        merged.push_str(&tail);
        self.body.splice(start.0..=end.0, [merged]);
        self.cursor_line = start.0;
        self.cursor_col = start.1;
        self.selection_anchor = None;
    }

    fn body_text(&self) -> String {
        self.body.join("\n")
    }

    fn counts(&self) -> (usize, usize) {
        let text = self.body_text();
        (text.split_whitespace().count(), text.chars().count())
    }

    fn persist(&mut self) -> bool {
        let body = self.body_text();
        let outcome = match &self.note_id {
            Some(id) => corvo_notes::save_note(id, &self.title, &body).map(Some),
            None => corvo_notes::create_note(&self.title, &body).map(|note| Some(note.id)),
        };
        match outcome {
            Ok(new_id) => {
                self.note_id = new_id;
                self.status = "Saved".into();
                true
            }
            Err(error) => {
                self.status = if error.kind() == std::io::ErrorKind::InvalidInput {
                    "Empty — start typing to save".into()
                } else {
                    format!("Could not save: {error}")
                };
                false
            }
        }
    }

    fn insert_text(&mut self, text: &str) {
        if self.title_focused {
            if self.has_selection() {
                self.title = String::new();
                self.title_col = 0;
                self.selection_anchor = None;
            }
            for ch in text.chars().filter(|c| !c.is_control()) {
                self.title.insert(byte_col(&self.title, self.title_col), ch);
                self.title_col += 1;
            }
            return;
        }
        if self.has_selection() {
            self.delete_selection();
        }
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let parts: Vec<String> = normalized
            .split('\n')
            .map(|part| {
                part.chars()
                    .filter(|c| !c.is_control() || *c == '\t')
                    .collect()
            })
            .collect();
        let (before, after) = self.body[self.cursor_line].split_at_char_boundary(self.cursor_col);
        let mut replacement = parts.clone();
        replacement[0].insert_str(0, before);
        let last = replacement.len() - 1;
        self.cursor_col = if last == 0 {
            self.cursor_col + parts[0].chars().count()
        } else {
            parts[last].chars().count()
        };
        replacement[last].push_str(after);
        self.body
            .splice(self.cursor_line..=self.cursor_line, replacement);
        self.cursor_line += last;
    }

    fn wrap_selection(&mut self, open: &str, close: &str) {
        if self.title_focused {
            return;
        }
        if !self.has_selection() {
            self.insert_text(open);
            let between = self.cursor_col;
            self.insert_text(close);
            self.cursor_col = between;
            return;
        }
        let selected = self.selected_text().unwrap_or_default();
        self.delete_selection();
        let marked = format!("{open}{selected}{close}");
        let start = (self.cursor_line, self.cursor_col);
        self.insert_text(&marked);
        self.cursor_line = start.0;
        self.cursor_col = start.1 + open.chars().count();
    }

    fn toggle_prefix_on_selection(&mut self, prefix: &str) {
        if self.title_focused {
            return;
        }
        let (first, last) = match self.selection_range() {
            Some((start, end)) => (
                start.0,
                if end.1 == 0 && end.0 > start.0 {
                    end.0 - 1
                } else {
                    end.0
                },
            ),
            None => (self.cursor_line, self.cursor_line),
        };
        let remove = prefix.is_empty()
            || self.body[first..=last].iter().all(|line| {
                let parsed = parse_line(line);
                if prefix == "- " {
                    parsed.kind == LineKind::Bullet
                } else if prefix == "1. " {
                    matches!(parsed.kind, LineKind::Numbered(_))
                } else if prefix == "- [ ] " {
                    matches!(parsed.kind, LineKind::Checkbox(_))
                } else {
                    line.trim_start_matches(' ').starts_with(prefix)
                }
            });
        for index in first..=last {
            let line = &self.body[index];
            let parsed = parse_line(line);
            let indent = line.len() - line.trim_start_matches(' ').len();
            let marker_end = byte_col(line, parsed.prefix_len).max(indent);
            let marker = if remove {
                String::new()
            } else if prefix == "1. " {
                format!("{}. ", index - first + 1)
            } else {
                prefix.to_owned()
            };
            let replacement = format!("{}{marker}{}", &line[..indent], &line[marker_end..]);
            if index == self.cursor_line {
                self.cursor_col = if self.cursor_col >= parsed.prefix_len {
                    indent
                        + marker.chars().count()
                        + self
                            .cursor_col
                            .saturating_sub(parsed.prefix_len.max(indent))
                } else {
                    indent + marker.chars().count()
                };
            }
            self.body[index] = replacement;
        }
        self.selection_anchor = None;
    }

    fn insert_table(&mut self) {
        if self.title_focused {
            return;
        }
        let empty = self.body[self.cursor_line].is_empty();
        let at = self.cursor_line + usize::from(!empty);
        self.body.splice(
            at..at + usize::from(empty),
            [
                "| Column 1 | Column 2 |".to_owned(),
                "| --- | --- |".to_owned(),
                "|  |  |".to_owned(),
                String::new(),
            ],
        );
        self.cursor_line = at;
        self.cursor_col = 2;
        self.selection_anchor = None;
    }

    fn insert_inline_code(&mut self) {
        if self.title_focused {
            return;
        }
        let Some((start, end)) = self.selection_range() else {
            self.wrap_selection("`", "`");
            return;
        };
        if start.0 == end.0 {
            self.wrap_selection("`", "`");
            return;
        }
        for index in start.0..=end.0 {
            let line = &self.body[index];
            let from = if index == start.0 { start.1 } else { 0 };
            let to = if index == end.0 {
                end.1
            } else {
                line.chars().count()
            };
            if from < to {
                self.body[index] = format!(
                    "{}`{}`{}",
                    line.split_at_char_boundary(from).0,
                    char_slice(line, from, to),
                    line.split_at_char_boundary(to).1
                );
            }
        }
        self.cursor_line = start.0;
        self.cursor_col = start.1 + 1;
        self.selection_anchor = None;
    }

    fn insert_code_fence(&mut self) {
        if self.title_focused {
            return;
        }
        let (first, last) =
            self.selection_range()
                .map_or((self.cursor_line, self.cursor_line), |(start, end)| {
                    (
                        start.0,
                        if end.1 == 0 && end.0 > start.0 {
                            end.0 - 1
                        } else {
                            end.0
                        },
                    )
                });
        self.body.insert(last + 1, "```".into());
        self.body.insert(first, "```".into());
        self.cursor_line = first + 1;
        self.cursor_col = 0;
        self.selection_anchor = None;
    }

    fn apply_toolbar(&mut self, action: &str) {
        match action {
            "paragraph" => self.toggle_prefix_on_selection(""),
            "heading2" => self.toggle_prefix_on_selection("## "),
            "heading3" => self.toggle_prefix_on_selection("### "),
            "heading" => self.toggle_prefix_on_selection("# "),
            "bold" => self.wrap_selection("**", "**"),
            "italic" => self.wrap_selection("*", "*"),
            "code" => self.insert_inline_code(),
            "list" => self.toggle_prefix_on_selection("- "),
            "checklist" => self.toggle_prefix_on_selection("- [ ] "),
            "numbered" => self.toggle_prefix_on_selection("1. "),
            "quote" => self.toggle_prefix_on_selection("> "),
            "fence" => self.insert_code_fence(),
            "table" => self.insert_table(),
            "link" => self.wrap_selection("[", "]()"),
            _ => {}
        }
        self.toolbar_menu = None;
        self.request_reveal();
        self.clamp_caret();
        self.persist();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        self.copy_feedback = false;
        if key == "escape" && self.context_menu.take().is_some() {
            cx.notify();
            return;
        }
        if key == "escape" && self.toolbar_menu.take().is_some() {
            cx.notify();
            return;
        }
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;

        let primary_held = match corvo_core::Primary::current() {
            corvo_core::Primary::Command => modifiers.platform,
            corvo_core::Primary::Control => modifiers.control,
        };
        if key != "escape" && !(primary_held && matches!(key, "c" | "w")) {
            self.request_reveal();
        }
        if primary_held {
            match key {
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.insert_text(&text);
                        self.clamp_caret();
                        self.persist();
                        cx.notify();
                    }
                }
                "a" if !self.title_focused => {
                    self.selection_anchor = Some((0, 0));
                    self.cursor_line = self.body.len() - 1;
                    self.cursor_col = self.body[self.cursor_line].chars().count();
                }
                "c" | "x" if !self.title_focused => {
                    if let Some(text) = self.selected_text() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                        if key == "x" {
                            self.delete_selection();
                            self.persist();
                        }
                    }
                }
                "w" => window.remove_window(),
                "b" => self.apply_toolbar("bold"),
                "i" => self.apply_toolbar("italic"),
                _ => {}
            }
            cx.notify();
            return;
        }

        if modifiers.shift {
            match key {
                "left" | "right" | "up" | "down" | "home" | "end" => {
                    if self.selection_anchor.is_none() {
                        self.selection_anchor = Some((self.cursor_line, self.cursor_col));
                    }
                    // Pure movement keys type nothing.
                    if self.title_focused {
                        self.selection_anchor = None;
                        self.handle_title_key(&TypedChar {
                            key: key.to_owned(),
                            glyph: None,
                        });
                    } else {
                        self.handle_body_key(&TypedChar {
                            key: key.to_owned(),
                            glyph: None,
                        });
                    }
                    self.clamp_caret();
                    self.persist();
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        if !modifiers.shift && matches!(key, "left" | "right" | "up" | "down" | "home" | "end") {
            self.selection_anchor = None;
        }
        if !self.title_focused && self.has_selection() && matches!(key, "enter" | "delete") {
            self.delete_selection();
            if key == "delete" {
                self.persist();
                cx.notify();
                return;
            }
        }
        let typed = typed_char(keystroke);
        match key {
            "escape" => {
                if self.has_selection() {
                    self.selection_anchor = None;
                    cx.notify();
                    return;
                }
                window.remove_window();
            }
            "tab" => {
                self.title_focused = !self.title_focused;
                self.selection_anchor = None;
                if self.title_focused {
                    self.title_col = self.title.chars().count();
                } else {
                    self.cursor_line = 0;
                    self.cursor_col = 0;
                }
                cx.notify();
            }
            "backspace" => {
                if self.has_selection() {
                    self.delete_selection();
                    self.persist();
                    cx.notify();
                    return;
                }
                if self.title_focused {
                    self.handle_title_key(&typed);
                } else {
                    self.handle_body_key(&typed);
                }
                self.clamp_caret();
                self.persist();
                cx.notify();
            }
            _ if self.title_focused => {
                self.handle_title_key(&typed);
                self.clamp_caret();
                self.persist();
                cx.notify();
            }
            _ => {
                self.handle_body_key(&typed);
                self.clamp_caret();
                self.persist();
                cx.notify();
            }
        }
    }

    fn handle_title_key(&mut self, typed: &TypedChar) {
        match typed.key() {
            "enter" => {
                self.title_focused = false;
                self.cursor_line = 0;
                self.cursor_col = 0;
            }
            "backspace" => {
                if self.title_col > 0 {
                    self.title_col -= 1;
                    self.title.remove(byte_col(&self.title, self.title_col));
                }
            }
            "delete" => {
                if self.title_col < self.title.chars().count() {
                    self.title.remove(byte_col(&self.title, self.title_col));
                }
            }
            "left" => self.title_col = self.title_col.saturating_sub(1),
            "right" => self.title_col = (self.title_col + 1).min(self.title.chars().count()),
            "home" => self.title_col = 0,
            "end" => self.title_col = self.title.chars().count(),
            _ => {
                if let Some(text) = typed.text() {
                    self.insert_text(text);
                }
            }
        }
    }

    fn handle_body_key(&mut self, typed: &TypedChar) {
        match typed.key() {
            "enter" => {
                let line = self.body.remove(self.cursor_line);
                let (before, after) = line.split_at_char_boundary(self.cursor_col);
                self.body.insert(self.cursor_line, before.to_owned());
                self.body.insert(self.cursor_line + 1, after.to_owned());
                self.cursor_line += 1;
                self.cursor_col = 0;
            }
            "backspace" => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                    let offset = byte_col(&self.body[self.cursor_line], self.cursor_col);
                    self.body[self.cursor_line].remove(offset);
                } else if self.cursor_line > 0 {
                    let line = self.body.remove(self.cursor_line);
                    self.cursor_line -= 1;
                    self.cursor_col = self.body[self.cursor_line].chars().count();
                    self.body[self.cursor_line].push_str(&line);
                }
            }
            "delete" => {
                let count = self.body[self.cursor_line].chars().count();
                if self.cursor_col < count {
                    let offset = byte_col(&self.body[self.cursor_line], self.cursor_col);
                    self.body[self.cursor_line].remove(offset);
                } else if self.cursor_line + 1 < self.body.len() {
                    let next = self.body.remove(self.cursor_line + 1);
                    self.body[self.cursor_line].push_str(&next);
                }
            }
            "left" => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                } else if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.body[self.cursor_line].chars().count();
                }
            }
            "right" => {
                if self.cursor_col < self.body[self.cursor_line].chars().count() {
                    self.cursor_col += 1;
                } else if self.cursor_line + 1 < self.body.len() {
                    self.cursor_line += 1;
                    self.cursor_col = 0;
                }
            }
            "up" => {
                if self.cursor_line == 0 && self.selection_anchor.is_none() {
                    self.title_focused = true;
                    self.title_col = self.title.chars().count();
                } else if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                }
            }
            "down" => {
                if self.cursor_line + 1 < self.body.len() {
                    self.cursor_line += 1;
                }
            }
            "home" => self.cursor_col = 0,
            "end" => self.cursor_col = self.body[self.cursor_line].chars().count(),
            _ => {
                if let Some(text) = typed.text() {
                    self.insert_text(text);
                }
            }
        }
    }

    fn clamp_caret(&mut self) {
        self.title_col = self.title_col.min(self.title.chars().count());
        if self.body.is_empty() {
            self.body.push(String::new());
        }
        self.cursor_line = self.cursor_line.min(self.body.len() - 1);
        self.cursor_col = self
            .cursor_col
            .min(self.body[self.cursor_line].chars().count());
        if self
            .selection_anchor
            .is_some_and(|anchor| anchor == (self.cursor_line, self.cursor_col))
        {
            self.selection_anchor = None;
        }
    }
}

/// What one key press means: the key's name for movement matching and
/// the glyph it types. `key_char` carries shifted letters and symbols;
/// gpui names space "space".
struct TypedChar {
    key: String,
    glyph: Option<String>,
}

impl TypedChar {
    fn key(&self) -> &str {
        &self.key
    }

    fn text(&self) -> Option<&str> {
        self.glyph.as_deref()
    }
}

fn typed_char(keystroke: &gpui::Keystroke) -> TypedChar {
    let key = keystroke.key.clone();
    let glyph = if let Some(ch) = keystroke.key_char.as_deref().filter(|ch| !ch.is_empty()) {
        Some(ch.to_owned())
    } else {
        match key.as_str() {
            "space" => Some(" ".to_owned()),
            other => other.chars().next().and_then(|first| {
                let mut chars = other.chars();
                chars.next();
                chars.next().is_none().then(|| first.to_string())
            }),
        }
    };
    TypedChar { key, glyph }
}

fn byte_col(text: &str, col: usize) -> usize {
    text.char_indices()
        .nth(col)
        .map_or(text.len(), |(offset, _)| offset)
}

fn char_slice(text: &str, from: usize, to: usize) -> &str {
    &text[byte_col(text, from)..byte_col(text, to)]
}

trait CharBoundary {
    fn split_at_char_boundary(&self, col: usize) -> (&str, &str);
}

impl CharBoundary for String {
    fn split_at_char_boundary(&self, col: usize) -> (&str, &str) {
        self.as_str().split_at_char_boundary(col)
    }
}

impl CharBoundary for str {
    fn split_at_char_boundary(&self, col: usize) -> (&str, &str) {
        for (index, (offset, _)) in self.char_indices().enumerate() {
            if index == col {
                return self.split_at(offset);
            }
        }
        self.split_at(self.len())
    }
}

/// The custom topbar: traffic lights left, centered title, and a new
/// note button right. Dragging moves the window; double click zooms.
fn editor_topbar(
    title: &str,
    on_new: impl Fn(&mut NoteEditor, &ClickEvent, &mut Window, &mut Context<NoteEditor>) + 'static,
    cx: &mut Context<NoteEditor>,
) -> Stateful<Div> {
    div()
        .id("note-topbar")
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .h(px(32.0))
        .flex_none()
        .bg(rgb(COLOR_BAR))
        .px(px(84.0))
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(FontWeight::BOLD)
                .min_w(px(0.0))
                .truncate()
                .text_color(rgb(COLOR_DIM))
                .child(if title.is_empty() {
                    "Untitled".to_string()
                } else {
                    format!("{title} — Corvo Notes")
                }),
        )
        .child(
            div()
                .id("note-new")
                .absolute()
                .right(px(14.0))
                .top(px(4.0))
                .flex()
                .items_center()
                .justify_center()
                .w(px(24.0))
                .h(px(24.0))
                .rounded_md()
                .text_size(px(15.0))
                .text_color(rgb(COLOR_DIM))
                .hover(|button| button.text_color(rgb(COLOR_TEXT)).bg(rgba(0xffffff14)))
                .child("+")
                .on_click(cx.listener(on_new)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(
                |_editor: &mut NoteEditor, event: &MouseDownEvent, window, _cx| {
                    if event.click_count >= 2 {
                        window.zoom_window();
                    } else {
                        window.start_window_move();
                    }
                },
            ),
        )
}

struct NoteTooltip(&'static str);
impl Render for NoteTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(5.0))
            .rounded_md()
            .bg(rgb(COLOR_BAR))
            .border_1()
            .border_color(rgb(COLOR_DIVIDER))
            .text_size(px(11.0))
            .text_color(rgb(COLOR_TEXT))
            .child(self.0)
    }
}

fn action_label(action: &str) -> &'static str {
    match action {
        "link" => "Insert link",
        "code" => "Inline code",
        "fence" => "Code block",
        "quote" => "Quote",
        _ => "Format",
    }
}

/// One floating toolbar button, mockup style.
fn floating_button(
    label: &'static str,
    action: &'static str,
    cx: &mut Context<NoteEditor>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("md-float-{action}")))
        .flex()
        .items_center()
        .justify_center()
        .w(px(30.0))
        .h(px(24.0))
        .cursor_pointer()
        .tooltip(move |_, cx| cx.new(|_| NoteTooltip(action_label(action))).into())
        .rounded_md()
        .text_size(px(13.0))
        .font_family("Helvetica")
        .text_color(rgb(COLOR_DIM))
        .hover(|button| button.text_color(rgb(COLOR_TEXT)).bg(rgba(0xffffff14)))
        .child(label)
        .on_click(cx.listener(
            move |editor: &mut NoteEditor, _event: &ClickEvent, _window, cx| {
                editor.apply_toolbar(action);
                cx.notify();
            },
        ))
}

fn toolbar_dropdown(
    label: &'static str,
    id: &'static str,
    menu: ToolbarMenu,
    open: bool,
    cx: &mut Context<NoteEditor>,
) -> Stateful<Div> {
    let mut button = div()
        .id(id)
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .w(px(42.0))
        .h(px(24.0))
        .rounded_md()
        .text_size(px(13.0))
        .cursor_pointer()
        .when(open, |button| button.bg(rgb(COLOR_SELECTION)))
        .tooltip(move |_, cx| {
            cx.new(|_| {
                NoteTooltip(match menu {
                    ToolbarMenu::Headings => "Text style",
                    ToolbarMenu::Styles => "Inline formatting",
                    ToolbarMenu::Lists => "Lists",
                })
            })
            .into()
        })
        .text_color(if open {
            rgb(COLOR_ACCENT)
        } else {
            rgb(COLOR_DIM)
        })
        .hover(|style| style.bg(rgba(0xffffff14)).text_color(rgb(COLOR_TEXT)))
        .child(label)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |editor: &mut NoteEditor, _, _, cx| {
            editor.toolbar_menu = if editor.toolbar_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
            cx.notify();
        }));
    if open {
        let items = menu
            .options()
            .iter()
            .map(|(label, action)| {
                let action = *action;
                div()
                    .id(SharedString::from(format!("note-menu-{action}")))
                    .w_full()
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded_md()
                    .text_size(px(12.0))
                    .text_color(rgb(COLOR_TEXT))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(COLOR_SELECTION)))
                    .child(*label)
                    .on_click(cx.listener(move |editor: &mut NoteEditor, _, window, cx| {
                        cx.stop_propagation();
                        editor.apply_toolbar(action);
                        editor.focus_handle.focus(window, cx);
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        button = button.child(
            div()
                .id(SharedString::from(format!("{id}-menu")))
                .absolute()
                .bottom(px(32.0))
                .left(px(0.0))
                .w(px(174.0))
                .p(px(4.0))
                .rounded_lg()
                .bg(rgb(COLOR_BAR))
                .border_1()
                .border_color(rgb(COLOR_DIVIDER))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(items),
        );
    }
    button
}

fn reveal_offset(
    offset: f32,
    max: f32,
    viewport_top: f32,
    viewport_bottom: f32,
    caret_top: f32,
    height: f32,
) -> f32 {
    let margin = 8.0;
    let delta = if caret_top < viewport_top + margin {
        viewport_top + margin - caret_top
    } else if caret_top + height > viewport_bottom - margin {
        viewport_bottom - margin - caret_top - height
    } else {
        0.0
    };
    (offset + delta).clamp(-max.max(0.0), 0.0)
}

fn code_block_active(
    start: usize,
    end: usize,
    caret: Option<usize>,
    selection: Option<((usize, usize), (usize, usize))>,
) -> bool {
    caret.is_some_and(|line| line >= start && line <= end)
        || selection.is_some_and(|(from, to)| {
            let last = if to.1 == 0 && to.0 > from.0 {
                to.0 - 1
            } else {
                to.0
            };
            from.0 <= end && last >= start
        })
}

fn word_range(chars: &[char], col: usize) -> (usize, usize) {
    if chars.is_empty() {
        return (0, 0);
    }
    let at = col.min(chars.len() - 1);
    let category = |ch: char| {
        if ch.is_alphanumeric() || ch == '_' {
            0
        } else if ch.is_whitespace() {
            1
        } else {
            2
        }
    };
    let kind = category(chars[at]);
    let mut from = at;
    let mut to = at + 1;
    while from > 0 && category(chars[from - 1]) == kind {
        from -= 1;
    }
    while to < chars.len() && category(chars[to]) == kind {
        to += 1;
    }
    (from, to)
}

fn display_source_columns(segments: &[InlineSeg]) -> Vec<usize> {
    let mut columns = Vec::new();
    for segment in segments {
        columns.extend((0..segment.chars()).map(|offset| segment.start + offset));
    }
    columns.push(
        segments
            .last()
            .map_or(0, |segment| segment.start + segment.chars()),
    );
    columns
}

fn source_column_at(text: &str, columns: &[usize], byte: usize) -> usize {
    let displayed_col = text
        .char_indices()
        .take_while(|(offset, _)| *offset < byte)
        .count();
    columns[displayed_col.min(columns.len() - 1)]
}

fn render_segments(
    segments: Vec<InlineSeg>,
    caret_before: Option<usize>,
    base_size: f32,
    source_line: Option<(usize, usize)>,
    reveal: Option<(usize, u64)>,
    cx: &mut Context<NoteEditor>,
) -> Div {
    let columns = display_source_columns(&segments);
    let mut display = String::new();
    let mut highlights = Vec::new();
    let mut fonts = Vec::new();
    let mut links = Vec::new();
    for segment in &segments {
        let start = display.len();
        display.push_str(&segment.text);
        let range = start..display.len();
        if let Some(url) = &segment.link {
            links.push((range.clone(), url.clone()));
        }
        highlights.push((
            range.clone(),
            gpui::HighlightStyle {
                font_weight: segment.bold.then_some(FontWeight::BOLD),
                font_style: segment.italic.then_some(gpui::FontStyle::Italic),
                underline: segment.link.as_ref().map(|_| gpui::UnderlineStyle {
                    thickness: px(1.0),
                    color: None,
                    wavy: false,
                }),
                color: segment
                    .link
                    .as_ref()
                    .map(|_| rgb(0x80c8ef).into())
                    .or_else(|| {
                        segment
                            .color
                            .map(|color| rgb(color).into())
                            .or_else(|| segment.code.then(|| rgb(COLOR_ACCENT).into()))
                    }),
                background_color: if segment.selected {
                    Some(rgb(COLOR_SELECTION).into())
                } else if segment.code {
                    Some(rgb(COLOR_CODE_BG).into())
                } else {
                    None
                },
                ..Default::default()
            },
        ));
        if segment.code {
            fonts.push((range, SharedString::from("Menlo")));
        }
    }
    let caret_byte = caret_before.map(|col| {
        let display_col = columns
            .iter()
            .position(|source| *source >= col)
            .unwrap_or(columns.len() - 1);
        byte_col(&display, display_col)
    });
    let reveal_byte = reveal.map(|(col, token)| {
        let display_col = columns
            .iter()
            .position(|source| *source >= col)
            .unwrap_or(columns.len() - 1);
        (byte_col(&display, display_col), token)
    });
    let empty = display.is_empty();
    let text = StyledText::new(display.clone())
        .with_highlights(highlights)
        .with_font_family_overrides(fonts);
    let layout = text.layout().clone();
    let mut row = div()
        .relative()
        .w_full()
        .min_w(px(0.0))
        .flex_1()
        .text_size(px(base_size))
        .line_height(px(base_size * 1.5))
        .min_h(px(base_size * 1.5))
        .cursor_text()
        .child(text);
    if let Some((line, prefix)) = source_line {
        let down_layout = layout.clone();
        let down_text = display.clone();
        let down_columns = columns.clone();
        let move_layout = layout.clone();
        let down_links = links.clone();
        row = row
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    move |editor: &mut NoteEditor, event: &MouseDownEvent, window, cx| {
                        let offset = if empty {
                            0
                        } else {
                            down_layout
                                .index_for_position(event.position)
                                .unwrap_or_else(|index| index)
                        };
                        let clicked_link = if empty {
                            None
                        } else {
                            down_layout
                                .index_for_position(event.position)
                                .ok()
                                .and_then(|byte| {
                                    down_links
                                        .iter()
                                        .find(|(range, _)| range.contains(&byte))
                                        .map(|(_, url)| url.clone())
                                })
                        };
                        let col = prefix + source_column_at(&down_text, &down_columns, offset);
                        editor.select_pointer_range(
                            line,
                            col,
                            event.click_count,
                            event.modifiers.shift,
                        );
                        if event.click_count == 1 && !event.modifiers.shift {
                            editor.pressed_link = clicked_link.map(|url| (url, event.position));
                        }
                        editor.focus_handle.focus(window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
            )
            .on_mouse_move(cx.listener(
                move |editor: &mut NoteEditor, event: &MouseMoveEvent, _window, cx| {
                    if event.pressed_button == Some(MouseButton::Left)
                        && editor.pointer_anchor.is_some()
                    {
                        if let Some((_, down)) = &editor.pressed_link {
                            if f32::from(event.position.x - down.x).abs() < 4.0
                                && f32::from(event.position.y - down.y).abs() < 4.0
                            {
                                return;
                            }
                        }
                        let offset = if empty {
                            0
                        } else {
                            move_layout
                                .index_for_position(event.position)
                                .unwrap_or_else(|index| index)
                        };
                        let col = prefix + source_column_at(&display, &columns, offset);
                        editor.extend_pointer_selection(line, col);
                        cx.stop_propagation();
                        cx.notify();
                    }
                },
            ));
    }
    if caret_byte.is_some() || reveal_byte.is_some() {
        let editor = cx.entity().downgrade();
        row = row.child(
            gpui::canvas(
                move |bounds, window, _| {
                    let point = |index| {
                        if empty {
                            Some(bounds.origin)
                        } else {
                            layout.position_for_index(index)
                        }
                    };
                    if let Some((index, token)) = reveal_byte {
                        if let Some(position) = point(index) {
                            window.on_next_frame(move |_, cx| {
                                let _ = editor.update(cx, |editor, cx| {
                                    if editor.pending_reveal != Some(token) {
                                        return;
                                    }
                                    if editor.reveal_generation.get() != token {
                                        editor.pending_reveal = None;
                                        return;
                                    }
                                    editor.pending_reveal = None;
                                    let viewport = editor.body_scroll.bounds();
                                    let old = editor.body_scroll.offset();
                                    let next_y = reveal_offset(
                                        f32::from(old.y),
                                        f32::from(editor.body_scroll.max_offset().y),
                                        f32::from(viewport.top()),
                                        f32::from(viewport.bottom()),
                                        f32::from(position.y),
                                        base_size * 1.5,
                                    );
                                    if px(next_y) != old.y {
                                        editor
                                            .body_scroll
                                            .set_offset(gpui::point(old.x, px(next_y)));
                                        cx.notify();
                                    }
                                });
                            });
                        }
                    }
                    caret_byte.and_then(point)
                },
                move |_, position, window, _| {
                    if let Some(position) = position {
                        window.paint_quad(gpui::fill(
                            gpui::Bounds::new(position, gpui::size(px(1.5), px(base_size * 1.5))),
                            rgb(COLOR_ACCENT),
                        ));
                    }
                },
            )
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full(),
        );
    }
    row
}

impl Render for NoteEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.title.clone();
        let title_col = self.title_col;
        let title_focused = self.title_focused;
        let body: std::rc::Rc<Vec<String>> = std::rc::Rc::new(self.body.clone());
        let cursor_line = self.cursor_line;
        let cursor_col = self.cursor_col;
        let selection = self.selection_range();
        let has_selection = self.has_selection();
        let status = self.status.clone();
        let (words, chars) = self.counts();

        // Title block: renders as the note's H1; click to edit.
        let mut title_segments = vec![InlineSeg {
            start: 0,
            text: title.clone(),
            bold: true,
            italic: false,
            code: false,
            selected: false,
            color: None,
            link: None,
        }];
        split_segments_at(&mut title_segments, title_col);
        let title_display = if title.is_empty() && !title_focused {
            div().text_color(rgb(COLOR_FAINT)).child("Untitled")
        } else {
            render_segments(
                title_segments,
                title_focused.then_some(title_col),
                26.0,
                None,
                None,
                cx,
            )
        };
        let title_block = div()
            .id("note-title")
            .w_full()
            .px(px(28.0))
            .pt(px(18.0))
            .pb(px(6.0))
            .flex_none()
            .on_click(
                cx.listener(|editor: &mut Self, _: &ClickEvent, _window, cx| {
                    editor.title_focused = true;
                    editor.toolbar_menu = None;
                    editor.context_menu = None;
                    editor.copy_feedback = false;
                    editor.pointer_anchor = None;
                    editor.title_col = editor.title.chars().count();
                    editor.selection_anchor = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .text_size(px(26.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(COLOR_TEXT))
                    .child(title_display),
            );

        // Body rows: rendered markdown per source line.
        let code_rows = note_syntax::code_rows(&body);
        let table_rows = note_tables::table_rows(&body);
        let rows: Vec<Stateful<Div>> = body
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let table_row = table_rows[index].as_ref();
                let table_active = table_row.is_some_and(|table| {
                    code_block_active(
                        table.start,
                        table.end,
                        (!title_focused).then_some(cursor_line),
                        selection,
                    )
                });
                if let Some(table) = table_row.filter(|_| !table_active) {
                    if matches!(table.kind, TableRowKind::Delimiter) {
                        return None;
                    }
                    let header = matches!(table.kind, TableRowKind::Header);
                    let cells = table
                        .cells
                        .iter()
                        .enumerate()
                        .map(|(column, cell)| {
                            let text = render_segments(
                                parse_inline(&cell.text, None),
                                None,
                                14.5,
                                Some((index, cell.start)),
                                None,
                                cx,
                            );
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .px(px(10.0))
                                .py(px(7.0))
                                .border_r_1()
                                .border_color(rgb(COLOR_DIVIDER))
                                .text_align(match table.align[column] {
                                    TableAlign::Left => gpui::TextAlign::Left,
                                    TableAlign::Center => gpui::TextAlign::Center,
                                    TableAlign::Right => gpui::TextAlign::Right,
                                })
                                .when(header, |cell| cell.font_weight(FontWeight::BOLD))
                                .child(text)
                        })
                        .collect::<Vec<_>>();
                    return Some(
                        div()
                            .id(SharedString::from(format!("note-line-{index}")))
                            .w_full()
                            .flex_none()
                            .px(px(28.0))
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .items_stretch()
                                    .border_l_1()
                                    .border_b_1()
                                    .border_color(rgb(COLOR_DIVIDER))
                                    .when(header, |row| row.border_t_1().bg(rgb(COLOR_BAR)))
                                    .children(cells),
                            ),
                    );
                }
                let code_row = code_rows[index].as_ref();
                let code_active = code_row.is_some_and(|block| {
                    code_block_active(
                        block.start,
                        block.end,
                        (!title_focused).then_some(cursor_line),
                        selection,
                    )
                });
                if let Some(block) = code_row {
                    if !code_active && matches!(block.kind, CodeRowKind::Closing) {
                        return None;
                    }
                    if !code_active && matches!(block.kind, CodeRowKind::Opening) {
                        let label = if block.language.is_empty() {
                            "Code".to_owned()
                        } else {
                            block.language.clone()
                        };
                        return Some(
                            div()
                                .id(SharedString::from(format!("note-line-{index}")))
                                .w_full()
                                .flex_none()
                                .px(px(28.0))
                                .pt(px(7.0))
                                .pb(px(4.0))
                                .bg(rgb(COLOR_CODE_BG))
                                .text_size(px(10.0))
                                .text_color(rgb(COLOR_DIM))
                                .cursor_pointer()
                                .child(label)
                                .tooltip(|_, cx| cx.new(|_| NoteTooltip("Edit code block")).into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |editor: &mut Self,
                                              event: &MouseDownEvent,
                                              window,
                                              cx| {
                                            editor.start_pointer_selection(
                                                index,
                                                editor.body[index].chars().count(),
                                                event.modifiers.shift,
                                            );
                                            editor.focus_handle.focus(window, cx);
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    ),
                                ),
                        );
                    }
                }
                let caret_here = !title_focused && index == cursor_line;
                let parsed = if caret_here || code_row.is_some() || table_active {
                    ParsedLine {
                        kind: LineKind::Paragraph,
                        prefix_len: 0,
                        content: line.clone(),
                    }
                } else {
                    parse_line(line)
                };
                // Selection bounds for this line in content columns.
                let line_sel = selection.and_then(|(start, end)| {
                    if index < start.0 || index > end.0 {
                        return None;
                    }
                    let len = parsed.content.chars().count();
                    let from = if index == start.0 {
                        start.1.saturating_sub(parsed.prefix_len)
                    } else {
                        0
                    };
                    let to = if index == end.0 {
                        end.1.saturating_sub(parsed.prefix_len).min(len)
                    } else {
                        len
                    };
                    (from < to).then_some((from, to))
                });
                let mut segments = if let Some(block) =
                    code_row.filter(|block| matches!(block.kind, CodeRowKind::Content))
                {
                    block
                        .spans
                        .iter()
                        .map(|span| InlineSeg {
                            start: span.start,
                            text: span.text.clone(),
                            bold: false,
                            italic: false,
                            code: false,
                            selected: false,
                            link: None,
                            color: span.tone.map(|tone| match tone {
                                CodeTone::Keyword => 0xc4a5f5,
                                CodeTone::String => 0x86d9a8,
                                CodeTone::Number => 0xf2c57c,
                                CodeTone::Comment => 0x8a929f,
                                CodeTone::Type => 0x80c8ef,
                            }),
                        })
                        .collect::<Vec<_>>()
                } else if caret_here || code_row.is_some() || table_active {
                    vec![InlineSeg {
                        start: 0,
                        text: parsed.content.clone(),
                        bold: false,
                        italic: false,
                        code: false,
                        selected: false,
                        color: None,
                        link: None,
                    }]
                } else {
                    parse_inline(&parsed.content, None)
                };
                if let Some((from, to)) = line_sel {
                    mark_selection(&mut segments, from, to);
                }
                // Caret position in content columns.
                let caret_here = !title_focused && index == cursor_line && !has_selection;
                let caret_col = if caret_here {
                    Some(cursor_col.saturating_sub(parsed.prefix_len))
                } else {
                    None
                };

                let (size, _text_color) = match parsed.kind {
                    LineKind::Heading1 => (24.0, rgb(COLOR_TEXT)),
                    LineKind::Heading2 => (19.0, rgb(COLOR_TEXT)),
                    LineKind::Heading3 => (16.0, rgb(COLOR_TEXT)),
                    LineKind::Quote => (14.0, rgb(COLOR_DIM)),
                    _ => (14.5, rgb(COLOR_TEXT)),
                };

                if let Some(col) = caret_col {
                    split_segments_at(&mut segments, col);
                }
                let content_row = render_segments(
                    std::mem::take(&mut segments),
                    caret_col,
                    size,
                    Some((index, parsed.prefix_len)),
                    self.pending_reveal
                        .filter(|token| {
                            !title_focused
                                && index == cursor_line
                                && self.reveal_generation.get() == *token
                        })
                        .map(|token| (cursor_col.saturating_sub(parsed.prefix_len), token)),
                    cx,
                );

                let mut row = div()
                    .id(SharedString::from(format!("note-line-{index}")))
                    .w_full()
                    .min_w(px(0.0))
                    .flex_none()
                    .px(px(28.0))
                    .when(code_row.is_some(), |row| {
                        row.bg(rgb(COLOR_CODE_BG)).font_family("Menlo")
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(
                            move |editor: &mut Self, event: &MouseDownEvent, window, cx| {
                                let col = editor.body[index].chars().count();
                                editor.select_pointer_range(
                                    index,
                                    col,
                                    event.click_count,
                                    event.modifiers.shift,
                                );
                                editor.focus_handle.focus(window, cx);
                                cx.stop_propagation();
                                cx.notify();
                            },
                        ),
                    )
                    .on_mouse_move(cx.listener(
                        move |editor: &mut Self, event: &MouseMoveEvent, _window, cx| {
                            if event.pressed_button == Some(MouseButton::Left)
                                && editor.pointer_anchor.is_some()
                            {
                                editor.extend_pointer_selection(
                                    index,
                                    editor.body[index].chars().count(),
                                );
                                cx.notify();
                            }
                        },
                    ));

                row = match parsed.kind {
                    LineKind::Heading1 => row
                        .pt(px(16.0))
                        .pb(px(6.0))
                        .font_weight(FontWeight::BOLD)
                        .child(content_row),
                    LineKind::Heading2 => row
                        .pt(px(14.0))
                        .pb(px(4.0))
                        .font_weight(FontWeight::BOLD)
                        .child(content_row),
                    LineKind::Heading3 => row
                        .pt(px(12.0))
                        .pb(px(3.0))
                        .font_weight(FontWeight::BOLD)
                        .child(content_row),
                    LineKind::Bullet => row
                        .pl(px(44.0))
                        .pr(px(28.0))
                        .flex()
                        .items_start()
                        .gap_2()
                        .child(
                            div()
                                .mt(px(8.0))
                                .flex_none()
                                .w(px(5.0))
                                .h(px(5.0))
                                .rounded_full()
                                .bg(rgb(COLOR_ACCENT)),
                        )
                        .child(content_row),
                    LineKind::Numbered(number) => row
                        .pl(px(44.0))
                        .pr(px(28.0))
                        .flex()
                        .items_start()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .min_w(px(20.0))
                                .text_size(px(14.5))
                                .line_height(px(21.75))
                                .text_color(rgb(COLOR_DIM))
                                .child(format!("{number}.")),
                        )
                        .child(content_row),
                    LineKind::Checkbox(checked) => {
                        // Clicking the box flips the source marker.
                        let prefix_len = parsed.prefix_len;
                        row.pl(px(40.0))
                            .pr(px(28.0))
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .id(SharedString::from(format!("note-check-{index}")))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .mt(px(3.0))
                                    .flex_none()
                                    .w(px(17.0))
                                    .h(px(17.0))
                                    .rounded_sm()
                                    .border_1()
                                    .when(checked, |box_el| {
                                        box_el.bg(rgb(COLOR_ACCENT)).border_color(rgb(COLOR_ACCENT))
                                    })
                                    .when(!checked, |box_el| {
                                        box_el
                                            .border_color(rgb(COLOR_FAINT))
                                            .hover(|hover| hover.border_color(rgb(COLOR_ACCENT)))
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .text_color(rgb(COLOR_MENU))
                                    .child(if checked { "✓" } else { " " })
                                    .on_click(cx.listener(
                                        move |editor: &mut Self,
                                              _event: &ClickEvent,
                                              _window,
                                              cx| {
                                            cx.stop_propagation();
                                            let target = if checked { "- [ ] " } else { "- [x] " };
                                            let line = &mut editor.body[index];
                                            let offset = byte_col(line, prefix_len - 6);
                                            line.replace_range(offset..offset + 6, target);
                                            editor.persist();
                                            cx.notify();
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .when(checked, |text| {
                                        text.text_color(rgb(COLOR_FAINT)).line_through()
                                    })
                                    .child(content_row),
                            )
                    }
                    LineKind::Quote => row
                        .pl(px(40.0))
                        .pr(px(28.0))
                        .flex()
                        .items_stretch()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .w(px(3.0))
                                .rounded_full()
                                .bg(rgb(COLOR_ACCENT)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .pt(px(1.0))
                                .text_color(rgb(COLOR_DIM))
                                .child(content_row),
                        ),
                    LineKind::Paragraph => row.py(px(2.0)).child(content_row),
                };
                Some(row)
            })
            .collect();

        // Markdown toolbar: a permanent corvo badge, bottom-left.
        let floating_toolbar = {
            let buttons = vec![
                toolbar_dropdown(
                    "H ⌄",
                    "note-headings",
                    ToolbarMenu::Headings,
                    self.toolbar_menu == Some(ToolbarMenu::Headings),
                    cx,
                ),
                toolbar_dropdown(
                    "I ⌄",
                    "note-styles",
                    ToolbarMenu::Styles,
                    self.toolbar_menu == Some(ToolbarMenu::Styles),
                    cx,
                ),
                floating_button("↗", "link", cx),
                floating_button("</>", "code", cx),
                floating_button("{ }", "fence", cx),
                floating_button("❝", "quote", cx),
                toolbar_dropdown(
                    "☷ ⌄",
                    "note-lists",
                    ToolbarMenu::Lists,
                    self.toolbar_menu == Some(ToolbarMenu::Lists),
                    cx,
                ),
            ];
            div()
                .id("md-floating")
                .h(px(FOOTER_BADGE_HEIGHT))
                .flex_none()
                .flex()
                .items_center()
                .gap_0p5()
                .px(px(4.0))
                .rounded_full()
                .bg(rgb(COLOR_BAR))
                .border_1()
                .border_color(rgb(COLOR_DIVIDER))
                .children(buttons)
        };

        div()
            .track_focus(&self.focus_handle)
            .key_context("NoteEditor")
            .on_key_down(cx.listener(Self::handle_key))
            .on_scroll_wheel(cx.listener(|editor: &mut Self, _, _, _| editor.cancel_reveal()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor: &mut Self, _, _, cx| {
                    let toolbar_open = editor.toolbar_menu.take().is_some();
                    let context_open = editor.context_menu.take().is_some();
                    if toolbar_open || context_open {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|editor: &mut Self, event: &MouseDownEvent, window, cx| {
                    let size = window.viewport_size();
                    editor.context_menu = Some(gpui::point(
                        event.position.x.min((size.width - px(184.0)).max(px(0.0))),
                        event.position.y.min((size.height - px(44.0)).max(px(0.0))),
                    ));
                    editor.toolbar_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|editor: &mut Self, event: &MouseUpEvent, _, cx| {
                    editor.pointer_anchor = None;
                    if let Some((url, down)) = editor.pressed_link.take() {
                        if (f32::from(event.position.x - down.x)).abs() < 4.0
                            && (f32::from(event.position.y - down.y)).abs() < 4.0
                            && !editor.has_selection()
                        {
                            cx.open_url(&url);
                        }
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|editor: &mut Self, _, _, _| {
                    editor.pointer_anchor = None;
                    editor.pressed_link = None;
                }),
            )
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(COLOR_MENU))
            .text_color(rgb(COLOR_TEXT))
            .child(editor_topbar(
                &title,
                |editor: &mut NoteEditor, _event: &ClickEvent, _window, cx| {
                    // The + button clears this editor into a fresh note.
                    if (!editor.title.trim().is_empty()
                        || editor.body.iter().any(|line| !line.trim().is_empty()))
                        && !editor.persist()
                    {
                        cx.notify();
                        return;
                    }
                    editor.note_id = None;
                    editor.cancel_reveal();
                    editor.pressed_link = None;
                    editor.body_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
                    editor.pointer_anchor = None;
                    editor.toolbar_menu = None;
                    editor.context_menu = None;
                    editor.copy_feedback = false;
                    editor.title.clear();
                    editor.title_col = 0;
                    editor.body = vec![String::new()];
                    editor.cursor_line = 0;
                    editor.cursor_col = 0;
                    editor.selection_anchor = None;
                    editor.title_focused = true;
                    editor.status = "Empty — start typing to save".into();
                    cx.notify();
                },
                cx,
            ))
            .child(title_block)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .id("note-body")
                            .flex_1()
                            .min_h(px(0.0))
                            .min_w(px(0.0))
                            .overflow_y_scroll()
                            .on_scroll_wheel(
                                cx.listener(|editor: &mut Self, _, _, _| editor.cancel_reveal()),
                            )
                            .track_scroll(&self.body_scroll)
                            .pb(px(28.0))
                            .flex()
                            .flex_col()
                            .children(rows),
                    )
                    .child(self.scrollbar.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px(px(20.0))
                    .py(px(12.0))
                    .flex_none()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(floating_toolbar)
                            .when(has_selection, |group| {
                                group.child(
                                    div()
                                        .id("note-copy-selection")
                                        .h(px(FOOTER_BADGE_HEIGHT))
                                        .px(px(10.0))
                                        .flex()
                                        .items_center()
                                        .rounded_full()
                                        .bg(rgb(COLOR_BAR))
                                        .border_1()
                                        .border_color(rgb(COLOR_DIVIDER))
                                        .text_size(px(11.0))
                                        .text_color(rgb(COLOR_TEXT))
                                        .cursor_pointer()
                                        .hover(|button| button.bg(rgb(COLOR_SELECTION)))
                                        .child(if self.copy_feedback { "Copied" } else { "Copy" })
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation()
                                        })
                                        .on_click(cx.listener(|editor: &mut Self, _, _, cx| {
                                            editor.copy_selection(cx)
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("note-status")
                            .h(px(FOOTER_BADGE_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2p5()
                            .rounded_full()
                            .bg(rgb(COLOR_BAR))
                            .border_1()
                            .border_color(rgb(COLOR_DIVIDER))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(if status == "Saved" {
                                        rgb(COLOR_ACCENT)
                                    } else {
                                        rgb(COLOR_DIM)
                                    })
                                    .child(status),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(COLOR_FAINT))
                                    .child(format!("{words}w · {chars}c")),
                            ),
                    ),
            )
            .when_some(self.context_menu, |root, position| {
                root.child(
                    div()
                        .id("note-copy-menu")
                        .absolute()
                        .left(position.x)
                        .top(position.y)
                        .w(px(174.0))
                        .p(px(4.0))
                        .rounded_lg()
                        .bg(rgb(COLOR_BAR))
                        .border_1()
                        .border_color(rgb(COLOR_DIVIDER))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .id("note-context-copy")
                                .px(px(10.0))
                                .py(px(7.0))
                                .rounded_md()
                                .text_size(px(12.0))
                                .text_color(if has_selection {
                                    rgb(COLOR_TEXT)
                                } else {
                                    rgb(COLOR_FAINT)
                                })
                                .when(has_selection, |item| {
                                    item.cursor_pointer()
                                        .hover(|item| item.bg(rgb(COLOR_SELECTION)))
                                })
                                .child("Copy selection")
                                .on_click(cx.listener(|editor: &mut Self, _, _, cx| {
                                    editor.copy_selection(cx)
                                })),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_show_labels_and_keep_source_columns() {
        let segments = parse_inline("See [OpenAI](https://openai.com) now", None);
        assert_eq!(
            segments
                .iter()
                .map(|part| part.text.as_str())
                .collect::<String>(),
            "See OpenAI now"
        );
        let link = segments.iter().find(|part| part.link.is_some()).unwrap();
        assert_eq!(link.start, 5);
        assert_eq!(link.link.as_deref(), Some("https://openai.com"));
        assert!(parse_inline("`[x](https://example.com)`", None)
            .iter()
            .all(|part| part.link.is_none()));
    }

    #[test]
    fn incomplete_and_unsupported_links_stay_literal() {
        for text in [
            "[name]()",
            "[name](javascript:alert(1))",
            "[name](https://example.com",
            "[name](file:///tmp/note)",
        ] {
            assert_eq!(
                parse_inline(text, None)
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<String>(),
                text
            );
        }
    }

    #[test]
    fn reveal_scrolls_only_as_far_as_needed() {
        assert_eq!(
            reveal_offset(-100.0, 500.0, 50.0, 350.0, 100.0, 22.0),
            -100.0
        );
        assert_eq!(
            reveal_offset(-100.0, 500.0, 50.0, 350.0, 360.0, 22.0),
            -140.0
        );
        assert_eq!(reveal_offset(-100.0, 500.0, 50.0, 350.0, 30.0, 22.0), -72.0);
        assert_eq!(
            reveal_offset(-490.0, 500.0, 50.0, 350.0, 360.0, 22.0),
            -500.0
        );
    }

    #[test]
    fn code_fences_follow_caret_and_selection() {
        assert!(code_block_active(2, 5, Some(2), None));
        assert!(code_block_active(2, 5, Some(5), None));
        assert!(!code_block_active(2, 5, Some(6), None));
        assert!(!code_block_active(2, 5, None, None));
        assert!(code_block_active(2, 5, None, Some(((0, 0), (3, 2)))));
    }

    #[test]
    fn word_selection_handles_unicode_and_empty_text() {
        assert_eq!(
            word_range(&"hola ágil 🙂".chars().collect::<Vec<_>>(), 6),
            (5, 9)
        );
        assert_eq!(word_range(&[], 0), (0, 0));
        assert_eq!(
            word_range(&"name_value".chars().collect::<Vec<_>>(), 4),
            (0, 10)
        );
    }

    #[test]
    fn display_positions_map_to_markdown_source() {
        let segments = parse_inline("**á🙂** and `code`", None);
        let text = segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<String>();
        let columns = display_source_columns(&segments);
        assert_eq!(text, "á🙂 and code");
        assert_eq!(source_column_at(&text, &columns, 0), 2);
        assert_eq!(source_column_at(&text, &columns, 2), 3);
        assert_eq!(source_column_at(&text, &columns, 6), 6);
        assert_eq!(source_column_at(&text, &columns, text.len()), 16);
    }

    #[test]
    fn empty_display_has_one_source_boundary() {
        assert_eq!(display_source_columns(&[]), vec![0]);
        assert_eq!(source_column_at("", &[0], 0), 0);
    }

    #[test]
    fn unicode_columns_use_character_boundaries() {
        let text = "á🙂z";
        assert_eq!(byte_col(text, 1), 2);
        assert_eq!(byte_col(text, 2), 6);
        assert_eq!(char_slice(text, 1, 2), "🙂");
        assert_eq!(text.split_at_char_boundary(2), ("á🙂", "z"));
    }

    #[test]
    fn caret_splits_source_at_each_character() {
        for col in 0..=9 {
            let mut segments = vec![InlineSeg {
                start: 0,
                text: "**á🙂abc**".into(),
                bold: false,
                italic: false,
                code: false,
                selected: false,
                color: None,
                link: None,
            }];
            split_segments_at(&mut segments, col);
            let mut position = 0;
            let mut found = col == 0;
            for segment in segments {
                position += segment.chars();
                found |= position == col;
            }
            assert!(found, "missing caret at {col}");
        }
    }

    #[test]
    fn inline_code_keeps_emphasis_literal() {
        let segments = parse_inline("`a_b**c`", None);
        assert_eq!(
            segments
                .iter()
                .map(|part| part.text.as_str())
                .collect::<String>(),
            "a_b**c"
        );
        assert!(segments
            .iter()
            .all(|part| part.code && !part.bold && !part.italic));
        assert_eq!(
            parse_inline("unfinished *text", None)
                .iter()
                .map(|part| part.text.as_str())
                .collect::<String>(),
            "unfinished *text"
        );
    }

    #[test]
    fn emphasis_requires_matching_markers() {
        for text in ["*a_", "foo_bar_baz", "* text*"] {
            assert_eq!(
                parse_inline(text, None)
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<String>(),
                text
            );
        }
    }

    #[test]
    fn numbered_items_require_digits() {
        assert!(parse_line(". text").kind == LineKind::Paragraph);
        assert!(parse_line("123456789. text").kind == LineKind::Numbered(123456789));
    }
}

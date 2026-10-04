# Views

An extension does not draw. It returns data in one of the shapes
below, and the launcher renders it — on every platform, identically.
This is the view catalog; each shape maps to the launcher-platform
vocabulary contributors may already know.

The entry point is `Command::page(query) -> Option<PageView>`:
returning `Some` gives the command a dedicated page for
`{id}-page:{filter}` queries (root search routes `{id}:open[:args]`
there); returning `None` keeps the plain results list. Builders live
in [`corvo_ext::pages`](pages.md).

## List — rows from search

The implicit default. `search` returns `SearchResult` rows with
titles, subtitles, icons, accessories, and sections; the launcher
renders, filters, and ranks them. See the [List module](modules/list.md).

## Blocks — composed live pages

A vertical stack of typed blocks, with a refresh cadence. The
pomodoro page is Badge + Hero countdown + Progress + Buttons ticking
every second; the weather page is Badge + Hero + a three-day Strip.

| Block | Renders |
|---|---|
| `Badge` | small status pill with a semantic tone |
| `Hero` | the dominant datum: glyph, headline, one very large value, quiet subtitle |
| `Progress` | determinate bar, 0.0..=1.0 |
| `Markdown` | launcher-subset markdown (headings, lists, bold, italic, code) |
| `Strip` | row of small stat cards |
| `Buttons` | clickable controls; clicks run `execute("{id}:page:{action_id}")` |

`Refresh::Every(secs)` keeps the page alive — the launcher re-asks
`page()` at that cadence while it is open. Keyboard: ←/→ cycles
buttons, Enter presses the focused one.

## Detail — a document

Rendered markdown plus a structured metadata panel (labels, links,
tag rows, separators) — the shape for reference material, error
reports, and read-only summaries.

## Grid — tiles

A wall of tiles: emoji glyphs, color swatches, cached PNG images, or
short text, in 1–8 columns with a per-page search filter. Clicks run
`execute("{id}:grid:{item-id}")`. The emoji picker is the bespoke
ancestor; new grids use this.

## Form — typed input

Single-line text (with password masking), checkboxes, and selects,
with per-field focus cycling (↑/↓) and a submit button. Values
arrive at `execute` percent-encoded in the submit id:
`{id}:page:form:{field=value&...}`. Keep values small and
secret-free.

## Declared commands

Beyond views, an extension's manifest declares its command surface:
title, description, icon, `View` or `NoView` mode, keywords, and
typed arguments (text, password, dropdown) with placeholders. Root
search renders declared commands from the manifest, shows the first
argument's placeholder in the search bar, and hotkeys can execute
`NoView` commands directly without opening the launcher. See the
[manifest module](modules/manifest.md).

## Roadmap

Documented but not yet built: multi-line TextArea, DatePicker,
FilePicker and TagPicker form fields, a push/pop navigation stack,
menu-bar surfaces, and an AI/runtime-tool surface. Ask before
building one by hand — these land in the kit, not in extensions.

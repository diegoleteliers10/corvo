# Pages

The declarative view model: what `Command::page` returns and how the
launcher interprets it. Builders live in `corvo_ext::pages`; the
types live in `corvo_core::page`.

## The page hook

```rust
fn page(&self, query: &str) -> Option<PageView>
```

- Called for `{id}-page:{filter}` queries once the page is open, and
  re-called at the page's [`Refresh`](#refresh) cadence.
- Runs through the launcher's unblock executor, so it may wait on a
  cache fill or one network round trip — but keep it bounded; the
  page cannot render until it returns.
- `query` is the page filter or the arguments typed after
  `{id}:open:`.
- Return `None` to close back to root search.

## Blocks

```rust
use corvo_ext::pages::PageBuilder;

PageBuilder::new("Minutes for a custom focus, or leave empty for 25...")
    .ticking(1)                                   // Refresh::Every(1)
    .badge("FOCUS", Tone::Warning)
    .hero(None, "Focus", "24:59", "Till 15:00", Tone::Warning)
    .progress(0.04, Tone::Warning)
    .buttons(vec![PageButton {
        action_id: "pause".into(),
        label: "Pause".into(),
        tone: Tone::Neutral,
        hotkey: Some("enter"),
    }])
    .build()
```

## Style — the design is the extension's

The launcher's components are generic; the design is yours. Every
block carries an optional `Style`, and every `Some` field overrides
the theme default:

| Field | Overrides |
|---|---|
| `color` | the block's primary text or accent color (sRGB, e.g. `0xff6600`) |
| `background` | the block's background fill |
| `size` | the primary text size in px (badge label, hero value, card value, button label) |
| `glyph_size` | the glyph size in px |
| `width`, `height` | fixed progress-track dimensions |
| `bold` | bold primary text |

Chain `.style(...)` after any builder call to restyle what it just
pushed; strips and button rows cascade into every card and button
inside:

```rust
PageBuilder::new("...")
    .badge("COUNTING DOWN", Tone::Neutral)
    .style(Style {
        color: Some(0x34d399),
        background: Some(0x11342a),
        ..Style::default()
    })
    .hero(None, "New year", "47 days", "2026-12-31", Tone::Accent)
    .style(Style {
        color: Some(0xfbbf24),   // amber value, the extension's call
        size: Some(64),
        ..Style::default()
    })
    .build()
```

`None` fields keep the theme — an extension that never styles looks
native; one that styles everywhere looks like itself.

## Keyboard: the page owns arrow and Enter semantics

On a Blocks page the launcher does not keep its own focus ring — the
command renders the selection and decides what the keys mean:

| Key | Sent to `execute` | Pomodoro | Media |
|---|---|---|---|
| ← | `{id}:page:left` | previous preset | previous track |
| → | `{id}:page:right` | next preset | next track |
| Enter | `{id}:page:enter` | start / pause / resume (the primary action) | play / pause |

The selected item is ordinary styling — a filled chip via `Style`,
not a UI highlight — so there is exactly one "selected" look and it
means what the user thinks. Keys a page does not implement fail
silently (the page just refreshes). Forms keep their own ↑/↓
field focus; Grids filter from the search bar.

## Filters — the second dimension

`.filters()` declares a chip row above the content, the pattern list
extensions use for a second axis (units, category, sort order). The
command owns the selection and re-renders when a chip runs
`{id}:page:filter:{value}`:

```rust
.filters(vec![
    PageFilter::new("c", "°C ●"),
    PageFilter::new("f", "°F"),
])
```

```ignore
if let Some(value) = key.strip_prefix("page:filter:") {
    self.units.store(/* ... */);
    return Ok(Action::ShowToast(String::new())); // silent re-render
}
```

## Row accessories — facts with tooltips

Rows carry one primary `accessory` plus structured facts, each with
its own hover tooltip — the grammar Kill Process uses for CPU and
memory and Hacker News for points and comments:

```rust
ListItem::new("chrome")
    .accessory("12.4%")
    .accessories([
        SearchAccessory::new("1.2 GB").with_tooltip("Memory: 1.24 GiB"),
        SearchAccessory::new("412").with_tooltip("% CPU: 12.40"),
    ])
```

## Recently used

`corvo_ext::recents` backs the "Recently used" section the
most-installed extensions render first:

```rust
// In execute, after acting on an item:
corvo_ext::recents::push(ID, "recent-items", &item_id, 10)?;
// In page()/search(), before everything else:
let recent = corvo_ext::recents::list(ID, "recent-items");
```

## Pushed details — the Show Details pattern

A row's `execute` can push a Detail view onto the launcher's
navigation stack; Escape (or backspace on an empty query) pops back
to the page:

```rust
return Ok(Action::ShowDetail {
    title: format!("{} — {}", item.name, item.date),
    markdown: item.report,
    metadata: vec![
        Metadata::Label { title: "Status".into(), text: "Done".into(), tone: Tone::Positive },
        Metadata::Link { title: "Source".into(), text: item.url.clone(), url: item.url.clone() },
        Metadata::Separator,
    ],
});
```

The content rides in the action (build it from the command's cache —
never fetch inside `execute`).

## Grid sections

Grid items carry an optional `section`; consecutive items sharing one
render under a header with the item count — the Unsplash pattern.
Build them with `pages::grid_sectioned`.

## Buttons and page actions

Every click becomes `execute("{id}:page:{action_id}")` in your
crate — the page never mutates the UI directly. Route them:

```rust
if let Some(action) = key.strip_prefix("page:") {
    let message = run_page_action(action)?;
    return Ok(Action::ShowToast(message));
}
```

Return an empty toast string to act silently; a `toasts` sentence is
the visible confirmation. `action_id` may carry per-render data
(`chip:40`) because the page rebuilds every render.

## Refresh

| Cadence | Use for |
|---|---|
| `Refresh::Manual` | forms, detail documents, grids |
| `Refresh::Every(1)` | countdowns, timers |
| `Refresh::Every(5)` | now-playing surfaces |

The launcher stops ticking the moment the page closes or the
returned cadence changes; an idle launcher pays nothing.

## Grid and Form

```rust
corvo_ext::pages::grid(
    vec![pages::grid_glyph("party", "🥳", "Party")],
    Some(6),          // columns 1..=8
    "Pick a glyph...",
    Refresh::Manual,
)

corvo_ext::pages::form(
    "New event",
    vec![
        pages::text_field("name", "Name", "Ada Lovelace", "", false),
        pages::checkbox("pin", "Pin to top", false),
        pages::select("tone", "Tone", vec![("soft".into(), "Soft".into())], "soft"),
    ],
    "Create",
)
```

Grid clicks: `{id}:page:grid:{item-id}`. Form submits:
`{id}:page:form:{field=value&...}` percent-encoded.

## Manifest and quick commands

Declare the extension identity and command surface with the
`corvo_ext::manifest` builders:

```rust
fn manifest(&self) -> ExtensionManifest {
    manifest::extension(
        "countdown", "Countdown", "Days until a date", icon(),
        &["Productivity"],
        vec![
            manifest::view_command("view", "Countdown",
                "Open the countdown page", &["countdown"])
                .argument(manifest::required_text_argument(
                    "date", "Date: 2026-12-25, Dec 25...")),
            manifest::action_command("saved", "Saved Countdowns",
                "Open the saved strip", &["saved"]),
        ],
    )
}
```

- `View` commands open the page; `NoView` commands act and confirm
  with a toast — including straight from a hotkey
  (`command:<extension>:<name>`), without opening the launcher.
- The first declared argument's placeholder becomes the search bar
  hint when the command is selected in root search.
- Root search and Settings render declared commands from the
  manifest; keep titles and descriptions user-facing.

## Reference implementation

[`commands/countdown`](https://github.com/diegoleteliers10/corvo/tree/main/commands/countdown)
— a live ticking page, saved state, declared arguments, and zero UI
code. The migrated `pomodoro`, `weather`, and `media-control` crates
show the same model over OS state.

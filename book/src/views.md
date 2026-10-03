# Views

A Corvo extension does not draw. It produces data in one of the
shapes below, and the launcher renders it — consistently, on every
platform. This chapter maps the shapes to the vocabulary contributors
may know from other launchers.

## List — the default view

A searchable list of rows. Every command has one: root search
results and every dedicated page are lists.

```rust
ListItem::new("Rust")
    .subtitle("systems language")
    .icon(Icon::Web)
    .accessory("Open")
    .section("Languages")
    .build("example:item:https://rust-lang.org", 700)
```

- `title` + `subtitle` — the row text; subtitles truncate with an
  ellipsis, keep them one line of information.
- `icon` — see [Icons](modules/icons.md).
- `accessory` — right-aligned hint (a unit, a count, a state, or a
  shortcut string like `cmd+c`).
- `section` — group label. Consecutive rows sharing a section render
  under one header, like Formulae and Casks on the Homebrew upgrade
  page.
- `score` — ranking. 700 is the house score for page rows; 1000 for
  keyword-routed openers; fuzzy results get `score + 120`.

Available now for every command. Reference: [List module](modules/list.md).

## Detail — a focused read

For an extension whose value is one thing — a note, a timer, a
forecast — the launcher opens a **dedicated page** instead of a list
row. The page is wired in `corvo-ui` (see
[Navigation](navigation.md)) and the extension supplies data through
its cache:

- Pomodoro: a countdown, progress bar, and transport buttons.
- Weather: current conditions and a three-day strip.
- Media: now playing with transport controls.

A markdown **Detail pane** for arbitrary rows (Raycast's
`List.Item.Detail`) is planned; today, put detail into the subtitle
or a dedicated page.

## Form — input

Two forms exist today: the search bar itself (queries, and command
arguments typed after a keyword) and dedicated input pages (the Notes
editor is the reference: a full editor window with its own state).
Structured Form fields with validation are planned; until then, do
not invent in-row editing.

## Grid — a tile surface

The emoji picker is the grid reference. Grid-as-a-module is planned;
ask before building one.

## Empty and loading states

Every page answers honestly when there is nothing to show:

- After a fetch resolved empty: a hint row
  (`corvo_ext::list::empty_state`) that says what would appear or how
  to enable it — see the browser extensions' "launch with
  `--remote-debugging-port`" hint.
- Before the first fetch lands: the page shows the cached/stale
  content and refreshes when the warm-up completes. Never leave a
  blank list and never fake a spinner row.

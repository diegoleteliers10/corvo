# List

The List module builds the rows every command returns. Everything
lives in `corvo_ext::list`.

## `ListItem`

A row under construction:

```rust
use corvo_ext::list::ListItem;

ListItem::new("Camden Hells")
    .subtitle("0,5 Liter")
    .icon(Icon::Web)
    .accessory("4.6%")
    .section("Lager")
    .build("beer:open:camden-hells", 700)
```

| Method | Field | Notes |
|---|---|---|
| `new(title)` | `title` | the row text |
| `.subtitle(s)` | `subtitle` | one line; truncates with `…` |
| `.icon(i)` | `icon` | see [Icons](icons.md) |
| `.accessory(s)` | `accessory` | right-aligned hint |
| `.section(s)` | `section` | group header, see below |
| `.build(id, score)` | — | finishes the row |

`build` takes the namespaced result id and the score. See
[the command contract](../concepts.md) for the id shapes.

## Sections

Consecutive rows sharing a `section` render under one header. Two
rules keep sections working:

1. **Keep sections contiguous.** Sort by section first, then score:

   ```rust
   results.sort_by(|a, b| {
       a.section.cmp(&b.section)
           .then_with(|| b.score.cmp(&a.score))
           .then_with(|| a.title.cmp(&b.title))
   });
   ```

2. **Name sections like the UI does:** short, upper-case in
   rendering, plural nouns where natural (`Formulae`, `Casks`).

Rows without a section fall into the default group, rendered as
`Results`.

## `open_entry`

The standard opener row that exposes a command's page:

```rust
let row = list::open_entry(
    "weather",
    "Weather",
    "Current conditions and forecast",
    Icon::Svg(phosphor_svgs::style::regular::CLOUD),
    1000,
);
// row.id == "weather:open"
```

The launcher intercepts `{id}:open` to open the page; `execute`'s
`open` arm is the fallback.

## `empty_state`

A quiet hint row for a page with nothing to show:

```rust
let hint = list::empty_state(ID, "No matches — add items in Settings");
```

It scores 1, so any real row outranks it, and it carries the
`{id}:empty` id.

## `fuzzy_open`

The shared fuzzy tail for root search. It scores the query against
your aliases and answers with the opener row when they match:

```rust
list::fuzzy_open(
    trimmed_query,
    &["Weather", "weather forecast temperature clima"],
    &open_entry(0),
)
```

Score math: a fuzzy match yields `match_score + 120`; an exact
keyword hit (routed before calling this) typically passes `1000`.

## Score conventions

| Score | Meaning |
|---|---|
| `1000` | keyword-routed opener or quick command |
| `1010` | live context row above the opener (now playing, timer state) |
| `700` | rows on a dedicated page |
| `score + 120` | fuzzy root matches |
| `1` | empty-state hints |

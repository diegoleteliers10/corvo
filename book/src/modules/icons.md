# Icons

`SearchResult.icon` is a `corvo_core::Icon`. One row, one icon; the
launcher renders it identically everywhere.

## The variants

| Variant | Use for | Example |
|---|---|---|
| `Icon::Svg(data)` | any conceptual glyph, from the bundled Phosphor set | `phosphor_svgs::style::regular::MAGNIFYING_GLASS` |
| `Icon::Image(path)` | a real application's icon, extracted to a cached PNG | browser tabs, app rows |
| `Icon::Glyph(c)` | emoji-as-icon | the emoji picker's numbered rows |
| `Icon::Web` | fallback for web-shaped rows | links, bookmarks |
| `Icon::Link` | bookmarks and quicklinks | |
| `Icon::File`, `Icon::App`, `Icon::Clipboard`, `Icon::Window`, `Icon::System`, `Icon::Calculator`, `Icon::Snippet`, `Icon::Emoji` | generic fallbacks when no Phosphor glyph fits | |

Prefer `Icon::Svg` with a Phosphor name — it scales, themes, and
needs no assets. `phosphor_svgs::style::regular::*` is the standard
weight; browse the names with `cargo doc -p phosphor-svgs --open`.

## Real application icons

When a row represents an installed application, show the
application's icon — `corvo-platform` extracts it to a cached PNG on
every OS:

```rust
let png = corvo_platform::browser_app_icon(
    "Google Chrome.app",                                // macOS bundle
    r"Google\Chrome\Application\chrome.exe",            // Windows exe
    "google-chrome",                                    // Linux .desktop
);
row.icon = png.map(Icon::Image).unwrap_or(Icon::Web);
```

Extraction runs once per icon and is cached on disk, but keep it off
the search path anyway (warm-up task), exactly like the browser
extensions do.

## Rules

- One icon per row; do not encode state in two icons.
- Do not ship bitmap assets in the crate — Phosphor covers glyphs,
  and `Icon::Image` covers real apps.
- Icon meaning is consistent across extensions: `Icon::Link` is
  always a bookmark-ish thing, `Icon::Web` a web page.

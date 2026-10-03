# Navigation

Root search is the home screen. A command that needs more than a
handful of rows gets a **dedicated page**: the user lands there from
the `{id}:open` result and leaves with Escape or backspace.

## Opening a page

The launcher intercepts the opener id before `execute` runs. Add one
branch to the result-activation handler in `crates/corvo-ui`:

```rust
// Every browser extension opens its own page.
for browser in corvo_browser_tabs::BrowserId::ALL {
    if result.id == format!("{}:open", browser.spec().id) {
        self.open_browser_page(browser, window, cx);
        return;
    }
}
```

## The page wiring checklist

`LauncherPage` is an enum in `crates/corvo-ui/src/lib.rs`; a new
variant must touch every site below. Two of them are exhaustive
matches — the compiler finds those. The rest are explicit lists; grep
for an existing variant to catch them all.

1. **The enum variant.** A data-less variant or one carrying a `Copy`
   payload (`LauncherPage::Browser(corvo_browser_tabs::BrowserId)`).
2. **`open_<name>_page`** — clears query, cursor, selection, results,
   and menus; calls the refresh fn; resyncs the palette size.
3. **The init match** in `Launcher::new` (exhaustive).
4. **`refresh_current_page`** (exhaustive).
5. **The escape chain** in `handle_escape` — clear query, else back
   to root.
6. **The backspace-at-empty-query list** — same fallback.
7. **`refresh_<name>`** — spawns the off-path warm-up
   (`smol::unblock(fetch)`), then re-runs the search when it lands.
8. **The apply-back guard** — the spawned task only applies results
   when `search_seq == seq && page == LauncherPage::X`; stale
   responses from another page or query must be dropped.
9. **The placeholder text** in the search row chain.
10. **The render dispatch** — the `is_<name>` flag and the
    `.when(is_<name>, ...)` wiring (search row + subheader + generic
    `results_list`).
11. **The `{id}:open` interception** in the activation handler.
12. **`open_launcher_with_page`** — the external-open match; its
    wildcard falls back to a Root refresh, so a new page needs its
    arm.
13. **A subheader** — the bold, dim section title between search row
    and results.

A page whose extension is not installed must not appear in root
search (see the browser extensions' install check), but the page
itself degrades gracefully: hint row, no crash.

## External entry points

Hotkey intents (`HotkeyIntent::ClipboardHistory`, `EmojiPicker`) and
the command-id mapping in `execute_command_intent` can open pages
directly — for example a global hotkey that lands straight on the
clipboard history. Adding an entry point means one arm there; pages
reachable by hotkey are discoverable in Settings.

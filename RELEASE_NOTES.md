# Corvo 0.5.1

This patch release fixes duplicated entries in the Clipboard History. Pasting an entry with Enter and copying one from the actions menu both end up writing the content to the system pasteboard — that is how the synthetic Cmd+V and the copy action work — and the clipboard watcher used to record that write as a brand-new copy unless the entry already sat at the top of the list. Selecting anything from further down the history therefore grew the list with a second copy of the same content.

## Clipboard

- Pasting an entry into the active app (Enter) no longer duplicates it. The watcher now matches the pasteboard content against the whole history: an existing entry moves to the top of the list with a fresh timestamp instead of a copy being inserted.
- Copy to Clipboard and Copy Image update the list the same way. Re-copying an entry raises it to the top with a new time, so the two actions keep their distinct roles — Enter pastes right away, the menu option leaves the content on the system pasteboard — and neither grows the history twice.
- Promoted entries keep the application they were originally copied from. While a paste is in flight the frontmost app is the paste target, not the origin, and the history no longer misattributes the entry to it.
- Launching Corvo no longer refreshes the top entry's timestamp. Starting the app is not a copy event, so "Today at …" keeps describing the real copy time.
- When the history reaches its configured limit, an evicted entry's saved image file is now deleted on the text path too, leaving no orphaned files behind.

## Platform requirements

- No new permissions or configuration. The fix applies on macOS, Windows, and Linux with the Clipboard extension enabled.

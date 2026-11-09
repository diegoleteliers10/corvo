# Corvo 0.7.0

Clipboard history with search, a dictionary command, and releases that verify
their downloads.

## Clipboard history

- History keeps three months and 2000 entries by default, with a 64 MB byte cap that drops the oldest entries first. Settings still offer 24 hours, 7 days, 30 days, 3 months and 1 year.
- Search the history, copy an entry, or paste it straight back into the previous app. "Direct paste on Enter" decides whether Enter pastes or only copies.
- Copies from Keychain Access, Apple Passwords and common password managers are skipped. On macOS and Windows a copy whose frontmost app cannot be named is not recorded at all, because the guard cannot run.
- On Wayland the source cannot be named, so the entry is stored as `Unknown`. The password-manager guard cannot run there, which means a password copied from a manager may enter the history. That is the accepted trade-off for keeping history on Wayland.

## Define Word and calculator

- `define <word>` looks the word up in the macOS dictionaries. Enter copies the definition and Cmd+Enter opens Dictionary.
- Typing in the launcher no longer queries the dictionary on every keystroke: only the explicit `define` query does, and the fallback row comes from a cache of the last eight definitions.
- The calculator keeps its last 200 evaluations. Type `calculator` to list them, copy a result, paste it, or run the expression again.

## Release pipeline

- Releases now publish a `SHA256SUMS` manifest for every asset, and the build fails when an asset is missing from it. The updater refuses any release without a checksum, which is why 0.6.0 to 0.6.2 could not update themselves.

## Upgrade note

- If you are on 0.6.0 to 0.6.2, an update needs a release that carries the manifest. Nothing else changes.

# Examples in the repo

The best reference is the shipped code. Each extension below is the
canonical example of one pattern — read it before inventing a new
shape.

| Pattern | Read | What to learn |
|---|---|---|
| The standard extension | `templates/extension` | every contract part at once: routing, page, sections, actions, prefs, cache, tests |
| Declarative rich page | `commands/countdown` | manifest with typed arguments, a ticking Blocks page, storage, and zero UI code |
| Network + cache + debounce | `commands/weather` | TTL cache keyed by argument, debounced UI fetch, emoji-coded conditions |
| Dedicated page with state | `commands/pomodoro` + the pomodoro page in `corvo-ui` | a state machine behind a mutex, notifications from a watcher thread, a bespoke page |
| Platform abstraction | `commands/media-control` | a thin crate over `corvo-platform` (AppleScript / SMTC / MPRIS) with one result model |
| Many commands, one crate | `commands/browser-tabs` | a `BrowserSpec` table + a macro generating eight commands with install detection, real icons, and CDP/AppleScript backends |
| Actions menus | `commands/brew` | menu builders, destructive actions, subprocess timeouts |
| Sections in a list | `commands/brew` (upgrades mode) | contiguous sections rendered as Formulae / Casks groups |
| Persistence | `commands/notes` | file-per-item storage, atomic saves, a full editor window |
| Search ranking | `commands/app-launcher` | frecency ranking via the shared search core |

## By module

- `corvo_ext::list` — every crate; start with `templates/extension`.
- `corvo_ext::cache` — `weather`, `media-control`, `browser-tabs`.
- `corvo_ext::prefs` — `templates/extension`; global settings live in
  `corvo-config`.
- `corvo_ext::actions` — `brew`, `text-utilities`, `kill-process`.
- `corvo_ext::routing` — `weather`, `browser-tabs`, `pomodoro`.
- `corvo-platform` subsystems — `media-control` (media),
  `browser-tabs` (browsers/CDP), `kill-process` (processes).

## Not yet covered

- Grid and Form as first-class modules (the emoji picker and the
  notes editor are hand-built precursors).
- Markdown Detail panes per row.
- A secret store for tokens.

If you need one of these, open an issue first — the module should
land in the kit, not in one extension.

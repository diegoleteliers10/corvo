# Extensions Plan — Batch 1

Plan for the first 8 extensions beyond the MVP command set. Reference for
mechanics: `raycast/extensions` repo (paths below). This document proposes
scope and order; nothing is implemented yet.

| # | Extension | Crate | Platforms | Priority |
|---|-----------|-------|-----------|----------|
| 1 | kill-process | `commands/kill-process` | all | P0 |
| 2 | brew | `commands/brew` | macOS only | P0 |
| 3 | text-utilities | `commands/text-utilities` | all | P1 |
| 4 | pomodoro | `commands/pomodoro` | all | P1 |
| 5 | weather | `commands/weather` | all | P2 |
| 6 | notes | `commands/notes` | all | P2 |
| 7 | media-control | `commands/media-control` | all | P2 |
| 8 | browser-tabs | `commands/browser-tabs` | macOS first | P3 |

---

## 1. kill-process — P0

Raycast reference: `extensions/kill-process` (single view command; lists
processes ordered by CPU or memory; kill / force kill).

### Features (MVP)

- List running processes with CPU %, memory, PID, sorted by CPU or memory.
- Fuzzy search over process names (`frizbee`, static-ish corpus).
- Actions: kill (SIGTERM), force kill (SIGKILL), copy PID.
- Exclude corvo's own process from results.

### Mechanics

- Use the `sysinfo` crate (cross-platform, no shell parsing). Refresh on a
  background task every ~2 s; `search` reads the cached snapshot, per the
  non-blocking rule in SPEC §5.1.
- Default sort: CPU. Toggle sort via a query modifier (`sort:mem` prefix) or
  a secondary keyword; decide at implementation time.
- Force kill asks for confirmation (query "confirm" pattern or toast flow).

### Notes

- `kill` on the PID. On Windows, `sysinfo` kill maps to `TerminateProcess`.
- Post-MVP: tree view (kill process group), sort cycling with Tab.

---

## 2. brew — P0, macOS only

Raycast reference: `extensions/brew`. Commands there: installed, search,
outdated, show-vulnerabilities, run-doctor, upgrade (all), manage-services,
clean-up, clear-cache.

### Features (MVP)

- Outdated: list outdated formulae and casks, upgrade selected or all.
- Installed: list installed formulae and casks, with versions.
- Search: search formulae/casks, open the page or install.
- Post-MVP: services (start/stop/restart), clean-up (`brew cleanup`), doctor,
  clear-cache, vulnerabilities (needs Homebrew 7).

### Mechanics

- All through the `brew` CLI, parsed from JSON: `brew outdated --json=v2`,
  `brew info --json=v2 --installed`, `brew search --json=v2 <query>`.
  Mutations (`upgrade <name>`, `install <name>`) run via `Action::RunShell`
  so they appear in the user's own terminal context if launched from there,
  or as awaited background tasks with a toast on completion.
- Cache `outdated` for ~5 minutes; refresh is slow (~seconds).
- macOS-only registration: wrap the `register_command!` call in
  `#[cfg(target_os = "macos")]`. Verify inventory emits nothing when the cfg
  is off (it does not compile the item at all, so this is safe).
- Requires `brew` on PATH; if missing, the command returns an empty result
  set plus a toast hint.

---

## 3. text-utilities — P1

Raycast reference: `extensions/change-case` (case conversions over selected
text or clipboard), plus the encode/decode pattern of `extensions/devutils`.

### Features (MVP)

- Case conversions: upper, lower, title, sentence, camel, pascal, snake,
  kebab, constant, path, dot.
- Encode/decode: Base64, URL, hex.
- JSON: pretty-print and minify (validate + error toast on bad input).
- Generate: UUID v4, UUID v7, random hex string.
- Hash: SHA-256, SHA-1 (via `sha2`); MD5 only for non-security uses.
- Line tools: sort, dedupe, reverse, trim, count chars/words/lines.

### Mechanics

- Input priority: query text after the keyword, else clipboard content.
  Output: `Action::Copy` plus a toast.
- One command crate, one entry point, subactions chosen from the result list
  (same shape Raycast uses: one view listing all conversions).
- Pure functions in a `text` module inside the crate; no state, no deps
  beyond `uuid`, `sha2`, `base64`, `serde_json`, and a small case library
  (or hand-rolled converters).

### Notes

- "Transform selected text in place" needs Accessibility (post-MVP; it is
  the Raycast behavior but requires paste-back machinery corvo does not have
  yet).

---

## 4. pomodoro — P1

Raycast reference: `extensions/pomodoro` (menu-bar timer + control command +
stats; Slack integration skipped).

### Features (MVP)

- Start work interval (default 25 min) and break (default 5 min) from a
  result list or `start 40` syntax with a custom duration.
- Pause, resume, stop, skip to break.
- Live remaining time shown as the result accessory while the timer runs;
  status visible with an empty query.
- OS notification when an interval ends.

### Mechanics

- Needs the first piece of long-lived state in a command. Plan: a
  `TimerService` behind `OnceLock`, owned by the crate, driven by a `tokio`
  task. State machine: `Idle | Working { ends_at } | Paused { remaining } |
  Breaking { ends_at }`. Persist nothing in MVP; survive-until-quit only.
- UI: the palette polls the timer snapshot at ~1 Hz while a timer is active
  (poll only when active; zero cost when idle). This is the only UI change
  in the batch.
- Notifications: new `notify(title, body)` on `PlatformOps`
  (see Cross-cutting).

---

## 5. weather — P2

Raycast reference: `extensions/weather` (view + menu bar, backed by
wttr.in).

### Features (MVP)

- Current conditions plus 3-day forecast for a configured location.
- `weather <city>` overrides the configured location for one query.
- Copy conditions as text.

### Mechanics

- Same backend as Raycast: `wttr.in/<city>?format=j1` (JSON, no API key).
  Cache responses for ~15 minutes in memory; single-flight in-flight
  requests.
- Default location in `settings.toml` under `[weather]` (city and units).
- Parse into a small `Forecast` struct; map condition codes to unicode
  glyphs (no image assets in MVP).
- Post-MVP: menu-bar / status item, geolocation, air quality.

---

## 6. notes — P2

Raycast reference: `extensions/capture-quick-notes` (add, quick capture from
root search, capture clipboard, search/archive/delete).

### Features (MVP)

- New note: type `note <text>` and enter; creates a timestamped markdown
  file.
- Capture clipboard: save current clipboard text as a note.
- Search notes (`frizbee` over titles + content) and open in the default
  editor (`Action::Open`).
- Delete note.

### Mechanics

- Storage: one markdown file per note under
  `data_dir()/notes/<YYYYMMDD-HHMMSS>.md`. Plain files, git-diffable, no
  LMDB (low volume, human-editable — same reasoning as snippets TOML).
- Frontmatter (first 10 lines, `created_at`, optional tags) parsed with
  plain string handling; no YAML crate in MVP.
- Post-MVP: archive instead of delete, tags, pin favorites.

---

## 7. media-control — P2

Raycast reference: no direct equivalent outside `spotify-player` (OAuth).
Corvo does it natively, player-agnostic. system-actions already ships
shell-based play/pause/next/prev on macOS (targets Music, falls back to
Spotify).

### Features (MVP)

- Play/pause, next, previous on the active player.
- "Now playing" result: title, artist, app, shown with an empty query and
  via search.

### Mechanics per OS

| OS | Control + metadata | Notes |
|----|--------------------|-------|
| Linux | MPRIS over D-Bus via `zbus` | Standard, all major players |
| Windows | `GlobalSystemMediaTransportControlsSessionManager` via the `windows` crate | SMTC, async; covers Spotify, browsers |
| macOS | `osascript`/AppleScript to the frontmost known player (Music, Spotify, Safari/Chrome media) | Private `MediaRemote` framework is an option but breaks silently on updates; keep it out of MVP |

### Notes

- New crate `commands/media-control`. Promote the system-actions media rows
  to this crate afterwards, or keep system-actions rows as fallback when the
  extension is disabled — decide during implementation; default: remove from
  system-actions to avoid duplicate results.
- Volume/mute stays in system-actions (sink-level, player-independent).

---

## 8. browser-tabs — P3, macOS first

Raycast reference: `extensions/google-chrome` (search tabs, history,
bookmarks, windows, all-in-one search, new tab/window/incognito).

### Features (MVP, macOS)

- Search open tabs across Chromium browsers (Chrome, Brave, Edge, Arc),
  focus the selected one.
- Search bookmarks (Chromium `Bookmarks` JSON in the profile dir).
- Search history (Chromium `History` SQLite; copy the file to a temp path
  before reading — the live DB is locked).
- Open URL, copy URL. Post-MVP: close tab, new incognito window, Firefox.

### Mechanics

- Tabs: AppleScript per Chromium browser
  (`tell application "Google Chrome" to ... window/tab enumeration`), same
  as Raycast. One `osascript` call per browser, run on the background task,
  cached ~5 s.
- Bookmarks: parse the `Bookmarks` JSON file directly from the profile path
  (`~/Library/Application Support/<browser>/<profile>/`).
- History: `rusqlite` over a copied `History` DB; limit to the last N days.
- Linux/Windows: bookmarks and history are portable (same file formats);
  live tabs are not reliable without CDP (browser started with
  `--remote-debugging-port`). Plan: phase 2 lands bookmarks + history
  everywhere, tabs via CDP where the port is open.

---

## Cross-cutting work (prerequisites)

1. **Notifications on `PlatformOps`** — `notify(title: &str, body: &str)`.
   macOS: `osascript display notification`; Windows: toast via
   `windows` crate or PowerShell fallback; Linux:
   `org.freedesktop.Notifications` via `zbus` (already a dependency for
   MPRIS). Needed by pomodoro first.
2. **cfg-gated command registration** — pattern for macOS-only crates
   (`brew`): `#[cfg(target_os = "macos")]` around the `register_command!`
   call. Document in `AGENT.md`.
3. **1 Hz accessory refresh in corvo-ui** — poll active services (pomodoro)
   only while they are active; idle cost must be zero.
4. **Background snapshot pattern** — kill-process, browser-tabs, weather all
   use: background task refreshes a cache, `search` reads the cache. Extract
   a tiny helper in `corvo-core` if the third one repeats the first two.

## Implementation order

1. kill-process (no prerequisites, highest demand)
2. brew (independent; needs cfg-gating pattern)
3. text-utilities (independent)
4. notifications + pomodoro (notifications land first)
5. weather
6. notes
7. media-control (zbus/windows deps land here)
8. browser-tabs (largest surface, macOS first, phase 2 for Linux/Windows)

## Decisions to confirm

- kill-process sort toggle interaction (prefix modifier vs keyword vs Tab).
- Remove media rows from system-actions when media-control lands, or keep
  both behind settings.
- Pomodoro: persist sessions for stats (post-MVP file) or skip stats
  entirely.
- Weather: single configured city enough, or a city search over the
  Open-Meteo geocoding API (also keyless)?

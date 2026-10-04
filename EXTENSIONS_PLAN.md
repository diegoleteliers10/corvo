# Extensions plan

Release target: Corvo 0.4.0. This file records implemented behavior and
remaining work. Native command crates compile into the application.

## Platform support

| Extension | macOS | Windows | Linux |
|---|---|---|---|
| kill-process | Available | Available | Available |
| brew | Available | Unavailable | Unavailable |
| text-utilities | Available | Available | Available |
| pomodoro | Available | Available | Available |
| weather | Available | Available | Available |
| notes | Available | Available | Available |
| media-control | Spotify and Music | SMTC | MPRIS |
| chrome | Available | CDP port 9222 | CDP port 9222 |
| brave | Available | CDP port 9224 | CDP port 9224 |
| edge | Available | CDP port 9223 | CDP port 9223 |
| firefox | Available (AppleScript) | Planned | Planned |
| arc | Available (AppleScript) | CDP port 9225 | CDP port 9225 |
| dia | Available (AppleScript) | CDP port 9226 | CDP port 9226 |
| safari | Available | Unavailable | Unavailable |
| aside | Available | CDP port 9227 | CDP port 9227 |

Media Control reads Windows SMTC through PowerShell and Linux MPRIS
through zbus. Browser extensions register on every OS but appear only
when their browser is installed. Live tabs on macOS go through
AppleScript; on Windows and Linux a Chromium browser answers CDP when
it runs with `--remote-debugging-port` on its port from the table.
Cross-platform registration does not confirm runtime behavior on
each OS.

## 1. kill-process

- List processes with CPU use, memory use, and PID. Exclude Corvo itself.
- Search names with fuzzy matching. Use `sort:mem` to sort by memory.
- Terminate a process, copy its PID, or request force termination.
- Require confirmation before force termination.
- Use the platform process snapshot and process termination APIs.

Remaining work: process trees and process group termination.

## 2. brew

- Show installed and outdated formulae and casks.
- Search formulae and casks. Install or upgrade a selected package.
- Upgrade all packages, remove a package, and manage services.
- Run cleanup, clear the cache, and open package information.
- Read Homebrew JSON for package details and services. Parse the text
  output of scoped formula and cask searches.
- Cache query results. Invalidate the cache after mutations.
- Limit queries to 60 s and package operations to 15 min.

Requires Homebrew on macOS. Vulnerability reports remain planned.

## 3. text-utilities

- Convert case, encode and decode Base64, URLs, and hex.
- Format or minify JSON. Generate UUID v4, UUID v7, and random hex.
- Calculate hashes and apply line tools.
- Use query text or clipboard text. Copy the output through the launcher.

Remaining work: replace selected text in another application.

## 4. pomodoro

- Start a 25 min focus or a 5 min break. Accept custom focus durations
  from 1 min to 600 min, for example `pomodoro 40`.
- Pause, resume, stop, or skip an interval.
- Show the countdown and progress on a dedicated page.
- Advance from focus to break, then stop after the break.
- Change timer state under one lock. Send notifications after the state change.
- Refresh the page once per second while the timer runs.
- A timer watcher checks the state every 250 ms after the first start.

Timer state lasts until Corvo exits. Session history and statistics remain planned.

## 5. weather

- Show current conditions and a three-day forecast from wttr.in.
- Accept `weather <city>`. Without a city, wttr.in uses the request IP address.
- Show temperature in degrees Celsius and wind speed in km/h.
- Cache the last city response for 15 min. Limit network requests to 10 s.
- Fetch data outside the search path. Copy current conditions as text.

Requires network access. Configured cities, unit preferences, request
coalescing, air quality, and a menu bar item remain planned.

## 6. notes — P2

### Implemented

- Open new and saved notes in a native Corvo editor window.
- Activate Corvo and focus the editor when the window opens.
- Save edits automatically through a temporary file and atomic replacement.
- Keep one editor per saved note. Focus that window when the note opens again.
- Show save status and word and character counts.
- Search titles and content. Copy or delete saved notes from the Notes page.
- Render headings, bold, italic, inline code, lists, checklists, and quotes.
- Keep the active source line editable. Render Markdown on inactive lines.
- Use compact toolbar menus for paragraphs, H1–H3, inline styles, and lists.
- Insert links, inline code, fenced code blocks, quotes, and tables.
- Show supported links as labels with color and an underline. Open a link
  after a click release without a drag. Accept HTTP, HTTPS, and mailto URLs.
- Show fenced code blocks with a full-width background and a monospace font.
  Hide fence markers outside the active block. Show a clickable language label.
- Read the language from the opening fence. Color TypeScript, JavaScript,
  Rust, Python, JSON, SQL, and shell code. Accept common language aliases.
  Use plain code text for unknown languages.
- Render Markdown tables with headers, borders, and column alignment.
  Add **Table** to the **Lists** menu. Show source rows while a table is active.
- Wrap long text, including words without spaces, within the editor width.
- Paint one caret without space in the text layout, including empty lines.
- Use character boundaries for edits, selections, accented text, and emoji.
- Preserve line breaks when text enters through the clipboard.
- Select text with a drag, Shift+click, or Shift+arrow keys.
  Double-click selects a word. Triple-click selects a source line.
- Copy a selection with the toolbar button, the context menu, or the keyboard.
- Follow the caret and selection head after text edits or keyboard movement.
  Cancel queued caret scroll after manual scroll input.
- Show a scrollbar when content exceeds the viewport. Support track clicks
  and thumb drag, including drag outside the track.
- Add tooltips, pointer feedback, and active states to toolbar controls.

### Mechanics

- Storage: one Markdown file per note under `data_dir()/notes/<slug>.md`.
  Derive the slug from the title. Add a suffix when a file name already exists.
  Update the file name when the title changes.
- Frontmatter stores `title` and `created_at`. Plain string parsing handles
  missing or empty metadata. The note body keeps its Markdown source.
- `commands/notes/src/lib.rs` owns storage, search, and note actions.
- `crates/corvo-ui/src/note_editor.rs` owns editor state, input, and rendering.
- `crates/corvo-ui/src/note_syntax.rs` parses fences and code color spans.
- `crates/corvo-ui/src/note_tables.rs` parses table rows and cell alignment.
- `crates/corvo-ui/src/note_scrollbar.rs` owns scrollbar geometry and pointer input.
- One `ScrollHandle` connects the body and scrollbar. A request token cancels
  stale caret scroll after a new edit or manual scroll input.
- Markdown, syntax colors, and tables use small parsers without new dependencies.
  They do not provide complete CommonMark or language grammar support.

### Next work

- Undo and redo.
- Search within the current note.
- Continue lists and checklists after Enter.
- Add strikethrough, a horizontal rule, and a remove-format action.
- Improve keyboard navigation and accessibility for toolbar menus.
- Expand Markdown parsing and language-specific syntax support.
- Add archive, tags, and pinned notes.

## 7. media-control

- Show the track title, artist, player, and playback state.
- Toggle playback, advance to the next track, or return to the previous track.
- Accept root commands `play`, `pause`, `next`, and `prev`.
  Both `play` and `pause` toggle playback.
- macOS: use AppleScript for Spotify, then Music, if those applications run.
- Windows: read the current SMTC session through PowerShell. Chrome,
  Edge, Brave, and other browsers register SMTC sessions, so browser
  playback works there too.
- Linux: read MPRIS players on the session bus through zbus. Prefer
  the player reporting `Playing`, else the first by bus name.
- Cache track data for 5 s. Clear the cache after a playback command.
- The OS backends live in `corvo-platform`; this crate only caches and
  renders.

Volume and mute stay in System Actions.

## 8. One extension per browser

Chrome, Brave, Edge, Firefox, Arc, Dia, Safari, and Aside each get
their own command crate entry, launcher page, and real application
icon. The former unified Browser Tabs page is gone.

- Each extension appears in root search only while its browser is
  installed on the current OS. The install check is a filesystem stat,
  cached per process.
- Each page lists that browser's open tabs and, while a filter is
  typed, that browser's bookmarks. Tab rows carry the browser's real
  app icon; bookmark rows use the link icon.
- `commands/browser-tabs/src/lib.rs` owns the `BrowserSpec` table and
  the eight `Command` impls generated by one macro.
  `commands/browser-tabs/src/bookmarks.rs` owns the bookmark readers.
- macOS: tabs and focus go through AppleScript. Chromium browsers take
  `set active tab index of window`; Safari takes
  `set current tab of window`. URLs pass as AppleScript argument data.
- Windows and Linux: tabs come from the browser's CDP HTTP endpoint.
  The browser must run with `--remote-debugging-port` set to its port
  from the platform table (Chrome 9222, Edge 9223, Brave 9224, Arc
  9225, Dia 9226, Aside 9227). A browser that does not answer its
  port shows a hint row instead of looking broken.
- Bookmarks: Chromium browsers answer the `Bookmarks` JSON in their
  Default profile under `browser_data_candidates` for each OS. Safari
  answers `Bookmarks.plist` through `plutil -convert json`, which may
  need Full Disk Access and degrades to no bookmarks.
- Cache open tabs for 5 s. Run browser enumeration and icon
  extraction outside the search path.

Remaining work: Firefox bookmarks (places.sqlite), Arc bookmarks (its
own store), history on every platform, tab closure, and Firefox live
tabs on Windows and Linux (Firefox removed its CDP endpoint).

## Shared changes

- Add the declarative extension platform: manifests with declared
  commands and typed arguments, PageView pages (Blocks, Detail,
  Grid, Form) with refresh cadences, quick commands on hotkeys, and
  a generic page interpreter in corvo-ui.
- Add dedicated launcher pages for the new commands.
- Add native note editor windows with Markdown previews, syntax colors,
  tables, selection, and a scrollbar.
- Move media playback into `corvo-platform` (`media_now_playing`,
  `media_transport`) and add per-browser primitives (`browser_tabs`,
  `focus_browser_tab`, `browser_app_icon`, `browser_app_installed`).
- Send macOS notifications through AppleScript, Windows notifications
  through encoded PowerShell, and Linux notifications through `notify-send`.
- Use a per-user IPC socket on Unix. Keep fallback access to the legacy socket.
- Preserve unreadable settings files. Cache settings by file time and size.
- Open Windows URLs with `ShellExecuteW`.
- Respect Windows clipboard concealment markers.
- Apply file search exclusions through native path components on each OS.
- Encode quicklink arguments before URL substitution. Append an argument
  when the URL has no placeholder.

## Verification

- Local macOS workspace checks and tests cover command routing, timer state,
  note storage, editor helpers, Markdown parsing, and platform helpers.
- CI checks macOS Intel, Windows x64, and Linux x64. Release builds also
  include macOS Apple Silicon. Native runtime checks remain necessary.
- On macOS, test note focus, selection, links, tables, code blocks, and scrollbar
  input. Test browser Automation permission and Spotify and Music playback.
- On Windows, test note filenames, Unicode input, URL opening, clipboard
  concealment, and notification delivery with the installed application.
- On Linux, test X11 and Wayland input, note windows, clipboard access,
  and notifications. Notifications require `notify-send` and a desktop service.

## Next priorities

1. Complete native UI checks on all supported systems, including the
   new browser pages and Windows SMTC and Linux MPRIS playback.
2. Add Firefox and Arc bookmark readers, history, and tab closure.
3. Add note undo and redo, in-note search, and toolbar keyboard navigation.

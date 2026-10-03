# Corvo

A native application launcher for macOS, Linux, and Windows. Written in Rust with GPUI.

## Features

- **Application Launcher**: Start applications with fast search and frecency ranking.
- **Window Management**: Tile windows into halves, thirds, and quarters. Configure window gaps and window layout templates.
- **System Actions & Settings**: Control system settings, dark mode, audio, and displays.
- **Clipboard History**: Store and search text, code, colors, and images.
- **Calculator**: Evaluate math expressions and view calculation history.
- **Snippets**: Insert stored text templates with custom keywords.
- **Emoji Picker**: Search and copy emojis with skin-tone selection.
- **Quicklinks**: Open custom URLs and web searches.
- **File Search**: Search files with configurable scopes and ignore rules.
- **Auto-Updater**: Automatic update checks with cryptographic signature verification.

- **Text Utilities**: Convert case, encode text, format JSON, generate UUIDs, and calculate hashes.
- **Pomodoro**: Start focus and break timers with desktop notifications.
- **Weather**: Get current conditions and a three-day forecast from wttr.in.
- **Notes**: Edit local Markdown notes with autosave, tables, code colors, and text selection.
- **Process Control**: Find and terminate processes on macOS, Windows, and Linux.
- **Homebrew**: Manage packages and services on macOS.
- **Media Control**: Play/pause and skip on macOS (Spotify, Music), Windows (System Media Transport Controls), and Linux (MPRIS) — browser playback included.
- **Per-browser extensions**: Chrome, Brave, Edge, Firefox, Arc, Dia, Safari, and Aside each get tabs, bookmarks, and their real app icon wherever the browser is installed.

See [the extensions plan](EXTENSIONS_PLAN.md) for platform limits and remaining work.

## Extensions

Corvo extensions are native Rust crates compiled into the launcher — no plugins, no scripts. The guide to building one, including the module reference (`list`, `actions`, `cache`, `preferences`) and the pull-request standard, lives at **[the extension book](https://diegoleteliers10.github.io/corvo/)** (source in [`book/`](book/src/SUMMARY.md)).

Quick start: copy [`templates/extension/`](templates/extension/) into `commands/`, rename it, and follow the checklist in its header. If a coding agent writes it for you, paste [`PROMPT.md`](PROMPT.md) into the session first.

## Contributing

Pull requests welcome — extension changes are reviewed against the checklist in [CONTRIBUTING.md](CONTRIBUTING.md): tests, OS parity, no blocking on the search path, and honest empty states. The [agent prompt](PROMPT.md) and the book's [examples page](book/src/examples.md) cover the rest.

## Installation

### macOS (Homebrew)

```bash
brew install diegoleteliers10/corvo/corvo
```

### Manual Downloads

Download installers and packages from [Releases](https://github.com/diegoleteliers10/corvo/releases):
- macOS: `.dmg` installer for Apple Silicon and Intel.
- Linux: `.AppImage` or `.deb` package.
- Windows: `.msi` installer or portable `.zip`.

## Build from Source

### Requirements

- Rust 1.80 or newer
- On Linux: Wayland development packages (`wayland-protocols`, `libxkbcommon`)

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release
```

The first instance stays resident in memory. Run the binary again to toggle the window. Default shortcut: `Alt+Space` (`⌥ Space` on macOS). On Linux, bind `corvo --toggle` in your compositor.

## Project Structure

- `crates/corvo-core`: Command trait and link-time registry.
- `crates/corvo-ext`: Extension kit — list, actions, cache, preferences, routing, feedback.
- `crates/corvo-platform`: Operating system APIs, global shortcuts, and window operations.
- `crates/corvo-ui`: GPUI interface, launcher palette, and settings window.
- `crates/corvo-config`: Configuration models, file persistence, and LMDB storage.
- `commands/*`: Built-in command crates.
- `book/`: The extension documentation (published on GitHub Pages).

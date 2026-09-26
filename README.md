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
- `crates/corvo-platform`: Operating system APIs, global shortcuts, and window operations.
- `crates/corvo-ui`: GPUI interface, launcher palette, and settings window.
- `crates/corvo-config`: Configuration models, file persistence, and LMDB storage.
- `commands/*`: Built-in command crates.

# Corvo — Project Spec

A Raycast-equivalent launcher written in Rust. Native, no runtime plugin
system, targeting Linux, Windows, and macOS. Design goal: as light and
fast as a launcher can be, with memory footprint as a first-class
constraint on every decision below.

This document is the source of truth for scope and architecture. See
`AGENT.md` in the repo root for the contributor workflow an AI coding
agent should follow when implementing pieces of this spec.

---

## 1. Goals

- Lightweight, fast, low memory footprint, on par with or better than
  native launchers (rofi/wofi-class), not Electron-class
- Feature parity with Raycast's core commands, not its full extension
  ecosystem
- Cross-platform: Linux, Windows, macOS
- Extensible only via native Rust code, contribution friction lowered
  through AI-agent-assisted development rather than a runtime plugin API

## 2. Non-goals (for now)

- WASM/scripted third-party extension runtime
- Raycast config/extension import (explicitly deferred, not MVP)
- Auto-paste into the previously focused app (`Action::PasteToActiveApp`)
- File-type clipboard entries (`CF_HDROP` / `file://` / `NSFilenamesPboardType`)
- Flatpak / MSI installer / notarized macOS build — portable/AppImage/ad-hoc
  signed builds are enough until there's a reason to distribute beyond
  personal use and early testers

## 3. Tech stack

| Layer | Choice | Why |
|---|---|---|
| UI / windowing | GPUI (Zed's UI framework) | Native performance, first-class on macOS, has Linux `layer_shell` support and a Windows backend |
| Fuzzy search / ranking | `fff-search` (built on `frizbee`, SIMD matcher) | Shared matching engine across commands, frecency ranking via LMDB out of the box |
| Config storage | TOML files | Human-editable, git-diffable, fits low-volume settings/snippets/quicklinks |
| Data storage | LMDB via `heed` | Reused from `fff-search`'s own dependency, avoids adding a second embedded DB just for clipboard history |
| Path resolution | `directories` crate | Correct per-OS config/data dirs without hand-rolled logic |
| Clipboard access | `arboard` | Cross-platform text/image clipboard read/write |
| Compile-time command registration | `inventory` | No central list of commands to maintain/merge-conflict on |

## 4. Workspace structure

```
corvo/
├── Cargo.toml                 # workspace root
├── crates/
│   ├── corvo-core/            # Command trait, CommandRegistry, SearchResult, Action
│   ├── corvo-platform/        # PlatformOps trait + linux.rs / windows.rs / macos.rs
│   ├── corvo-ui/              # GPUI window (layer_shell / popup / native), rendering, theming
│   ├── corvo-config/          # settings/snippets/quicklinks TOML, path resolution, frecency+clipboard LMDB access
│   └── commands/
│       ├── app-launcher/
│       ├── file-search/
│       ├── clipboard-manager/
│       ├── calculator/
│       ├── snippets/
│       ├── window-management/
│       ├── emoji-picker/
│       ├── quicklinks/
│       ├── system-actions/
│       └── web-search-fallback/
└── src/main.rs                 # binary entrypoint, wires registry + UI, never edited to add a command
```

Each `commands/*` crate depends only on `corvo-core` (and `corvo-platform`
if it needs OS-level actions). None depend on `corvo-ui`.

## 5. Core abstractions

### 5.1 `Command` trait

```rust
#[async_trait::async_trait]
pub trait Command: Send + Sync {
    fn id(&self) -> &'static str;
    fn keywords(&self) -> &[&'static str] { &[] }
    fn prefix(&self) -> Option<&'static str> { None }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult>;
    async fn execute(&self, result_id: &str, ctx: &ExecutionContext) -> Result<Action>;

    fn priority(&self) -> u8 { 50 }
}

pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Icon,
    pub score: f32,
    pub accessory: Option<String>,
}

pub enum Action {
    Open(PathBuf),
    Copy(String),
    RunShell(String),
    ShowToast(String),
    CloseWindow,
}
```

- `search` must stay non-blocking; slow work happens on a background task
  feeding a cache, `search` reads the cache.
- Static search spaces (`emoji-picker`, `system-actions`) call `frizbee`
  directly for matching. Commands with real corpora (`app-launcher`,
  `file-search`) use `fff-search`'s full `FilePicker`/frecency machinery.

### 5.2 Registration (no central list)

```rust
// corvo-core
pub struct CommandDescriptor { pub factory: fn() -> Box<dyn Command> }
inventory::collect!(CommandDescriptor);

#[macro_export]
macro_rules! register_command {
    ($ty:ty) => {
        inventory::submit! {
            $crate::CommandDescriptor { factory: || Box::new(<$ty>::default()) }
        }
    };
}
```

```rust
// commands/calculator/src/lib.rs
pub struct CalculatorCommand;
impl Command for CalculatorCommand { /* ... */ }
corvo_core::register_command!(CalculatorCommand);
```

Adding a crate to `commands/` and one dependency line in the binary's
`Cargo.toml` is the entire integration surface — `main.rs` logic doesn't
change.

### 5.3 `PlatformOps` — the only place with `#[cfg(target_os = ...)]`

```rust
pub trait PlatformOps: Send + Sync {
    fn lock(&self) -> Result<()>;
    fn sleep(&self) -> Result<()>;
    fn shutdown(&self) -> Result<()>;
    fn list_windows(&self) -> Result<Vec<WindowHandle>>;
    fn focus_window(&self, handle: &WindowHandle) -> Result<()>;
}
```

Implemented once per OS in `corvo-platform/src/{linux,windows,macos}.rs`.
Commands never call raw OS APIs directly.

## 6. Window & UI

- Centered floating panel, ~640px wide, search input on top, scrollable
  result list below (icon + title + subtitle/accessory)
- **Linux (Wayland):** `WindowKind::LayerShell`, no anchor (centers),
  `KeyboardInteractivity::OnDemand` (needs real keyboard focus for the
  search box, unlike a purely decorative overlay)
- **Windows:** borderless always-on-top popup window, centered on the
  active monitor at runtime
- **macOS:** GPUI's native windowing, no special-casing needed, this is
  GPUI's original/most mature platform

### Hotkey / toggle model

- Single-instance binary with Unix-socket (Linux/macOS) or named-pipe
  (Windows) IPC. Invoking the binary again toggles the resident instance.
- **Windows:** native global hotkey via `RegisterHotKey` (the `global-hotkey`
  crate), works out of the box.
- **macOS:** native global hotkey is available too (Carbon/Cocoa APIs),
  same `global-hotkey` crate covers it.
- **Linux/Wayland:** no standard global-hotkey mechanism exists at the
  app level. The user binds the shortcut in their compositor/WM (Sway,
  Hyprland, GNOME custom shortcut) to run `corvo --toggle`, which hits
  the IPC socket. This sidesteps the Wayland global-shortcuts portal,
  which isn't implemented consistently across compositors.

## 7. Config & data persistence

Path resolution via `directories::ProjectDirs`:

| What | Where | Format |
|---|---|---|
| Settings (hotkey, theme, enabled commands, onboarding flags) | `config_dir()/settings.toml` | TOML |
| Snippets | `config_dir()/snippets.toml` | TOML |
| Quicklinks | `config_dir()/quicklinks.toml` | TOML |
| Frecency (app-launcher, file-search) | `data_dir()/frecency/` | LMDB via `fff-search` |
| Clipboard history metadata | `data_dir()/clipboard.mdb` | LMDB via `heed` |
| Clipboard images (full + thumbnail) | `data_dir()/clipboard/images/<id>.png` | files on disk, referenced from LMDB |

`corvo-config` is the only crate that knows about paths and file
formats; other crates ask it for data, they don't touch the filesystem
directly.

## 8. Clipboard manager

```rust
pub struct ClipboardEntry {
    pub id: Uuid,
    pub captured_at: DateTime<Utc>,
    pub source_app: Option<String>,
    pub kind: ClipboardKind,
}

pub enum ClipboardKind {
    Text { content: String, looks_like_code: bool },
    Image { thumbnail_path: PathBuf, full_path: PathBuf, width: u32, height: u32 },
}
```

**Listening for changes** (`ClipboardWatcher` in `corvo-platform`, one
impl per OS):

| OS | Mechanism |
|---|---|
| Windows | Push, native. `AddClipboardFormatListener` on a message-only window, reacts to `WM_CLIPBOARDUPDATE` |
| Linux / X11 | Push, native. XFixes selection-notify (`XFixesSelectSelectionInput`) |
| Linux / Wayland | Push where available: `wlr-data-control-unstable-v1` on wlroots compositors (Sway, Hyprland). GNOME/KDE don't implement it — polling fallback (~250-300ms) there |
| macOS | Polling only. Apple provides no push notification; `NSPasteboard.changeCount` polling at ~250-300ms is what Raycast/Alfred do too |

Polling, where unavoidable, runs on a low-priority background thread.

**Rules:**
- MVP content types: plain text and image (PNG). File-drag clipboard
  entries are out of scope for MVP (see §2).
- Respect "don't save this" flags: `org.nspasteboard.ConcealedType`
  (macOS), `CF_EXCLUDECLIPBOARDCONTENTFROMMONITORPROCESSING` (Windows),
  `x-kde-passwordManagerHint` (KDE). Non-negotiable, this is table
  stakes for a clipboard manager.
- Dedup consecutive identical content via hash before persisting.
- Retention configurable in `settings.toml`
  (`[clipboard] max_entries` / `retention_days`); pruning an LMDB entry
  also deletes its associated image/thumbnail files.
- Selecting an entry triggers `Action::Copy` (restores it to the system
  clipboard, user pastes manually). Auto-paste-to-active-app is post-MVP.

## 9. Onboarding

First-run flow requests OS permissions up front rather than failing
silently later:
- **macOS:** Accessibility permission (`AXUIElement`), required for
  `window-management`
- Flags for what's been granted live in `settings.toml` under
  `[onboarding]`
- Raycast config import is explicitly NOT part of onboarding for now
  (see §2)

## 10. MVP command set

app-launcher, file-search, clipboard-manager, calculator, snippets,
window-management, emoji-picker, quicklinks, system-actions,
web-search-fallback.

## 11. Roadmap

| Phase | Scope | Milestone |
|---|---|---|
| 0 — Chassis | Workspace, `corvo-core` (trait+registry), empty GPUI window per OS, single-instance IPC + hotkey | App opens/closes on hotkey, empty search bar |
| 1 — First vertical slice | `app-launcher` only, end to end | Usable as a basic app launcher |
| 2 — Stateless commands | `calculator`, `emoji-picker`, `web-search-fallback`, `file-search` | 5 commands working |
| 3 — Persistence | `corvo-config` finalized, `clipboard-manager`, `snippets`, `quicklinks` | Persistent clipboard history, configurable snippets/quicklinks |
| 4 — Platform-heavy commands | `window-management`, `system-actions` for all 3 OS | Full MVP, all 10 commands, all 3 platforms |
| 5 — Packaging | AppImage (Linux), portable `.exe` (Windows), ad-hoc signed `.app` (macOS) | Installable without compiling |

## 12. Packaging & distribution

- **Linux:** AppImage via `linuxdeploy`/`cargo-appimage`. Don't bundle
  Wayland/X11/fontconfig system libs, they're ubiquitous and bundling
  them breaks portability. Flatpak is a possible future step if
  distributing via Flathub, not needed for MVP.
- **Windows:** portable `.exe`, statically linked CRT
  (`RUSTFLAGS="-C target-feature=+crt-static"`) so it runs without the
  VC++ Redistributable installed. MSI/NSIS installer with autostart
  registration is a post-MVP nice-to-have.
- **macOS:** `.app` bundle, ad-hoc signed (`codesign --sign -`) for
  personal use. Developer ID signing + notarization only if/when this
  goes beyond personal testing.
- **CI:** `cargo-dist` for cross-platform release builds via GitHub
  Actions, producing per-OS artifacts with checksums.

## 13. Naming

Project name: **Corvo**. Binary name: `corvo`. Workspace crate prefix:
`corvo-*`. IPC socket/pipe namespace: `corvo`.

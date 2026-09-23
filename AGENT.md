# AGENT.md — Corvo contributor guide for AI coding agents

This file tells an AI coding agent (Claude Code or similar) how to extend
Corvo safely. Corvo is a Raycast-equivalent launcher written in Rust,
targeting Linux, Windows, and macOS, built on GPUI. Extensions are **not** a
runtime plugin system — every command is native Rust code compiled into
the binary. This file exists so a contributor (human or agent) can add a
new command without needing to understand the whole codebase.

## 1. Project shape

```
corvo/
├── Cargo.toml                 # workspace root — lists every crate as a member
├── crates/
│   ├── corvo-core/           # Command trait, CommandRegistry, SearchResult, Action
│   ├── corvo-platform/       # PlatformOps trait + linux/windows/macos implementations
│   ├── corvo-ui/             # GPUI window, layer_shell/popup, rendering
│   ├── corvo-config/         # settings, hotkeys, persistence
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
└── src/main.rs                 # binary entrypoint — wires the registry and the UI
```

Each crate under `commands/` implements the `Command` trait from
`corvo-core` and registers itself at compile time via the `inventory`
crate. **`src/main.rs` is never edited to add a command.**

## 2. Adding a new command — the only workflow you should follow

1. Create `crates/commands/<name>/` with a standard `Cargo.toml` and
   `src/lib.rs`.
2. Depend only on `corvo-core` (and `corvo-platform` if the command
   needs OS-level actions — see §4). Never depend on `corvo-ui`.
3. Implement `Command` for a struct in that crate (see §3 for the exact
   interface).
4. Call `corvo_core::register_command!(YourCommandType)` once in that
   crate's `lib.rs`. Do not touch `CommandRegistry` itself.
5. Add exactly one line to the workspace root `Cargo.toml`: the new crate
   as a member, and one line to `src/main.rs`'s `Cargo.toml` dependencies
   list so it gets linked into the binary. This is the only place outside
   the new crate that should change.
6. Run `cargo check --workspace` and `cargo clippy --workspace` before
   considering the task done.

**Out of scope for a "new command" task — do not touch these unless the
task explicitly asks for it:** `corvo-ui`, `corvo-platform`,
`corvo-core`, `src/main.rs` logic beyond the one dependency line, any
other crate under `commands/`.

## 3. The `Command` trait

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
```

- `search` must be fast and non-blocking. If the command needs disk or
  network I/O, do it on a background task and cache; `search` itself
  should read from the cache.
- `execute` returns an `Action` (`Open`, `Copy`, `RunShell`, `ShowToast`,
  `CloseWindow`). It does not directly manipulate the window — the UI
  layer interprets the `Action`.
- Fuzzy matching and ranking: use `fff-search` (the search core behind
  FFF, built on the `frizbee` SIMD fuzzy matcher) as the shared matching
  engine across commands, not a one-off matcher per crate. It also gives
  frecency-based ranking (LMDB-backed) out of the box, which is what lets
  `app-launcher` and `file-search` surface recently/frequently used
  results first, the same way Raycast does. If a command's search space
  is a short static list (e.g. `emoji-picker`, `system-actions`), calling
  `frizbee` directly for the match score is enough — no need to pull in
  the full `FilePicker`/frecency machinery for those.

## 4. `PlatformOps` — the only place with `#[cfg(target_os = ...)]`

Commands that need OS-specific behavior (`window-management`,
`system-actions`) call into `corvo-platform::PlatformOps`, never into
raw platform APIs themselves.

```rust
pub trait PlatformOps: Send + Sync {
    fn lock(&self) -> Result<()>;
    fn sleep(&self) -> Result<()>;
    fn shutdown(&self) -> Result<()>;
    fn list_windows(&self) -> Result<Vec<WindowHandle>>;
    fn focus_window(&self, handle: &WindowHandle) -> Result<()>;
}
```

If a task needs a new platform capability that doesn't exist yet, add the
method to this trait and implement it in `linux.rs`, `windows.rs`, and
`macos.rs` inside `corvo-platform` — do not add a one-off OS-specific call inside a
`commands/` crate.

## 5. Conventions

- Rust edition: 2021 (confirm against the workspace `Cargo.toml` before
  assuming otherwise).
- No `unwrap()`/`expect()` outside tests — return `Result` and let the
  caller decide how to surface the error.
- Command IDs are kebab-case and match the crate directory name
  (`app-launcher`, not `AppLauncher` or `app_launcher`).
- Every new command crate needs at least one unit test for `search()`
  covering a match and a non-match.

## 6. Before marking a task done

- [ ] `cargo check --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] Only the files listed in §2 step 5 changed outside the new crate
- [ ] The new command has at least one test

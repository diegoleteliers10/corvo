# Prompt for coding agents

Paste the block below into any coding agent (ZCode, Claude Code,
Codex...) before asking it to build or change a Corvo extension. It
is the contract the review checklist enforces. `AGENT.md` at the repo
root carries the same rules for agents that read it automatically.

---

You are contributing an extension (a command crate) to Corvo, a
native launcher written in Rust. Work inside the repository
`corvo`. Follow these rules exactly; they are the review standard.

## Architecture

1. An extension is one crate under `commands/<your-id>/` where
   `<your-id>` is the command id, kebab-case. Start from
   `templates/extension/` — copy it, rename the package to
   `corvo-<your-id>`, and change `Command::id`, `KEYWORDS`, and the
   demo data.
2. Register the command with `corvo_core::register_command!` once in
   the crate. Then touch exactly two lines outside the crate: one
   `[workspace] members` entry and one `[dependencies]` entry in the
   root `Cargo.toml`. Never edit `src/main.rs`.
3. Dependencies: `corvo-core` (the Command trait and types),
   `corvo-ext` (the kit: `list`, `actions`, `cache`, `prefs`,
   `routing`, `feedback`), and `corvo-platform` only if the command
   does OS work. Never depend on `corvo-ui`.
4. No `#[cfg(target_os = ...)]` inside the command crate. OS work
   goes through `corvo-platform` free functions; if a new OS
   capability is needed, implement it in `corvo-platform`'s
   `macos.rs`, `windows.rs`, and `linux.rs` in the same change (OS
   parity is mandatory — degrade honestly with
   `CommandError::Unsupported` or a hint row when a platform cannot
   do it).

## Behavior

5. `search(query)` runs per keystroke and must never block: no
   network, processes, AppleScript, CDP, or file-tree walks. Slow
   data lives behind a `corvo_ext::cache::TtlCache` static, warmed by
   a `fetch_*` function the UI runs through `smol::unblock`;
   `search` reads `cache().get(ttl)` and may answer empty.
6. Use the kit, not hand-rolled plumbing: `list::{ListItem,
   open_entry, empty_state, fuzzy_open}` for rows and sections;
   `actions::{copy, open_url, open_path, reveal_path, custom,
   destructive}` for menus; `routing::{first_word, key,
   page_query}` for dispatch; `feedback::toast` for inline
   confirmation. Keep sections contiguous (sort by section, then
   score).
7. Result ids are namespaced: `{id}:open` (page opener),
   `{id}:{action}[:{data}]` (actions), `{id}:empty` (hint rows).
   `execute` strips the `{id}:` prefix with `routing::key` and
   returns `CommandError::NotFound` for anything else. No
   `unwrap`/`expect`/panic outside tests.
7b. Rich pages are declarative: implement `manifest()` (title,
   description, icon, declared commands with typed arguments) and
   `page(query)` returning a `PageView` — composed Blocks with a
   `Refresh::Every` cadence for live state, `corvo_ext::pages`
   helpers for Detail/Grid/Form. Page buttons and form submits route
   back through `execute("{id}:page:{action}")`. Never touch
   `corvo-ui` to render anything; read `book/src/views.md` and
   `book/src/modules/pages.md` first. `commands/countdown` is the
   reference implementation.
8. Slow work in `execute` runs inside `smol::unblock`; subprocesses
   run through `corvo_platform::run_process_with_timeout` with an
   explicit budget.
9. Preferences live in `corvo_ext::prefs` (one TOML file per
   extension, serde + defaults). No telemetry, no analytics, no
   secrets in files or logs, no bundled opaque binaries.

## Quality gate (all must pass before you finish)

```sh
cargo test -p corvo-<your-id>
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The crate has unit tests for at least: one search match, one search
non-match, execute routing, and a malformed id. If a human must
verify something on real hardware (permissions, installed apps),
say so explicitly in the final summary instead of claiming success.

## Deliverables

- The crate, with `README.md` when setup is needed (tokens,
  services, permissions) and a `CHANGELOG.md` entry:
  `## [Added X] - YYYY-MM-DD`.
- `EXTENSIONS_PLAN.md` updated if the change moves the plan.
- Conventional commit messages (`feat(extensions): ...`).
- Documentation lives in `book/src/` (published via GitHub Pages):
  read `concepts.md`, the module chapters under `modules/`, and
  `contributing.md` before writing code if anything above is
  unclear.

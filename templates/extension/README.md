# Example — the Corvo extension template

This folder is the official starting point for a new Corvo extension.
It is **not** a workspace member: it never compiles into the app from
here. Copy it into `commands/` to bring it to life.

## Quick start

1. Copy this folder to `commands/<your-id>/`.
2. Rename the package in `Cargo.toml` to `corvo-<your-id>`.
3. In `src/lib.rs`, change `Command::id`, `KEYWORDS`, the titles, and
   the item catalog (the file's header comment walks every edit).
4. Register the crate in the root `Cargo.toml`:
   - one line under `[workspace] members`: `"commands/<your-id>"`,
   - one line under `[dependencies]`:
     `corvo-<your-id> = { path = "commands/<your-id>" }`.
5. Verify:

   ```sh
   cargo test -p corvo-<your-id>
   cargo clippy --workspace --all-targets -- -D warnings
   ```

## What the template demonstrates

- Root routing with keywords and a fuzzy fallback (`corvo_ext::routing`,
  `corvo_ext::list::fuzzy_open`).
- A dedicated page namespace (`{id}-page:{filter}`) answered from a
  `TtlCache` that is warmed off the search path.
- List sections via `ListItem::section`.
- An actions menu built from `corvo_ext::actions` builders.
- Per-extension preferences via `corvo_ext::prefs`.
- The required unit tests (match, non-match, execute routing).

## Requirements for a PR (summary)

See the extension book and `CONTRIBUTING.md`. At minimum: tests for
search and execute, `cargo clippy --workspace --all-targets -- -D
warnings` clean, OS parity (or an honest graceful fallback per
platform), this file renamed to your extension, and a
`CHANGELOG.md` entry.

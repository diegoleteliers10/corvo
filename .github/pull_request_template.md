<!-- Reviewers merge when every box holds; see CONTRIBUTING.md and the extension book. -->

## What

<!-- One paragraph: what changes and why. Link the issue if there is one. -->

## Checklist

- [ ] `cargo test --workspace` passes, including new/changed crates
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] Tests cover: search match, search non-match, execute routing (incl. malformed id)
- [ ] Crate deps limited to `corvo-core`, `corvo-ext`, optional `corvo-platform`
- [ ] No `#[cfg(target_os)]` in command crates (platform work landed in `corvo-platform`, three OS files)
- [ ] `search` stays non-blocking; slow data behind `TtlCache` warmed off the search path
- [ ] Result ids follow `{id}:open` / `{id}:{action}[:{data}]` / `{id}:empty`
- [ ] OS parity: native mode or honest degradation; manual checks listed below
- [ ] Honest empty states; toasts are sentences; no fake loading
- [ ] No telemetry, no secrets in files/logs, no opaque binaries
- [ ] `CHANGELOG.md` entry added; crate `README.md` if setup is required
- [ ] `EXTENSIONS_PLAN.md` updated if the plan moved

## Manual verification

<!-- What CI cannot check, and what you ran it on: "focus_tab tested with Chrome 141 + macOS 15". -->

## Screenshots / capture

<!-- For UI-visible changes, before/after on each platform if available. -->

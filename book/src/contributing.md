# Contribution standard

Every extension change lands through a pull request on
[github.com/diegoleteliers10/corvo](https://github.com/diegoleteliers10/corvo).
Review is checklist-based: if all boxes pass, the PR merges.

## The checklist

### Correctness

- [ ] `cargo check --workspace --all-targets` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes, and the extension has tests
      for at least: a search match, a search non-match, and execute
      routing (including a malformed id).
- [ ] No `unwrap`/`expect`/panic outside tests.
- [ ] Errors are values (`CommandError`); no swallowed failures.

### Architecture

- [ ] The crate depends only on `corvo-core`, `corvo-ext`, and (if
      needed) `corvo-platform` — never `corvo-ui`.
- [ ] No `#[cfg(target_os)]` in the command crate; OS work goes
      through `corvo-platform` (all three OS files in the same PR).
- [ ] `search` never blocks: slow work sits behind a `TtlCache`,
      warmed off the search path.
- [ ] Result ids follow the namespace table
      (`{id}:open`, `{id}:{action}[:{data}]`), and the command id
      equals the crate directory name.

### Platform parity

- [ ] Every macOS behavior has a native Windows and Linux story — or
      degrades honestly (`Unsupported`, an empty list, a hint row).
- [ ] CI is green on all three targets. Runtime checks that CI
      cannot do (permissions, installed apps) are listed in the PR
      description as "tested manually on ...".

### UX

- [ ] Titles follow the naming rules (below); labels are Title Case
      with an ellipsis `…` for anything that opens further UI.
- [ ] Empty states are honest: a hint row, never a blank list.
- [ ] Preferences use the `prefs` module; no configuration commands.
- [ ] Toasts are sentences; notifications only from background work.

### Safety

- [ ] No telemetry, analytics, or outbound requests beyond the
      feature's documented purpose.
- [ ] No bundled opaque binaries; subprocesses are named, versioned,
      and downloaded from infrastructure the user can audit.
- [ ] Secrets stay out of preferences files and logs.

### Documentation

- [ ] `CHANGELOG.md` entry per user-visible change:
      `## [Added X] - 2026-10-03` (Keep-a-Changelog style).
- [ ] `README.md` in the crate when setup is required (tokens,
      services, permissions).
- [ ] `EXTENSIONS_PLAN.md` updated when the change moves the plan
      (new extension, new platform support).

## Naming rules

- **Extension titles**: nouns over verbs, specific over general —
  `Hacker News`, `Notion Search`. Apple Style Guide capitalization;
  lowercase stays lowercase for names like `npm` or `iOS`.
- **Commands inside an extension**: `<verb> <noun>` (`Create Issue`,
  `Search Files`) or a bare noun; no articles.
- **Ids**: kebab-case, stable, never renamed without a migration
  note.
- **Keywords**: include the extension name, a natural alias, and the
  domain noun; they are lowercase-matched on the query's first word.

## Review flow

1. Small fixes go into the existing extension's PR; get the
   maintainer's sign-off for structural changes.
2. A new extension for a different configuration of the same service
   is its own extension (`GitHub Cloud` vs `GitHub Enterprise`), not
   a merged mega-extension.
3. The reviewer checks out the PR branch and runs the extension
   locally (`cargo run`) — review includes using it.
4. Squash-merge with a conventional commit title
   (`feat(extensions): ...`, `fix(weather): ...`).

## Versioning

Extensions ship with Corvo releases (per-OS installers and the
Homebrew cask update automatically). A PR that should appear in the
next release says so in its description; release notes are assembled
from `CHANGELOG.md` entries.

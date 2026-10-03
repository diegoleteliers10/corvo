# Contributing to Corvo

Thanks for helping. Corvo is a native launcher: every extension is a
Rust crate compiled into the binary, reviewed against the checklist
below. The full guide — tutorial, module reference, and the reasoning
behind each rule — is the
[extension book](https://diegoleteliers10.github.io/corvo/) (source
in [`book/`](book/src/SUMMARY.md)).

## Getting started

- Building a new extension? Start from
  [`templates/extension/`](templates/extension/) — the tutorial walks
  the whole path.
- Improving an existing extension? Read its crate first; each one is
  the canonical example of a pattern (see the book's
  [examples page](book/src/examples.md)).
- Working with a coding agent? Give it [`PROMPT.md`](PROMPT.md).

## The review checklist

The PR template mirrors this list; reviewers merge when every box
holds.

1. **Quality gate** — `cargo test --workspace`, `cargo clippy
   --workspace --all-targets -- -D warnings`, and `cargo check
   --workspace --all-targets` all pass, including CI on Windows,
   macOS, and Linux.
2. **Tests** — the changed extension covers a search match, a search
   non-match, and execute routing (including a malformed id).
3. **Architecture** — the crate depends only on `corvo-core`,
   `corvo-ext`, and optionally `corvo-platform`. No OS-specific code
   in the crate (`corvo-platform` owns all `#[cfg(target_os)]`, three
   OS files in the same PR). `search` never blocks; slow work sits
   behind `corvo_ext::cache::TtlCache` warmed off the search path.
4. **Conventions** — command id equals the crate directory name
   (kebab-case); result ids are namespaced (`{id}:open`,
   `{id}:{action}[:{data}]`); labels Title Case; no
   `unwrap`/`expect`/panics outside tests.
5. **Parity** — every macOS behavior has a native Windows/Linux
   story or degrades honestly. Manual runtime checks (permissions,
   installed apps, notifications) are listed in the PR description.
6. **Honesty in UX** — hint rows instead of blank lists, sentences in
   toasts, notifications only from background work, no fake loading
   states.
7. **Safety** — no telemetry or analytics, no secrets in files or
   logs, no bundled opaque binaries.
8. **Docs** — crate `README.md` when setup is needed, a
   `CHANGELOG.md` entry per user-visible change, and
   `EXTENSIONS_PLAN.md` updated when the plan moves.

## Commit style

Conventional commits, imperative mood: `feat(extensions): ...`,
`fix(weather): ...`, `docs(book): ...`. Squash-merge on approval.

## Code of conduct

Keep it technical, keep it kind. Review comments address code, not
people.

# Introduction

Corvo is a lightweight native launcher built on Rust and GPUI. An
**extension** is one launcher feature: an application search, a
clipboard history, a pomodoro timer, a browser tab switcher. This
book explains how extensions are structured and how to build one that
passes review.

Three facts shape everything else:

- **Extensions are native Rust, not plugins.** There is no runtime
  scripting or WASM. Every extension is a crate compiled into the
  binary, registered at link time. That is why Corvo stays fast and
  small — and why extension authors get the same tooling as core
  developers: `cargo`, `clippy`, unit tests, CI on three operating
  systems.
- **The UI layer is shared.** Extensions do not draw their own
  windows. They return data (rows, actions, toasts) and the launcher
  renders it, so every extension looks and behaves like part of the
  app. Consistency is enforced by the type system, not by taste.
- **The patterns are the standard.** The
  [`corvo-ext`](https://crates.io) kit crate ships the recurring
  shapes — list rows and sections, action menus, TTL caches,
  preferences, routing — so an extension reads like the rest of the
  codebase instead of inventing its own plumbing.

## Where to start

- New extension? Follow [Build your first extension](getting-started.md)
  from the template in `templates/extension/`.
- Reviewing a pull request? The [contribution standard](contributing.md)
  is the checklist.
- Letting a coding agent do the work? Give it the
  [agent prompt](agent-prompt.md).

## Reference

The `Command` trait and the workspace layout are specified in
[`SPEC.md`](https://github.com/diegoleteliers10/corvo/blob/main/SPEC.md)
and [`AGENT.md`](https://github.com/diegoleteliers10/corvo/blob/main/AGENT.md).
Rustdoc for the kit: `cargo doc -p corvo-ext --open`.

# Corvo

A lightweight native launcher for Linux, Windows, and macOS. Rust with
GPUI. Feature parity with Raycast's core commands, no runtime plugin
system.

Status: phase 0, chassis. `SPEC.md` holds the full plan, `AGENT.md` the
contributor workflow.

## Build

    cargo build

## Run

    cargo run

The first instance stays resident. Start the binary again to toggle the
window. Default hotkey: Alt+Space on macOS and Windows. On Linux, bind
`corvo --toggle` in the compositor.

## Layout

- `crates/corvo-core` — command trait and link-time registry
- `crates/corvo-platform` — OS boundary, IPC, hotkey
- `crates/corvo-ui` — GPUI launcher window
- `crates/corvo-config` — TOML settings, snippets, quicklinks
- `commands/*` — one crate per launcher command

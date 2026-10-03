# Platform

`corvo-platform` is the only crate allowed to contain
`#[cfg(target_os = ...)]`. Extensions call its free functions and
traits; they never run raw OS APIs inline. If your extension needs a
new OS capability, add it to the platform trait and implement it in
`macos.rs`, `windows.rs`, and `linux.rs` — the OS parity tests fail
when one is missing.

## Everyday helpers

| Function | What it does |
|---|---|
| `open_url(url)` | system handler for URLs and app URIs |
| `open_path(path)` | system handler for files and folders |
| `run_shell(cmd)` | a shell line, CREATE_NO_WINDOW on Windows |
| `run_process_with_timeout(program, args, timeout)` | blocking process run with a deadline and clean kill |
| `notify(title, body)` | desktop notification |
| `read_clipboard_text()` / `copy_image_to_pasteboard(...)` | clipboard |

## Richer subsystems

| Area | Entry points |
|---|---|
| Processes | `process_snapshot`, `terminate_process`, `listening_port_snapshot` |
| Media | `media_now_playing`, `media_transport` (AppleScript / SMTC / MPRIS) |
| Browsers | `browser_tabs`, `focus_browser_tab`, `browser_app_icon`, `browser_app_installed`, `browser_data_candidates`, plus the `cdp` module |
| Windows & apps | `platform_ops().list_windows()/focus_window()`, `list_apps_in_scopes` |
| Permissions | `permissions::is_granted`, `permissions::request` (macOS TCC) |
| Updates | the `updates` module (used by the app, not extensions) |

## Calling convention

Slow platform calls run through `smol::unblock` in `execute`, or in
a warm-up fetch behind a `TtlCache` for `search`:

```rust
return smol::unblock(move || corvo_platform::focus_browser_tab(&app, focus, port, &url))
    .await
    .then_some(Action::CloseWindow)
    .ok_or_else(|| CommandError::Platform("could not focus that tab".into()));
```

Timeouts are the caller's job: `brew` runs every subprocess through
`run_process_with_timeout` with a 60 s budget. Copy that pattern —
a hung subprocess hangs the async worker pool.

## Parity rule

A capability that works on macOS must work on Windows and Linux in
their native mode, or degrade honestly: `CommandError::Unsupported`,
an empty list, or a hint row. The three OS files land in the same
commit, and CI runs the tests on all three platforms.

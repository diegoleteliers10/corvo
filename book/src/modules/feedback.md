# Feedback

There are two ways an extension talks back, and choosing the right
one is most of the UX work.

## Toast — inline confirmation

`corvo_ext::feedback::toast(message)` returns
`Action::ShowToast(message)`. The launcher shows it while its window
is up. Use it for transport-class feedback:

```rust
"toggle" => Ok(feedback::toast("Media control sent")),
```

Write a sentence the user reads, not a debug string. An empty or
technical message (`"ok"`, `"result_id=7"`) fails review.

## Desktop notification

Work that finishes when no launcher window is in flight — a pomodoro
interval, a background download — calls
`corvo_platform::notify(title, body)`:

```rust
corvo_platform::notify("Pomodoro", "Focus interval finished")?;
```

It is blocking (AppleScript on macOS, an encoded PowerShell toast on
Windows, `notify-send` on Linux): call it from the background thread
that finished the work, never from `search`. Record a diagnostic on
failure the way `pomodoro` does instead of failing silently.

## Errors

`execute` returns errors as values and the UI renders them:

- `CommandError::NotFound` — unknown or malformed result id.
- `CommandError::Unsupported` — this platform cannot do it (yet).
- `CommandError::Platform(message)` — the OS refused; write what a
  user can do about it ("could not focus that tab").

Failures that happen off-path (a warm-up fetch) must not crash
either: answer empty from the cache, and surface a hint row when the
page resolves empty.

## Anti-patterns

- Toasts for state the user can see (`Action::Open` already closes
  the window and shows the result).
- Notifications from `execute` — the window is up; a toast is enough.
- `println!`/`eprintln!` debugging left in; use
  `corvo_platform::diagnostics` for local error logs.

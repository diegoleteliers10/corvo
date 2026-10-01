# Corvo 0.3.5

## Windows

- Read application icons through native Windows Shell APIs. Shortcut icons no longer need PowerShell. Packaged apps use native extraction first.
- Use a new icon cache and reject empty icons. Save PNG files through an atomic write.
- Move window clipping outside the GPUI update to prevent a nested update during compact-mode resize.
- Apply rounded edges after the viewport size or display scale changes. Check both width and height when the interface size changes.
- Use a transparent window background instead of the native acrylic backdrop.

## Local error logs

- Add local error logs on Windows, macOS, and Linux. Corvo sends no logs automatically.
- Record the app version, system, time, component, error event, and source location. Omit searches, clipboard content, file paths, log messages, and panic payloads.
- Keep up to three files of 1 MiB each. Limit repeated events. A log write failure does not stop the app.
- Add **Open logs** in **Settings > About** for manual sharing.

## Maintenance

- Resolve Clippy warnings in the calculator, result sorting, settings, and launcher code.
- Add explicit calculator error types and remove an unnecessary macOS `unwrap`.
- Run diagnostics tests on all three systems in CI. Add native Windows icon and window-region tests.

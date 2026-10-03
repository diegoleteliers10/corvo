# Corvo 0.4.0

## Extensions

- Add text utilities for case conversion, encoding, JSON, UUIDs, hashes, and line tools on macOS, Windows, and Linux.
- Add Pomodoro focus and break timers with pause, resume, skip, and desktop notifications on all three systems.
- Add current weather and a three-day forecast from wttr.in. Accept city searches and cache responses for 15 min.
- Add a native Markdown notes editor with autosave, headings, lists, checklists, links, code colors, tables, selection, and a scrollbar.
- Add Spotify and Music playback controls on macOS.
- Add Chrome, Brave, and Edge tab search and focus on macOS. Read bookmarks from the first available Default profile. Browser history remains planned.
- Keep process control available on all three systems and Homebrew package and service management on macOS.

## Reliability

- Preserve note filenames during autosave. Save through an atomic file replacement and keep one editor per note. Reject unsafe note paths and Windows reserved filenames.
- Keep timer expiry and state changes under one lock.
- Bound process execution and pipe reads. On Unix, stop the process group when the timeout expires.
- Pass browser URLs as AppleScript arguments.
- Preserve unreadable settings files and cache settings reads.
- Encode quicklink arguments before URL substitution.
- Apply file search exclusions to both slash formats on Windows.
- Use per-user Unix IPC sockets with a legacy socket fallback.
- Open Windows URLs through the native Shell API and respect clipboard concealment markers.

## Platform requirements

- Homebrew, Media Control, and Browser Tabs are macOS-only commands.
- macOS browser and media commands require Automation permission for the target application.
- Linux desktop notifications require notify-send and a notification service.
- Weather requires network access. Notes and timer state stay local.
- Native UI and notification delivery still need manual checks on each system. CI checks target compilation. It does not confirm runtime behavior.

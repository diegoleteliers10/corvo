# Corvo 0.6.2

This release cuts idle memory, keeps the launcher in one place, and makes the
shortcut on a row match Settings.

## Memory

- The launcher releases every decoded image when its window closes. GPUI keeps decoded images for the life of the process, so each app icon and clipboard screenshot stayed resident. Idle memory drops from about 524 MB to about 74 MB, and the launcher peaks around 139 MB while it is open.
- Clipboard image previews decode to the preview card size instead of the full screenshot. Browsing the history keeps one preview at a time.
- App icons render at 128x128 instead of 256x256. That covers the largest slot the launcher draws and cuts the decoded size by four.

## Launcher position

- The launcher cannot be moved. It opens at the same top edge on every page and grows downward, so Clipboard, Files, and Emoji no longer open higher than the first summon.
- Settings and the onboarding window stay movable.

## Shortcuts

- A row shows the shortcut Settings stores for it. The emoji row showed a default that ignored the recorded key, and the clipboard row did the same.
- App, command, and system action rows show their configured shortcut at the right edge.
- Clearing a shortcut in Settings removes it from the row. A state label such as Playing or Now stays.

## Release

- A merge into main now starts the release build. The tag workflow dispatches the Release workflow explicitly, because a tag pushed with the default token does not trigger another workflow.
- A merge where the tag already exists but the release is missing now publishes the release instead of stopping after the tag step.

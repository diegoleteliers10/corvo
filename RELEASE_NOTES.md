# Corvo 0.3.0

## First-launch setup

- Corvo opens a welcome window on the first start instead of showing the launcher. The window has the steps that apply: the launcher shortcut, macOS Accessibility, and a final step that shows the recorded shortcut and opens the launcher.
- The window runs once. The `shown` flag in `settings.toml` is written when the window opens, so quitting in the middle does not bring it back.
- Settings > General has "Run first-launch setup" to run it again on every platform.
- Windows and Linux run the two steps that apply to them. They have no system grant like macOS Accessibility, so their wizard does not ask for one.

## Hotkey recording

- Recording a shortcut in the welcome window or in Settings unregisters every global hotkey while you record. The previous binding can no longer fire the launcher over the recorder, and the recorder can capture the combination that is already bound.
- Recording stops when the window closes on every platform, including the Windows path where Settings keeps its window open.

## Screen Recording on macOS

- macOS kills Corvo when you grant Screen Recording. A detached watcher now relaunches Corvo the moment the process disappears, so the grant becomes a background restart.
- Quitting Corvo on purpose still quits. The watcher stands down when the exit is clean, and the updater stands it down too.

## Permissions

- The welcome window asks only for Accessibility, which is what window tiling needs. Calendars, Screen Recording, and Full Disk Access stay in Settings > Permissions.
- Both places read the grant state live from the system. macOS keeps the grants across updates because they belong to the signed app, not to a version.

## Notes

- The welcome window embeds the app icon, so every platform binary is about 160 KB larger.
- Users who already have Corvo installed see the welcome window once after this update.

# Corvo 0.2.0

## Updates

- The Relaunch Corvo button prepares the update without blocking Settings. It shows install errors and lets you retry with the downloaded file.
- Windows detects an updated Corvo process that exits within 500 ms of relaunch. It then restores the previous executable and records the failure.
- macOS and Linux record errors from the restart helper. Linux restores the previous executable if the new one stops within 1 s of relaunch.
- Linux system package installs continue to use the package manager for updates.

## Upgrade from 0.1.9

If the Relaunch Corvo button in 0.1.9 does nothing, download and install 0.2.0 manually once. The fix runs only after 0.2.0 starts.

# Corvo 0.1.9

## Windows

- Alt+Space shows or hides the launcher. The launcher stays above normal windows while it is open.
- Closing the launcher returns focus to the previous app without starting PowerShell.
- Settings keeps its window after close, so it opens without creating the window again.
- App icons load for search results outside the initial list. Corvo removes damaged icon files and extracts them again.

# Corvo 0.1.8

## Fixed

- The background update check now opens Settings > About with the release changelog plus Skip and Download buttons when a newer version is found, instead of silently dropping it.
- A previously found update stays visible when reopening About, without needing another manual check.
- Skipping a version dismisses the prompt and clears the pending update.

## System actions parity across macOS, Windows, and Linux

- Every system action now works on all three systems, each one in its native mode. Linux and Windows gain Dismiss Notifications, Eject All Disks, Open Trash, Next Track, Play / Pause, and Previous Track.
- Every system setting category now exists on all three systems. Linux and Windows gain About and Battery settings with native targets.
- Fixed Windows settings links: keyboard opens typing settings, accessibility opens Ease of Access.
- Linux settings now try GNOME, KDE, and XFCE handlers in order instead of picking one from the desktop name.
- Fixed macOS Hide All Apps and Quit All Applications: they no longer hide or quit Corvo itself (wrong process name case).
- Fixed Linux volume and brightness controls reporting failure when the change applied but the new level could not be read.

## Launch at login on all systems

- macOS uses the Login Item API for bundled builds and a LaunchAgent plist for dev builds, never both at once, so Corvo starts exactly once.
- Windows registers the HKCU Run key. Linux writes `~/.config/autostart/corvo.desktop`.
- The setting stays on by default and applies at startup on every OS.

# Corvo 0.1.7

## Fixed

- Show Windows application icons with launcher results. Load saved icons before the launcher opens and scan new apps in the background.
- Prefer Windows Start menu shortcuts and remove duplicate entries from `Get-StartApps`.
- Reuse and focus the Windows launcher window when the user opens it again.
- Hide console processes during Windows updates, app scans, background actions, and relaunch. Restore the previous executable if replacement or the relaunch command fails.
- Sign macOS builds with a stable identity so the app keeps the same code requirement across later updates.
- Open System Settings and prompt for calendar access when requested from permissions settings.
- Enable launch at login by default and register startup entries across macOS, Windows, and Linux.

## Notes

- The first Windows app scan needs time to extract icons. Later launches use the saved app list and icons.
- The macOS signing identity changed in this release. macOS can ask for permissions once after this update. New macOS downloads use a self-signed certificate and may need manual approval in system settings.

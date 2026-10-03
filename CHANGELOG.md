# Changelog

Every released version of Corvo. The notes for the current release
are in RELEASE_NOTES.md.

# Corvo 0.3.6

## Launcher

- Use the same background transparency in compact and full modes.
- Remove an application from the cached list after uninstall. On macOS, removal of associated files alone keeps the application in the list.
- Refresh the application list each time the launcher opens on macOS, Linux, and Windows.
- Remove deleted application paths from cached results and the Windows startup cache.
- Keep a Windows shortcut during icon loading only if the current scan still finds it.
- Make **Reload Applications** start a new scan.

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

# Corvo 0.3.4

The launcher opens above the middle of the screen now. It sat dead center,
which placed the result rows low on a laptop display and made the eye
travel down to read them. The middle of the panel now sits at 30 percent of
the display height. The position is derived from where the middle of the
panel should land rather than from a fixed offset, so the compact and
expanded sizes both open in the same place.

Uninstalled applications now leave the launcher. On Windows a scan that
failed to find an app left the previous entry in the list, and a previous
entry is exactly what an uninstalled app looks like, so it stayed there
for good. Only an entry that is still installed and still waiting for its
icon to decode is carried forward now. Store apps are read from
`Get-StartApps` on every scan, so they never needed carrying forward either.

# Corvo 0.3.3

This release repairs the software update relaunch on macOS. Every update
refused to install with a signature error, even when the downloaded app
was correctly signed.

- Relaunch reported "Cannot read the macOS app code requirement" on
  every attempt. `codesign -dr -` sends the `designated =>` line to
  stdout and the `Executable=` line to stderr. Corvo read stderr only, so
  it never found a requirement and rejected its own download before it
  compared anything. The reader checks both streams now.
- The error was raised ahead of the identity comparison, so it hid the
  real state of the update and gave nothing to act on.
- A local build signed ad-hoc has no identifier, and the tool comments
  the designated line out with a leading `# `. That marker is handled
  too, so an unsigned build reports an unexpected signing identity
  instead of a read failure.

The update card also grew past its own rounded edge when the status
message was long. The status row now fills the card and the action
button keeps its width.

# Corvo 0.3.2

This release makes 0.3.1 installable on Windows. The 0.3.1 build fails to
compile, so the Windows fixes it described never reached a binary. Nine
compile errors are repaired here, and the Windows build is now checked
against the real Windows target before a release is tagged.

- Two PowerShell scripts were held in `format!` calls whose braces were
  no longer escaped. A `format!` macro reads a `{` as the start of a
  placeholder, so the first PowerShell brace opened an argument that
  swallowed the rest of the crate. This is the error that stopped the
  build. Both scripts are plain strings now, with the shortcut path
  substituted into a single token.
- A closure body reads `unsafe { .. }` as a statement, which left the
  theme toggle's success check as a stray token.
- The media key action named a type that does not exist, two volume calls
  read a COM vtable pointer without a dereference, and two imports were
  unused.
- The launcher re-clipped its window after a resize through a call that
  took one argument too many, and the file preview returned a different
  element type on Windows than on the other platforms.
- The unused Windows declarations left over from the disk eject work are
  gone.

# Corvo 0.3.1

This release repairs Windows. Every item below was broken on Windows and
either did nothing or reported success while doing nothing. macOS and
Linux keep their current behaviour, apart from the shared shortcut and
defaults work, which now name each platform's own keys.

## Shortcuts

- The launcher bound every shortcut to the wrong key. It read the platform modifier as the primary one, which is Command on macOS, the Windows key on Windows, and the Super key on Linux. On Windows and Linux the actions menu, the settings window, and quit were all dead.
- Ctrl+K opens the actions menu, Ctrl+, opens Settings, and Ctrl+Q quits, on Windows and Linux. macOS keeps its Command shortcuts.
- Ctrl+C and Ctrl+V did nothing in the search field on Windows and Linux. The typing guard swallowed every Ctrl chord before the copy and paste handlers could run. Both work now, and Ctrl+C falls back to copying the query text when the selected row has no copy action.
- The labels named a Mac key. Windows and Linux now read Ctrl, Alt, and Shift as words, which is the convention on those platforms. The keycap renderer also drew one keycap per character, so a multi-key label would have appeared as `C` `t` `r` `l` `+` `K`.
- Two shortcuts moved because the obvious key is taken on Windows. The emoji picker uses Ctrl+Alt+Space, because Ctrl+Space is the input-method switch. Delete-to-end-of-line uses Ctrl+Alt+K, because Ctrl+K opens the actions menu.
- Delete and Backspace are separate keys on Windows and Linux. Corvo collapsed them, so Ctrl+Delete ran delete-to-start-of-line instead.
- Recording a shortcut in Settings stored a Mac key for a Windows key press, then displayed it as a Mac key. The recorder and the settings list agree now.
- The uninstaller said "Show in Finder" and "Show Info in Finder" on every platform. They name the platform's file manager. The app launcher's reveal action is hidden for Store apps, which have no file to reveal.

## System actions

- Eleven Windows actions never ran. Each was a PowerShell one-liner inside a command that goes through `cmd /c`, and the escaping did not survive the trip. Lock, sleep, empty trash, toggle appearance, mute, the three media keys, show desktop, quit all, eject disks, and dismiss notifications are all affected.
- Every one of them now calls the Windows API directly. Mute and the media keys use SendInput, so the key reaches the system volume mixer and the active media player. The old code sent a character instead of a key code, so those four did nothing even without the command problem.
- Dismiss Notifications opens the notification settings. Windows has no API that clears the Action Center from another app, so the action is named for what it does.
- Toggle Appearance now keeps the light and dark values in step. The old script read one and wrote both, which desynchronised them for good on a machine where the user had set them differently. Open windows repaint at once instead of on the next sign-in.
- Open Trash no longer reports a failure after succeeding. Windows Explorer exits with an error code even when it opened the window.
- Empty Trash says the bin is already empty instead of failing.
- Eject Disks ejects the drive itself rather than the volume on its top, and reports what Windows refused and why.
- Restart, Shut Down, and Log Out were already working and are unchanged.

## Windows features that were missing

- Window management did not exist. Left half, right half, the thirds, the quarters, maximize, center, restore, and moving to another display all did nothing, and the launcher reported each one as done. They now work, and they respect the window gap from Settings.
- Brightness did not work on any machine without a laptop panel. It now reaches external monitors, and a display with no brightness control says so instead of failing.
- Volume changed the level but stalled for seconds on every press, because it compiled C# each time. It is now a direct call.
- Compact mode worked on the first launch of a session and never again, because the resident panel kept the previous height. It now collapses on every open.
- The file listing and preview were empty. The default scope resolved the home directory from a variable Windows does not set, so the page had nothing to search. Existing installs are fixed without editing your settings.
- The Files page also ignores AppData, .git, and .venv by default, and does not follow directory junctions that would otherwise loop.
- The launcher panel had square corners, because Windows draws a square window for a borderless panel. The window is now clipped to the painted shape, and the two radii cannot drift apart.
- App icons never appeared once the Start Menu grew past about sixty shortcuts. The extraction overflowed the Windows command-line limit, failed, and repeated on every launch. The job list moved to a file.
- File preview images showed with red and blue swapped.
- Opening an application was slow and reported an error even when it worked.

## Changes for everyone

- The launcher hotkey, the clipboard and emoji hotkeys, the application search scopes, and the file-search scopes no longer ship macOS values on every platform.
- Two hotkey-triggered actions ran on the UI thread and froze the launcher while they waited. They run in the background now.

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

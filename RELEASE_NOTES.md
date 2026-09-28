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

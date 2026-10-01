# Corvo 0.3.6

## Launcher

- Use the same background transparency in compact and full modes.
- Remove an application from the cached list after uninstall. On macOS, removal of associated files alone keeps the application in the list.
- Refresh the application list each time the launcher opens on macOS, Linux, and Windows.
- Remove deleted application paths from cached results and the Windows startup cache.
- Keep a Windows shortcut during icon loading only if the current scan still finds it.
- Make **Reload Applications** start a new scan.

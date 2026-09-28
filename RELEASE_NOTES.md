# Corvo 0.1.3

## New

- Search files in a separate page with fuzzy matching and previews for images, text, code, and folders.
- Manage Homebrew formulae and casks, including available upgrades.
- Open Port Manager to list processes that listen on ports and stop a process by port.
- Preview and uninstall supported Windows and Linux apps.

## Improved

- Search apps, commands, system actions, and extension commands with more relevant matches.
- Show action notifications after the launcher closes.
- Show volume and brightness levels with a compact progress indicator.
- Resolve app icons from Windows shortcuts, Store packages, Linux desktop entries, and common icon formats.
- Keep app permissions, commands, quicklinks, and hotkeys across updates.
- Improve clipboard previews, file search responsiveness, and app uninstall flows.

## Platform support

- Windows app removal supports MSIX, MSI, and registered Win32 uninstallers that pass Corvo's safety checks.
- Linux app removal supports Flatpak, Snap, AppImage, APT, DNF, Zypper, and Pacman when Corvo can verify the app and open a supported terminal.
- Homebrew commands are available on macOS.

Linux package removal needs GNOME Terminal or xterm and a valid permission path. Wayland global shortcuts and automatic paste depend on compositor support.

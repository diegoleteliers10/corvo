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

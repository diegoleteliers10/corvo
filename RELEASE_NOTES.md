# Corvo 0.6.0

This release is about reaching things by name. A settings row that showed an
alias field but produced no search result could not be found at all, and a
shortcut recorded next to it stayed dead until the app restarted. Twelve of the
built-in commands, five feature commands, and the four quicklink commands were
in that state. They are emitted now, aliases resolve from the row you set them
on, and creating a snippet or a quicklink happens inside the launcher.

## Aliases and shortcuts

- An alias works on every row that offers one. Typing it brings the row up, including the first letters of the alias rather than only the whole thing, and scattered letters never match so a two-letter alias stays worth setting.
- A recorded shortcut takes effect at once. Shortcut collection no longer sits behind each section's "enabled" toggle, and the Navigation pane had no collection pass at all, so shortcuts recorded there did nothing.
- Turning a section off hides its rows from search and keeps its shortcuts, so a feature you switch off does not silently lose the keys bound to it.
- Renaming a snippet or a quicklink carries its alias, shortcut, and visibility over to the new name. Before, the row fell back to defaults and the old entry was left behind.
- Built-in rows are keyed by a stable identifier instead of the sentence they display. Rewording a row no longer discards the alias and shortcut saved under the old wording. Settings migrates entries saved under the old keys the first time it loads.

## Commands

- The Commands pane lists every built-in command in root search: About Corvo, Calculator History, Check for Updates, Define Word, Export and Import Backup, Import from Raycast, Open Camera, Quit Corvo, Reload Applications, Open Settings, and Toggle System Appearance. They had a settings row, an alias, and a shortcut but no result to show.
- The feature panes emit their command rows too: Clear Clipboard History, Search Snippets, Create Snippet, Switch Windows, and Search Menu Bar Items. Menu Bar search appears on macOS, matching the pane that shows it.
- Create Quicklink, Search Quicklinks, Import Quicklinks, and Export Quicklinks now work from root search. They were listed with no result and no action behind them.
- Typing `snippet:` or `quicklink:` searches only that kind of entry.

## Snippets and quicklinks

- Create Snippet and Create Quicklink open an editor over the launcher instead of opening Settings. The alias on a creation command now leads straight into the form.
- Cmd+E opens the selected snippet or quicklink for editing in place, and Cmd+N creates a new one inside a scoped search.
- Tab moves between fields, Enter saves, Enter inside a snippet body adds a line, and Escape cancels.
- Snippet rows open an editor for name, keyword, and body instead of writing a blank row that had to be edited by hand.
- Snippets and quicklinks expand `{clipboard}`, `{date}`, `{time}`, and `{datetime}` placeholders.

## Calculator

- Currency conversion in the palette: `10 usd to eur` or `900 clp in usd` convert through a USD pivot. Rates come from open.er-api.com, need no key, are cached for six hours, and are fetched on a background task that never blocks a keystroke. A bare amount converts to a default target and stays silent while the rates load.

## Search

- An exact alias match surfaces its row even when the alias text cannot fuzzy-match the row's own name.
- Aliases are matched from the start of the alias rather than only in full, and the row with an exact match ranks first.

## Clipboard

- History entries can be pasted directly with Cmd+1 through Cmd+9, with the selected entry pasted immediately.

## Platform

- A recorded application shortcut toggles the app: it focuses the app when another one is in front and hides it when it is already focused.
- Exact command aliases surface in search even without a text match, and alias ranks are honored across every section.

## Requirements

- No new permissions or configuration. Every change applies on macOS, Windows, and Linux; the Menu Bar command row appears on macOS only.
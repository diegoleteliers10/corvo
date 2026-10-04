# Manifest

What an extension declares about itself: identity plus its command
surface. Root search, the Settings command list, and hotkey binding
render from the manifest — no hand-written plumbing. Builders live
in `corvo_ext::manifest`; the types in `corvo_core::page`.

```rust
fn manifest(&self) -> ExtensionManifest {
    manifest::extension(
        "countdown",                    // == Command::id
        "Countdown",                    // display title
        "Days until a date",            // one-line description
        icon(),
        &["Productivity"],
        vec![/* CommandSpec entries */],
    )
}
```

## Command specs

| Field | Meaning |
|---|---|
| `name` | the command's name inside the extension |
| `title` | display name in root search and Settings |
| `description` | what it does, one sentence |
| `mode` | `View` opens a page; `NoView` acts and confirms with a toast |
| `icon` | optional override of the extension icon |
| `arguments` | typed argument slots, required first, up to 3 |
| `keywords` | root-search words |

Builders: `view_command`, `action_command`, then chain
`CommandSpecBuilder::argument(spec, arg)` and `::icon(spec, icon)`.

## Arguments

| Kind | Behavior |
|---|---|
| `text_argument(name, placeholder)` | free text |
| `required_text_argument(name, placeholder)` | the launcher keeps asking until filled |
| `password_argument(name, placeholder)` | masked input |
| `dropdown_argument(name, placeholder, options)` | fixed `(value, title)` choices |

The first argument's placeholder becomes the search bar hint when the
declared command is selected in root search; the typed value rides
into the command as the page query / argument segment.

## Quick commands

Every declared command is a quick command: root search lists it, and
a hotkey can target it:

- `command:<extension>:<name>` executes a `NoView` command directly —
  no launcher window; the command's toast is the confirmation.
- `View` commands open their page from the hotkey.

Declare the small verbs of your extension (`Play / Pause`, `Next
Track`, `Saved Countdowns`) as `NoView` commands so users can bind
them.

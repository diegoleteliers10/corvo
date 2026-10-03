# Actions

A result's actions menu opens with ⌘K (or the platform's primary
modifier + K). The command supplies the menu; the UI runs each
`Action` directly. Builders live in `corvo_ext::actions`.

## Building a menu

```rust
use corvo_ext::actions;

fn actions(&self, result_id: &str) -> Vec<CommandAction> {
    let Some(url) = result_id.strip_prefix("hackernews:story:") else {
        return Vec::new();
    };
    vec![
        actions::open_url("hackernews:open", url),
        actions::copy("hackernews:copy", url),
    ]
}
```

The first item is the menu's default; give it a hotkey when one is
obvious:

```rust
let mut open = actions::open_url("a:open", url);
open.hotkey = Some("cmd+o");
```

## Built-in builders

| Builder | Runs | Group |
|---|---|---|
| `copy(id, text)` | `Action::Copy` — clipboard + feedback | Standard |
| `open_url(id, url)` | `Action::OpenUrl` — system handler | Standard |
| `open_path(id, path)` | `Action::Open` — file or folder | Standard |
| `reveal_path(id, path)` | `Action::RunShell` — Finder / Explorer / `xdg-open` | Standard |
| `custom(id, label, action, icon)` | any concrete `Action` | Standard |
| `destructive(id, label, action)` | removal-class actions | Destructive |

`custom` exists for extension-specific work: express it as a concrete
`Action` the UI can run — `Action::RunProcess { program, args, title }`
to invoke a CLI, `Action::RunNative` for a system action, and so on.

## The Action catalog

These variants exist today; the UI interprets every one on all three
platforms:

`Open`, `OpenUrl`, `OpenFileSearch`, `OpenAppUninstaller`,
`Copy`, `PasteText`, `CopyImage`, `PasteImage`, `RunShell`,
`RunProcess`, `RunNative(NativeAction)`, `ShowToast`, `CloseWindow`,
`TileWindow`, `AdjustBrightness`, `AdjustVolume`,
`ConfirmProcessTermination`, `TerminateProcess`,
`SetResultFavorite`, `SetResultHidden`.

## Conventions

- **Labels** in Title Case, verb first: `Copy Link`, `Stop Service`.
- **Destructive** group only for deletion-class actions; the UI
  renders them apart, in red, and asks for confirmation where the
  action is irreversible.
- **No chained shell tricks** when a builder exists: `actions::copy`
  instead of `Action::RunShell("pbcopy ...")` — the clipboard path
  handles every platform and the concealment markers.
- **Menu ids** follow the same namespace rules as result ids:
  `{id}:{action}[:{data}]`.

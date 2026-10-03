# What extensions are

An extension is a Rust crate under `commands/` in the
[corvo repository](https://github.com/diegoleteliers10/corvo). It
implements the `Command` trait from `corvo-core`, registers itself
with one macro, and the launcher does the rest.

## The anatomy

```
commands/<your-id>/
├── Cargo.toml        # package name corvo-<your-id>, deps: corvo-core, corvo-ext
├── src/lib.rs        # the Command impl, data loading, tests
├── README.md         # only if setup is needed (tokens, permissions)
└── CHANGELOG.md      # one entry per merged change
```

Registration is compile-time. In `src/lib.rs`:

```rust
corvo_core::register_command!(YourCommand);
```

The build script scans `commands/*/Cargo.toml` and links every crate
into the binary automatically. You never edit `src/main.rs`.

## What a command can do

| You provide | The launcher provides |
|---|---|
| `id`, `keywords`, `priority` | placement in root search |
| `search(query)` → rows | the result list, fuzzy ranking, rendering |
| `execute(result_id)` → an `Action` | running the action (open, copy, shell, toast...) |
| `actions(result_id)` → a menu | the ⌘K actions panel |
| data, loaded off the search path | background task execution |

The full contract is on [The command contract](concepts.md).

## What an extension must not do

- **Never block `search`.** Disk, network, AppleScript, and CDP run
  in a background task behind a cache. `search` reads the cache.
- **Never depend on `corvo-ui`.** Extensions produce data; only the
  launcher renders.
- **Never reach for raw OS APIs in the crate.** OS work goes through
  `corvo-platform`, which owns every `#[cfg(target_os)]` in the
  workspace.
- **No telemetry, no analytics, no opaque bundled binaries.** Corvo
  sends nothing anywhere, and extensions must not either.

## The kit

`crates/corvo-ext` ships the shared patterns:

| Module | Purpose | Raycast analog |
|---|---|---|
| `list` | rows, sections, open entries, empty states | `List`, `List.Item`, `List.Section` |
| `actions` | action-menu builders | `ActionPanel`, built-in actions |
| `cache` | TTL cache for off-path loading | `Cache`, `useCachedPromise` |
| `prefs` | per-extension TOML settings | `Preferences` |
| `routing` | query and result-id dispatch | command lifecycle |
| `feedback` | toasts and notifications | `Toast`, `HUD` |

Each module has a chapter under [Module reference](modules/list.md).

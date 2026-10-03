# Preferences

Extensions that need user settings (a default city, a token, an
enabled feature) read a **typed TOML file** through
`corvo_ext::prefs`. Each extension owns exactly one file:

```
config_dir()/extensions/<extension-id>.toml
```

`config_dir()` is the per-user Corvo config directory
(`~/Library/Application Support/corvo` on macOS, `%APPDATA%\corvo` on
Windows, `~/.config/corvo` on Linux).

## Declaring and reading

```rust
use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(default)]
struct WeatherPrefs {
    city: String,
    units: Units,       // any Deserialize + Default type
}

impl Default for Units { /* ... */ }

let prefs = corvo_ext::prefs::load::<WeatherPrefs>("weather");
```

`#[serde(default)]` keeps unknown and missing keys harmless.

## Semantics

| Situation | Result |
|---|---|
| File missing | `T::default()` |
| Key missing | the field's default |
| Unknown keys | ignored |
| File unreadable or unparseable | `load` → `T::default()`; `load_result` → `Err(String)` |

Use `load_result` when a broken file should surface to the user
instead of silently reverting to defaults:

```rust
match corvo_ext::prefs::load_result::<WeatherPrefs>(ID) {
    Ok(prefs) => prefs,
    Err(message) => return vec![list::empty_state(ID, &message)],
}
```

## Rules

- **Read off the hot path.** Load once per search call at most —
  better once per warm-up — not inside a per-row loop.
- **No configuration commands.** A command whose only job is editing
  settings is a smell; the file is plain TOML and Settings already
  covers global toggles. Document the keys in the extension README.
- **Secrets stay out.** Preferences are plain files. If a token is
  required, store it per the README instructions and never log it.
  A standard secret store is planned.

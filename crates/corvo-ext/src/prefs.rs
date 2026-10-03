//! The Preferences module: typed per-extension settings.
//!
//! Each extension may own one TOML file at
//! `config_dir()/extensions/<extension-id>.toml`. Declare a settings
//! struct with serde derives and defaults, then read it — from the
//! execute path or a warm-up task, not per keystroke:
//!
//! ```ignore
//! #[derive(serde::Deserialize)]
//! #[serde(default)]
//! struct Prefs {
//!     city: String,
//!     days: u8,
//! }
//!
//! impl Default for Prefs {
//!     fn default() -> Self {
//!         Self { city: String::new(), days: 3 }
//!     }
//! }
//!
//! let prefs = corvo_ext::prefs::load::<Prefs>("weather");
//! ```
//!
//! Unknown keys are ignored, a missing or unreadable file yields
//! `Default`, and a broken file is reported through
//! [`load_result`] instead of failing the command.

use std::path::PathBuf;

use serde::de::DeserializeOwned;

/// Where this extension's preferences file lives.
pub fn prefs_path(extension_id: &str) -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "corvo")
        .map(|dirs| dirs.config_dir().join("extensions").join(format!("{extension_id}.toml")))
}

/// Reads and deserializes the extension's preferences, falling back
/// to `T::default()` when the file is missing or unreadable.
pub fn load<T: DeserializeOwned + Default>(extension_id: &str) -> T {
    load_result(extension_id).unwrap_or_default()
}

/// The same read as [`load`], surfacing file and parse failures so a
/// command can tell a broken configuration from an absent one.
pub fn load_result<T: DeserializeOwned + Default>(
    extension_id: &str,
) -> Result<T, String> {
    let Some(path) = prefs_path(extension_id) else {
        return Err("no home directory".into());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    toml::from_str(&text)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize, Default, PartialEq, Debug)]
    #[serde(default)]
    struct Sample {
        city: String,
        days: u8,
    }

    #[test]
    fn toml_roundtrip_ignores_unknown_keys() {
        let parsed: Sample = toml::from_str(r#"city = "Oslo"
days = 5
mystery = true"#)
            .expect("parses");
        assert_eq!(parsed, Sample { city: "Oslo".into(), days: 5 });
    }

    #[test]
    fn empty_table_fills_defaults() {
        let parsed: Sample = toml::from_str("").expect("parses");
        assert_eq!(parsed, Sample::default());
    }
}

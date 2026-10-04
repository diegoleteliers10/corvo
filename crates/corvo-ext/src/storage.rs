//! The Storage module: per-extension key-value persistence, the
//! LocalStorage analog. Values are strings, numbers, or booleans in
//! one JSON file per extension under
//! `data_dir()/extensions/<id>/storage.json`.
//!
//! Not for large payloads — write files for those. Not for secrets:
//! the file is plain JSON.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Number(f64),
    String(String),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

fn storage_path(extension_id: &str) -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "corvo").map(|dirs| {
        dirs.data_dir()
            .join("extensions")
            .join(extension_id)
            .join("storage.json")
    })
}

fn read_all(extension_id: &str) -> BTreeMap<String, Value> {
    let Some(path) = storage_path(extension_id) else {
        return BTreeMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_all(extension_id: &str, items: &BTreeMap<String, Value>) -> Result<(), String> {
    let Some(path) = storage_path(extension_id) else {
        return Err("no home directory".into());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(items).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text)
        .and_then(|_| std::fs::rename(&temporary, &path))
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Reads one value, `None` when the key is absent.
pub fn get(extension_id: &str, key: &str) -> Option<Value> {
    read_all(extension_id).remove(key)
}

/// Creates or updates one key. Blocking; keep it off the search path.
pub fn set(extension_id: &str, key: &str, value: Value) -> Result<(), String> {
    let mut items = read_all(extension_id);
    items.insert(key.to_owned(), value);
    write_all(extension_id, &items)
}

pub fn remove(extension_id: &str, key: &str) -> Result<(), String> {
    let mut items = read_all(extension_id);
    items.remove(key);
    write_all(extension_id, &items)
}

/// Every stored key and value.
pub fn all(extension_id: &str) -> BTreeMap<String, Value> {
    read_all(extension_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_roundtrip() {
        let value: Value = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(value.as_str(), Some("hello"));
        let value: Value = serde_json::from_str("true").unwrap();
        assert_eq!(value.as_bool(), Some(true));
        let value: Value = serde_json::from_str("42").unwrap();
        assert_eq!(value.as_f64(), Some(42.0));
    }
}

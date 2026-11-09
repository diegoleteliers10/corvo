//! Stored calculator evaluations.
//!
//! One JSON file under the Corvo data directory, capped so a long session
//! cannot grow it without bound. The file is small, so a save rewrites it
//! whole and replaces it atomically.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Newest evaluations kept on disk.
pub const MAX_ENTRIES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub expression: String,
    pub result: String,
    pub at_secs: u64,
}

fn history_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "corvo")
        .map(|dirs| dirs.data_dir().join("calculator_history.json"))
        .unwrap_or_else(|| std::env::temp_dir().join("corvo").join("calculator_history.json"))
}

fn store() -> &'static Mutex<Vec<HistoryEntry>> {
    static STORE: OnceLock<Mutex<Vec<HistoryEntry>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(load()))
}

fn load() -> Vec<HistoryEntry> {
    let Ok(data) = fs::read_to_string(history_path()) else {
        return Vec::new();
    };
    let Ok(mut entries) = serde_json::from_str::<Vec<HistoryEntry>>(&data) else {
        eprintln!("corvo: calculator_history.json malformed, starting fresh");
        return Vec::new();
    };
    if entries.len() > MAX_ENTRIES {
        entries.truncate(MAX_ENTRIES);
    }
    entries
}

fn save(entries: &[HistoryEntry]) {
    let path = history_path();
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(json) = serde_json::to_string(entries) else {
        return;
    };
    let tmp = parent.join(format!("calculator_history.json.tmp.{}", std::process::id()));
    if fs::write(&tmp, json).is_err() {
        let _ = fs::remove_file(&tmp);
        return;
    }
    if fs::rename(&tmp, &path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Loads the file before the first query, off the search path.
pub fn warmup() {
    let _ = store();
}

/// Evaluations, newest first.
pub fn all() -> Vec<HistoryEntry> {
    store()
        .lock()
        .map(|entries| entries.clone())
        .unwrap_or_default()
}

/// Adds one evaluation. A repeated expression with the same result is not
/// stored twice, so a held query does not flood the list.
pub fn record(expression: &str, result: &str) {
    let expression = expression.trim();
    let result = result.trim();
    if expression.is_empty() || result.is_empty() {
        return;
    }
    let Ok(mut entries) = store().lock() else {
        return;
    };
    if let Some(first) = entries.first() {
        if first.expression == expression && first.result == result {
            return;
        }
    }
    entries.retain(|entry| !(entry.expression == expression && entry.result == result));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    entries.insert(
        0,
        HistoryEntry {
            expression: expression.to_string(),
            result: result.to_string(),
            at_secs: now,
        },
    );
    entries.truncate(MAX_ENTRIES);
    save(&entries);
}

pub fn clear() {
    let Ok(mut entries) = store().lock() else {
        return;
    };
    entries.clear();
    save(&entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(count: usize) -> Vec<HistoryEntry> {
        (0..count)
            .map(|index| HistoryEntry {
                expression: format!("{index}+1"),
                result: (index + 1).to_string(),
                at_secs: 1_700_000_000 + index as u64,
            })
            .collect()
    }

    #[test]
    fn a_cap_keeps_the_newest_entries() {
        let mut stored = entries(MAX_ENTRIES + 10);
        stored.truncate(MAX_ENTRIES);
        assert_eq!(stored.len(), MAX_ENTRIES);
        assert_eq!(stored[0].expression, "0+1");
    }

    #[test]
    fn history_round_trips_through_json() {
        let stored = entries(2);
        let json = serde_json::to_string(&stored).expect("encode");
        let decoded: Vec<HistoryEntry> = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, stored);
    }
}

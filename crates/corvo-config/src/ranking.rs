//! Persistent frecency store with anchor decay math and learned query terms.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Half-life in milliseconds: 10 days.
pub const HALF_LIFE_MILLIS: f64 = 10.0 * 24.0 * 60.0 * 60.0 * 1000.0;
/// ln(2) / half_life
pub const DECAY_RATE: f64 = std::f64::consts::LN_2 / HALF_LIFE_MILLIS;
/// Score boost added on each visit.
pub const VISIT_BOOST: f64 = 100.0;
/// Maximum exponent for f64::exp to prevent overflow (+inf).
pub const EXP_MAX_CEILING: f64 = 709.78;
/// Learned terms time-to-live: 17 days in milliseconds.
pub const LEARNED_TERM_TTL_MILLIS: i64 = 17 * 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LearnedTerm {
    pub query: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrecencyEntry {
    /// Anchor timestamp in milliseconds.
    pub anchor: i64,
    /// Last opened timestamp in milliseconds.
    pub opened_at: i64,
    /// Last 3 distinct learned queries (folded, max 64 chars) with timestamp.
    #[serde(default)]
    pub terms: Vec<LearnedTerm>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FrecencyData {
    #[serde(default)]
    pub entries: HashMap<String, FrecencyEntry>,
}

/// Thread-safe in-memory cache backed by atomic file persistence.
pub struct FrecencyStore {
    data: FrecencyData,
    file_path: Option<PathBuf>,
}

static GLOBAL_STORE: OnceLock<Arc<Mutex<FrecencyStore>>> = OnceLock::new();

impl FrecencyStore {
    pub fn global() -> Arc<Mutex<Self>> {
        GLOBAL_STORE
            .get_or_init(|| {
                let path = ProjectDirs::from("", "", "corvo")
                    .map(|d| d.config_dir().join("frecency.toml"));
                let store = Self::load(path);
                Arc::new(Mutex::new(store))
            })
            .clone()
    }

    pub fn load(file_path: Option<PathBuf>) -> Self {
        let mut data = FrecencyData::default();
        if let Some(ref path) = file_path {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(loaded) = toml::from_str::<FrecencyData>(&content) {
                    data = loaded;
                }
            }
        }

        let now = now_millis();
        // Lazy expiration on load: prune expired terms
        for entry in data.entries.values_mut() {
            entry
                .terms
                .retain(|term| now.saturating_sub(term.updated_at) < LEARNED_TERM_TTL_MILLIS);
        }
        migrate_recent_app_entries(&mut data, now);

        Self { data, file_path }
    }

    /// Calculates current frecency score for an item in O(1) without mutation.
    pub fn get_frecency(&self, item_key: &str) -> f64 {
        let now = now_millis();
        self.get_frecency_at(item_key, now)
    }

    pub fn get_frecency_at(&self, item_key: &str, now: i64) -> f64 {
        match self.data.entries.get(item_key) {
            Some(entry) => {
                let delta = (entry.anchor - now) as f64;
                let exponent = (DECAY_RATE * delta).min(EXP_MAX_CEILING);
                exponent.exp().max(1.0)
            }
            None => 1.0,
        }
    }

    /// Returns learned terms for an item valid at the current time.
    pub fn get_learned_terms(&self, item_key: &str) -> Vec<String> {
        let now = now_millis();
        match self.data.entries.get(item_key) {
            Some(entry) => entry
                .terms
                .iter()
                .filter(|term| now.saturating_sub(term.updated_at) < LEARNED_TERM_TTL_MILLIS)
                .map(|term| term.query.clone())
                .collect(),
            None => Vec::new(),
        }
    }

    /// Records a user visit, updating the anchor and adding the query to learned terms.
    pub fn record_visit(&mut self, item_key: &str, query: Option<&str>) {
        let now = now_millis();
        let current_score = self.get_frecency_at(item_key, now);
        let new_score = current_score + VISIT_BOOST;

        let new_anchor = now + (new_score.ln() / DECAY_RATE).round() as i64;

        let entry = self
            .data
            .entries
            .entry(item_key.to_string())
            .or_insert_with(|| FrecencyEntry {
                anchor: new_anchor,
                opened_at: now,
                terms: Vec::new(),
            });

        entry.anchor = new_anchor;
        entry.opened_at = now;

        if let Some(q) = query {
            let trimmed = q.trim();
            if !trimmed.is_empty() {
                let mut folded = corvo_core::search::fold(trimmed);
                if folded.len() > 64 {
                    folded.truncate(64);
                }

                // Remove existing if duplicate
                entry.terms.retain(|t| t.query != folded);
                // Prepend to front (FIFO, cap 3)
                entry.terms.insert(
                    0,
                    LearnedTerm {
                        query: folded,
                        updated_at: now,
                    },
                );
                if entry.terms.len() > 3 {
                    entry.terms.truncate(3);
                }
            }
        }

        self.save();
    }

    /// Atomically persists frecency data to disk.
    pub fn save(&self) {
        let Some(ref path) = self.file_path else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(serialized) = toml::to_string(&self.data) {
            let temp_path = path.with_extension("tmp");
            if fs::write(&temp_path, serialized).is_ok() {
                let _ = fs::rename(&temp_path, path);
            }
        }
    }
}

fn migrate_recent_app_entries(data: &mut FrecencyData, now: i64) {
    let legacy_keys: Vec<String> = data
        .entries
        .keys()
        .filter(|key| key.starts_with("app-launcher:recent:"))
        .cloned()
        .collect();

    for legacy_key in legacy_keys {
        let Some(recent) = data.entries.remove(&legacy_key) else {
            continue;
        };
        let canonical_key = legacy_key.replacen("app-launcher:recent:", "app-launcher:", 1);
        if let Some(current) = data.entries.get_mut(&canonical_key) {
            let score = [current.anchor, recent.anchor]
                .into_iter()
                .map(|anchor| (DECAY_RATE * (anchor - now) as f64).min(EXP_MAX_CEILING).exp())
                .sum::<f64>()
                .max(1.0);
            current.anchor = now + (score.ln() / DECAY_RATE).round() as i64;
            current.opened_at = current.opened_at.max(recent.opened_at);
            current.terms.extend(recent.terms);
            current.terms.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            let mut seen = HashSet::new();
            current.terms.retain(|term| seen.insert(term.query.clone()));
            current.terms.truncate(3);
        } else {
            data.entries.insert(canonical_key, recent);
        }
    }
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_decay_halves_after_ten_days() {
        let mut store = FrecencyStore::load(None);
        let start = 1_000_000_000_000i64;

        // Simulate visit at start
        let initial_score = store.get_frecency_at("test-app", start);
        let new_score = initial_score + VISIT_BOOST;
        let anchor = start + (new_score.ln() / DECAY_RATE).round() as i64;
        store.data.entries.insert(
            "test-app".into(),
            FrecencyEntry {
                anchor,
                opened_at: start,
                terms: Vec::new(),
            },
        );

        let score_at_start = store.get_frecency_at("test-app", start);
        assert!((score_at_start - 101.0).abs() < 1.0);

        // 10 days later
        let ten_days_later = start + (10 * 24 * 60 * 60 * 1000);
        let score_10d = store.get_frecency_at("test-app", ten_days_later);
        // Half of 101 is ~50.5
        assert!((score_10d - 50.5).abs() < 1.0);
    }

    #[test]
    fn learned_terms_cap_at_three_fifo() {
        let mut store = FrecencyStore::load(None);
        store.record_visit("app", Some("term1"));
        store.record_visit("app", Some("term2"));
        store.record_visit("app", Some("term3"));
        store.record_visit("app", Some("term4"));

        let terms = store.get_learned_terms("app");
        assert_eq!(terms.len(), 3);
        assert_eq!(terms[0], "term4");
        assert_eq!(terms[1], "term3");
        assert_eq!(terms[2], "term2");
    }

    #[test]
    fn recent_app_history_moves_to_canonical_result() {
        let now = 1_000_000_000_000i64;
        let mut data = FrecencyData::default();
        data.entries.insert(
            "app-launcher:recent:/Safari".into(),
            FrecencyEntry {
                anchor: now + 1000,
                opened_at: now,
                terms: vec![LearnedTerm { query: "browser".into(), updated_at: now }],
            },
        );

        migrate_recent_app_entries(&mut data, now);

        assert!(!data.entries.contains_key("app-launcher:recent:/Safari"));
        let entry = data.entries.get("app-launcher:/Safari");
        assert_eq!(entry.map(|entry| entry.anchor), Some(now + 1000));
        assert_eq!(entry.map(|entry| entry.terms[0].query.as_str()), Some("browser"));
    }

    #[test]
    fn recent_app_history_merges_with_existing_history() {
        let now = 1_000_000_000_000i64;
        let mut data = FrecencyData::default();
        data.entries.insert(
            "app-launcher:recent:/Safari".into(),
            FrecencyEntry {
                anchor: now + 1000,
                opened_at: now - 10,
                terms: vec![LearnedTerm { query: "browser".into(), updated_at: now - 10 }],
            },
        );
        data.entries.insert(
            "app-launcher:/Safari".into(),
            FrecencyEntry {
                anchor: now + 2000,
                opened_at: now,
                terms: vec![LearnedTerm { query: "safari".into(), updated_at: now }],
            },
        );

        migrate_recent_app_entries(&mut data, now);

        assert!(!data.entries.contains_key("app-launcher:recent:/Safari"));
        let entry = data.entries.get("app-launcher:/Safari");
        assert!(entry.is_some_and(|entry| entry.anchor > now + 2000));
        assert_eq!(entry.map(|entry| entry.opened_at), Some(now));
        assert_eq!(entry.map(|entry| entry.terms.len()), Some(2));
    }
}

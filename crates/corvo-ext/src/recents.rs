//! The Recents module: the "Recently used" section the most-loved
//! extensions render first. Values live in the extension's storage
//! (`corvo_ext::storage`) under one key, deduplicated, newest first,
//! capped.
//!
//! ```ignore
//! const CAP: usize = 10;
//!
//! corvo_ext::recents::push(ID, "recent-emojis", emoji_id, CAP);
//! let recent: Vec<String> = corvo_ext::recents::list(ID, "recent-emojis");
//! ```

use super::storage;

const KEY_PREFIX: &str = "recents:";

fn bucket_key(bucket: &str) -> String {
    format!("{KEY_PREFIX}{bucket}")
}

/// Moves `value` to the front of the recents bucket, deduplicating
/// and capping the list. Blocking; keep it off the search path (call
/// it from `execute`, right after the user acts on an item).
pub fn push(extension_id: &str, bucket: &str, value: &str, cap: usize) -> Result<(), String> {
    let cap = cap.max(1);
    let mut items: Vec<String> = list(extension_id, bucket);
    items.retain(|item| item != value);
    items.insert(0, value.to_owned());
    items.truncate(cap);
    storage::set(
        extension_id,
        &bucket_key(bucket),
        storage::Value::String(serde_json::to_string(&items).map_err(|error| error.to_string())?),
    )
}

/// The bucket's values, newest first.
pub fn list(extension_id: &str, bucket: &str) -> Vec<String> {
    let Some(value) = storage::get(extension_id, &bucket_key(bucket)) else {
        return Vec::new();
    };
    let Some(text) = value.as_str() else {
        return Vec::new();
    };
    serde_json::from_str(text).unwrap_or_default()
}

/// Drops every value in the bucket.
pub fn clear(extension_id: &str, bucket: &str) -> Result<(), String> {
    storage::remove(extension_id, &bucket_key(bucket))
}

#[cfg(test)]
mod tests {
    #[test]
    fn values_roundtrip_through_json() {
        let items = vec!["a".to_owned(), "b".to_owned()];
        let text = serde_json::to_string(&items).unwrap();
        let parsed: Vec<String> = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, items);
    }
}

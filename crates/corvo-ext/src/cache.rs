//! The Cache module: the time-to-live pattern for off-search-path
//! data.
//!
//! `search` must stay non-blocking, so slow work (AppleScript, CDP,
//! HTTP, disk scans) happens on a background task through
//! `smol::unblock`, and `search` reads a cache. `TtlCache` is that
//! cache: drop it in a `static` with `OnceLock`, warm it off the
//! search path, read it on the search path.
//!
//! ```ignore
//! static CACHE: OnceLock<corvo_ext::cache::TtlCache<Weather>> = OnceLock::new();
//!
//! fn cache() -> &'static corvo_ext::cache::TtlCache<Weather> {
//!     static C: std::sync::OnceLock<corvo_ext::cache::TtlCache<Weather>> =
//!         std::sync::OnceLock::new();
//!     C.get_or_init(corvo_ext::cache::TtlCache::new)
//! }
//!
//! // Off the search path (UI spawn + smol::unblock):
//! let weather = cache().get_or_fetch(Duration::from_secs(900), fetch_weather);
//!
//! // On the search path — never touches the network:
//! let maybe = cache().get(Duration::from_secs(900));
//! ```

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct TtlCache<T> {
    entry: Mutex<Option<(Instant, T)>>,
}

impl<T: Clone> Default for TtlCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> TtlCache<T> {
    pub fn new() -> Self {
        Self {
            entry: Mutex::new(None),
        }
    }

    /// The cached value when it is younger than `ttl`.
    pub fn get(&self, ttl: Duration) -> Option<T> {
        let guard = self.entry.lock().ok()?;
        let (fetched_at, value) = guard.as_ref()?;
        (fetched_at.elapsed() < ttl).then(|| value.clone())
    }

    /// The cached value when fresh, else `fetch()`, stored under the
    /// same lock after the fetch completes. Blocking; call it from an
    /// unblock executor, never from `search`.
    pub fn get_or_fetch(&self, ttl: Duration, fetch: impl FnOnce() -> T) -> T {
        if let Some(cached) = self.get(ttl) {
            return cached;
        }
        let fetched = fetch();
        self.store(fetched.clone());
        fetched
    }

    /// Stores a value, restarting the clock.
    pub fn store(&self, value: T) {
        if let Ok(mut guard) = self.entry.lock() {
            *guard = Some((Instant::now(), value));
        }
    }

    /// Drops the cached value so the next read refetches. Call after
    /// a mutation the cache would hide.
    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.entry.lock() {
            *guard = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetches_once_within_ttl() {
        let cache = TtlCache::new();
        let mut calls = 0;
        let value = |calls: &mut usize| -> u32 {
            *calls += 1;
            41
        };
        let first = cache.get_or_fetch(Duration::from_secs(60), || value(&mut calls));
        let second = cache.get_or_fetch(Duration::from_secs(60), || value(&mut calls));
        assert_eq!((first, second), (41, 41));
        assert_eq!(calls, 1);
    }

    #[test]
    fn invalidate_forces_a_refetch() {
        let cache = TtlCache::new();
        cache.store("old");
        cache.invalidate();
        assert_eq!(cache.get(Duration::from_secs(60)), None);
        let fetched = cache.get_or_fetch(Duration::from_secs(60), || "new");
        assert_eq!(fetched, "new");
    }

    #[test]
    fn empty_ttl_reads_nothing() {
        let cache = TtlCache::new();
        cache.store("value");
        assert_eq!(cache.get(Duration::ZERO), None);
    }
}

# Cache

`corvo_ext::cache::TtlCache` is the standard time-to-live cache. It
exists because of one rule: **`search` never blocks.**

## The pattern

Every command with slow data — HTTP, AppleScript, CDP, SQLite, file
scans — uses the same three pieces:

1. **A static cache** in the command crate:

   ```rust
   use corvo_ext::cache::TtlCache;

   fn cache() -> &'static TtlCache<Vec<Tab>> {
       static CACHE: std::sync::OnceLock<TtlCache<Vec<Tab>>> = std::sync::OnceLock::new();
       CACHE.get_or_init(TtlCache::new)
   }
   ```

2. **A warm-up fetch** the UI calls off the search path (through
   `smol::unblock`), which fills the cache:

   ```rust
   pub fn fetch_tabs() -> Vec<Tab> {
       cache().get_or_fetch(Duration::from_secs(5), load_tabs_from_os)
   }
   ```

3. **A cache-only search**, which may answer empty before the first
   warm-up lands:

   ```rust
   let tabs = cache().get(Duration::from_secs(5)).unwrap_or_default();
   ```

## API

| Method | Behavior |
|---|---|
| `get(ttl)` | the cached value when younger than `ttl`, else `None` |
| `get_or_fetch(ttl, f)` | `get`, else `f()` stored under the same lock; blocking, off-path only |
| `store(value)` | write and restart the clock |
| `invalidate()` | drop the value so the next read refetches |

## House TTLs

| Data | TTL | Example |
|---|---|---|
| OS state that changes while you watch | 5 s | open tabs, now playing |
| Documents and files | 60 s | notes index |
| Network payloads | 15 min | weather |
| Expensive installs | 24 h+ | application list (rebuilt on launcher open) |

## Rules

- **Invalidate after mutations.** `media-control` clears its cache
  after a transport command so the next read reflects the change.
- **Never call `get_or_fetch` from `search`.** That is the blocking
  bug the cache exists to prevent. The only exception is an
  instant, deterministic source — and then say so in a comment.
- **Errors are not cached.** A failed fetch returns `None`/empty and
  tries again on the next warm-up; do not store failure states users
  cannot clear.

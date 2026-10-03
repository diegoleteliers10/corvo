# The command contract

Every extension implements one trait, `corvo_core::Command`:

```rust
#[async_trait::async_trait]
pub trait Command: Send + Sync {
    fn id(&self) -> &'static str;
    fn keywords(&self) -> &'static [&'static str] { &[] }
    fn prefix(&self) -> Option<&'static str> { None }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult>;
    async fn execute(&self, result_id: &str, ctx: &ExecutionContext)
        -> Result<Action, CommandError>;

    fn actions(&self, result_id: &str) -> Vec<CommandAction> { Vec::new() }
    fn priority(&self) -> u8 { 50 }
}
```

## Identity

- **`id`** — kebab-case, equal to the crate directory name. It is the
  namespace of every result id, the page query, and the preferences
  file. One registry-wide rule: ids never collide.
- **`keywords`** — words that route a query to this command. The
  first word of the query, lowercased, is matched against them.
- **`priority`** — wins ties when two commands answer the same
  query. Default 50; raise only with a reason.

## Result ids

Rows and results carry namespaced ids:

| Shape | Meaning | Example |
|---|---|---|
| `{id}:open` | opens the command's page | `weather:open` |
| `{id}:{action}` | a concrete action | `media:toggle` |
| `{id}:{action}:{data}` | an action with data | `weather:open:oslo` |
| `{id}:empty` | an empty-state hint row | `notes:empty` |

The data segment travels inside the id, so keep it stable and
executable. Data that may contain `:` (a URL) is fine as the last
segment — split on the first one:

```rust
let Some((action, data)) = key.split_once(':') else { ... };
```

## The search path

`search` runs on every keystroke. The rules:

1. **No blocking work.** No network, no processes, no file trees.
2. **Read the cache.** `TtlCache::get` is the only source of slow
   data; it may answer `None` before the first warm-up lands.
3. **Keep answers stable.** Empty query → usually nothing (root
   search is shared); page queries (`{id}-page:{filter}`) answer from
   the cache and never launch anything.
4. **Sections stay contiguous.** Sort by section, then score, or the
   UI cannot group them under one header.

## The execute path

`execute` may do slow work — wrap it in `smol::unblock`:

```rust
return smol::unblock(move || do_slow_thing(&data))
    .await
    .then_some(Action::CloseWindow)
    .ok_or(CommandError::Platform("failed".into()));
```

Errors are values: `CommandError::NotFound` for unknown ids,
`CommandError::Platform(String)` for OS refusals,
`CommandError::Unsupported` for not-yet-here platforms. The UI
surfaces them; never `unwrap`, never panic.

## Caching discipline

- TTLs: 5 s for fast-changing OS state, 15 min for network payloads
  is the house style (see `weather`).
- Invalidate after a mutation the cache would hide
  (`cache.invalidate()`).
- Fetch coalescing: the UI debounces and guards with `search_seq`;
  commands only need thread-safe caches.

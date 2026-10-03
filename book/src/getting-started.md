# Build your first extension

This tutorial turns the template into a working extension. It takes
about ten minutes and touches every part of the contract once.

## 1. Copy the template

```sh
cp -R templates/extension commands/wordcount
mv commands/wordcount/src/lib.rs commands/wordcount/src/lib.rs   # already correct
```

## 2. Rename it

In `commands/wordcount/Cargo.toml`:

```toml
[package]
name = "corvo-wordcount"
```

In `commands/wordcount/src/lib.rs`:

```rust
const ID: &str = "wordcount";
const KEYWORDS: &[&str] = &["wordcount", "count words"];
```

The id must equal the crate directory name. It namespaces every
result id, the page query, and the preferences file.

## 3. Register it

Two lines in the root `Cargo.toml` — nothing else:

```toml
[workspace]
members = [
    # ...
    "commands/wordcount",
]

[dependencies]
corvo-wordcount = { path = "commands/wordcount" }
```

## 4. Replace the demo data

The template's catalog is a static table. Make it yours — parse a
file, read a database, or fetch an API. The one rule: **loading runs
off the search path, behind a `TtlCache`**.

```rust
fn cache() -> &'static TtlCache<Words> {
    static CACHE: std::sync::OnceLock<TtlCache<Words>> = std::sync::OnceLock::new();
    CACHE.get_or_init(TtlCache::new)
}
```

`search` reads `cache().get(TTL)` — possibly `None`, that is fine.
The launcher calls `fetch_*`-style warm-ups through
`smol::unblock`; follow `commands/weather` for the shape. Only
`execute` may do slow work inline (wrapped in `smol::unblock`).

## 5. Shape the results

Rows come from the List module:

```rust
ListItem::new(word)
    .subtitle("used 42 times")
    .icon(Icon::File)
    .accessory("42")
    .section("Frequent")
    .build(format!("{ID}:word:{word}"), score)
```

Consecutive rows sharing a `section` render under one header.

## 6. Route and act

`execute` strips the `{id}:` prefix and answers:

```rust
let key = routing::key(result_id, ID)?;
match key {
    "open" => Ok(feedback::toast("Wordcount")),
    key => match key.strip_prefix("word:") {
        Some(word) => Ok(Action::Copy(word.to_owned())),
        None => Err(CommandError::NotFound),
    },
}
```

## 7. Verify

```sh
cargo test -p corvo-wordcount
cargo clippy --workspace --all-targets -- -D warnings
cargo run            # the extension is already in root search
```

CI runs the same checks on Windows, macOS, and Linux, plus the
template check.

## 8. Document and ship

- Fill `README.md` if your extension needs setup (tokens, enabled
  services, permissions).
- Add a `CHANGELOG.md` entry:
  `## [Count words in text] - 2026-10-03`.
- Open a pull request against `main`. The PR template carries the
  review checklist; the full standard is in
  [Contribution standard](contributing.md).

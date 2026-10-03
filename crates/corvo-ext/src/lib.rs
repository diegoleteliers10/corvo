//! # corvo-ext — the extension kit
//!
//! The standard library every Corvo command builds on. It mirrors the
//! module vocabulary of launcher extension platforms: `list` for
//! building result rows and sections, `actions` for result action
//! menus, `cache` for the off-search-path TTL pattern, `prefs` for
//! per-extension preferences, `routing` for query and execute
//! dispatch, and `feedback` for toasts and notifications.
//!
//! A command crate depends on `corvo-core` (the `Command` trait and
//! types), this crate (the patterns), and optionally `corvo-platform`
//! (OS primitives). Never on `corvo-ui`.
//!
//! See the extension book (`book/`) for the full guide.

pub mod actions;
pub mod cache;
pub mod feedback;
pub mod list;
pub mod prefs;
pub mod routing;

pub use corvo_core::{
    search_match_score, Action, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult,
};

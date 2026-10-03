//! The Routing module: the two dispatch points of every command.
//!
//! Root search routes a query to a command by keywords; execute
//! routes a result id to an action by its `{command-id}:{action}`
//! prefix. These helpers keep both shapes identical across
//! extensions.

use corvo_core::CommandError;

/// The first word of the query, lowercased, for keyword routing:
/// `weather oslo` routes on `weather` and keeps `oslo` as the
/// argument.
pub fn first_word(query: &str) -> String {
    query
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// Strips the command's `{command-id}:` prefix off a result id, or
/// reports `NotFound` — the standard execute entry:
///
/// ```ignore
/// let key = routing::key(result_id, "weather")?;
/// match key {
///     "open" => ...,
///     _ => Err(CommandError::NotFound),
/// }
/// ```
pub fn key<'a>(result_id: &'a str, command_id: &str) -> Result<&'a str, CommandError> {
    result_id
        .strip_prefix(&format!("{command_id}:"))
        .ok_or(CommandError::NotFound)
}

/// Builds the page query the UI sends to a command's dedicated page:
/// `{command-id}-page:{filter}`. The command answers it from cache.
pub fn page_query(command_id: &str, filter: &str) -> String {
    format!("{command_id}-page:{filter}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_word_lowercases_the_head() {
        assert_eq!(first_word("Weather Oslo"), "weather");
        assert_eq!(first_word("  next  "), "next");
        assert_eq!(first_word(""), "");
    }

    #[test]
    fn key_strips_the_namespace_or_fails() {
        assert_eq!(key("weather:open:oslo", "weather").unwrap(), "open:oslo");
        assert_eq!(
            key("other:open", "weather").unwrap_err(),
            CommandError::NotFound
        );
        assert_eq!(
            key("noprefix", "weather").unwrap_err(),
            CommandError::NotFound
        );
    }

    #[test]
    fn page_query_uses_the_dash_namespace() {
        assert_eq!(page_query("weather", "oslo"), "weather-page:oslo");
        assert_eq!(page_query("weather", ""), "weather-page:");
    }
}

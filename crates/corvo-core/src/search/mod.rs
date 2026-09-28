pub mod dp;
pub mod order;
pub mod relevance;
pub mod text;

pub use dp::{SearchSensitivity, match_launcher_dp};
pub use order::{CandidateItem, LauncherOrder, LearnedTermMatch, natural_cmp};
pub use relevance::{FieldRole, Match, Tier, calculate_shape, cell_weight, quality};
pub use text::{SearchText, fold, is_separator};

/// Returns true when a query selects a command prefix as a separate word.
pub fn matches_command_prefix(query: &str, prefix: &str) -> bool {
    let query = query.trim_start();
    let prefix = prefix.trim_end();
    if prefix.is_empty() {
        return false;
    }
    let Some(matched) = query.get(..prefix.len()) else {
        return false;
    };
    matched.eq_ignore_ascii_case(prefix)
        && query[prefix.len()..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

/// Backward-compatible search match scorer returning quality score in integer points.
/// The first field is treated as primary `FieldRole::Name`, and remaining fields as `FieldRole::Subtitle`.
pub fn search_match_score(query: &str, fields: &[&str]) -> Option<i32> {
    if query.trim().is_empty() || fields.is_empty() {
        return None;
    }

    let q = SearchText::new(query);
    let field_texts: Vec<SearchText> = fields.iter().map(|f| SearchText::new(f)).collect();

    let role_fields: Vec<(FieldRole, &SearchText)> = field_texts
        .iter()
        .enumerate()
        .map(|(idx, st)| {
            let role = if idx == 0 {
                FieldRole::Name
            } else if idx == 1 {
                FieldRole::Subtitle
            } else {
                FieldRole::Keyword
            };
            (role, st)
        })
        .collect();

    quality(&q, &role_fields)
}

#[cfg(test)]
mod prefix_tests {
    use super::*;

    #[test]
    fn command_prefix_requires_a_word_boundary() {
        assert!(matches_command_prefix("brew", "brew"));
        assert!(matches_command_prefix("brew install", "brew"));
        assert!(matches_command_prefix("  KILL 3000", "kill "));
        assert!(!matches_command_prefix("brewing", "brew"));
        assert!(!matches_command_prefix("skill", "kill "));
    }
}

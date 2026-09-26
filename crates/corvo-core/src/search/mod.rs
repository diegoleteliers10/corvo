pub mod dp;
pub mod order;
pub mod relevance;
pub mod text;

pub use dp::{match_launcher_dp, SearchSensitivity};
pub use order::{natural_cmp, CandidateItem, LauncherOrder, LearnedTermMatch};
pub use relevance::{calculate_shape, cell_weight, quality, FieldRole, Match, Tier};
pub use text::{fold, is_separator, SearchText};

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

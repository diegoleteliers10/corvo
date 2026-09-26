use super::text::SearchText;

/// Search sensitivity threshold configured by user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchSensitivity {
    Low,
    #[default]
    Medium,
    High,
}

impl SearchSensitivity {
    /// Determines whether an alignment score satisfies the sensitivity threshold.
    pub fn accepts(&self, score: i32, query_len: usize) -> bool {
        if score <= 0 {
            return false;
        }
        match self {
            Self::Low => true,
            Self::Medium => {
                if query_len <= 1 {
                    score >= 2
                } else {
                    let threshold = (1.5 * (query_len as f64 - 2.0) + 4.0).ceil() as i32;
                    score >= threshold
                }
            }
            Self::High => score > (2 * query_len) as i32,
        }
    }
}

/// Computes the LauncherMatch score for a query against a target candidate using banded DP.
/// Returns `Some(score)` if aligned and passes sensitivity, or `None` if discarded.
pub fn match_launcher_dp(
    query: &SearchText,
    target: &SearchText,
    sensitivity: SearchSensitivity,
) -> Option<i32> {
    if query.units.is_empty() {
        return None;
    }
    if target.units.is_empty() {
        return None;
    }

    // 1. Exact match shortcut
    if query.units == target.units {
        return Some(100_000);
    }

    // 2. Precheck 1: Non-separator count must fit in target
    let query_letters_count = query.units.iter().filter(|&&u| !is_unit_separator(u)).count();
    let target_letters_count = target.units.iter().filter(|&&u| !is_unit_separator(u)).count();
    if query_letters_count > target_letters_count {
        return None;
    }

    // 3. Precheck 2: Rough subsequence check (when query > 2 letters)
    if query_letters_count > 2 && !is_rough_subsequence(&query.units, &target.units) {
        return None;
    }

    // 4. Banded dynamic programming alignment
    let score = align_banded(query, target)?;

    // 5. Sensitivity threshold check
    if sensitivity.accepts(score, query_letters_count) {
        Some(score)
    } else {
        None
    }
}

/// Greedy test to check whether the letters of the query appear as a subsequence in target.
fn is_rough_subsequence(query: &[u16], target: &[u16]) -> bool {
    let mut q_idx = 0;
    let q_len = query.len();
    let t_len = target.len();

    // Skip initial separators
    while q_idx < q_len && is_unit_separator(query[q_idx]) {
        q_idx += 1;
    }

    let mut t_idx = 0;
    while q_idx < q_len && t_idx < t_len {
        let q_ch = query[q_idx];
        if is_unit_separator(q_ch) {
            // Query separators can skip
            q_idx += 1;
            continue;
        }

        if target[t_idx] == q_ch {
            q_idx += 1;
        }
        t_idx += 1;
    }

    // Verify all non-separators matched
    while q_idx < q_len && is_unit_separator(query[q_idx]) {
        q_idx += 1;
    }
    q_idx == q_len
}

/// Banded DP alignment with running maximum.
fn align_banded(query: &SearchText, target: &SearchText) -> Option<i32> {
    let q_units = &query.units;
    let t_units = &target.units;
    let q_len = q_units.len();
    let t_len = t_units.len();

    if q_len > t_len {
        return None;
    }

    // Stack scratch buffer for candidate lengths up to 128, heap fallback otherwise
    let mut prev_row = vec![i32::MIN; t_len];
    let mut curr_row = vec![i32::MIN; t_len];

    let mut anchor: usize = 0;

    for (row, &q_char) in q_units.iter().enumerate() {
        let remaining_letters = q_len - 1 - row;
        let band_start = if row == 0 { 0 } else { anchor + 1 };
        let band_end = t_len.saturating_sub(remaining_letters);

        if band_start >= t_len || band_start > band_end {
            return None;
        }

        curr_row.fill(i32::MIN);
        let mut row_max = i32::MIN;
        let mut first_match_col: Option<usize> = None;

        for col in band_start..=band_end.min(t_len - 1) {
            let t_char = t_units[col];

            let matches = if is_unit_separator(q_char) && is_unit_separator(t_char) {
                true
            } else {
                q_char == t_char
            };

            if matches {
                let letter_score = if is_unit_separator(q_char) {
                    1
                } else if col == 0 {
                    4
                } else if is_unit_separator(t_units[col - 1]) || target.humps.contains(&col) {
                    3
                } else {
                    2
                };

                let score_from_prev = if row == 0 {
                    letter_score
                } else {
                    // Check previous adjacent match (no penalty)
                    let adjacent = prev_row[col - 1];
                    // Check previous gap match (penalty -1)
                    let gap = if col >= 2 {
                        prev_row[..col - 1].iter().copied().max().unwrap_or(i32::MIN)
                    } else {
                        i32::MIN
                    };

                    let best_prev = adjacent.max(if gap != i32::MIN { gap - 1 } else { i32::MIN });
                    if best_prev == i32::MIN {
                        continue;
                    }
                    best_prev + letter_score
                };

                curr_row[col] = score_from_prev;
                if score_from_prev > row_max {
                    row_max = score_from_prev;
                }
                if first_match_col.is_none() {
                    first_match_col = Some(col);
                }
            }
        }

        let first_col = first_match_col?;
        anchor = first_col;
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    let final_max = prev_row.iter().copied().max()?;
    if final_max > 0 {
        Some(final_max)
    } else {
        None
    }
}

#[inline]
fn is_unit_separator(unit: u16) -> bool {
    if unit > 127 {
        return false;
    }
    let ch = unit as u8 as char;
    matches!(ch, ' ' | '-' | '_' | '.' | '/' | '\\' | ':' | '@' | '(' | ')' | '[' | ']' | '{' | '}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_gives_max_score() {
        let q = SearchText::new("chrome");
        let t = SearchText::new("chrome");
        let score = match_launcher_dp(&q, &t, SearchSensitivity::Medium);
        assert_eq!(score, Some(100_000));
    }

    #[test]
    fn matches_subsequence_with_humps() {
        let q = SearchText::new("chrme");
        let t = SearchText::new("Google Chrome");
        let score = match_launcher_dp(&q, &t, SearchSensitivity::Medium);
        assert!(score.is_some());
    }

    #[test]
    fn matches_acronym_vscode() {
        let q = SearchText::new("vscode");
        let t = SearchText::new("Visual Studio Code");
        let score = match_launcher_dp(&q, &t, SearchSensitivity::Medium);
        assert!(score.is_some());
    }

    #[test]
    fn sensitivity_filtering() {
        let q = SearchText::new("xyz");
        let t = SearchText::new("Visual Studio Code");
        let score = match_launcher_dp(&q, &t, SearchSensitivity::Low);
        assert!(score.is_none());
    }
}

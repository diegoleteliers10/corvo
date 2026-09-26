use super::text::SearchText;

/// Roles representing candidate search fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldRole {
    Name,
    Alias,
    Owner,
    Subtitle,
    Keyword,
    Path,
}

/// Match tier ordered strictly by confidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Subsequence = 1,
    Substring = 2,
    WordStart = 3,
    Prefix = 4,
    Exact = 5,
}

/// Metadata describing how a query matched a target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub tier: Tier,
    pub offset: usize,
    pub query_len: usize,
    pub candidate_len: usize,
    pub spread: usize,
}

/// Fixed cell values for (FieldRole, Tier) combinations.
/// Higher numbers represent higher priority product decisions.
pub fn cell_weight(role: FieldRole, tier: Tier) -> Option<i32> {
    match (role, tier) {
        (FieldRole::Name | FieldRole::Alias, Tier::Exact) => Some(6500),
        (FieldRole::Name | FieldRole::Alias, Tier::Prefix) => Some(3000),
        (FieldRole::Owner, Tier::Exact) => Some(2500),
        (FieldRole::Name | FieldRole::Alias, Tier::WordStart) => Some(2400),
        (FieldRole::Owner, Tier::Prefix) => Some(2000),
        (FieldRole::Name | FieldRole::Alias, Tier::Substring) => Some(1800),
        (FieldRole::Keyword, Tier::Exact) => Some(1700),
        (FieldRole::Subtitle, Tier::Exact) => Some(1500),
        (FieldRole::Owner, Tier::WordStart) => Some(1400),
        (FieldRole::Subtitle, Tier::Prefix) => Some(1200),
        (FieldRole::Subtitle, Tier::WordStart) => Some(1100),
        (FieldRole::Name | FieldRole::Alias, Tier::Subsequence) => Some(1000),
        (FieldRole::Subtitle, Tier::Substring) => Some(900),
        (FieldRole::Keyword, Tier::Prefix) => Some(850),
        (FieldRole::Keyword, Tier::WordStart) => Some(800),
        (FieldRole::Keyword, Tier::Substring) => Some(700),
        (FieldRole::Path, Tier::Exact) => Some(450),
        (FieldRole::Path, Tier::Prefix) => Some(400),
        (FieldRole::Path, Tier::WordStart) => Some(350),
        (FieldRole::Path, Tier::Substring) => Some(300),
        (FieldRole::Path, Tier::Subsequence) => Some(100),
        // Subtitle, keyword, and owner subsequences are strictly discarded (shared descriptions/words would otherwise flood the results)
        (FieldRole::Subtitle, Tier::Subsequence) => None,
        (FieldRole::Keyword, Tier::Subsequence) => None,
        (FieldRole::Owner, Tier::Substring) => Some(200),
        (FieldRole::Owner, Tier::Subsequence) => None,
    }
}

/// Calculates internal tier shape score in 0..99.
/// Formula: 0.6 * coverage + 0.4 * positional
pub fn calculate_shape(m: &Match) -> i32 {
    let q = m.query_len as f64;
    let c = m.candidate_len as f64;
    if q <= 0.0 || c <= 0.0 {
        return 0;
    }

    let coverage = (q / c).min(1.0);

    let positional = match m.tier {
        Tier::Exact | Tier::Prefix | Tier::WordStart | Tier::Substring => {
            1.0 / (1.0 + (m.offset as f64) / 4.0)
        }
        Tier::Subsequence => {
            // theoretical maximum reference spread for contiguous match from index 0:
            // 13 + (q - 1) + 3 * q * (q - 1) / 2
            let reference_spread = 13.0 + (q - 1.0) + (3.0 * q * (q - 1.0) / 2.0);
            let actual = m.spread as f64;
            if actual <= 0.0 {
                0.0
            } else {
                (actual / reference_spread).min(1.0)
            }
        }
    };

    let normalized = 0.6 * coverage + 0.4 * positional;
    ((normalized * 99.0).round() as i32).clamp(0, 99)
}

/// Matches a query against a target candidate to find its highest Tier and Match characteristics.
pub fn match_tier(query: &SearchText, target: &SearchText) -> Option<Match> {
    let q = &query.units;
    let t = &target.units;
    let q_len = q.len();
    let t_len = t.len();

    if q_len == 0 || t_len == 0 || q_len > t_len {
        return None;
    }

    // 1. Exact match
    if q == t {
        return Some(Match {
            tier: Tier::Exact,
            offset: 0,
            query_len: q_len,
            candidate_len: t_len,
            spread: 0,
        });
    }

    // 2. Prefix match
    if t.starts_with(q) {
        return Some(Match {
            tier: Tier::Prefix,
            offset: 0,
            query_len: q_len,
            candidate_len: t_len,
            spread: q_len,
        });
    }

    // 3. WordStart match
    for &hump_idx in &target.humps {
        if hump_idx + q_len <= t_len && &t[hump_idx..hump_idx + q_len] == q {
            return Some(Match {
                tier: Tier::WordStart,
                offset: hump_idx,
                query_len: q_len,
                candidate_len: t_len,
                spread: q_len,
            });
        }
    }

    // 4. Substring match
    if let Some(pos) = t.windows(q_len).position(|window| window == q) {
        return Some(Match {
            tier: Tier::Substring,
            offset: pos,
            query_len: q_len,
            candidate_len: t_len,
            spread: q_len,
        });
    }

    // 5. Greedy subsequence match with scoring
    if let Some((offset, spread)) = match_subsequence_greedy(query, target) {
        return Some(Match {
            tier: Tier::Subsequence,
            offset,
            query_len: q_len,
            candidate_len: t_len,
            spread,
        });
    }

    None
}

/// Greedy subsequence matcher:
/// +1 base, streak +3*streak, text start +12, after non-alphanumeric or hump +8.
fn match_subsequence_greedy(query: &SearchText, target: &SearchText) -> Option<(usize, usize)> {
    let q = &query.units;
    let t = &target.units;
    let q_len = q.len();
    let t_len = t.len();

    let mut q_idx = 0;
    let mut t_idx = 0;
    let mut first_match_offset = 0;
    let mut last_match_offset = 0;
    let mut score = 0usize;
    let mut streak = 0usize;

    while q_idx < q_len && t_idx < t_len {
        if q[q_idx] == t[t_idx] {
            if q_idx == 0 {
                first_match_offset = t_idx;
            }
            last_match_offset = t_idx;

            // Score components
            score += 1;
            streak += 1;
            score += 3 * streak;

            if t_idx == 0 {
                score += 12;
            } else if target.humps.contains(&t_idx) {
                score += 8;
            }

            q_idx += 1;
        } else {
            streak = 0;
        }
        t_idx += 1;
    }

    if q_idx == q_len {
        let span = last_match_offset.saturating_sub(first_match_offset) + 1;
        let effective_spread = score.max(span);
        Some((first_match_offset, effective_spread))
    } else {
        None
    }
}

/// Evaluates fields under their roles against the query, returning the maximum `cell + shape` quality score.
pub fn quality(query: &SearchText, fields: &[(FieldRole, &SearchText)]) -> Option<i32> {
    let mut best_score = None;

    for (role, text) in fields {
        if let Some(m) = match_tier(query, text) {
            if let Some(cell) = cell_weight(*role, m.tier) {
                let shape = calculate_shape(&m);
                let total = cell + shape;
                best_score = Some(best_score.map_or(total, |prev: i32| prev.max(total)));
            }
        }
    }

    best_score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_name_scores_highest() {
        let q = SearchText::new("chrome");
        let name = SearchText::new("chrome");
        let q_score = quality(&q, &[(FieldRole::Name, &name)]).expect("matches");
        // cell(6500) + shape(99) = 6599
        assert_eq!(q_score, 6599);
    }

    #[test]
    fn prefix_beats_subsequence() {
        let q = SearchText::new("chr");
        let prefix = SearchText::new("chrome");
        let subseq = SearchText::new("cache router");

        let s1 = quality(&q, &[(FieldRole::Name, &prefix)]).unwrap();
        let s2 = quality(&q, &[(FieldRole::Name, &subseq)]).unwrap();
        assert!(s1 > s2);
    }

    #[test]
    fn owner_subsequence_is_discarded() {
        let q = SearchText::new("apn");
        let owner = SearchText::new("Applications");
        assert!(cell_weight(FieldRole::Owner, Tier::Subsequence).is_none());
        assert_eq!(quality(&q, &[(FieldRole::Owner, &owner)]), None);
    }
}

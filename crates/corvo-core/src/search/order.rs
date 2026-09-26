use std::cmp::Ordering;

/// Match quality of a query against a learned term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LearnedTermMatch {
    Exact,
    Prefix,
    Overbounds { delta: usize },
}

/// Rich candidate metadata handed to the lexicographical launcher comparator.
#[derive(Clone, Debug)]
pub struct CandidateItem<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub has_user_alias: bool,
    pub is_exact_user_alias: bool,
    pub is_prefix_user_alias: bool,
    pub is_boosted: bool,
    pub learned_term: Option<LearnedTermMatch>,
    pub frecency: f64,
    pub quality: i32,
    pub title_is_exact: bool,
    pub title_is_prefix: bool,
    pub subtitle_is_exact: bool,
    pub priority: i32,
}

pub struct LauncherOrder;

impl LauncherOrder {
    /// Compares two candidates using the strict 8-stage lexicographical ranking rules.
    /// Returns `Ordering::Less` if `a` ranks ABOVE `b` (higher priority in launcher list).
    pub fn compare(a: &CandidateItem, b: &CandidateItem, query_len: usize) -> Ordering {
        // Stage 1: Exact user alias wins outright
        match (a.is_exact_user_alias, b.is_exact_user_alias) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }

        // Stage 2: Boosted query term (unless competitor has significantly higher real frecency)
        match (a.is_boosted, b.is_boosted) {
            (true, false) if b.frecency <= a.frecency * 1.5 + 200.0 => return Ordering::Less,
            (false, true) if a.frecency <= b.frecency * 1.5 + 200.0 => return Ordering::Greater,
            _ => {}
        }

        // Stage 3: Exact title (when query > 3 chars) -> learned term strength -> frecency
        if query_len > 3 {
            match (a.title_is_exact, b.title_is_exact) {
                (true, false) => return Ordering::Less,
                (false, true) => return Ordering::Greater,
                (true, true) => {
                    let ord = compare_frecency(a.frecency, b.frecency);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                _ => {}
            }
        }

        // Stage 4: Exact learned term -> frecency
        let a_learned_exact = matches!(a.learned_term, Some(LearnedTermMatch::Exact));
        let b_learned_exact = matches!(b.learned_term, Some(LearnedTermMatch::Exact));
        match (a_learned_exact, b_learned_exact) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (true, true) => {
                let ord = compare_frecency(a.frecency, b.frecency);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            _ => {}
        }

        // Stage 5: Exact subtitle -> frecency -> best title score
        match (a.subtitle_is_exact, b.subtitle_is_exact) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (true, true) => {
                let ord = compare_frecency(a.frecency, b.frecency);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            _ => {}
        }

        // Stage 6: Alias prefix -> term prefix -> overbounds (query extends learned term by <= 3)
        match (a.is_prefix_user_alias, b.is_prefix_user_alias) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }

        let a_learned_prefix = matches!(a.learned_term, Some(LearnedTermMatch::Prefix));
        let b_learned_prefix = matches!(b.learned_term, Some(LearnedTermMatch::Prefix));
        match (a_learned_prefix, b_learned_prefix) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }

        let a_overbounds = matches!(a.learned_term, Some(LearnedTermMatch::Overbounds { .. }));
        let b_overbounds = matches!(b.learned_term, Some(LearnedTermMatch::Overbounds { .. }));
        match (a_overbounds, b_overbounds) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }

        // Stage 7: Raw quality score -> frecency -> titlePrefix -> priority
        let quality_ord = b.quality.cmp(&a.quality);
        if quality_ord != Ordering::Equal {
            // If quality score differs substantially (more than 300 points), quality takes precedence
            if (a.quality - b.quality).abs() > 300 {
                return quality_ord;
            }
            // If close, frecency can break the tie
            let frec_ord = compare_frecency(a.frecency, b.frecency);
            if frec_ord != Ordering::Equal {
                return frec_ord;
            }
            return quality_ord;
        }

        // Stage 8: Tiebreak: frecency, has alias, priority, natural numerical collation
        let frec_ord = compare_frecency(a.frecency, b.frecency);
        if frec_ord != Ordering::Equal {
            return frec_ord;
        }

        match (a.has_user_alias, b.has_user_alias) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }

        let priority_ord = b.priority.cmp(&a.priority);
        if priority_ord != Ordering::Equal {
            return priority_ord;
        }

        natural_cmp(a.title, b.title)
    }
}

#[inline]
fn compare_frecency(a: f64, b: f64) -> Ordering {
    // If difference is greater than 10.0 frecency points
    if (a - b).abs() > 10.0 {
        if a > b {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else {
        Ordering::Equal
    }
}

/// Natural numerical collation comparator (e.g. "Item 2" < "Item 10").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    // Extract continuous digits as numbers
                    let mut num_a: u64 = 0;
                    while let Some(&d) = a_chars.peek() {
                        if d.is_ascii_digit() {
                            num_a = num_a.saturating_mul(10).saturating_add((d as u8 - b'0') as u64);
                            a_chars.next();
                        } else {
                            break;
                        }
                    }

                    let mut num_b: u64 = 0;
                    while let Some(&d) = b_chars.peek() {
                        if d.is_ascii_digit() {
                            num_b = num_b.saturating_mul(10).saturating_add((d as u8 - b'0') as u64);
                            b_chars.next();
                        } else {
                            break;
                        }
                    }

                    let ord = num_a.cmp(&num_b);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else {
                    let ord = ca.to_lowercase().cmp(cb.to_lowercase());
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    a_chars.next();
                    b_chars.next();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_collation_sorts_numbers_correctly() {
        assert_eq!(natural_cmp("Item 2", "Item 10"), Ordering::Less);
        assert_eq!(natural_cmp("Item 10", "Item 2"), Ordering::Greater);
        assert_eq!(natural_cmp("Item 10", "Item 10"), Ordering::Equal);
        assert_eq!(natural_cmp("File 9.png", "File 10.png"), Ordering::Less);
    }

    #[test]
    fn exact_alias_dominates() {
        let a = CandidateItem {
            id: "1",
            title: "VS Code",
            subtitle: None,
            has_user_alias: true,
            is_exact_user_alias: true,
            is_prefix_user_alias: false,
            is_boosted: false,
            learned_term: None,
            frecency: 5.0,
            quality: 1000,
            title_is_exact: false,
            title_is_prefix: false,
            subtitle_is_exact: false,
            priority: 0,
        };
        let b = CandidateItem {
            id: "2",
            title: "Vim",
            subtitle: None,
            has_user_alias: false,
            is_exact_user_alias: false,
            is_prefix_user_alias: false,
            is_boosted: false,
            learned_term: None,
            frecency: 500.0,
            quality: 6500,
            title_is_exact: true,
            title_is_prefix: true,
            subtitle_is_exact: false,
            priority: 10,
        };

        assert_eq!(LauncherOrder::compare(&a, &b, 2), Ordering::Less);
    }
}

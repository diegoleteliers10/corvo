use unicode_normalization::UnicodeNormalization;

/// Normalizes and folds text once for fuzzy search:
/// - Case-insensitive (lowercase)
/// - Diacritic-insensitive (NFKD decomposed, combining marks stripped, recomposed NFC)
/// - Width-insensitive (fullwidth ASCII mapped to standard ASCII)
/// - Format character stripping (Unicode Cf category: zero-width spaces/joiners)
/// - Ligature decomposition (e.g. ﬁ -> fi, ﬂ -> fl)
pub fn fold(text: &str) -> String {
    let mut decomposed = String::with_capacity(text.len());

    // 1. Lowercase and decompose via NFKD (handles ligatures like ﬁ -> fi and accents like é -> e + \u{301})
    for ch in text.to_lowercase().nfkd() {
        // Fullwidth ASCII normalization (0xFF01..=0xFF5E -> 0x21..=0x7E)
        let ch = match ch as u32 {
            0xFF01..=0xFF5E => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
            0x3000 => ' ', // Ideographic space
            _ => ch,
        };

        // Filter out combining diacritical marks (Unicode Mn category: 0x0300..=0x036F, 0x1DC0..=0x1DFF, 0x20D0..=0x20FF, 0xFE20..=0xFE2F)
        if matches!(ch as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
        {
            continue;
        }

        // Filter out format characters (Unicode Cf category: zero-width joiners/spaces/bidi marks)
        if matches!(ch as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0xFEFF) {
            continue;
        }

        decomposed.push(ch);
    }

    // Recompose into NFC
    decomposed.nfc().collect()
}

/// A pre-folded text candidate or query with UTF-16 code units and precomputed hump boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SearchText {
    pub original: String,
    pub units: Vec<u16>,
    pub humps: Vec<usize>,
}

impl SearchText {
    /// Creates a new `SearchText` by folding the string and precomputing camelCase/word boundaries.
    pub fn new(text: &str) -> Self {
        let folded = fold(text);
        let units: Vec<u16> = folded.encode_utf16().collect();
        let humps = extract_humps(text, &units);

        Self {
            original: text.to_string(),
            units,
            humps,
        }
    }

    /// Creates an empty `SearchText`.
    pub const fn empty() -> Self {
        Self {
            original: String::new(),
            units: Vec::new(),
            humps: Vec::new(),
        }
    }

    /// Combines two `SearchText` instances as a joined phrase separated by a space.
    /// Used when searching fields like `brew search` -> `Search under Brew`.
    pub fn joined(first: &Self, second: &Self) -> Self {
        if first.units.is_empty() {
            return second.clone();
        }
        if second.units.is_empty() {
            return first.clone();
        }

        let mut units = Vec::with_capacity(first.units.len() + 1 + second.units.len());
        units.extend_from_slice(&first.units);
        units.push(' ' as u16);
        units.extend_from_slice(&second.units);

        let second_offset = first.units.len() + 1;
        let mut humps = first.humps.clone();
        // The start of the second part is always a hump (after the joining space)
        humps.push(second_offset);
        for &h in &second.humps {
            if h != 0 {
                humps.push(second_offset + h);
            }
        }
        humps.sort_unstable();
        humps.dedup();

        Self {
            original: format!("{} {}", first.original, second.original),
            units,
            humps,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.units.len()
    }
}

/// Precomputes word and camelCase humps from original text, mapping them to UTF-16 code units.
fn extract_humps(original: &str, units: &[u16]) -> Vec<usize> {
    let mut humps = Vec::new();
    let chars: Vec<char> = original.chars().collect();
    if chars.is_empty() || units.is_empty() {
        return humps;
    }

    // Always mark index 0 as a word start
    humps.push(0);

    let mut is_sep = false;
    for i in 0..chars.len() {
        let ch = chars[i];
        if is_separator(ch) {
            is_sep = true;
            continue;
        }

        if is_sep {
            humps.push(i);
            is_sep = false;
            continue;
        }

        // CamelCase: lower -> UPPER (e.g. `visualStudio` -> hump at `S`)
        if i > 0 && chars[i - 1].is_lowercase() && ch.is_uppercase() {
            humps.push(i);
            continue;
        }

        // Digit -> letter (e.g. `mp3Player` -> hump at `P`)
        if i > 0 && chars[i - 1].is_ascii_digit() && ch.is_alphabetic() {
            humps.push(i);
            continue;
        }

        // Acronym boundary: UPPER UPPER lower (e.g. `XCode` -> hump at `C`)
        if i >= 2 && chars[i - 2].is_uppercase() && chars[i - 1].is_uppercase() && ch.is_lowercase()
        {
            humps.push(i - 1);
        }
    }

    // Map character indices to UTF-16 code unit indices if ASCII; if non-ASCII length matches, keep
    let valid_indices: Vec<usize> = humps.into_iter().filter(|&idx| idx < units.len()).collect();

    let mut result = valid_indices;
    result.sort_unstable();
    result.dedup();
    result
}

#[inline]
pub fn is_separator(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '-' | '_' | '.' | '/' | '\\' | ':' | '@' | '(' | ')' | '[' | ']' | '{' | '}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_accents_and_case() {
        assert_eq!(fold("Calendário"), "calendario");
        assert_eq!(fold("München"), "munchen");
        assert_eq!(fold("café"), "cafe");
    }

    #[test]
    fn folds_ligatures_and_fullwidth() {
        assert_eq!(fold("ﬁle"), "file");
        assert_eq!(fold("Ｃｏｄｅ"), "code");
    }

    #[test]
    fn precomputes_camel_case_and_separator_humps() {
        let st = SearchText::new("VSCode");
        assert_eq!(st.units, SearchText::new("vscode").units);
        // Humps should contain 0 (V), and 2 (C)
        assert!(st.humps.contains(&0));
        assert!(st.humps.contains(&2));

        let st2 = SearchText::new("Visual Studio Code");
        // 0 ('v'), 7 ('s'), 14 ('c')
        assert!(st2.humps.contains(&0));
        assert!(st2.humps.contains(&7));
        assert!(st2.humps.contains(&14));
    }

    #[test]
    fn joins_search_texts_with_space() {
        let first = SearchText::new("Visual");
        let second = SearchText::new("Studio");
        let joined = SearchText::joined(&first, &second);
        assert_eq!(joined.original, "Visual Studio");
        assert_eq!(joined.units, SearchText::new("visual studio").units);
        assert!(joined.humps.contains(&0));
        assert!(joined.humps.contains(&(first.len() + 1)));
    }
}

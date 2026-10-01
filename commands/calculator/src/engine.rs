//! Pure unified calculation and conversion engine (parse-or-fall, try-don't-guess).

use crate::scalar::{evaluate_scalar, format_value};
use crate::units::{convert_units, default_conversion, find_temperature, find_unit, UNITS};

#[derive(Clone, Debug, PartialEq)]
pub enum CalcOutcome {
    Calculation {
        expression: String,
        display_result: String,
        raw_value: String,
        category: &'static str,
    },
    Conversion {
        expression: String,
        display_result: String,
        raw_value: String,
        category: &'static str,
    },
    Error {
        expression: String,
        message: String,
    },
}

impl CalcOutcome {
    pub fn display_result(&self) -> &str {
        match self {
            Self::Calculation { display_result, .. } => display_result,
            Self::Conversion { display_result, .. } => display_result,
            Self::Error { message, .. } => message,
        }
    }

    pub fn raw_value(&self) -> &str {
        match self {
            Self::Calculation { raw_value, .. } => raw_value,
            Self::Conversion { raw_value, .. } => raw_value,
            Self::Error { message, .. } => message,
        }
    }

    pub fn expression(&self) -> &str {
        match self {
            Self::Calculation { expression, .. } => expression,
            Self::Conversion { expression, .. } => expression,
            Self::Error { expression, .. } => expression,
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Calculation { category, .. } => category,
            Self::Conversion { category, .. } => category,
            Self::Error { .. } => "Error",
        }
    }
}

/// Evaluates a query according to the strict parse-or-fall pipeline.
/// Returns `None` if the query does not completely parse as math or conversion.
pub fn evaluate(raw_query: &str) -> Option<CalcOutcome> {
    let q = raw_query.trim();

    // 1. Cheap prefilters
    if q.is_empty() || q.len() > 256 {
        return None;
    }

    // Explicit prefix `=` forces evaluation
    let clean = q.strip_prefix('=').unwrap_or(q).trim();

    // Semver check: reject patterns like 1.2.3 or 1.2.3 + 1
    if has_semver_pattern(clean) {
        return None;
    }

    // Single ASCII words / non-math rejection:
    // If there are no digits, no '=', and no conversion keywords, reject immediately.
    let has_digits = clean.chars().any(|c| c.is_ascii_digit());
    let has_conversion_keyword = has_conversion_separator(clean);
    if !q.starts_with('=') && !has_digits && !has_conversion_keyword {
        return None;
    }

    // Normalize European decimal commas: 1,5 -> 1.5 if preceded and followed by digits
    let normalized = normalize_decimal_commas(clean);

    // 2. Base/radix conversion (e.g. 0xff, 0b1010, 0o77)
    if let Some(outcome) = try_radix_conversion(&normalized) {
        return Some(outcome);
    }

    // 3. Explicit conversion (10 kg to lb, 32 f in c)
    if let Some(outcome) = try_explicit_conversion(&normalized) {
        return Some(outcome);
    }

    // 4. Arithmetic with typed units (10kg + 500g, 5m * 4m, 100km / 2h)
    if let Some(outcome) = try_typed_arithmetic(&normalized) {
        return Some(outcome);
    }

    // 5. Pure scalar math (10-5, 1.5 + 3, (2+3)*4)
    // Bare numbers like '1' or '42' without operators must not trigger unless explicitly starting with '='
    let is_explicit = q.starts_with('=');
    if is_explicit || has_scalar_operator(&normalized) {
        if let Ok(value) = evaluate_scalar(&normalized) {
            let fmt = format_value(value);
            return Some(CalcOutcome::Calculation {
                expression: normalized.clone(),
                display_result: fmt.clone(),
                raw_value: fmt,
                category: "Calculation",
            });
        }
    }

    // 6. Single quantity autoconversion (1m -> 3.28 ft)
    if let Some(outcome) = try_single_quantity_autoconvert(&normalized) {
        return Some(outcome);
    }

    // 7. Partial expression with trailing hanging operator (2+, 2*)
    if let Some(outcome) = try_partial_hanging_operator(&normalized) {
        return Some(outcome);
    }

    None
}

fn has_scalar_operator(s: &str) -> bool {
    if s.chars().any(|c| matches!(c, '+' | '*' | '/' | '%' | '^' | '(' | ')')) {
        return true;
    }
    let trimmed = s.trim();
    for (i, c) in trimmed.char_indices() {
        if c == '-' && i > 0 {
            let before = trimmed[..i].trim();
            let after = trimmed[i + 1..].trim();
            if !before.is_empty() && !after.is_empty() {
                return true;
            }
        }
    }
    false
}

fn has_semver_pattern(s: &str) -> bool {
    for word in s.split_whitespace() {
        let parts: Vec<&str> = word.split('.').collect();
        if parts.len() >= 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
            return true;
        }
    }
    false
}

fn has_conversion_separator(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains(" to ")
        || lower.contains(" in ")
        || lower.contains("->")
        || lower.contains('→')
        || lower.contains("=>")
}

fn normalize_decimal_commas(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == ',' {
            let prev_digit = i > 0 && chars[i - 1].is_ascii_digit();
            let next_digit = i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
            if prev_digit && next_digit {
                result.push('.');
                continue;
            }
        }
        result.push(chars[i]);
    }
    result
}

fn try_radix_conversion(s: &str) -> Option<CalcOutcome> {
    let trimmed = s.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        let hex = &trimmed[2..];
        if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(val) = i64::from_str_radix(hex, 16) {
                let formatted = val.to_string();
                return Some(CalcOutcome::Calculation {
                    expression: trimmed.to_string(),
                    display_result: formatted.clone(),
                    raw_value: formatted,
                    category: "Base conversion",
                });
            }
        }
    } else if trimmed.starts_with("0b") || trimmed.starts_with("0B") {
        let bin = &trimmed[2..];
        if !bin.is_empty() && bin.chars().all(|c| c == '0' || c == '1') {
            if let Ok(val) = i64::from_str_radix(bin, 2) {
                let formatted = val.to_string();
                return Some(CalcOutcome::Calculation {
                    expression: trimmed.to_string(),
                    display_result: formatted.clone(),
                    raw_value: formatted,
                    category: "Base conversion",
                });
            }
        }
    } else if trimmed.starts_with("0o") || trimmed.starts_with("0O") {
        let oct = &trimmed[2..];
        if !oct.is_empty() && oct.chars().all(|c| matches!(c, '0'..='7')) {
            if let Ok(val) = i64::from_str_radix(oct, 8) {
                let formatted = val.to_string();
                return Some(CalcOutcome::Calculation {
                    expression: trimmed.to_string(),
                    display_result: formatted.clone(),
                    raw_value: formatted,
                    category: "Base conversion",
                });
            }
        }
    }
    None
}

fn try_explicit_conversion(expr: &str) -> Option<CalcOutcome> {
    let lowered = expr.to_lowercase();
    for separator in [" to ", " in ", "->", "→", "=>"] {
        let Some(at) = lowered.find(separator) else { continue };
        let left = lowered[..at].trim();
        let target = lowered[at + separator.len()..].trim();

        if left.is_empty() || target.is_empty() {
            continue;
        }

        // Rule of honest ambiguity: conversion followed by * or / without parens (e.g. 20 eur to usd * 30) -> silence (None)
        if target.contains('*') || target.contains('/') {
            return None;
        }

        // Try evaluating left as typed arithmetic first (e.g. 100km / 2h to km/h, 1 / 20ms to hz)
        let (val, from_unit) = if let Some(outcome) = try_typed_arithmetic(left) {
            let num = outcome.raw_value().parse::<f64>().ok()?;
            let unit = outcome.display_result().split_whitespace().last()?.to_string();
            (num, unit)
        } else {
            let (num_expr, unit_str) = extract_number_and_unit(left)?;
            let num = evaluate_scalar(num_expr).ok()?;
            (num, unit_str.to_string())
        };

        match convert_units(val, &from_unit, target) {
            Ok((result_val, target_sym)) => {
                let fmt = format_value(result_val);
                let display = format!("{fmt} {target_sym}");
                return Some(CalcOutcome::Conversion {
                    expression: expr.to_string(),
                    display_result: display,
                    raw_value: fmt,
                    category: "Unit conversion",
                });
            }
            Err(Ok(mismatch)) => {
                let msg = format!("Cannot convert {} to {}", mismatch.from_dim.name(), mismatch.to_dim.name());
                return Some(CalcOutcome::Error {
                    expression: expr.to_string(),
                    message: msg,
                });
            }
            Err(Err(())) => {
                // If units don't exist, continue checking
                continue;
            }
        }
    }
    None
}

fn extract_number_and_unit(s: &str) -> Option<(&str, &str)> {
    let trimmed = s.trim();
    if let Some((num_part, unit_part)) = trimmed.rsplit_once(char::is_whitespace) {
        return Some((num_part.trim(), unit_part.trim()));
    }

    // Split on first trailing non-digit/operator sequence
    let split_pos = trimmed.rfind(|c: char| c.is_ascii_digit() || c == '.' || c == ')')?;
    if split_pos + 1 < trimmed.len() {
        let num_part = trimmed[..=split_pos].trim();
        let unit_part = trimmed[split_pos + 1..].trim();
        if !num_part.is_empty() && !unit_part.is_empty() {
            return Some((num_part, unit_part));
        }
    }
    None
}

fn try_typed_arithmetic(expr: &str) -> Option<CalcOutcome> {
    // Check for operators with units on both sides: e.g. 10kg + 500g, 5m * 4m, 100km / 2h
    for op in ['+', '-', '*', '/'] {
        // Find operator outside of parentheses
        if let Some(pos) = find_binary_operator_outside_parens(expr, op) {
            let left_str = expr[..pos].trim();
            let right_str = expr[pos + 1..].trim();

            let (val_a, unit_a) = extract_number_and_unit(left_str)?;
            let (val_b, unit_b) = extract_number_and_unit(right_str)?;

            let num_a = evaluate_scalar(val_a).ok()?;
            let num_b = evaluate_scalar(val_b).ok()?;

            let u_a = find_unit(unit_a)?;
            let u_b = find_unit(unit_b)?;

            match op {
                '+' | '-' => {
                    if u_a.dim != u_b.dim {
                        return None;
                    }
                    // Answer in unit of the second term (last term written)
                    let num_a_in_b = num_a * u_a.factor / u_b.factor;
                    let result = if op == '+' { num_a_in_b + num_b } else { num_a_in_b - num_b };
                    let fmt = format_value(result);
                    let display = format!("{fmt} {}", u_b.symbol);
                    return Some(CalcOutcome::Calculation {
                        expression: expr.to_string(),
                        display_result: display,
                        raw_value: fmt,
                        category: "Calculation",
                    });
                }
                '*' => {
                    let total_dim = u_a.dim.product(u_b.dim);
                    let result_base = (num_a * u_a.factor) * (num_b * u_b.factor);

                    let compound_sym = if u_a.symbol == u_b.symbol {
                        format!("{}²", u_a.symbol)
                    } else {
                        String::new()
                    };

                    let target_u = UNITS
                        .iter()
                        .find(|u| u.symbol == compound_sym)
                        .or_else(|| UNITS.iter().find(|u| u.dim == total_dim));

                    if let Some(target_u) = target_u {
                        let result_val = result_base / target_u.factor;
                        let fmt = format_value(result_val);
                        let display = format!("{fmt} {}", target_u.symbol);
                        return Some(CalcOutcome::Calculation {
                            expression: expr.to_string(),
                            display_result: display,
                            raw_value: fmt,
                            category: "Calculation",
                        });
                    }
                }
                '/' => {
                    if num_b == 0.0 {
                        return None;
                    }
                    let total_dim = u_a.dim.quotient(u_b.dim);
                    let result_base = (num_a * u_a.factor) / (num_b * u_b.factor);

                    if total_dim.is_scalar() {
                        let fmt = format_value(result_base);
                        return Some(CalcOutcome::Calculation {
                            expression: expr.to_string(),
                            display_result: fmt.clone(),
                            raw_value: fmt,
                            category: "Calculation",
                        });
                    }

                    let compound_sym = format!("{}/{}", u_a.symbol, u_b.symbol);
                    let target_u = UNITS
                        .iter()
                        .find(|u| u.symbol == compound_sym)
                        .or_else(|| UNITS.iter().find(|u| u.dim == total_dim));

                    if let Some(target_u) = target_u {
                        let result_val = result_base / target_u.factor;
                        let fmt = format_value(result_val);
                        let display = format!("{fmt} {}", target_u.symbol);
                        return Some(CalcOutcome::Calculation {
                            expression: expr.to_string(),
                            display_result: display,
                            raw_value: fmt,
                            category: "Calculation",
                        });
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn find_binary_operator_outside_parens(s: &str, op: char) -> Option<usize> {
    let mut paren_depth: usize = 0;
    for (idx, ch) in s.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            c if c == op && paren_depth == 0 && idx > 0 && idx + 1 < s.len() => {
                return Some(idx);
            }
            _ => {}
        }
    }
    None
}

fn try_single_quantity_autoconvert(s: &str) -> Option<CalcOutcome> {
    let (num_part, unit_part) = extract_number_and_unit(s)?;
    let val = evaluate_scalar(num_part).ok()?;
    let (converted_val, sym) = default_conversion(val, unit_part)?;
    let fmt = format_value(converted_val);
    let display = format!("{fmt} {sym}");
    Some(CalcOutcome::Conversion {
        expression: s.to_string(),
        display_result: display,
        raw_value: fmt,
        category: "Unit conversion",
    })
}

fn try_partial_hanging_operator(s: &str) -> Option<CalcOutcome> {
    let trimmed = s.trim();
    if trimmed.len() <= 1 {
        return None;
    }
    let last = trimmed.chars().last()?;
    if matches!(last, '+' | '-' | '*' | '/' | '^' | '%') {
        let prefix = trimmed[..trimmed.len() - last.len_utf8()].trim();
        if prefix.is_empty() {
            return None;
        }

        // Try evaluating prefix as scalar
        if let Ok(val) = evaluate_scalar(prefix) {
            let fmt = format_value(val);
            return Some(CalcOutcome::Calculation {
                expression: trimmed.to_string(),
                display_result: fmt.clone(),
                raw_value: fmt,
                category: "Calculation",
            });
        }

        // Try evaluating prefix as single quantity
        if let Some((val_str, unit_str)) = extract_number_and_unit(prefix) {
            if let Ok(val) = evaluate_scalar(val_str) {
                if find_unit(unit_str).is_some() || find_temperature(unit_str).is_some() {
                    let fmt = format_value(val);
                    let display = format!("{fmt} {unit_str}");
                    return Some(CalcOutcome::Calculation {
                        expression: trimmed.to_string(),
                        display_result: display,
                        raw_value: fmt,
                        category: "Calculation",
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceptance_test_cases() {
        // 1. font-size shows search (None)
        assert_eq!(evaluate("font-size"), None);
        assert_eq!(evaluate("chrome"), None);
        assert_eq!(evaluate("today"), None);
        assert_eq!(evaluate("july"), None);

        // 2. 10-5 shows card (5)
        let res = evaluate("10-5").unwrap();
        assert_eq!(res.display_result(), "5");

        // 3. 1.2.3 + 1 shows search (None due to semver pattern)
        assert_eq!(evaluate("1.2.3 + 1"), None);
        assert_eq!(evaluate("1.2.3"), None);

        // 4. 10 kg to lb shows card with category badge
        let res = evaluate("10 kg to lb").unwrap();
        assert!(res.display_result().starts_with("22.0462"));
        assert_eq!(res.category(), "Unit conversion");

        // 5. 10 kg to gal shows error ("Cannot convert mass to volume")
        let res = evaluate("10 kg to gal").unwrap();
        assert_eq!(res.display_result(), "Cannot convert mass to volume");
        assert_eq!(res.category(), "Error");

        // 6. 2* shows evaluated prefix (2)
        let res = evaluate("2*").unwrap();
        assert_eq!(res.display_result(), "2");

        // 7. 2+ shows evaluated prefix (2)
        let res = evaluate("2+").unwrap();
        assert_eq!(res.display_result(), "2");

        // 8. 0xff shows base conversion (255)
        let res = evaluate("0xff").unwrap();
        assert_eq!(res.display_result(), "255");

        // 9. 1.5 + 3 shows card (4.5)
        let res = evaluate("1.5 + 3").unwrap();
        assert_eq!(res.display_result(), "4.5");

        // 10. Typed arithmetic: uses the last unit written as answer unit
        let res = evaluate("500g + 10kg").unwrap();
        assert_eq!(res.display_result(), "10.5 kg");

        let res = evaluate("10kg + 500g").unwrap();
        assert_eq!(res.display_result(), "10500 g");

        let res = evaluate("5m * 4m").unwrap();
        assert_eq!(res.display_result(), "20 m²");

        let res = evaluate("100km / 2h").unwrap();
        assert_eq!(res.display_result(), "50 km/h");

        // 11. Bare numbers do NOT show calculator card (avoid blocking apps/files starting with digits)
        assert_eq!(evaluate("1"), None);
        assert_eq!(evaluate("42"), None);
        assert_eq!(evaluate("0"), None);

        // 12. Operator with digit shows card (hanging operator / partial calculation)
        let res = evaluate("1 +").unwrap();
        assert_eq!(res.display_result(), "1");

        let res = evaluate("1/").unwrap();
        assert_eq!(res.display_result(), "1");

        // 13. Single quantity autoconversion shows card
        let res = evaluate("1 kg").unwrap();
        assert!(res.display_result().starts_with("2.20462"));
        assert_eq!(res.category(), "Unit conversion");

        let res = evaluate("1 lb").unwrap();
        assert!(res.display_result().starts_with("0.45359"));
        assert_eq!(res.category(), "Unit conversion");

        // 14. Explicit '=' forces calculation even for bare numbers
        let res = evaluate("=42").unwrap();
        assert_eq!(res.display_result(), "42");
    }
}

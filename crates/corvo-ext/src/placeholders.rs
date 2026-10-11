//! Shared placeholder expansion for snippets and quicklinks.
//!
//! Supported placeholders: `{clipboard}`, `{date}`, `{time}`, and
//! `{datetime}`. Unknown placeholders stay literal so a template never
//! loses text it does not mean to expand. `{clipboard}` resolves only
//! when the caller supplies the current clipboard text: previews pass
//! `None` and keep the placeholder visible, execution passes the real
//! clipboard.

use chrono::Local;

/// Expands placeholders in `text`. See the module docs for the rules.
pub fn expand(text: &str, clipboard: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        match name {
            "date" => out.push_str(&Local::now().format("%Y-%m-%d").to_string()),
            "time" => out.push_str(&Local::now().format("%H:%M").to_string()),
            "datetime" => out.push_str(&Local::now().format("%Y-%m-%d %H:%M").to_string()),
            "clipboard" => match clipboard {
                Some(text) => out.push_str(text),
                None => out.push_str("{clipboard}"),
            },
            _ => out.push_str(&rest[open..open + close + 2]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_known_placeholders() {
        let out = expand("hi {clipboard}!", Some("world"));
        assert_eq!(out, "hi world!");
    }

    #[test]
    fn clipboard_stays_literal_without_a_value() {
        assert_eq!(expand("x {clipboard} y", None), "x {clipboard} y");
        assert_eq!(expand("x {clipboard} y", Some("z")), "x z y");
    }

    #[test]
    fn unknown_placeholders_stay_literal() {
        assert_eq!(expand("{foo} {date}", None).starts_with("{foo} "), true);
    }

    #[test]
    fn formats_date_and_time() {
        let date = expand("{date}", None);
        assert_eq!(date.len(), 10);
        assert_eq!(date.as_bytes()[4], b'-');

        let time = expand("{time}", None);
        assert_eq!(time.len(), 5);
        assert_eq!(time.as_bytes()[2], b':');
    }

    #[test]
    fn handles_multiple_and_unbalanced_braces() {
        assert_eq!(expand("{clipboard}{clipboard}", Some("a")), "aa");
        assert_eq!(expand("no closing {brace", None), "no closing {brace");
        assert_eq!(expand("}", None), "}");
        assert_eq!(expand("plain text", None), "plain text");
    }
}

//! Pure text transforms behind the text-utilities command. No state, no
//! platform access; everything here is a function from input to output.

/// Splits input into words on whitespace, `-`, `_`, `.`, `/`, and
/// camelCase boundaries, the same split change-case libraries use, so
/// `XMLHttpRequest`, `xml_http_request`, and `xml http request` all
/// produce the same word list.
fn split_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    let mut chars = input.chars().peekable();
    let mut prev: Option<char> = None;
    while let Some(c) = chars.next() {
        if c.is_whitespace() || matches!(c, '-' | '_' | '.' | '/') {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev = Some(c);
            continue;
        }
        if let Some(p) = prev {
            let boundary = (p.is_lowercase() && c.is_uppercase())
                || (p.is_uppercase()
                    && c.is_uppercase()
                    && chars.peek().is_some_and(|next| next.is_lowercase()))
                || (p.is_ascii_digit() && c.is_uppercase());
            if boundary && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(c);
        prev = Some(c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn capitalize(word: &str) -> String {
    let lower = word.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    if let Some(first) = lower.chars().next() {
        out.extend(first.to_uppercase());
        let rest = &lower[first.len_utf8()..];
        out.push_str(rest);
    }
    out
}

fn join_words(words: &[String], separator: &str, transform: fn(&str) -> String) -> String {
    words
        .iter()
        .map(|word| transform(word))
        .collect::<Vec<_>>()
        .join(separator)
}

pub fn to_upper(input: &str) -> String {
    input.to_uppercase()
}

pub fn to_lower(input: &str) -> String {
    input.to_lowercase()
}

pub fn to_title(input: &str) -> String {
    join_words(&split_words(input), " ", capitalize)
}

pub fn to_sentence(input: &str) -> String {
    let words = split_words(input);
    let mut parts: Vec<String> = Vec::with_capacity(words.len());
    for (index, word) in words.iter().enumerate() {
        if index == 0 {
            parts.push(capitalize(word));
        } else {
            parts.push(word.to_lowercase());
        }
    }
    parts.join(" ")
}

pub fn to_camel(input: &str) -> String {
    let words = split_words(input);
    let mut parts: Vec<String> = Vec::with_capacity(words.len());
    for (index, word) in words.iter().enumerate() {
        if index == 0 {
            parts.push(word.to_lowercase());
        } else {
            parts.push(capitalize(word));
        }
    }
    parts.join("")
}

pub fn to_pascal(input: &str) -> String {
    join_words(&split_words(input), "", capitalize)
}

pub fn to_snake(input: &str) -> String {
    join_words(&split_words(input), "_", |word| word.to_lowercase())
}

pub fn to_kebab(input: &str) -> String {
    join_words(&split_words(input), "-", |word| word.to_lowercase())
}

pub fn to_constant(input: &str) -> String {
    join_words(&split_words(input), "_", |word| word.to_uppercase())
}

pub fn to_path_case(input: &str) -> String {
    join_words(&split_words(input), "/", |word| word.to_lowercase())
}

pub fn to_dot(input: &str) -> String {
    join_words(&split_words(input), ".", |word| word.to_lowercase())
}

pub fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input.as_bytes())
}

/// Decodes Base64, ignoring embedded whitespace and accepting missing
/// padding. Returns `None` unless the result is valid UTF-8 text.
pub fn base64_decode(input: &str) -> Option<String> {
    use base64::Engine;
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() || !cleaned.chars().all(|c| c.is_ascii_alphanumeric() || c == '=' ) {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(cleaned.trim_end_matches('='))
                .ok()
        })?;
    String::from_utf8(bytes).ok()
}

pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decodes percent-escape sequences. Only answers when the input has a
/// `%` to begin with; there is nothing to decode otherwise.
pub fn url_decode(input: &str) -> Option<String> {
    if !input.contains('%') {
        return None;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = (*bytes.get(index + 1)? as char).to_digit(16)?;
            let lo = (*bytes.get(index + 2)? as char).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn hex_encode(input: &str) -> String {
    hex::encode(input.as_bytes())
}

/// Decodes hexadecimal text (optional `0x` prefix, whitespace ignored).
/// Returns `None` unless the result is valid UTF-8 text.
pub fn hex_decode(input: &str) -> Option<String> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let cleaned = cleaned.strip_prefix("0x").unwrap_or(&cleaned);
    if cleaned.len() < 2 || !cleaned.len().is_multiple_of(2) {
        return None;
    }
    if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let bytes = hex::decode(cleaned).ok()?;
    String::from_utf8(bytes).ok()
}

pub fn json_pretty(input: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(input).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

pub fn json_minify(input: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(input).ok()?;
    serde_json::to_string(&value).ok()
}

pub fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn sha1_hex(input: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn sort_lines(input: &str) -> String {
    let mut lines: Vec<&str> = input.lines().collect();
    lines.sort_unstable();
    lines.join("\n")
}

/// Removes duplicate lines, keeping the first occurrence of each.
pub fn dedupe_lines(input: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    input
        .lines()
        .filter(|line| seen.insert((*line).to_owned()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn reverse_lines(input: &str) -> String {
    input.lines().rev().collect::<Vec<_>>().join("\n")
}

pub fn trim_lines(input: &str) -> String {
    input.lines().map(str::trim).collect::<Vec<_>>().join("\n")
}

pub fn count_stats(input: &str) -> String {
    let chars = input.chars().count();
    let words = input.split_whitespace().count();
    let lines = input.lines().count();
    format!("{chars} chars · {words} words · {lines} lines")
}

pub fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn uuid_v7() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 32 hex characters drawn from a v4 UUID's random bits.
pub fn random_hex() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_conversions_use_word_boundaries() {
        let input = "hello world_foo-bar";
        assert_eq!(to_snake(input), "hello_world_foo_bar");
        assert_eq!(to_kebab(input), "hello-world-foo-bar");
        assert_eq!(to_constant(input), "HELLO_WORLD_FOO_BAR");
        assert_eq!(to_camel(input), "helloWorldFooBar");
        assert_eq!(to_pascal(input), "HelloWorldFooBar");
        assert_eq!(to_title(input), "Hello World Foo Bar");
        assert_eq!(to_sentence("hello WORLD"), "Hello world");
        assert_eq!(to_path_case(input), "hello/world/foo/bar");
        assert_eq!(to_dot(input), "hello.world.foo.bar");
        assert_eq!(to_upper(input), "HELLO WORLD_FOO-BAR");
        assert_eq!(to_lower("HeLLo"), "hello");
    }

    #[test]
    fn case_conversions_split_camel_boundaries() {
        assert_eq!(to_snake("XMLHttpRequest"), "xml_http_request");
        assert_eq!(to_snake("HTTPServer2Foo"), "http_server2_foo");
        assert_eq!(to_camel("HTTP server"), "httpServer");
    }

    #[test]
    fn base64_roundtrips() {
        assert_eq!(base64_encode("hello"), "aGVsbG8=");
        assert_eq!(base64_decode("aGVsbG8=").as_deref(), Some("hello"));
        assert_eq!(base64_decode("aGVsbG8").as_deref(), Some("hello"));
        assert_eq!(base64_decode("a G V s b G 8=").as_deref(), Some("hello"));
        assert_eq!(base64_decode("not base64 !!"), None);
        assert_eq!(base64_decode("///"), None);
    }

    #[test]
    fn url_encoding_roundtrips() {
        assert_eq!(url_encode("hello world/a"), "hello%20world%2Fa");
        assert_eq!(url_decode("hello%20world%2Fa").as_deref(), Some("hello world/a"));
        assert_eq!(url_decode("no escapes"), None);
        assert_eq!(url_decode("%zz"), None);
        assert_eq!(url_decode("%e2%9c%93").as_deref(), Some("✓"));
    }

    #[test]
    fn hex_roundtrips() {
        assert_eq!(hex_encode("hi"), "6869");
        assert_eq!(hex_decode("6869").as_deref(), Some("hi"));
        assert_eq!(hex_decode("0x6869").as_deref(), Some("hi"));
        assert_eq!(hex_decode("686"), None);
        assert_eq!(hex_decode("zz"), None);
    }

    #[test]
    fn json_formats_only_valid_json() {
        let pretty = json_pretty("{\"a\":1}").unwrap();
        assert_eq!(pretty, "{\n  \"a\": 1\n}");
        assert_eq!(json_minify("{\n  \"a\" : 1\n}").as_deref(), Some("{\"a\":1}"));
        assert_eq!(json_pretty("{not json}"), None);
    }

    #[test]
    fn hashes_match_known_vectors() {
        assert_eq!(
            sha256_hex("hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(sha1_hex("hello"), "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    }

    #[test]
    fn line_tools_operate_on_lines() {
        let input = "b\na\nb\n  c  \n";
        assert_eq!(sort_lines(input), "  c  \na\nb\nb");
        assert_eq!(dedupe_lines(input), "b\na\n  c  ");
        assert_eq!(reverse_lines("1\n2\n3"), "3\n2\n1");
        assert_eq!(trim_lines("  a \n b "), "a\nb");
    }

    #[test]
    fn generators_produce_valid_ids() {
        let v4 = uuid::Uuid::parse_str(&uuid_v4()).unwrap();
        assert_eq!(v4.get_version_num(), 4);
        let v7 = uuid::Uuid::parse_str(&uuid_v7()).unwrap();
        assert_eq!(v7.get_version_num(), 7);
        assert_eq!(random_hex().len(), 32);
        assert!(random_hex().chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(uuid_v4(), uuid_v4());
    }
}

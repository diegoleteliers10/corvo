#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodeTone {
    Keyword,
    String,
    Number,
    Comment,
    Type,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CodeSpan {
    pub start: usize,
    pub text: String,
    pub tone: Option<CodeTone>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodeRowKind {
    Opening,
    Content,
    Closing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CodeRow {
    pub start: usize,
    pub end: usize,
    pub language: String,
    pub kind: CodeRowKind,
    pub spans: Vec<CodeSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Language {
    Js,
    Rust,
    Python,
    Json,
    Sql,
    Shell,
    Plain,
}
fn language(name: &str) -> Language {
    match name.to_ascii_lowercase().as_str() {
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" => Language::Js,
        "rust" | "rs" => Language::Rust,
        "python" | "py" => Language::Python,
        "json" | "jsonc" => Language::Json,
        "sql" => Language::Sql,
        "sh" | "shell" | "bash" | "zsh" => Language::Shell,
        _ => Language::Plain,
    }
}
fn fence(line: &str) -> Option<(char, usize, &str)> {
    let spaces = line.chars().take_while(|c| *c == ' ').count();
    if spaces > 3 {
        return None;
    }
    let tail = &line[spaces..];
    let marker = tail.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let count = tail.chars().take_while(|c| *c == marker).count();
    if count < 3 {
        return None;
    }
    Some((marker, count, &tail[count..]))
}
#[derive(Default)]
struct Lexer {
    block: usize,
    quote: Option<char>,
}
fn push(spans: &mut Vec<CodeSpan>, start: usize, chars: &[char], tone: Option<CodeTone>) {
    if chars.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.tone == tone {
            last.text.extend(chars);
            return;
        }
    }
    spans.push(CodeSpan {
        start,
        text: chars.iter().collect(),
        tone,
    });
}
fn word_tone(word: &str, lang: Language) -> Option<CodeTone> {
    let lower = word.to_ascii_lowercase();
    let value = if lang == Language::Sql {
        lower.as_str()
    } else {
        word
    };
    let keywords = match lang {
        Language::Js => "const let var function return if else switch case break continue for while do new class extends import export from default async await try catch finally throw typeof instanceof in of void delete yield this super true false null undefined interface type enum implements public private protected readonly declare abstract static as satisfies keyof infer",
        Language::Rust => "fn let mut const static pub use mod impl trait struct enum match if else for while loop return break continue move async await unsafe where self Self super crate ref in as dyn true false",
        Language::Python => "def class return if elif else for while in is not and or import from as with try except finally raise pass break continue lambda yield async await global nonlocal assert del True False None",
        Language::Json => "true false null",
        Language::Sql => "select from where insert into values update set delete create table drop alter join left right inner outer on group by order having limit offset union all distinct as and or not null true false case when then else end asc desc exists",
        Language::Shell => "if then else elif fi for while do done case esac in function select until export local readonly return break continue",
        Language::Plain => "",
    };
    if keywords.split_whitespace().any(|keyword| keyword == value) {
        return Some(CodeTone::Keyword);
    }
    let types = match lang {
        Language::Js => "string number boolean any unknown never object symbol bigint Array Promise Record Map Set Date String Number Boolean",
        Language::Rust => "bool char str String usize isize u8 u16 u32 u64 u128 i8 i16 i32 i64 i128 f32 f64 Vec Option Result Box Some None Ok Err",
        Language::Python => "str int float bool list dict set tuple bytes object",
        Language::Sql => "int integer text varchar boolean numeric decimal timestamp date",
        _ => "",
    };
    types
        .split_whitespace()
        .any(|kind| kind == value)
        .then_some(CodeTone::Type)
}
impl Lexer {
    fn spans(&mut self, text: &str, start: usize, lang: Language) -> Vec<CodeSpan> {
        let chars: Vec<char> = text.chars().collect();
        let mut spans = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let begin = i;
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            let tone;
            if self.block > 0 {
                while i < chars.len() {
                    if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        self.block -= 1;
                        i += 2;
                        if self.block == 0 {
                            break;
                        }
                    } else if lang == Language::Rust
                        && chars[i] == '/'
                        && chars.get(i + 1) == Some(&'*')
                    {
                        self.block += 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                tone = Some(CodeTone::Comment);
            } else if self.quote.is_some()
                || c == '"'
                || c == '\''
                || (lang == Language::Js && c == '`')
            {
                let quote = self.quote.unwrap_or(c);
                if self.quote.is_none() {
                    i += 1;
                }
                self.quote = Some(quote);
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i = (i + 2).min(chars.len());
                    } else if chars[i] == quote {
                        i += 1;
                        self.quote = None;
                        break;
                    } else {
                        i += 1;
                    }
                }
                if quote != '`' && lang != Language::Shell {
                    self.quote = None;
                }
                tone = Some(CodeTone::String);
            } else if (matches!(lang, Language::Js | Language::Rust | Language::Json)
                && c == '/'
                && next == Some('/'))
                || (lang == Language::Sql && c == '-' && next == Some('-'))
                || (matches!(lang, Language::Python | Language::Shell) && c == '#')
            {
                i = chars.len();
                tone = Some(CodeTone::Comment);
            } else if matches!(
                lang,
                Language::Js | Language::Rust | Language::Json | Language::Sql
            ) && c == '/'
                && next == Some('*')
            {
                self.block = 1;
                i += 2;
                while i < chars.len() {
                    if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        self.block -= 1;
                        i += 2;
                        if self.block == 0 {
                            break;
                        }
                    } else if lang == Language::Rust
                        && chars[i] == '/'
                        && chars.get(i + 1) == Some(&'*')
                    {
                        self.block += 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                tone = Some(CodeTone::Comment);
            } else if c.is_ascii_digit() {
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric()
                        || chars[i] == '_'
                        || (chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)))
                {
                    i += 1;
                }
                tone = Some(CodeTone::Number);
            } else if c.is_alphabetic() || c == '_' || c == '$' {
                i += 1;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
                {
                    i += 1;
                }
                tone = word_tone(&chars[begin..i].iter().collect::<String>(), lang);
            } else {
                i += 1;
                tone = None;
            }
            push(&mut spans, start + begin, &chars[begin..i], tone);
        }
        spans
    }
}
pub(super) fn code_rows(lines: &[String]) -> Vec<Option<CodeRow>> {
    let mut rows = vec![None; lines.len()];
    let mut active: Option<(char, usize, String, usize)> = None;
    let mut lexer = Lexer::default();
    for (index, line) in lines.iter().enumerate() {
        if let Some((marker, count, name, start)) = &active {
            let closing = fence(line)
                .is_some_and(|(m, n, tail)| m == *marker && n >= *count && tail.trim().is_empty());
            let lang = language(name);
            let spans = if closing {
                Vec::new()
            } else if lang == Language::Plain {
                let mut spans = Vec::new();
                push(&mut spans, 0, &line.chars().collect::<Vec<_>>(), None);
                spans
            } else {
                lexer.spans(line, 0, lang)
            };
            rows[index] = Some(CodeRow {
                start: *start,
                end: lines.len() - 1,
                language: name.clone(),
                kind: if closing {
                    CodeRowKind::Closing
                } else {
                    CodeRowKind::Content
                },
                spans,
            });
            if closing {
                for row in rows[*start..=index].iter_mut().flatten() {
                    row.end = index;
                }
                active = None;
                lexer = Lexer::default();
            }
        } else if let Some((marker, count, tail)) =
            fence(line).filter(|(marker, _, tail)| *marker != '`' || !tail.contains('`'))
        {
            let name = tail
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            active = Some((marker, count, name.clone(), index));
            rows[index] = Some(CodeRow {
                start: index,
                end: lines.len() - 1,
                language: name,
                kind: CodeRowKind::Opening,
                spans: Vec::new(),
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rows(lines: &[&str]) -> Vec<Option<CodeRow>> {
        code_rows(&lines.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    #[test]
    fn fences_match_marker_and_length() {
        let r = rows(&["````ts", "```", "~~~~", "```` nope", "`````  ", "outside"]);
        assert_eq!(r[0].as_ref().unwrap().kind, CodeRowKind::Opening);
        for row in &r[1..4] {
            assert_eq!(row.as_ref().unwrap().kind, CodeRowKind::Content);
        }
        assert_eq!(r[4].as_ref().unwrap().kind, CodeRowKind::Closing);
        assert!(r[5].is_none());
        assert!(rows(&["    ```ts"])[0].is_none());
    }
    #[test]
    fn block_ranges_use_source_rows_and_unclosed_end() {
        let r = rows(&[
            "before", "```py", "x = 1", "```", "between", "  ~~~sql", "SELECT 2",
        ]);
        for row in r[1..=3].iter().flatten() {
            assert_eq!((row.start, row.end), (1, 3));
        }
        for row in r[5..].iter().flatten() {
            assert_eq!((row.start, row.end), (5, 6));
        }
        assert!(r[0].is_none());
        assert!(r[4].is_none());
        assert!(code_rows(&[]).is_empty());
        assert_eq!(r[2].as_ref().unwrap().spans[0].start, 0);
    }
    #[test]
    fn aliases_and_unknown_languages() {
        for alias in ["ts", "typescript", "tsx", "js", "javascript", "jsx"] {
            assert_eq!(language(alias), Language::Js);
        }
        let r = rows(&["~~~unknown", "const value = 10", "~~~"]);
        assert!(r[1]
            .as_ref()
            .unwrap()
            .spans
            .iter()
            .all(|s| s.tone.is_none()));
    }
    #[test]
    fn typescript_colors_and_preserves_unicode_offsets() {
        let r = rows(&["```typescript", "const café: number = 10;", "```"]);
        let row = r[1].as_ref().unwrap();
        assert_eq!(
            row.spans
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>(),
            "const café: number = 10;"
        );
        assert_eq!(row.spans[0].tone, Some(CodeTone::Keyword));
        assert!(row
            .spans
            .iter()
            .any(|s| s.text == "number" && s.tone == Some(CodeTone::Type)));
        assert!(row
            .spans
            .iter()
            .any(|s| s.text == "10" && s.tone == Some(CodeTone::Number)));
        let mut position = 0;
        for span in &row.spans {
            assert_eq!(span.start, position);
            position += span.text.chars().count();
        }
        assert_eq!(position, "const café: number = 10;".chars().count());
        assert_eq!((row.start, row.end), (0, 2));
    }
    #[test]
    fn comments_and_strings_keep_their_contents() {
        let r = rows(&[
            "```ts",
            "const s = \"// if 10\"; /* start",
            "const ignored = 20 */ let n = 1;",
            "`line",
            "const template = 5` // comment",
        ]);
        assert!(r[1]
            .as_ref()
            .unwrap()
            .spans
            .iter()
            .any(|s| s.text == "\"// if 10\"" && s.tone == Some(CodeTone::String)));
        assert_eq!(
            r[2].as_ref().unwrap().spans[0].tone,
            Some(CodeTone::Comment)
        );
        assert_eq!(r[4].as_ref().unwrap().spans[0].tone, Some(CodeTone::String));
        assert_eq!(
            r[4].as_ref().unwrap().spans.last().unwrap().tone,
            Some(CodeTone::Comment)
        );
    }
}

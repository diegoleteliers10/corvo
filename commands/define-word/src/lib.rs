//! Define a word with the system dictionary, and open Dictionary.
//!
//! `define word` is the explicit form. A single word with no other match
//! also returns a low-ranked definition row, the way a dictionary fallback
//! works, and that row disappears when the command is hidden in Settings.

use std::sync::{Mutex, OnceLock};

use corvo_core::{
    phosphor_svgs, Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext,
    Icon, NativeAction, SearchContext, SearchResult,
};
use corvo_platform::DictionaryDefinition;

const PREFIX: &str = "define ";
const MAX_FALLBACK_WORD_CHARS: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Lookup {
    Sense {
        word: String,
        sense: DictionaryDefinition,
    },
    Missing,
    Unsupported,
}

fn cache() -> &'static Mutex<Option<(String, Lookup)>> {
    static CACHE: OnceLock<Mutex<Option<(String, Lookup)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Looks `word` up and keeps the last answer, because search and execute
/// both need it and the dictionary call is not free.
fn lookup(word: &str) -> Option<Lookup> {
    let key = word.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    if let Ok(cached) = cache().lock() {
        if let Some((cached_word, result)) = cached.as_ref() {
            if cached_word == &key {
                return Some(result.clone());
            }
        }
    }

    let result = match corvo_platform::dictionary_definitions(&key) {
        Ok(definitions) if !definitions.is_empty() => {
            let sense = definitions[0].clone();
            Lookup::Sense {
                word: key.clone(),
                sense,
            }
        }
        Ok(_) => Lookup::Missing,
        Err(_) => Lookup::Unsupported,
    };

    if let Ok(mut cached) = cache().lock() {
        *cached = Some((key, result.clone()));
    }
    Some(result)
}

/// The word a query asks about, and whether the user typed the prefix.
///
/// `define stone` is explicit. A bare `stone` only qualifies as a fallback
/// when it is one plain word.
fn requested_word(query: &str) -> Option<(String, bool)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Some((String::new(), true));
    }
    let prefix_word = PREFIX.trim_end();
    if let Some(head) = trimmed.get(..prefix_word.len()) {
        if head.eq_ignore_ascii_case(prefix_word) {
            let rest = &trimmed[prefix_word.len()..];
            if rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace) {
                return Some((rest.trim().to_string(), true));
            }
        }
    }
    let single_word = !trimmed.chars().any(char::is_whitespace);
    let plain = trimmed
        .chars()
        .all(|character| character.is_alphabetic() || matches!(character, '-' | '\''))
        && trimmed.chars().count() >= 3
        && trimmed.chars().count() <= MAX_FALLBACK_WORD_CHARS;
    (single_word && plain).then(|| (trimmed.to_string(), false))
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

fn entry_text(word: &str, sense: &DictionaryDefinition) -> String {
    match &sense.part_of_speech {
        Some(part) => format!("{word} ({part}): {}", sense.text),
        None => format!("{word}: {}", sense.text),
    }
}

fn preview(text: &str) -> String {
    const LIMIT: usize = 90;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let slice: String = text.chars().take(LIMIT).collect();
    format!("{slice}...")
}

fn sense_row(word: &str, sense: &DictionaryDefinition, explicit: bool) -> SearchResult {
    let accessory = sense
        .part_of_speech
        .as_deref()
        .map(capitalize)
        .unwrap_or_else(|| "Definition".into());
    SearchResult {
        id: format!("define-word:sense:{word}"),
        title: capitalize(word),
        subtitle: Some(preview(&sense.text)),
        icon: Icon::Svg(phosphor_svgs::style::regular::BOOK_OPEN),
        score: if explicit { 980 } else { 25 },
        accessory: Some(accessory),
        section: None,
        accessories: Vec::new(),
    }
}

fn notice_row(word: &str, title: &str, subtitle: &str) -> SearchResult {
    SearchResult {
        id: format!("define-word:notice:{word}"),
        title: title.to_string(),
        subtitle: Some(subtitle.to_string()),
        icon: Icon::Svg(phosphor_svgs::style::regular::BOOK_OPEN),
        score: 970,
        accessory: Some("Dictionary".into()),
        section: None,
        accessories: Vec::new(),
    }
}

#[derive(Default)]
pub struct DefineWordCommand;

corvo_core::register_command!(DefineWordCommand);

#[async_trait::async_trait]
impl Command for DefineWordCommand {
    fn id(&self) -> &'static str {
        "define-word"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["define", "dictionary", "meaning", "definition", "word"]
    }

    fn prefix(&self) -> Option<&'static str> {
        Some(PREFIX)
    }

    fn priority(&self) -> u8 {
        20
    }

    async fn search(&self, query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        let Some((word, explicit)) = requested_word(query) else {
            return Vec::new();
        };
        if word.is_empty() {
            return if explicit {
                vec![notice_row(
                    "hint",
                    "Type a word to define",
                    "For example: define stone",
                )]
            } else {
                Vec::new()
            };
        }
        if !explicit && !query.trim().eq_ignore_ascii_case(&word) {
            return Vec::new();
        }

        match lookup(&word) {
            Some(Lookup::Sense { word, sense }) => vec![sense_row(&word, &sense, explicit)],
            Some(Lookup::Missing) if explicit => vec![notice_row(
                &word,
                &format!("No definition found for {word}"),
                "The system dictionaries have no entry for this word.",
            )],
            Some(Lookup::Unsupported) if explicit => vec![notice_row(
                &word,
                "Dictionary is unavailable",
                "The system dictionary is available on macOS only.",
            )],
            _ => Vec::new(),
        }
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        if result_id.starts_with("define-word:notice:") {
            return Ok(Action::ShowDetail {
                title: "Define Word".into(),
                markdown: "The system dictionary is available on macOS only. Windows and Linux have no dictionary API here.".into(),
                metadata: Vec::new(),
            });
        }
        let Some(word) = result_id.strip_prefix("define-word:sense:") else {
            return Err(CommandError::NotFound);
        };
        match lookup(word) {
            Some(Lookup::Sense { word, sense }) => Ok(Action::Copy(entry_text(&word, &sense))),
            Some(Lookup::Unsupported) => Ok(Action::ShowToast(
                "The system dictionary is available on macOS only".into(),
            )),
            _ => Err(CommandError::NotFound),
        }
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(word) = result_id.strip_prefix("define-word:sense:") else {
            return Vec::new();
        };
        let Some(Lookup::Sense { word, sense }) = lookup(word) else {
            return Vec::new();
        };
        let text = entry_text(&word, &sense);
        let part = sense
            .part_of_speech
            .as_deref()
            .map(capitalize)
            .unwrap_or_else(|| "Definition".into());
        vec![
            CommandAction {
                id: "define-word-action:copy".into(),
                label: "Copy Definition".into(),
                action: Action::Copy(text.clone()),
                icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            },
            CommandAction {
                id: "define-word-action:open".into(),
                label: "Open in Dictionary".into(),
                action: Action::RunNative(NativeAction::OpenDictionary),
                icon: Icon::Svg(phosphor_svgs::style::regular::BOOK_OPEN),
                group: ActionGroup::Standard,
                hotkey: Some("cmd+enter"),
            },
            CommandAction {
                id: "define-word-action:detail".into(),
                label: "Show Full Definition".into(),
                action: Action::ShowDetail {
                    title: capitalize(&word),
                    markdown: format!("**{}**\n\n{}", part, sense.text),
                    metadata: Vec::new(),
                },
                icon: Icon::Svg(phosphor_svgs::style::regular::NOTEBOOK),
                group: ActionGroup::Standard,
                hotkey: Some("alt+enter"),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_query_returns_the_word() {
        assert_eq!(
            requested_word("define stone"),
            Some(("stone".to_string(), true))
        );
        assert_eq!(
            requested_word("  DEFINE Stone "),
            Some(("Stone".to_string(), true))
        );
    }

    #[test]
    fn an_empty_prefix_query_asks_for_a_word() {
        assert_eq!(requested_word("define"), Some((String::new(), true)));
        assert_eq!(requested_word("define "), Some((String::new(), true)));
    }

    #[test]
    fn a_single_plain_word_is_a_fallback_candidate() {
        assert_eq!(requested_word("stone"), Some(("stone".to_string(), false)));
        assert_eq!(
            requested_word("well-known"),
            Some(("well-known".to_string(), false))
        );
    }

    #[test]
    fn multi_word_and_symbol_queries_are_not_candidates() {
        assert_eq!(requested_word("stone soup"), None);
        assert_eq!(requested_word("2+2"), None);
        assert_eq!(requested_word("ab"), None);
    }

    #[test]
    fn entry_text_names_the_part_of_speech() {
        let sense = DictionaryDefinition {
            part_of_speech: Some("noun".into()),
            text: "a hard solid mineral.".into(),
        };
        assert_eq!(
            entry_text("stone", &sense),
            "stone (noun): a hard solid mineral."
        );
    }

    #[test]
    fn preview_truncates_long_text() {
        let text = "a".repeat(200);
        let shown = preview(&text);
        assert!(shown.ends_with("..."));
        assert_eq!(shown.chars().count(), 93);
    }
}

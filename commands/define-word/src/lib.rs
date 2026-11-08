//! Define a word with the system dictionary, and open Dictionary.
//!
//! `define word` is the explicit form and the only one that asks the system
//! dictionary. A bare word can still return a low-ranked definition row, but
//! only when that word was already defined this session: ordinary launcher
//! typing must not run a system lookup per keystroke.

use std::sync::{Mutex, OnceLock};

use corvo_core::{
    phosphor_svgs, Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext,
    Icon, NativeAction, SearchContext, SearchResult,
};
use corvo_platform::DictionaryDefinition;

const PREFIX: &str = "define ";
const MAX_FALLBACK_WORD_CHARS: usize = 40;
/// Definitions kept for the implicit fallback. Small on purpose: this only
/// avoids repeating a lookup the user already asked for.
const MAX_CACHED_WORDS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Lookup {
    Sense {
        word: String,
        sense: DictionaryDefinition,
    },
    Missing,
    Unsupported,
}

fn cache() -> &'static Mutex<Vec<(String, Lookup)>> {
    static CACHE: OnceLock<Mutex<Vec<(String, Lookup)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

fn normalize(word: &str) -> String {
    word.trim().to_lowercase()
}

/// A word the user already defined. Never touches the system dictionary.
fn cached(word: &str) -> Option<Lookup> {
    let key = normalize(word);
    if key.is_empty() {
        return None;
    }
    let mut entries = cache().lock().ok()?;
    let position = entries.iter().position(|(entry, _)| entry == &key)?;
    let entry = entries.remove(position);
    let result = entry.1.clone();
    entries.push(entry);
    Some(result)
}

fn remember(word: &str, result: &Lookup) {
    let key = normalize(word);
    if key.is_empty() {
        return;
    }
    let Ok(mut entries) = cache().lock() else {
        return;
    };
    entries.retain(|(entry, _)| entry != &key);
    entries.push((key, result.clone()));
    if entries.len() > MAX_CACHED_WORDS {
        entries.remove(0);
    }
}

/// Ask the system dictionary. Only the explicit `define ` path calls this.
fn lookup(word: &str) -> Option<Lookup> {
    let key = normalize(word);
    if key.is_empty() {
        return None;
    }
    if let Some(result) = cached(&key) {
        return Some(result);
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
    remember(&key, &result);
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

/// Rows for one query. `lookup_word` is the system call, injected so a test can
/// prove when it runs and when it must not.
///
/// Only an explicit `define ` query calls it. A bare word reads the session
/// cache and nothing else: the launcher searches on every keystroke, and a
/// system dictionary query per keystroke is exactly what this command must not
/// do while the user is typing an ordinary search.
fn search_with<F>(query: &str, mut lookup_word: F) -> Vec<SearchResult>
where
    F: FnMut(&str) -> Option<Lookup>,
{
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

    let found = if explicit {
        // The cache answers first so a repeated explicit query, and the row
        // that follows it, never repeat the system call.
        cached(&word).or_else(|| lookup_word(&word))
    } else {
        cached(&word)
    };

    match found {
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
        search_with(query, lookup)
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

    /// Serializes the cases: the definition cache is process-wide.
    fn isolate() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        let guard = LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(mut entries) = cache().lock() {
            entries.clear();
        }
        guard
    }

    fn sense(text: &str) -> Lookup {
        Lookup::Sense {
            word: "stone".into(),
            sense: DictionaryDefinition {
                part_of_speech: Some("noun".into()),
                text: text.into(),
            },
        }
    }

    #[test]
    fn ordinary_search_never_calls_the_dictionary() {
        let _guard = isolate();
        let mut calls = 0;
        let results = search_with("stone", |_| {
            calls += 1;
            None
        });
        assert_eq!(calls, 0, "an ordinary keystroke must not query the system dictionary");
        assert!(results.is_empty());
    }

    #[test]
    fn every_keystroke_of_an_ordinary_word_stays_off_the_dictionary() {
        let _guard = isolate();
        let mut calls = 0;
        for query in ["sto", "ston", "stone", "stonex"] {
            search_with(query, |_| {
                calls += 1;
                None
            });
        }
        assert_eq!(calls, 0, "typing a word must not accumulate lookups");
    }

    #[test]
    fn explicit_query_asks_the_dictionary_once() {
        let _guard = isolate();
        let mut calls = 0;
        let mut seen = None;
        let results = search_with("define stone", |word| {
            calls += 1;
            seen = Some(word.to_string());
            Some(sense("a hard solid mineral."))
        });
        assert_eq!(calls, 1);
        assert_eq!(seen.as_deref(), Some("stone"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 980);
    }

    #[test]
    fn a_cached_word_answers_an_ordinary_query_without_a_lookup() {
        let _guard = isolate();
        remember("stone", &sense("a hard solid mineral."));
        let mut calls = 0;
        let results = search_with("stone", |_| {
            calls += 1;
            None
        });
        assert_eq!(calls, 0, "the fallback row comes from the cache");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 25);
    }

    #[test]
    fn an_explicit_query_answers_from_the_cache_too() {
        let _guard = isolate();
        remember("stone", &sense("a hard solid mineral."));
        let mut calls = 0;
        let results = search_with("define stone", |word| {
            calls += 1;
            lookup(word)
        });
        assert_eq!(results.len(), 1, "the cache must answer before the system call");
        assert_eq!(calls, 0);
    }

    #[test]
    fn the_definition_cache_is_bounded() {
        let _guard = isolate();
        for index in 0..(MAX_CACHED_WORDS + 4) {
            remember(&format!("word{index}"), &sense("x"));
        }
        assert_eq!(cache().lock().unwrap().len(), MAX_CACHED_WORDS);
        assert!(
            cached("word0").is_none(),
            "the oldest entry is evicted"
        );
        assert!(cached(&format!("word{}", MAX_CACHED_WORDS + 3)).is_some());
    }

    #[test]
    fn a_missing_definition_is_a_notice_only_when_explicit() {
        let _guard = isolate();
        let explicit = search_with("define zzzz", |_| Some(Lookup::Missing));
        assert_eq!(explicit.len(), 1);
        assert_eq!(explicit[0].score, 970);
        let implicit = search_with("zzzz", |_| Some(Lookup::Missing));
        assert!(implicit.is_empty());
    }

    #[test]
    fn non_word_queries_do_not_reach_the_dictionary() {
        let _guard = isolate();
        let mut calls = 0;
        for query in ["git status", "2+2", "ab", ""] {
            search_with(query, |_| {
                calls += 1;
                None
            });
        }
        assert_eq!(calls, 0);
    }

    #[test]
    fn an_empty_prefix_asks_for_a_word_without_a_lookup() {
        let _guard = isolate();
        let mut calls = 0;
        let results = search_with("define ", |_| {
            calls += 1;
            None
        });
        assert_eq!(calls, 0);
        assert_eq!(results.len(), 1);
        assert!(results[0].id.starts_with("define-word:notice:hint"));
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

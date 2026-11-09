//! Inline arithmetic and unit conversion with pure parse-or-fall semantics.
//! Answers any query that parses completely as math or conversion, like `2+2` or `10 kg to lb`.

pub mod currency;
pub mod engine;
pub mod history;
pub mod scalar;
pub mod units;

pub use engine::{evaluate, evaluate_with, CalcOutcome};
pub use scalar::{evaluate_scalar, ScalarError};
pub use units::{convert_units, Dim, UNITS};

/// Starts the background exchange-rates fetch, so the first currency
/// query usually finds warm rates. Also loads the evaluation history off
/// the search path.
pub fn warmup() {
    currency::ensure_warm();
    history::warmup();
}

fn history_enabled(ctx: &SearchContext) -> bool {
    ctx.store
        .as_ref()
        .is_none_or(|store| store.command_enabled("calculator-history"))
}

fn history_row(entry: &history::HistoryEntry, index: usize) -> SearchResult {
    SearchResult {
        id: format!("calculator:history:{index}"),
        title: entry.result.clone(),
        subtitle: Some(entry.expression.clone()),
        icon: Icon::Calculator,
        score: 900 - index as i32,
        accessory: Some("History".into()),
        section: None,
        accessories: Vec::new(),
    }
}

fn clear_history_row() -> SearchResult {
    SearchResult {
        id: "calculator:clear-history".into(),
        title: "Clear Calculator History".into(),
        subtitle: Some("Remove every stored evaluation".into()),
        icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
        score: 10,
        accessory: Some("History".into()),
        section: None,
        accessories: Vec::new(),
    }
}

use corvo_core::{
    phosphor_svgs, Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext,
    Icon, SearchContext, SearchResult,
};

#[derive(Default)]
pub struct CalculatorCommand;

corvo_core::register_command!(CalculatorCommand);

#[async_trait::async_trait]
impl Command for CalculatorCommand {
    fn id(&self) -> &'static str {
        "calculator"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["calc", "math", "convert", "conversion", "units"]
    }

    fn priority(&self) -> u8 {
        80
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        currency::ensure_warm();
        let trimmed = query.trim();
        let after_prefix = trimmed
            .get(.."calculator".len())
            .filter(|head| head.eq_ignore_ascii_case("calculator"))
            .map(|_| trimmed["calculator".len()..].trim_start());
        let scoped = after_prefix.is_some();
        let expression = after_prefix.unwrap_or(trimmed);

        if scoped && expression.is_empty() {
            if !history_enabled(ctx) {
                return Vec::new();
            }
            let mut rows: Vec<SearchResult> = history::all()
                .iter()
                .enumerate()
                .map(|(index, entry)| history_row(entry, index))
                .collect();
            rows.push(clear_history_row());
            return rows;
        }

        if expression.is_empty() {
            return Vec::new();
        }

        let Some(outcome) = evaluate(expression) else {
            return Vec::new();
        };

        let is_error = matches!(outcome, CalcOutcome::Error { .. });
        let title = outcome.display_result().to_string();
        let subtitle = Some(format!("= {}", outcome.expression()));
        let accessory = Some(outcome.category().to_string());
        let raw_val = outcome.raw_value().to_string();

        vec![SearchResult {
            id: if is_error {
                format!("calculator:error:{raw_val}")
            } else {
                format!("calculator:ok:{expression}")
            },
            title,
            subtitle,
            icon: Icon::Calculator,
            score: 100_000,
            accessory,
            section: None,
            accessories: Vec::new(),
        }]
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        if result_id == "calculator:clear-history" {
            history::clear();
            return Ok(Action::ShowToast("Calculator history cleared".into()));
        }
        if let Some(index) = result_id.strip_prefix("calculator:history:") {
            let index: usize = index.parse().map_err(|_| CommandError::NotFound)?;
            let entry = history::all()
                .into_iter()
                .nth(index)
                .ok_or(CommandError::NotFound)?;
            return Ok(Action::Copy(entry.result));
        }
        if let Some(expression) = result_id.strip_prefix("calculator:ok:") {
            let outcome = evaluate(expression).ok_or(CommandError::NotFound)?;
            let value = outcome.display_result().to_string();
            if !matches!(outcome, CalcOutcome::Error { .. }) {
                history::record(outcome.expression(), &value);
            }
            return Ok(Action::Copy(value));
        }
        if let Some(message) = result_id.strip_prefix("calculator:error:") {
            return Ok(Action::Copy(message.to_owned()));
        }
        Err(CommandError::NotFound)
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        if result_id == "calculator:clear-history" {
            return Vec::new();
        }
        if let Some(index) = result_id.strip_prefix("calculator:history:") {
            let Ok(index) = index.parse::<usize>() else {
                return Vec::new();
            };
            let Some(entry) = history::all().into_iter().nth(index) else {
                return Vec::new();
            };
            let rerun = evaluate(&entry.expression)
                .map(|outcome| outcome.display_result().to_string())
                .unwrap_or_else(|| entry.result.clone());
            return vec![
                CommandAction {
                    id: "calculator:history-copy".into(),
                    label: "Copy Result".into(),
                    action: Action::Copy(entry.result.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                    group: ActionGroup::Primary,
                    hotkey: Some("enter"),
                },
                CommandAction {
                    id: "calculator:history-paste".into(),
                    label: "Paste Result".into(),
                    action: Action::PasteText(entry.result.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                    group: ActionGroup::Standard,
                    hotkey: Some("cmd+enter"),
                },
                CommandAction {
                    id: "calculator:history-copy-expression".into(),
                    label: "Copy Expression".into(),
                    action: Action::Copy(entry.expression.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::MATH_OPERATIONS),
                    group: ActionGroup::Standard,
                    hotkey: Some("alt+enter"),
                },
                CommandAction {
                    id: "calculator:history-rerun".into(),
                    label: "Run Again".into(),
                    action: Action::Copy(rerun),
                    icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_CLOCKWISE),
                    group: ActionGroup::Standard,
                    hotkey: None,
                },
            ];
        }
        if let Some(expression) = result_id.strip_prefix("calculator:ok:") {
            let Some(outcome) = evaluate(expression) else {
                return Vec::new();
            };
            let value = outcome.display_result().to_string();
            return vec![
                CommandAction {
                    id: "calculator:copy".into(),
                    label: "Copy Answer".into(),
                    action: Action::Copy(value.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                    group: ActionGroup::Primary,
                    hotkey: Some("enter"),
                },
                CommandAction {
                    id: "calculator:paste".into(),
                    label: "Paste to Active App".into(),
                    action: Action::PasteText(value),
                    icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                    group: ActionGroup::Standard,
                    hotkey: Some("cmd+enter"),
                },
            ];
        }

        if let Some(message) = result_id.strip_prefix("calculator:error:") {
            return vec![CommandAction {
                id: "calculator:copy_error".into(),
                label: "Copy Message".into(),
                action: Action::Copy(message.to_owned()),
                icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            }];
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    /// The history store is process-wide, so a test that clears it must not
    /// interleave with a test that records and then reads it.
    static HISTORY_TESTS: Mutex<()> = Mutex::new(());

    fn isolate_history() -> MutexGuard<'static, ()> {
        // A panicking test poisons the lock; recovering keeps one failure from
        // failing every later test that needs the same store.
        HISTORY_TESTS.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn search_context() -> SearchContext {
        SearchContext {
            max_results: 50,
            store: None,
        }
    }

    fn execution_context() -> ExecutionContext {
        ExecutionContext { store: None }
    }

    #[test]
    fn a_bare_expression_is_evaluated() {
        let command = CalculatorCommand;
        let results = smol::block_on(command.search("10/2", &search_context()));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "5");
        assert_eq!(results[0].id, "calculator:ok:10/2");
    }

    #[test]
    fn a_word_search_returns_no_calculator_row() {
        let command = CalculatorCommand;
        for query in ["c", "s", "v", "chrome", "calculator history"] {
            let results = smol::block_on(command.search(query, &search_context()));
            assert!(results.is_empty(), "{query} returned {results:?}");
        }
    }

    #[test]
    fn typing_does_not_record_history() {
        let _history = isolate_history();
        let command = CalculatorCommand;
        let _ = smol::block_on(command.search("76543*3", &search_context()));
        assert!(!history::all()
            .iter()
            .any(|entry| entry.expression == "76543*3"));
    }

    #[test]
    fn committing_an_answer_records_history_and_copies_it() {
        let _history = isolate_history();
        let command = CalculatorCommand;
        let action =
            smol::block_on(command.execute("calculator:ok:321*3", &execution_context())).unwrap();
        assert_eq!(action, Action::Copy("963".into()));
        assert!(history::all()
            .iter()
            .any(|entry| entry.expression == "321*3" && entry.result == "963"));
    }

    #[test]
    fn a_prefixed_expression_still_evaluates_and_commits() {
        let _history = isolate_history();
        let command = CalculatorCommand;
        let results = smol::block_on(command.search("calculator 7*6", &search_context()));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "42");
        let action = smol::block_on(command.execute(&results[0].id, &execution_context())).unwrap();
        assert_eq!(action, Action::Copy("42".into()));
    }

    #[test]
    fn scoped_query_lists_history_and_a_clear_row() {
        let _history = isolate_history();
        history::record("2+2", "4");
        let command = CalculatorCommand;
        let results = smol::block_on(command.search("calculator", &search_context()));
        assert!(results.iter().any(|result| result.title == "4"));
        assert!(results
            .iter()
            .any(|result| result.id == "calculator:clear-history"));
    }

    #[test]
    fn scoped_query_still_evaluates() {
        let command = CalculatorCommand;
        let results = smol::block_on(command.search("calculator 7*6", &search_context()));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "42");
    }

    #[test]
    fn a_plain_expression_still_evaluates() {
        let command = CalculatorCommand;
        let results = smol::block_on(command.search("7*6", &search_context()));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "42");
    }

    #[test]
    fn clearing_history_leaves_the_clear_row_alone() {
        let _history = isolate_history();
        history::record("9*9", "81");
        history::clear();
        let command = CalculatorCommand;
        let results = smol::block_on(command.search("calculator", &search_context()));
        assert!(!results.iter().any(|result| result.title == "81"));
        assert!(results
            .iter()
            .any(|result| result.id == "calculator:clear-history"));
    }

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
    }

    #[test]
    fn converts_temperature() {
        let res = evaluate("32 f in c").unwrap();
        assert_eq!(res.display_result(), "0 °C");

        let res = evaluate("100 c to f").unwrap();
        assert_eq!(res.display_result(), "212 °F");

        let res = evaluate("0 c to k").unwrap();
        assert_eq!(res.display_result(), "273.15 K");
    }

    #[test]
    fn respects_precedence() {
        assert_eq!(evaluate_scalar("2 + 2 * 2"), Ok(6.0));
        assert_eq!(evaluate_scalar("(2 + 2) * 2"), Ok(8.0));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(evaluate_scalar("1 / 0"), Err(ScalarError::DivisionByZero));
        assert_eq!(evaluate_scalar("(1 + 2"), Err(ScalarError::InvalidExpression));
        assert_eq!(evaluate_scalar(""), Err(ScalarError::InvalidExpression));
        assert_eq!(evaluate_scalar("foo"), Err(ScalarError::InvalidExpression));
    }
}

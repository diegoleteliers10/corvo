//! Inline arithmetic and unit conversion with pure parse-or-fall semantics.
//! Answers any query that parses completely as math or conversion, like `2+2` or `10 kg to lb`.

pub mod engine;
pub mod scalar;
pub mod units;

pub use engine::{evaluate, CalcOutcome};
pub use scalar::{evaluate_scalar, ScalarError};
pub use units::{convert_units, Dim, UNITS};

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

    async fn search(&self, query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        let Some(outcome) = evaluate(query) else {
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
                format!("calculator:{raw_val}")
            },
            title,
            subtitle,
            icon: Icon::Calculator,
            score: 100_000,
            accessory,
        }]
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let value = if let Some(err) = result_id.strip_prefix("calculator:error:") {
            err
        } else if let Some(val) = result_id.strip_prefix("calculator:") {
            val
        } else {
            return Err(CommandError::NotFound);
        };
        Ok(Action::Copy(value.to_owned()))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let (value, is_error) = if let Some(err) = result_id.strip_prefix("calculator:error:") {
            (err, true)
        } else if let Some(val) = result_id.strip_prefix("calculator:") {
            (val, false)
        } else {
            return Vec::new();
        };

        if is_error {
            return vec![CommandAction {
                id: "calculator:copy_error".into(),
                label: "Copy Message".into(),
                action: Action::Copy(value.to_owned()),
                icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            }];
        }

        vec![
            CommandAction {
                id: "calculator:copy".into(),
                label: "Copy Answer".into(),
                action: Action::Copy(value.to_owned()),
                icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            },
            CommandAction {
                id: "calculator:paste".into(),
                label: "Paste to Active App".into(),
                action: Action::PasteText(value.to_owned()),
                icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                group: ActionGroup::Standard,
                hotkey: Some("cmd+enter"),
            },
        ]
    }
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

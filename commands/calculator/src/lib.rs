//! Inline arithmetic and unit conversion. Answers any query that
//! parses as math, like `2+2` or `10 kg to lb`, with no prefix.

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
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let (text, detail) = match try_convert(query) {
            Some((value, unit)) => (format!("{} {unit}", format_conversion(value)), "Unit conversion"),
            None => match evaluate(query) {
                Ok(value) => (format_value(value), "Copy to clipboard"),
                Err(()) => return Vec::new(),
            },
        };
        vec![SearchResult {
            id: format!("calculator:{text}"),
            title: text,
            subtitle: Some(format!("= {query}")),
            icon: Icon::Calculator,
            score: 100.0,
            accessory: Some(detail.into()),
        }]
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let Some(value) = result_id.strip_prefix("calculator:") else {
            return Err(CommandError::NotFound);
        };
        Ok(Action::Copy(value.to_owned()))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(value) = result_id.strip_prefix("calculator:") else {
            return Vec::new();
        };
        vec![
            CommandAction {
                id: "calculator:copy".into(),
                label: "Copy Result".into(),
                action: Action::Copy(value.to_owned()),
                icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Primary,
                hotkey: Some("↵"),
            },
            CommandAction {
                id: "calculator:paste".into(),
                label: "Paste to Active App".into(),
                action: Action::Copy(value.to_owned()),
                icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                group: ActionGroup::Standard,
                hotkey: Some("⌘↵"),
            },
        ]
    }
}

fn format_value(value: f64) -> String {
    if value == value.trunc() && value.abs() < 9.0e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Token for the shunting-yard conversion.
#[derive(Clone, Copy, Debug)]
enum Tok {
    Num(f64),
    Bin(char),
    Un,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Result<Vec<Tok>, ()> {
    let mut toks = Vec::new();
    let mut prev: Option<Tok> = None;
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        match c {
            '0'..='9' | '.' => {
                let mut num = String::new();
                while matches!(chars.peek(), Some(d) if d.is_ascii_digit() || *d == '.') {
                    let Some(digit) = chars.next() else { return Err(()) };
                    num.push(digit);
                }
                let value: f64 = num.parse().map_err(|_| ())?;
                prev = Some(Tok::Num(value));
                toks.push(Tok::Num(value));
            }
            '+' | '*' | '/' | '%' | '^' => {
                chars.next();
                prev = Some(Tok::Bin(c));
                toks.push(Tok::Bin(c));
            }
            '-' => {
                chars.next();
                let unary = matches!(prev, None | Some(Tok::Bin(_)) | Some(Tok::Un) | Some(Tok::LParen));
                let tok = if unary { Tok::Un } else { Tok::Bin('-') };
                prev = Some(tok);
                toks.push(tok);
            }
            '(' => {
                chars.next();
                prev = Some(Tok::LParen);
                toks.push(Tok::LParen);
            }
            ')' => {
                chars.next();
                prev = Some(Tok::RParen);
                toks.push(Tok::RParen);
            }
            _ => return Err(()),
        }
    }
    Ok(toks)
}

fn precedence(op: char) -> u8 {
    match op {
        '+' | '-' => 1,
        '*' | '/' | '%' => 2,
        '^' => 3,
        _ => 0,
    }
}

fn apply_bin(op: char, a: f64, b: f64) -> Result<f64, ()> {
    match op {
        '+' => Ok(a + b),
        '-' => Ok(a - b),
        '*' => Ok(a * b),
        '/' if b != 0.0 => Ok(a / b),
        '%' if b != 0.0 => Ok(a % b),
        '^' => Ok(a.powf(b)),
        _ => Err(()),
    }
}

/// Shunting-yard evaluation of `+ - * / % ^`, parens, and unary minus.
fn evaluate(input: &str) -> Result<f64, ()> {
    let toks = tokenize(input)?;
    if toks.is_empty() {
        return Err(());
    }

    let mut output: Vec<Tok> = Vec::new();
    let mut ops: Vec<Tok> = Vec::new();
    for tok in toks {
        match tok {
            Tok::Num(_) => output.push(tok),
            Tok::Un => ops.push(tok),
            Tok::Bin(op) => {
                while let Some(top) = ops.last().copied() {
                    let pop = match top {
                        Tok::Bin(t) => {
                            precedence(t) > precedence(op)
                                || (precedence(t) == precedence(op) && op != '^')
                        }
                        Tok::Un => true,
                        _ => false,
                    };
                    if !pop {
                        break;
                    }
                    output.push(ops.pop().unwrap());
                }
                ops.push(tok);
            }
            Tok::LParen => ops.push(tok),
            Tok::RParen => loop {
                match ops.pop() {
                    Some(Tok::LParen) => break,
                    Some(t) => output.push(t),
                    None => return Err(()),
                }
            },
        }
    }
    while let Some(t) = ops.pop() {
        if matches!(t, Tok::LParen) {
            return Err(());
        }
        output.push(t);
    }

    let mut stack: Vec<f64> = Vec::new();
    for tok in output {
        match tok {
            Tok::Num(v) => stack.push(v),
            Tok::Un => {
                let Some(v) = stack.pop() else { return Err(()) };
                stack.push(-v);
            }
            Tok::Bin(op) => {
                let (Some(b), Some(a)) = (stack.pop(), stack.pop()) else {
                    return Err(());
                };
                stack.push(apply_bin(op, a, b)?);
            }
            _ => return Err(()),
        }
    }
    if stack.len() != 1 {
        return Err(());
    }
    let value = stack[0];
    if value.is_finite() {
        Ok(value)
    } else {
        Err(())
    }
}

/// Inline unit conversion without network access: `=10 kg to lb`,
/// `=32 f in c`. The left side may hold arithmetic, `=2*3 ft to m`.
fn try_convert(expr: &str) -> Option<(f64, String)> {
    let lowered = expr.to_lowercase();
    for separator in [" to ", " in ", "->", "→", "=>"] {
        let Some(at) = lowered.find(separator) else { continue };
        let left = lowered[..at].trim();
        let target = lowered[at + separator.len()..].trim();
        if left.is_empty() || target.contains(char::is_whitespace) {
            continue;
        }
        if let Some(result) = convert_pair(left, target) {
            return Some(result);
        }
    }
    None
}

/// Converts `<arith> <unit>` into `target`. The label echoes the target
/// token, or a symbol for temperature.
fn convert_pair(left: &str, target: &str) -> Option<(f64, String)> {
    let (number_expr, from) = left.rsplit_once(char::is_whitespace)?;
    let value = evaluate(number_expr.trim()).ok()?;
    if let (Some(from_temp), Some(to_temp)) = (temperature(from), temperature(target)) {
        return Some((convert_temperature(value, from_temp, to_temp), to_temp.label()));
    }
    let (from_group, from_factor) = linear_unit(from)?;
    let (to_group, to_factor) = linear_unit(target)?;
    if from_group != to_group {
        return None;
    }
    Some((value * from_factor / to_factor, target.to_string()))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UnitGroup {
    Length,
    Mass,
    Data,
    Time,
}

/// A unit's group and factor against the group base. The first alias is
/// the conventional short form.
fn linear_unit(name: &str) -> Option<(UnitGroup, f64)> {
    use UnitGroup::{Data, Length, Mass, Time};
    const UNITS: &[(&[&str], UnitGroup, f64)] = &[
        (&["m", "meter", "meters", "metre", "metres"], Length, 1.0),
        (&["km", "kilometer", "kilometers", "kilometre", "kilometres"], Length, 1000.0),
        (&["cm", "centimeter", "centimeters", "centimetre", "centimetres"], Length, 0.01),
        (&["mm", "millimeter", "millimeters", "millimetre", "millimetres"], Length, 0.001),
        (&["mi", "mile", "miles"], Length, 1609.344),
        (&["yd", "yard", "yards"], Length, 0.9144),
        (&["ft", "foot", "feet"], Length, 0.3048),
        (&["in", "inch", "inches"], Length, 0.0254),
        (&["kg", "kilogram", "kilograms", "kilo", "kilos"], Mass, 1.0),
        (&["g", "gram", "grams"], Mass, 0.001),
        (&["t", "ton", "tons", "tonne", "tonnes"], Mass, 1000.0),
        (&["lb", "lbs", "pound", "pounds"], Mass, 0.453_592_37),
        (&["oz", "ounce", "ounces"], Mass, 0.028_349_523_125),
        (&["b", "byte", "bytes"], Data, 1.0),
        (&["kb", "kilobyte", "kilobytes"], Data, 1000.0),
        (&["mb", "megabyte", "megabytes"], Data, 1_000_000.0),
        (&["gb", "gigabyte", "gigabytes"], Data, 1_000_000_000.0),
        (&["tb", "terabyte", "terabytes"], Data, 1_000_000_000_000.0),
        (&["kib", "kibibyte", "kibibytes"], Data, 1024.0),
        (&["mib", "mebibyte", "mebibytes"], Data, 1_048_576.0),
        (&["gib", "gibibyte", "gibibytes"], Data, 1_073_741_824.0),
        (&["s", "sec", "secs", "second", "seconds"], Time, 1.0),
        (&["min", "minute", "minutes"], Time, 60.0),
        (&["h", "hr", "hrs", "hour", "hours"], Time, 3600.0),
        (&["d", "day", "days"], Time, 86400.0),
        (&["wk", "week", "weeks"], Time, 604_800.0),
    ];
    UNITS
        .iter()
        .find(|(names, _, _)| names.contains(&name))
        .map(|(_, group, factor)| (*group, *factor))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Temperature {
    Celsius,
    Fahrenheit,
    Kelvin,
}

impl Temperature {
    fn label(self) -> String {
        match self {
            Self::Celsius => "°C".into(),
            Self::Fahrenheit => "°F".into(),
            Self::Kelvin => "K".into(),
        }
    }
}

fn temperature(name: &str) -> Option<Temperature> {
    match name {
        "c" | "celsius" | "centigrade" => Some(Temperature::Celsius),
        "f" | "fahrenheit" => Some(Temperature::Fahrenheit),
        "k" | "kelvin" => Some(Temperature::Kelvin),
        _ => None,
    }
}

fn convert_temperature(value: f64, from: Temperature, to: Temperature) -> f64 {
    use Temperature::{Celsius, Fahrenheit, Kelvin};
    let celsius = match from {
        Celsius => value,
        Fahrenheit => (value - 32.0) * 5.0 / 9.0,
        Kelvin => value - 273.15,
    };
    match to {
        Celsius => celsius,
        Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        Kelvin => celsius + 273.15,
    }
}

/// Trims conversion results to four decimals so `=10 kg to lb` reads
/// `22.0462 lb` instead of the full float tail.
fn format_conversion(value: f64) -> String {
    if value == value.trunc() && value.abs() < 9.0e15 {
        return format!("{}", value as i64);
    }
    let rounded = (value * 10_000.0).round() / 10_000.0;
    let mut text = format!("{rounded:.4}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{evaluate, format_conversion, try_convert};

    fn assert_converts(expr: &str, value: f64, unit: &str) {
        let Some((actual, actual_unit)) = try_convert(expr) else {
            panic!("{expr} converted to nothing");
        };
        assert!((actual - value).abs() < 1e-9, "{expr} gave {actual}, want {value}");
        assert_eq!(actual_unit, unit);
    }

    #[test]
    fn converts_length() {
        assert_converts("1 mi to km", 1.609344, "km");
        assert_converts("2*3 ft to m", 1.8288, "m");
    }

    #[test]
    fn converts_mass_and_data() {
        assert_converts("10 kg to lb", 22.046226218487757, "lb");
        assert_converts("1 gb to mb", 1000.0, "mb");
    }

    #[test]
    fn converts_temperature() {
        assert_converts("32 f in c", 0.0, "°C");
        assert_converts("100 c to f", 212.0, "°F");
        assert_converts("0 c to k", 273.15, "K");
    }

    #[test]
    fn rejects_mismatched_and_missing_units() {
        assert_eq!(try_convert("1 m to kg"), None);
        assert_eq!(try_convert("2 + 2"), None);
        assert_eq!(try_convert("10 parsecs to km"), None);
    }

    #[test]
    fn rounds_conversion_display() {
        assert_eq!(format_conversion(1.609344), "1.6093");
        assert_eq!(format_conversion(1000.0), "1000");
        assert_eq!(format_conversion(0.0), "0");
    }

    #[test]
    fn respects_precedence() {
        assert_eq!(evaluate("2 + 2 * 2"), Ok(6.0));
        assert_eq!(evaluate("(2 + 2) * 2"), Ok(8.0));
    }

    #[test]
    fn handles_unary_minus_and_power() {
        assert_eq!(evaluate("-3 + 10"), Ok(7.0));
        assert_eq!(evaluate("2 ^ 3 ^ 2"), Ok(512.0));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(evaluate("1 / 0"), Err(()));
        assert_eq!(evaluate("(1 + 2"), Err(()));
        assert_eq!(evaluate(""), Err(()));
        assert_eq!(evaluate("foo"), Err(()));
    }
}

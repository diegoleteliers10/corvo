//! Pure scalar shunting-yard arithmetic evaluation.

#[derive(Clone, Copy, Debug, PartialEq)]
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
                let mut dot_count = 0;
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        num.push(d);
                        chars.next();
                    } else if d == '.' {
                        dot_count += 1;
                        if dot_count > 1 {
                            return Err(());
                        }
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
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
                let unary = matches!(
                    prev,
                    None | Some(Tok::Bin(_)) | Some(Tok::Un) | Some(Tok::LParen)
                );
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

/// Evaluates a pure mathematical expression containing numbers, operators, and parentheses.
pub fn evaluate_scalar(input: &str) -> Result<f64, ()> {
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

pub fn format_value(value: f64) -> String {
    if value == value.trunc() && value.abs() < 9.0e15 {
        format!("{}", value as i64)
    } else {
        // Format with up to 6 decimal places, stripping trailing zeros
        let formatted = format!("{:.6}", value);
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        assert_eq!(evaluate_scalar("10-5"), Ok(5.0));
        assert_eq!(evaluate_scalar("1.5 + 3"), Ok(4.5));
        assert_eq!(evaluate_scalar("2 * (3 + 4)"), Ok(14.0));
        assert_eq!(evaluate_scalar("2^8"), Ok(256.0));
    }

    #[test]
    fn rejects_malformed() {
        assert!(evaluate_scalar("font-size").is_err());
        assert!(evaluate_scalar("1..2").is_err());
    }
}

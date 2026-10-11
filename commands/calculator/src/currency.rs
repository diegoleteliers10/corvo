//! Currency conversion with periodically refreshed exchange rates.
//!
//! Rates are USD-pivot (one USD in each currency), fetched from
//! open.er-api.com without an API key, cached for six hours. `search`
//! calls [`ensure_warm`] and reads [`active_rates`]: the fetch runs on a
//! background task and never blocks the search path.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Fiat currency codes the calculator recognizes, lowercase.
pub const CURRENCIES: &[&str] = &[
    "usd", "eur", "gbp", "jpy", "clp", "ars", "brl", "mxn", "cad", "chf", "cny", "inr", "aud",
    "nzd", "sek", "nok", "dkk", "pln", "czk", "huf", "ron", "try", "ils", "krw", "sgd", "hkd",
    "thb", "idr", "myr", "php", "zar", "cop", "pen", "uyu", "bob", "pyg",
];

const RATES_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const RATES_URL: &str = "https://open.er-api.com/v6/latest/USD";

/// USD-pivot exchange rates: how much one USD is worth in each currency.
#[derive(Clone, Debug, Default)]
pub struct Rates(pub HashMap<String, f64>);

impl Rates {
    pub fn rate(&self, code: &str) -> Option<f64> {
        self.0.get(code).copied()
    }

    pub fn has(&self, code: &str) -> bool {
        self.0.contains_key(code)
    }
}

/// True when the code names a supported currency (case-insensitive).
pub fn is_currency(code: &str) -> bool {
    let lower = code.trim().to_lowercase();
    CURRENCIES.contains(&lower.as_str())
}

/// Converts `value` between two currencies through the USD pivot.
pub fn convert(value: f64, from: &str, to: &str, rates: &Rates) -> Option<f64> {
    let from = from.trim().to_lowercase();
    let to = to.trim().to_lowercase();
    let from_rate = rates.rate(&from)?;
    let to_rate = rates.rate(&to)?;
    Some(value / from_rate * to_rate)
}

/// The default target for single-quantity input: USD goes to EUR,
/// everything else goes to USD.
pub fn default_target(from: &str) -> Option<&'static str> {
    match from.trim().to_lowercase().as_str() {
        "usd" => Some("eur"),
        code if CURRENCIES.contains(&code) => Some("usd"),
        _ => None,
    }
}

static RATES: RwLock<Option<Rates>> = RwLock::new(None);
static FETCHED_AT: RwLock<Option<Instant>> = RwLock::new(None);
static FETCH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// A snapshot of the cached rates, when rates have loaded.
pub fn active_rates() -> Option<Rates> {
    RATES.read().ok().and_then(|guard| guard.clone())
}

/// Spawns a background rates refresh when the cache is missing or
/// stale. Returns immediately; one fetch runs at a time.
pub fn ensure_warm() {
    let fresh = FETCHED_AT
        .read()
        .ok()
        .and_then(|guard| *guard)
        .is_some_and(|at| at.elapsed() < RATES_TTL);
    if fresh {
        return;
    }
    if FETCH_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    smol::spawn(async move {
        match smol::unblock(fetch_rates).await {
            Ok(rates) => {
                if let Ok(mut guard) = RATES.write() {
                    *guard = Some(rates);
                }
                if let Ok(mut guard) = FETCHED_AT.write() {
                    *guard = Some(Instant::now());
                }
            }
            Err(error) => {
                eprintln!("corvo: {error}");
            }
        }
        FETCH_IN_FLIGHT.store(false, Ordering::SeqCst);
    })
    .detach();
}

/// Fetches and parses USD-pivot rates. Public for callers that prefer
/// to own the task, like a startup warm-up.
pub fn fetch_rates() -> Result<Rates, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(FETCH_TIMEOUT)
        .build();
    let body = agent
        .get(RATES_URL)
        .call()
        .map_err(|error| format!("could not reach the rates service: {error}"))?
        .into_string()
        .map_err(|error| format!("could not read the rates: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("invalid rates data: {error}"))?;
    parse_rates(&value)
}

fn parse_rates(value: &serde_json::Value) -> Result<Rates, String> {
    let table = value
        .get("rates")
        .and_then(|rates| rates.as_object())
        .ok_or_else(|| "rates table missing".to_string())?;
    let mut rates = HashMap::new();
    for (code, amount) in table {
        let lower = code.to_lowercase();
        if CURRENCIES.contains(&lower.as_str()) {
            if let Some(amount) = amount.as_f64() {
                if amount > 0.0 {
                    rates.insert(lower, amount);
                }
            }
        }
    }
    if rates.is_empty() {
        return Err("rates table empty".to_string());
    }
    Ok(Rates(rates))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rates() -> Rates {
        let mut rates = HashMap::new();
        rates.insert("usd".to_string(), 1.0);
        rates.insert("eur".to_string(), 0.9);
        rates.insert("clp".to_string(), 900.0);
        Rates(rates)
    }

    #[test]
    fn converts_through_the_usd_pivot() {
        let rates = test_rates();
        let out = convert(10.0, "usd", "eur", &rates).unwrap();
        assert!((out - 9.0).abs() < 1e-9);
        let out = convert(900.0, "clp", "usd", &rates).unwrap();
        assert!((out - 1.0).abs() < 1e-9);
        assert_eq!(convert(10.0, "usd", "xyz", &rates), None);
    }

    #[test]
    fn recognizes_known_codes_only() {
        assert!(is_currency("USD"));
        assert!(is_currency("clp"));
        assert!(!is_currency("kg"));
        assert!(!is_currency("foo"));
    }

    #[test]
    fn default_target_picks_a_sensible_other_currency() {
        assert_eq!(default_target("usd"), Some("eur"));
        assert_eq!(default_target("eur"), Some("usd"));
        assert_eq!(default_target("clp"), Some("usd"));
        assert_eq!(default_target("kg"), None);
    }

    #[test]
    fn parses_a_rates_payload() {
        let payload = serde_json::json!({
            "result": "success",
            "rates": { "USD": 1.0, "EUR": 0.9, "CLP": 900.0, "GARBAGE": -1.0 }
        });
        let rates = parse_rates(&payload).unwrap();
        assert_eq!(rates.0.len(), 3);
        assert!(!rates.has("garbage"));
    }
}

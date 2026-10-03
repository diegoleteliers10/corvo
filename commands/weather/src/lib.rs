//! Weather conditions and a 3-day forecast, backed by wttr.in (JSON,
//! no API key) with a 15-minute cache.
//!
//! Routing: root search exposes a "Weather" entry row (plus a current
//! conditions row when the cache is warm); the dedicated visual page is
//! fed by [`fetch_cached`], which the UI calls off the search path.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use corvo_core::{
    phosphor_svgs, Action, Command, CommandError, ExecutionContext, Icon, SearchContext,
    SearchResult,
};

const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const FORECAST_DAYS: usize = 3;

/// What the weather page paints.
#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub city: String,
    pub description: String,
    pub glyph: &'static str,
    pub temperature_c: i32,
    pub feels_like_c: i32,
    pub humidity: i32,
    pub wind_kph: i32,
    pub days: Vec<ForecastDay>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForecastDay {
    pub label: String,
    pub glyph: &'static str,
    pub description: String,
    pub max_c: i32,
    pub min_c: i32,
}

fn cache() -> &'static Mutex<Option<(String, Instant, Weather)>> {
    static CACHE: OnceLock<Mutex<Option<(String, Instant, Weather)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The cached forecast for `city` when it is fresh enough.
pub fn cached(city: Option<&str>) -> Option<Weather> {
    let guard = cache().lock().ok()?;
    let (cached_city, fetched_at, weather) = guard.as_ref()?;
    let key = city_key(city);
    if *cached_city == key && fetched_at.elapsed() < CACHE_TTL {
        Some(weather.clone())
    } else {
        None
    }
}

fn city_key(city: Option<&str>) -> String {
    city.unwrap_or_default().trim().to_lowercase()
}

/// Fetches the forecast, serving a fresh cache hit without network.
pub fn fetch_cached(city: Option<&str>) -> Result<Weather, String> {
    if let Some(weather) = cached(city) {
        return Ok(weather);
    }
    let weather = fetch(city)?;
    if let Ok(mut guard) = cache().lock() {
        *guard = Some((city_key(city), Instant::now(), weather.clone()));
    }
    Ok(weather)
}

fn fetch(city: Option<&str>) -> Result<Weather, String> {
    let location = city.unwrap_or_default().trim();
    let url = if location.is_empty() {
        // No city: wttr.in geolocates by IP.
        "https://wttr.in/?format=j1".to_string()
    } else {
        format!(
            "https://wttr.in/{}?format=j1",
            urlencode(location.replace(' ', "+"))
        )
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(FETCH_TIMEOUT)
        .build();
    let body = agent
        .get(&url)
        .call()
        .map_err(|error| format!("Could not reach wttr.in: {error}"))?
        .into_string()
        .map_err(|error| format!("Could not read the forecast: {error}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| format!("Invalid forecast data: {error}"))?;
    parse_weather(&value)
}

fn urlencode(text: String) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn parse_weather(value: &serde_json::Value) -> Result<Weather, String> {
    let current = value
        .pointer("/current_condition/0")
        .ok_or("the forecast has no current conditions")?;
    let city = value
        .pointer("/nearest_area/0/areaName/0/value")
        .and_then(|area| area.as_str())
        .unwrap_or("Your location")
        .to_owned();

    let temperature_c = int_field(current, "temp_C")?;
    let description = current
        .pointer("/weatherDesc/0/value")
        .and_then(|desc| desc.as_str())
        .unwrap_or("Unknown")
        .to_owned();
    let code = int_field(current, "weatherCode")?;

    let mut days = Vec::new();
    for day in value
        .pointer("/weather")
        .and_then(|weather| weather.as_array())
        .into_iter()
        .flatten()
        .take(FORECAST_DAYS)
    {
        let date = day.get("date").and_then(|date| date.as_str()).unwrap_or("");
        let label = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map(|parsed| parsed.format("%a").to_string())
            .unwrap_or_else(|_| "—".to_string());
        let day_code = day
            .pointer("/hourly/4/weatherCode")
            .and_then(|code| {
                code.as_i64()
                    .or_else(|| code.as_str().and_then(|text| text.parse::<i64>().ok()))
            })
            .unwrap_or(code as i64);
        days.push(ForecastDay {
            label,
            glyph: glyph_for(day_code),
            description: day
                .pointer("/hourly/4/weatherDesc/0/value")
                .and_then(|desc| desc.as_str())
                .unwrap_or(&description)
                .to_owned(),
            max_c: int_field(day, "maxtempC")?,
            min_c: int_field(day, "mintempC")?,
        });
    }

    Ok(Weather {
        city,
        glyph: glyph_for(code as i64),
        description,
        temperature_c,
        feels_like_c: int_field(current, "FeelsLikeC")?,
        humidity: int_field(current, "humidity")?,
        wind_kph: int_field(current, "windspeedKmph")?,
        days,
    })
}

fn int_field(value: &serde_json::Value, field: &str) -> Result<i32, String> {
    value
        .get(field)
        .and_then(|field| field.as_str())
        .and_then(|text| text.parse::<i32>().ok())
        .or_else(|| value.get(field).and_then(|field| field.as_i64()).map(|v| v as i32))
        .ok_or_else(|| format!("the forecast is missing {field}"))
}

/// Maps a WWO weather code to a display glyph.
fn glyph_for(code: i64) -> &'static str {
    match code {
        113 => "☀️",
        116 => "🌤️",
        119 | 122 => "☁️",
        143 | 248 | 260 => "🌫️",
        176 | 263 | 353 => "🌦️",
        200 | 386 | 389 | 392 | 395 => "⛈️",
        179 | 182 | 185 | 323 | 326 => "🌨️",
        227 | 230 | 320 | 329 | 332 | 335 | 338 | 368 | 371 => "❄️",
        _ => "🌧️",
    }
}

fn open_result(input: &str, score: i32) -> SearchResult {
    let subtitle = if input.is_empty() {
        "Conditions and 3-day forecast".to_owned()
    } else {
        format!("Forecast for {input}")
    };
    SearchResult {
        id: if input.is_empty() {
            "weather:open".into()
        } else {
            format!("weather:open:{input}")
        },
        title: "Weather".into(),
        subtitle: Some(subtitle),
        icon: Icon::Svg(phosphor_svgs::style::regular::CLOUD_SUN),
        score,
        accessory: None,
    }
}

fn conditions_result(weather: &Weather, score: i32) -> SearchResult {
    SearchResult {
        id: "weather:current".into(),
        title: format!("{} {}°C", weather.glyph, weather.temperature_c),
        subtitle: Some(format!(
            "{} · feels {}°C · {}% humidity · {} km/h wind · {}",
            weather.description,
            weather.feels_like_c,
            weather.humidity,
            weather.wind_kph,
            weather.city
        )),
        icon: Icon::Svg(phosphor_svgs::style::regular::CLOUD_SUN),
        score,
        accessory: Some("Now".into()),
    }
}

#[derive(Default)]
pub struct WeatherCommand;

corvo_core::register_command!(WeatherCommand);

#[async_trait::async_trait]
impl Command for WeatherCommand {
    fn id(&self) -> &'static str {
        "weather"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["weather", "forecast", "clima", "tiempo"]
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn search(&self, query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return vec![open_result("", 1000)];
        }

        let first = trimmed.split_whitespace().next().unwrap_or_default();
        let lowered = first.to_lowercase();
        if Self::keywords_contains(&lowered) {
            let rest = trimmed[first.len()..].trim();
            // The cached conditions row is instant; the page does the
            // network work off the search path.
            if let Some(weather) = cached(Some(rest).filter(|city| !city.is_empty())) {
                return vec![
                    conditions_result(&weather, 1010),
                    open_result(rest, 1000),
                ];
            }
            return vec![open_result(rest, 1000)];
        }

        corvo_core::search_match_score(
            trimmed,
            &["Weather", "weather forecast temperature clima"],
        )
        .map(|score| vec![open_result("", score + 120)])
        .unwrap_or_default()
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some(key) = result_id.strip_prefix("weather:") else {
            return Err(CommandError::NotFound);
        };
        if key == "open" || key.starts_with("open:") {
            return Ok(Action::ShowToast("Weather".into()));
        }
        if key == "current" {
            let summary = cached(None)
                .map(|weather| {
                    format!("{}°C — {}", weather.temperature_c, weather.description)
                })
                .unwrap_or_else(|| "No cached forecast yet".into());
            return Ok(Action::ShowToast(summary));
        }
        Err(CommandError::NotFound)
    }
}

impl WeatherCommand {
    fn keywords_contains(word: &str) -> bool {
        ["weather", "forecast", "clima", "tiempo"].contains(&word)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "current_condition": [{
            "temp_C": "18",
            "FeelsLikeC": "17",
            "humidity": "63",
            "windspeedKmph": "12",
            "weatherCode": "116",
            "weatherDesc": [{"value": "Partly cloudy"}]
        }],
        "weather": [
            {"date": "2026-10-02", "maxtempC": "21", "mintempC": "11",
             "hourly": [{"weatherCode": "116", "weatherDesc": [{"value": "Partly cloudy"}]},
                        {"weatherCode": "116"}, {"weatherCode": "119"}, {"weatherCode": "119"},
                        {"weatherCode": "116", "weatherDesc": [{"value": "Clear"}]}]},
            {"date": "2026-10-03", "maxtempC": "23", "mintempC": "12",
             "hourly": [{"weatherCode": "113"}, {"weatherCode": "113"}, {"weatherCode": "113"}, {"weatherCode": "113"},
                        {"weatherCode": "113", "weatherDesc": [{"value": "Sunny"}]}]},
            {"date": "2026-10-04", "maxtempC": "19", "mintempC": "10",
             "hourly": [{"weatherCode": "296"}, {"weatherCode": "296"}, {"weatherCode": "296"}, {"weatherCode": "296"},
                        {"weatherCode": "296", "weatherDesc": [{"value": "Light rain"}]}]},
            {"date": "2026-10-05", "maxtempC": "20", "mintempC": "11",
             "hourly": [{"weatherCode": "113"}, {"weatherCode": "113"}, {"weatherCode": "113"}, {"weatherCode": "113"},
                        {"weatherCode": "113"}]}
        ],
        "nearest_area": [{"areaName": [{"value": "Santiago"}]}]
    }"#;

    #[test]
    fn parses_the_forecast_sample() {
        let value: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        let weather = parse_weather(&value).unwrap();
        assert_eq!(weather.city, "Santiago");
        assert_eq!(weather.temperature_c, 18);
        assert_eq!(weather.humidity, 63);
        assert_eq!(weather.glyph, "🌤️");
        assert_eq!(weather.days.len(), 3, "only three forecast days");
        assert_eq!(weather.days[0].label, "Fri");
        assert_eq!(weather.days[1].glyph, "☀️");
        assert_eq!(weather.days[2].max_c, 19);
    }

    #[test]
    fn cache_is_keyed_by_city_and_expires() {
        // Prime the cache through the public API surface only.
        let weather = parse_weather(&serde_json::from_str::<serde_json::Value>(SAMPLE).unwrap())
            .unwrap();
        if let Ok(mut guard) = cache().lock() {
            *guard = Some(("santiago".into(), Instant::now(), weather));
        }
        assert!(cached(Some("Santiago")).is_some());
        assert!(cached(None).is_none(), "a different city must not hit");

        if let Ok(mut guard) = cache().lock() {
            let (_, fetched_at, weather) = guard.take().unwrap();
            *guard = Some((
                "santiago".into(),
                fetched_at - CACHE_TTL - Duration::from_secs(1),
                weather,
            ));
        }
        assert!(cached(Some("santiago")).is_none(), "stale cache misses");
    }

    #[test]
    fn root_routing_responds_to_keywords() {
        let search = |query: &str| {
            smol::block_on(<WeatherCommand as Command>::search(
                &WeatherCommand,
                query,
                &SearchContext::default(),
            ))
        };
        // Match: keyword + city rides along to the page.
        let results = search("weather santiago");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "weather:open:santiago");

        // Match: empty query surfaces the entry.
        assert_eq!(search("")[0].id, "weather:open");

        // Non-match.
        assert!(search("hello world").is_empty());
    }
}

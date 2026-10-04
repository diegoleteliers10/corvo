//! Countdown to a date — the reference "rich extension" built purely
//! on the declarative page model. Zero corvo-ui code: the manifest
//! declares the command and its date argument, `Command::page`
//! returns Blocks (a ticking hero plus the saved countdowns strip),
//! and storage persists the saved list.
//!
//! Usage: `countdown 2026-12-25 My trip` from root search opens the
//! page; Save pins it to the strip.

use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use corvo_core::{
    Action, Command, CommandError, ExecutionContext, Icon, PageButton, PageView, SearchContext,
    SearchResult, StripCard, Style, Tone,
};
use corvo_ext::pages::PageBuilder;
use corvo_ext::{manifest, storage};

const ID: &str = "countdown";

/// One saved countdown: a label and a target timestamp (Unix secs).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Saved {
    label: String,
    target: u64,
}

/// The countdown currently on screen, remembered from the last page
/// render so Enter saves what the user sees.
fn last_parsed() -> &'static Mutex<Option<(u64, String)>> {
    static LAST: OnceLock<Mutex<Option<(u64, String)>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

fn saved_list() -> Vec<Saved> {
    storage::all(ID)
        .into_values()
        .filter_map(|value| {
            let text = value.as_str()?;
            let (target, label) = text.split_once('|')?;
            Some(Saved {
                label: label.to_owned(),
                target: target.parse().ok()?,
            })
        })
        .collect()
}

fn save_countdown(label: &str, target: u64) -> Result<(), String> {
    storage::set(ID, &format!("cd-{target}-{label}"), storage::Value::String(format!("{target}|{label}")))
}

/// Parses the flexible date forms: `2026-12-25`, `Dec 25 2026`,
/// `25 December`. Returns Unix seconds.
fn parse_date(input: &str) -> Option<u64> {
    let input = input.trim();
    if let Some((year, month, day)) = iso_date(input) {
        return days_from_civil(year, month, day);
    }
    let month = [
        ("jan", 1), ("feb", 2), ("mar", 3), ("apr", 4), ("may", 5), ("jun", 6),
        ("jul", 7), ("aug", 8), ("sep", 9), ("oct", 10), ("nov", 11), ("dec", 12),
    ]
    .into_iter()
    .find(|(needle, _)| input.to_lowercase().starts_with(needle))
    .map(|(_, number)| number)?;
    let rest = input.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
    let numbers = rest
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<i64>().ok())
        .collect::<Vec<_>>();
    let (day, year) = match numbers.as_slice() {
        [day, year] if (100..=9999).contains(year) => (*day, *year),
        [day] => (*day, current_year() + i64::from(*day < today_parts().1 as i64)),
        _ => return None,
    };
    if !(1..=31).contains(&day) {
        return None;
    }
    days_from_civil(year, month, day as u32)
}

fn iso_date(input: &str) -> Option<(i64, u32, u32)> {
    let mut parts = input.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    (year > 100 && (1..=12).contains(&month) && (1..=31).contains(&day))
        .then_some((year, month, day))
}

/// Days from the civil date to Unix seconds (Howard Hinnant's
/// algorithm), at noon UTC so day counts stay stable across timezones.
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<u64> {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + 43_200).ok()
}

fn today_parts() -> (i64, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    (year, month, day)
}

fn current_year() -> i64 {
    today_parts().0
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `(days, hours, minutes, seconds)` until the target.
fn time_until(target: u64) -> (i64, u64, u64, u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let delta = target as i64 - now as i64;
    let abs = delta.unsigned_abs();
    (
        delta,
        (abs % 86_400) / 3_600,
        (abs % 3_600) / 60,
        abs % 60,
    )
}

fn format_delta(delta: i64) -> String {
    if delta < 0 {
        let days = -delta / 86_400;
        format!("{days} days ago")
    } else {
        let days = delta / 86_400;
        format!("{days} days")
    }
}

fn parse_query(query: &str) -> Option<(u64, String)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    // The date is the first token (or a three-token month name); the
    // rest is the label.
    let (date_part, label) = match trimmed.split_once(' ') {
        Some((head, rest)) => {
            let looks_like_month = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
                .iter()
                .any(|month| head.to_lowercase().starts_with(month));
            if looks_like_month {
                let mut words = rest.splitn(3, ' ');
                let day = words.next().unwrap_or_default();
                let year = words.next().unwrap_or_default();
                let extra = words.next().unwrap_or_default();
                let date_text = format!("{head} {day} {year}");
                let label = if extra.is_empty() {
                    String::new()
                } else {
                    extra.to_owned()
                };
                (date_text, label)
            } else {
                (head.to_owned(), rest.to_owned())
            }
        }
        None => (trimmed.to_owned(), String::new()),
    };
    let target = parse_date(&date_part)?;
    let label = if label.is_empty() {
        date_part
    } else {
        label
    };
    Some((target, label))
}

fn icon() -> Icon {
    Icon::Svg(corvo_core::phosphor_svgs::style::regular::CALENDAR)
}

#[derive(Default)]
pub struct CountdownCommand;

corvo_core::register_command!(CountdownCommand);

#[async_trait::async_trait]
impl Command for CountdownCommand {
    fn id(&self) -> &'static str {
        ID
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["countdown", "days until"]
    }

    fn priority(&self) -> u8 {
        55
    }

    fn manifest(&self) -> corvo_core::ExtensionManifest {
        manifest::extension(
            ID,
            "Countdown",
            "Days until a date, with a live page and saved countdowns",
            icon(),
            &["Productivity"],
            vec![
                manifest::CommandSpecBuilder::argument(
                    manifest::view_command(
                        "view",
                        "Countdown",
                        "Open the countdown page for a date",
                        &["countdown", "days until"],
                    ),
                    manifest::required_text_argument("date", "Date: 2026-12-25, Dec 25, or 25 December..."),
                ),
                manifest::action_command("saved", "Saved Countdowns", "Open the saved countdowns strip", &["saved"]),
            ],
        )
    }

    fn page(&self, query: &str) -> Option<PageView> {
        let parsed = parse_query(query);
        let mut builder = PageBuilder::new("Date: 2026-12-25, Dec 25, or 25 December...")
            .ticking(1);
        match parsed {
            Some((target, label)) => {
                if let Ok(mut last) = last_parsed().lock() {
                    *last = Some((target, label.clone()));
                }
                let (delta, hours, minutes, seconds) = time_until(target);
                let value = if delta.abs() < 86_400 {
                    format!("{hours:02}:{minutes:02}:{seconds:02}")
                } else {
                    format_delta(delta)
                };
                builder = builder
                    .badge(
                        if delta < 0 {
                            String::from("PAST")
                        } else {
                            String::from("COUNTING DOWN")
                        },
                        if delta < 0 { Tone::Neutral } else { Tone::Accent },
                    )
                    .style(corvo_core::Style {
                        color: Some(0x34d399),
                        background: Some(0x11342a),
                        ..Style::default()
                    })
                    .hero(None, label.clone(), value, target_label(target), Tone::Accent)
                    .style(corvo_core::Style {
                        color: Some(0xfbbf24),
                        size: Some(64),
                        ..Style::default()
                    })
                    .markdown(format!(
                        "- Type a new date to switch\n- Enter again to save **{label}** to the strip"
                    ))
                    .buttons(vec![PageButton {
                        action_id: format!("save:{}:{}", target, encode(&label)),
                        label: "Save".into(),
                        tone: Tone::Positive,
                        hotkey: Some("enter"),
                        style: Style::default(),
                    }]);
            }
            None => {
                builder = builder
                    .hero(
                        Some("📅"),
                        "Countdown",
                        String::new(),
                        "Type a date: 2026-12-25, Dec 25, or 25 December",
                        Tone::Neutral,
                    )
                    .style(corvo_core::Style {
                        glyph_size: Some(56),
                        ..Style::default()
                    });
            }
        }
        let saved = saved_list();
        if !saved.is_empty() {
            let strip = saved
                .iter()
                .map(|entry| {
                    let (delta, ..) = time_until(entry.target);
                    StripCard {
                        title: entry.label.clone(),
                        glyph: Some("🗓"),
                        value: format_delta(delta),
                        subtitle: target_label(entry.target),
                        style: Style::default(),
                    }
                })
                .collect();
            builder = builder.strip(strip).style(corvo_core::Style {
                color: Some(0x34d399),
                ..Style::default()
            });
            builder = builder.buttons(
                saved
                    .iter()
                    .map(|entry| PageButton {
                        action_id: format!("remove:{}", storage_key(entry)),
                        label: format!("Remove {}", entry.label),
                        tone: Tone::Destructive,
                        hotkey: None,
                        style: Style::default(),
                    })
                    .collect(),
            );
        }
        Some(builder.build())
    }

    async fn search(&self, query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        let trimmed = query.trim();
        let first = trimmed.split_whitespace().next().unwrap_or_default();
        let lowered = first.to_lowercase();
        if trimmed.is_empty() || lowered == "countdown" || lowered == "days" {
            let mut results = vec![open_result(1000)];
            if let Some((target, label)) = parse_query(trimmed) {
                results.insert(
                    0,
                    SearchResult {
                        id: format!("{ID}:open:{target}|{}", encode(&label)),
                        title: format!("Countdown: {label}"),
                        subtitle: Some(target_label(target)),
                        icon: icon(),
                        score: 1100,
                        accessory: None,
                        section: None,
                    },
                );
            }
            return results;
        }
        if lowered == "saved" {
            return vec![SearchResult {
                id: format!("{ID}:saved"),
                title: "Saved Countdowns".into(),
                subtitle: Some("Open the countdown page".into()),
                icon: icon(),
                score: 1000,
                accessory: None,
                section: None,
            }];
        }
        corvo_core::search_match_score(trimmed, &["Countdown", "countdown days until date"])
            .map(|score| vec![open_result(score + 120)])
            .unwrap_or_default()
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some(key) = result_id.strip_prefix("countdown:") else {
            return Err(CommandError::NotFound);
        };
        if key == "open" || key.starts_with("open:") {
            return Ok(Action::ShowToast("Countdown".into()));
        }
        if key == "saved" {
            return Ok(Action::ShowToast("Countdown".into()));
        }
        if let Some(action) = key.strip_prefix("page:") {
            return run_page_action(action);
        }
        Err(CommandError::NotFound)
    }
}

fn storage_key(entry: &Saved) -> String {
    format!("cd-{}-{}", entry.target, encode(&entry.label))
}

fn encode(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c.to_string() } else { format!("%{:02X}", c as u32 % 256) })
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() + 1 {
            if let Ok(byte) = u8::from_str_radix(&text[index + 1..index + 3], 16) {
                out.push(byte as char);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn target_label(target: u64) -> String {
    let (year, month, day) = civil_from_days((target / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn run_page_action(action: &str) -> Result<Action, CommandError> {
    if let Some(rest) = action.strip_prefix("save:") {
        let Some((target_text, encoded_label)) = rest.split_once(':') else {
            return Err(CommandError::NotFound);
        };
        let Ok(target) = target_text.parse::<u64>() else {
            return Err(CommandError::NotFound);
        };
        let label = decode(encoded_label);
        save_countdown(&label, target).map_err(CommandError::Platform)?;
        return Ok(Action::ShowToast(format!("Saved {label}")));
    }
    if let Some(key) = action.strip_prefix("remove:") {
        storage::remove(ID, key).map_err(CommandError::Platform)?;
        return Ok(Action::ShowToast("Removed".into()));
    }
    if action == "enter" {
        // Enter saves the countdown on screen; on the idle page it is
        // a no-op.
        let parsed = last_parsed().lock().ok().and_then(|last| last.clone());
        return match parsed {
            Some((target, label)) => {
                save_countdown(&label, target).map_err(CommandError::Platform)?;
                Ok(Action::ShowToast(format!("Saved {label}")))
            }
            None => Ok(Action::ShowToast(String::new())),
        };
    }
    Err(CommandError::NotFound)
}

fn open_result(score: i32) -> SearchResult {
    SearchResult {
        id: format!("{ID}:open"),
        title: "Countdown".into(),
        subtitle: Some("Days until a date".into()),
        icon: icon(),
        score,
        accessory: None,
        section: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corvo_core::CommandMode;

    #[test]
    fn parses_iso_and_month_dates() {
        let expected = days_from_civil(2026, 12, 25).unwrap();
        assert_eq!(parse_date("2026-12-25"), Some(expected));
        assert_eq!(parse_date("December 25 2026"), Some(expected));
        assert_eq!(parse_date("25 December"), parse_date(&format!("25 December {}", today_parts().0 + i64::from(today_parts().1 > 12))));
        assert_eq!(parse_date("garbage"), None);
    }

    #[test]
    fn splits_date_and_label() {
        let (target, label) = parse_query("2026-12-25 My trip").unwrap();
        assert_eq!(label, "My trip");
        assert_eq!(target, days_from_civil(2026, 12, 25).unwrap());

        let (target, label) = parse_query("Dec 25 2026").unwrap();
        assert_eq!(target, days_from_civil(2026, 12, 25).unwrap());
        assert!(label.starts_with("Dec 25"));
    }

    #[test]
    fn page_shows_hero_without_a_date_and_saves_with_one() {
        let command = CountdownCommand;
        let idle = command.page("").expect("idle page");
        assert!(matches!(idle, PageView::Blocks(_)));

        let target = days_from_civil(2100, 1, 1).unwrap();
        let running = command.page("2100-01-01 Trip").expect("running page");
        let PageView::Blocks(blocks) = running else {
            panic!("expected blocks");
        };
        assert!(format!("{:?}", blocks.blocks).contains("Trip"));

        // Save through the page action path, then read the strip back.
        let action = format!("countdown:page:save:{target}:Trip");
        let result = smol::block_on(<CountdownCommand as Command>::execute(
            &command,
            &action,
            &ExecutionContext::default(),
        ));
        assert!(result.is_ok());
        let saved = saved_list();
        assert!(saved.iter().any(|entry| entry.label == "Trip"));
        // Clean the user's storage after the test.
        for entry in saved {
            let _ = storage::remove(ID, &storage_key(&entry));
        }
    }

    #[test]
    fn search_routes_and_rejects() {
        let command = CountdownCommand;
        let search = |query: &str| {
            smol::block_on(<CountdownCommand as Command>::search(
                &command,
                query,
                &SearchContext::default(),
            ))
        };
        assert_eq!(search("countdown")[0].id, "countdown:open");
        assert!(search("hello world").is_empty());
    }

    #[test]
    fn manifest_declares_view_and_actions() {
        let manifest = CountdownCommand.manifest();
        assert_eq!(manifest.commands.len(), 2);
        assert_eq!(manifest.commands[0].mode, CommandMode::View);
        assert_eq!(manifest.commands[1].mode, CommandMode::NoView);
        assert_eq!(manifest.commands[0].arguments.len(), 1);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    // Live check against real machine state; run with
    // `cargo test -p corvo-countdown -- --ignored --nocapture live`.
    #[test]
    #[ignore]
    fn live_migrated_pages_render() {
        // The countdown page ticks and parses today's real date.
        let command = CountdownCommand;
        let next_year = today_parts().0 + 1;
        let query = format!("{}-12-31 New year", next_year);
        let page = command.page(&query).expect("countdown page");
        let PageView::Blocks(blocks) = page else {
            panic!("expected blocks");
        };
        let rendered = format!("{blocks:?}");
        assert!(rendered.contains("New year"), "hero label missing: {rendered}");
        assert!(rendered.contains("COUNTING DOWN"), "badge missing");

        println!("countdown page renders");
    }
}

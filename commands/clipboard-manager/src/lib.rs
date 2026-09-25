//! Clipboard history manager and watcher.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

use corvo_core::{
    phosphor_svgs, Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext,
    search_match_score, Icon, SearchContext, SearchResult,
};

const MAX_ENTRY_CHARS: usize = 100_000;
#[cfg(test)]
const RETENTION_SECS: u64 = 30 * 86_400;

#[derive(Clone, Copy)]
pub struct ClipboardPreferences {
    pub enabled: bool,
    pub retention_days: u32,
    pub max_entries: usize,
    pub save_images: bool,
    pub save_colors: bool,
}

impl Default for ClipboardPreferences {
    fn default() -> Self {
        Self { enabled: true, retention_days: 30, max_entries: 500, save_images: true, save_colors: true }
    }
}

fn preferences_store() -> &'static RwLock<ClipboardPreferences> {
    static PREFERENCES: OnceLock<RwLock<ClipboardPreferences>> = OnceLock::new();
    PREFERENCES.get_or_init(|| RwLock::new(ClipboardPreferences::default()))
}

fn preferences() -> ClipboardPreferences {
    preferences_store().read().map(|current| *current).unwrap_or_default()
}

fn current_clipboard_image() -> Option<Vec<u8>> {
    preferences().save_images.then(corvo_platform::read_clipboard_image).flatten()
}

pub fn set_preferences(value: ClipboardPreferences) {
    let value = ClipboardPreferences { max_entries: value.max_entries.max(1), ..value };
    let was_enabled = preferences_store().read().map(|current| current.enabled).unwrap_or(true);
    if !was_enabled && value.enabled {
        let count = corvo_platform::clipboard_change_count();
        if let Some(store) = STORE.get() {
            if let Ok(mut history) = store.write() {
                history.last_change_count = count;
            }
        }
    }
    let should_prune = if let Ok(mut current) = preferences_store().write() {
        let changed = current.retention_days != value.retention_days || current.max_entries != value.max_entries;
        *current = value;
        changed
    } else {
        false
    };
    if should_prune {
        prune_history_to_preferences();
    }
}

fn history_file_path() -> PathBuf {
    let dirs = directories::ProjectDirs::from("", "", "corvo")
        .expect("corvo: no home directory");
    dirs.data_dir().join("clipboard_history.json")
}

pub fn images_dir_path() -> PathBuf {
    let dirs = directories::ProjectDirs::from("", "", "corvo")
        .expect("corvo: no home directory");
    dirs.data_dir().join("clipboard_images")
}

fn simple_hash(bytes: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

fn load_persisted_entries() -> Vec<ClipboardEntry> {
    let path = history_file_path();
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<ClipboardEntry>>(&data) else {
        eprintln!("corvo: clipboard_history.json malformed, starting fresh");
        return Vec::new();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff = now.saturating_sub(u64::from(preferences().retention_days) * 86_400);

    let (keep, expired): (Vec<_>, Vec<_>) = entries.into_iter().partition(|e| e.timestamp_secs >= cutoff);
    for e in expired {
        if let Some(path) = e.image_path() {
            let _ = fs::remove_file(path);
        }
    }
    let mut entries = keep;
    let max_entries = preferences().max_entries;
    if entries.len() > max_entries {
        for e in entries.drain(max_entries..) {
            if let Some(path) = e.image_path() {
                let _ = fs::remove_file(path);
            }
        }
    }
    entries
}

fn save_entries_atomic(entries: &[ClipboardEntry]) {
    let path = history_file_path();
    let Some(parent) = path.parent() else { return };
    let _ = fs::create_dir_all(parent);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff = now.saturating_sub(u64::from(preferences().retention_days) * 86_400);

    let (keep, expired): (Vec<_>, Vec<_>) = entries.iter().cloned().partition(|e| e.timestamp_secs >= cutoff);
    for e in expired {
        if let Some(path) = e.image_path() {
            let _ = fs::remove_file(path);
        }
    }
    let mut filtered = keep;
    let max_entries = preferences().max_entries;
    if filtered.len() > max_entries {
        for e in filtered.drain(max_entries..) {
            if let Some(path) = e.image_path() {
                let _ = fs::remove_file(path);
            }
        }
    }

    let Ok(json) = serde_json::to_string_pretty(&filtered) else { return };
    let tmp_path = parent.join(format!("clipboard_history.json.tmp.{}", std::process::id()));

    if fs::write(&tmp_path, json).is_ok() {
        let _ = fs::rename(&tmp_path, &path);
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub text: String,
    pub source_app: String,
    pub char_count: usize,
    pub word_count: usize,
    pub copied_at_str: String,
    pub section: String,
    pub timestamp_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_hash: Option<u64>,
}

impl ClipboardEntry {
    pub fn is_image(&self) -> bool {
        self.image_file.is_some()
    }

    pub fn image_path(&self) -> Option<PathBuf> {
        self.image_file.as_ref().map(|filename| images_dir_path().join(filename))
    }
}

struct ClipboardHistory {
    last_change_count: isize,
    entries: Vec<ClipboardEntry>,
}

static STORE: OnceLock<Arc<RwLock<ClipboardHistory>>> = OnceLock::new();

fn store() -> &'static Arc<RwLock<ClipboardHistory>> {
    STORE.get_or_init(|| {
        let mut history = ClipboardHistory {
            last_change_count: -1,
            entries: load_persisted_entries(),
        };
        let count = corvo_platform::clipboard_change_count();
        history.last_change_count = count;
        if preferences().enabled && !corvo_platform::clipboard_is_concealed() {
            let source = corvo_platform::frontmost_app_info()
                .map(|(_, name)| name)
                .filter(|name| name != "Corvo" && name != "corvo")
                .unwrap_or_else(|| "Fastty".into());

            if let Some(png_bytes) = current_clipboard_image() {
                let hash = simple_hash(&png_bytes);
                let already_latest = history.entries.first().map_or(false, |first| {
                    first.image_hash == Some(hash)
                });
                if !already_latest {
                    if let Some(entry) = create_image_entry(png_bytes, source) {
                        history.entries.insert(0, entry);
                        if history.entries.len() > preferences().max_entries {
                            if let Some(removed) = history.entries.pop() {
                                if let Some(path) = removed.image_path() {
                                    let _ = fs::remove_file(path);
                                }
                            }
                        }
                        save_entries_atomic(&history.entries);
                    }
                }
            } else if let Some(text) = corvo_platform::read_clipboard_text() {
                let trimmed = text.trim();
                if !trimmed.is_empty() && should_record_text(&text) {
                    let text = if text.len() > MAX_ENTRY_CHARS {
                        text.chars().take(MAX_ENTRY_CHARS).collect()
                    } else {
                        text
                    };
                    let already_latest = history
                        .entries
                        .first()
                        .map_or(false, |first| first.text == text && !first.is_image());
                    if !already_latest {
                        let entry = create_entry(text, source);
                        history.entries.insert(0, entry);
                        if history.entries.len() > preferences().max_entries {
                            history.entries.truncate(preferences().max_entries);
                        }
                        save_entries_atomic(&history.entries);
                    }
                }
            }
        }
        Arc::new(RwLock::new(history))
    })
}

fn format_time_and_section(now_secs: u64, entry_secs: u64) -> (String, String) {
    unsafe {
        let entry_time = entry_secs as libc::time_t;
        let mut entry_tm = std::mem::zeroed();
        libc::localtime_r(&entry_time, &mut entry_tm);

        let now_time = now_secs as libc::time_t;
        let mut now_tm = std::mem::zeroed();
        libc::localtime_r(&now_time, &mut now_tm);

        let hour_24 = entry_tm.tm_hour;
        let is_pm = hour_24 >= 12;
        let hour_12 = match hour_24 % 12 {
            0 => 12,
            h => h,
        };
        let am_pm = if is_pm { "PM" } else { "AM" };
        let time_str = format!(
            "{}:{:02}:{:02} {}",
            hour_12, entry_tm.tm_min, entry_tm.tm_sec, am_pm
        );

        let is_same_year = entry_tm.tm_year == now_tm.tm_year;
        let day_diff = if is_same_year {
            now_tm.tm_yday - entry_tm.tm_yday
        } else if now_tm.tm_year == entry_tm.tm_year + 1 && now_tm.tm_yday == 0 {
            let year = entry_tm.tm_year + 1900;
            let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            let last_yday = if is_leap { 365 } else { 364 };
            if entry_tm.tm_yday == last_yday {
                1
            } else {
                999
            }
        } else {
            999
        };

        let (section, copied_at_str) = match day_diff {
            0 => ("Today".to_string(), format!("Today at {time_str}")),
            1 => ("Yesterday".to_string(), format!("Yesterday at {time_str}")),
            _ => ("Older".to_string(), format!("Previous at {time_str}")),
        };

        (copied_at_str, section)
    }
}

fn create_entry(text: String, source_app: String) -> ClipboardEntry {
    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let timestamp_secs = now.as_secs();
    let id = format!("{}", now.as_millis());
    let (copied_at_str, section) = format_time_and_section(timestamp_secs, timestamp_secs);

    ClipboardEntry {
        id,
        text,
        source_app,
        char_count,
        word_count,
        copied_at_str,
        section,
        timestamp_secs,
        image_file: None,
        image_width: None,
        image_height: None,
        image_bytes: None,
        image_hash: None,
    }
}

fn create_image_entry(png_bytes: Vec<u8>, source_app: String) -> Option<ClipboardEntry> {
    let (w, h) = corvo_platform::parse_png_dimensions(&png_bytes).unwrap_or((0, 0));
    let byte_count = png_bytes.len();
    let hash = simple_hash(&png_bytes);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let timestamp_secs = now.as_secs();
    let id = format!("{}", now.as_millis());
    let filename = format!("{}.png", id);

    let img_dir = images_dir_path();
    let _ = fs::create_dir_all(&img_dir);
    let final_path = img_dir.join(&filename);
    let tmp_path = img_dir.join(format!("{}.tmp.{}", filename, std::process::id()));

    if fs::write(&tmp_path, &png_bytes).is_err() {
        return None;
    }
    if fs::rename(&tmp_path, &final_path).is_err() {
        let _ = fs::remove_file(&tmp_path);
        return None;
    }

    let (copied_at_str, section) = format_time_and_section(timestamp_secs, timestamp_secs);
    let text = if w > 0 && h > 0 {
        format!("Image {} × {} px", w, h)
    } else {
        "Image".to_string()
    };

    Some(ClipboardEntry {
        id,
        text,
        source_app,
        char_count: 0,
        word_count: 0,
        copied_at_str,
        section,
        timestamp_secs,
        image_file: Some(filename),
        image_width: Some(w),
        image_height: Some(h),
        image_bytes: Some(byte_count),
        image_hash: Some(hash),
    })
}

pub fn poll_clipboard() {
    poll_clipboard_with_source(None);
}

pub fn poll_clipboard_with_source(preferred_source: Option<&str>) {
    if !preferences().enabled {
        return;
    }
    let count = corvo_platform::clipboard_change_count();
    let store_arc = store();
    let Ok(mut history) = store_arc.write() else { return };
    if count == history.last_change_count {
        return;
    }
    history.last_change_count = count;
    if corvo_platform::clipboard_is_concealed() {
        return;
    }

    let source = preferred_source
        .map(|s| s.to_string())
        .or_else(|| {
            corvo_platform::frontmost_app_info()
                .map(|(_, name)| name)
                .filter(|name| name != "Corvo" && name != "corvo")
        })
        .unwrap_or_else(|| "Fastty".into());

    // 1. Check for image content first
    if let Some(png_bytes) = current_clipboard_image() {
        let hash = simple_hash(&png_bytes);
        let already_latest = history.entries.first().map_or(false, |first| {
            first.image_hash == Some(hash)
        });
        if !already_latest {
            if let Some(entry) = create_image_entry(png_bytes, source) {
                history.entries.insert(0, entry);
                if history.entries.len() > preferences().max_entries {
                    if let Some(removed) = history.entries.pop() {
                        if let Some(path) = removed.image_path() {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
                save_entries_atomic(&history.entries);
            }
        }
        return;
    }

    // 2. Fall back to text content
    if let Some(text) = corvo_platform::read_clipboard_text() {
        if text.trim().is_empty() || !should_record_text(&text) {
            return;
        }
        let text = if text.len() > MAX_ENTRY_CHARS {
            text.chars().take(MAX_ENTRY_CHARS).collect()
        } else {
            text
        };
        if let Some(first) = history.entries.first() {
            if first.text == text && !first.is_image() {
                return;
            }
        }
        let entry = create_entry(text, source);
        history.entries.insert(0, entry);
        if history.entries.len() > preferences().max_entries {
            if let Some(removed) = history.entries.pop() {
                if let Some(path) = removed.image_path() {
                    let _ = fs::remove_file(path);
                }
            }
        }
        save_entries_atomic(&history.entries);
    }
}

fn refresh_entry_time(entry: &mut ClipboardEntry) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (copied, section) = format_time_and_section(now, entry.timestamp_secs);
    entry.copied_at_str = copied;
    entry.section = section;
}

pub fn get_entry(id: &str) -> Option<ClipboardEntry> {
    let store_arc = store();
    let history = store_arc.read().ok()?;
    let mut entry = history.entries.iter().find(|e| e.id == id).cloned()?;
    refresh_entry_time(&mut entry);
    Some(entry)
}

pub fn all_entries() -> Vec<ClipboardEntry> {
    poll_clipboard();
    let store_arc = store();
    let Ok(history) = store_arc.read() else { return Vec::new() };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let prefs = preferences();
    let cutoff = now.saturating_sub(u64::from(prefs.retention_days) * 86_400);
    let mut entries: Vec<_> = history
        .entries
        .iter()
        .filter(|entry| entry.timestamp_secs >= cutoff)
        .take(prefs.max_entries)
        .cloned()
        .collect();
    for entry in &mut entries {
        refresh_entry_time(entry);
    }
    entries
}

fn prune_history_to_preferences() {
    let Some(store) = STORE.get() else { return };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let prefs = preferences();
    let cutoff = now.saturating_sub(u64::from(prefs.retention_days) * 86_400);
    if let Ok(mut history) = store.write() {
        let (keep, expired): (Vec<_>, Vec<_>) = history
            .entries
            .drain(..)
            .partition(|entry| entry.timestamp_secs >= cutoff);
        for entry in expired {
            if let Some(path) = entry.image_path() {
                let _ = fs::remove_file(path);
            }
        }
        history.entries = keep;
        if history.entries.len() > prefs.max_entries {
            for entry in history.entries.drain(prefs.max_entries..) {
                if let Some(path) = entry.image_path() {
                    let _ = fs::remove_file(path);
                }
            }
        }
        save_entries_atomic(&history.entries);
    }
}

pub fn delete_entry(id: &str) {
    let store_arc = store();
    if let Ok(mut history) = store_arc.write() {
        if let Some(pos) = history.entries.iter().position(|e| e.id == id) {
            let removed = history.entries.remove(pos);
            if let Some(path) = removed.image_path() {
                let _ = fs::remove_file(path);
            }
            save_entries_atomic(&history.entries);
        }
    }
}

pub fn clear_history() {
    let store_arc = store();
    if let Ok(mut history) = store_arc.write() {
        for entry in &history.entries {
            if let Some(path) = entry.image_path() {
                let _ = fs::remove_file(path);
            }
        }
        history.entries.clear();
        save_entries_atomic(&history.entries);
    }
}

/// Spawns a lightweight background watcher (400ms interval) that detects
/// copies in external apps while Corvo is closed or resident, capturing
/// the active frontmost app at copy-time and persisting history to disk.
pub fn start_watcher() {
    let _ = store();
    smol::spawn(async {
        loop {
            smol::Timer::after(std::time::Duration::from_millis(400)).await;
            if !preferences().enabled {
                continue;
            }
            let current_count = corvo_platform::clipboard_change_count();
            let last_count = {
                let s = store();
                s.read().map(|h| h.last_change_count).unwrap_or(-1)
            };
            if current_count != last_count {
                poll_clipboard();
            }
        }
    })
    .detach();
}

fn single_line_preview(text: &str) -> String {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if first.is_empty() {
        return "Empty".into();
    }
    let char_count = first.chars().count();
    if char_count <= 24 {
        return first.to_string();
    }
    let target = 22;
    let slice: String = first.chars().take(target).collect();
    if let Some(last_space) = slice.rfind(' ') {
        if last_space >= 8 {
            return format!("{}...", &slice[..last_space]);
        }
    }
    format!("{slice}...")
}

fn is_url(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

fn is_json(text: &str) -> bool {
    let trimmed = text.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    } else {
        false
    }
}

fn is_color_code(text: &str) -> bool {
    let value = text.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    let lower = value.to_ascii_lowercase();
    let Some((name, values)) = lower.split_once('(') else { return false };
    let Some(values) = values.strip_suffix(')') else { return false };
    if !matches!(name.trim(), "rgb" | "rgba") {
        return false;
    }
    let components: Vec<_> = values.split(',').map(str::trim).collect();
    let expected = if name.trim() == "rgba" { 4 } else { 3 };
    components.len() == expected
        && components.iter().all(|component| {
            component.strip_suffix('%').unwrap_or(component).parse::<f32>().is_ok()
        })
}

fn should_record_text(text: &str) -> bool {
    preferences().save_colors || !is_color_code(text)
}

#[derive(Default)]
pub struct ClipboardManagerCommand;

corvo_core::register_command!(ClipboardManagerCommand);

#[async_trait::async_trait]
impl Command for ClipboardManagerCommand {
    fn id(&self) -> &'static str {
        "clipboard-manager"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["clipboard", "clip", "paste", "history", "copied"]
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let q = query.trim().to_lowercase();
        if let Some(subquery) = q.strip_prefix("clipboard-page:") {
            let (filter, sub) = if let Some(rest) = subquery.strip_prefix("filter=") {
                if let Some((f, s)) = rest.split_once(':') {
                    (f.trim(), s.trim())
                } else {
                    ("all", rest.trim())
                }
            } else {
                ("all", subquery.trim())
            };

            let mut entries = all_entries();
            match filter {
                "text" => entries.retain(|e| e.image_path().is_none() && !is_url(&e.text) && !is_json(&e.text)),
                "links" => entries.retain(|e| is_url(&e.text)),
                "images" => entries.retain(|e| e.image_path().is_some()),
                "json" => entries.retain(|e| is_json(&e.text)),
                _ => {}
            }

            if sub.is_empty() {
                return entries
                    .into_iter()
                    .take(ctx.max_results)
                    .map(|entry| {
                        let icon = if let Some(path) = entry.image_path() {
                            Icon::Image(path)
                        } else if is_url(&entry.text) {
                            Icon::Svg(phosphor_svgs::style::regular::LINK)
                        } else if is_json(&entry.text) {
                            Icon::Svg(phosphor_svgs::style::regular::CODE)
                        } else {
                            Icon::Svg(phosphor_svgs::style::regular::FILE_TEXT)
                        };
                        let accessory = if is_json(&entry.text) {
                            "JSON".to_string()
                        } else if is_url(&entry.text) {
                            "Link".to_string()
                        } else if entry.image_path().is_some() {
                            "Image".to_string()
                        } else if preferences().save_colors && is_color_code(&entry.text) {
                            "Color".to_string()
                        } else {
                            entry.source_app
                        };
                        SearchResult {
                            id: format!("clipboard-manager:entry:{}", entry.id),
                            title: single_line_preview(&entry.text),
                            subtitle: Some(entry.section),
                            icon,
                            score: 50.0,
                            accessory: Some(accessory),
                        }
                    })
                    .collect();
            }

            let texts: Vec<String> = entries.iter().map(|e| e.text.clone()).collect();
            let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            let mut matcher = frizbee::Matcher::new(sub, &frizbee::Config::default());
            let mut matches: Vec<frizbee::Match> = matcher.match_list(&text_refs).to_vec();
            matches.sort();
            return matches
                .iter()
                .filter_map(|m| entries.get(m.index as usize))
                .take(ctx.max_results)
                .map(|entry| {
                    let icon = if let Some(path) = entry.image_path() {
                        Icon::Image(path)
                    } else if is_url(&entry.text) {
                        Icon::Svg(phosphor_svgs::style::regular::LINK)
                    } else if is_json(&entry.text) {
                        Icon::Svg(phosphor_svgs::style::regular::CODE)
                    } else {
                        Icon::Svg(phosphor_svgs::style::regular::FILE_TEXT)
                    };
                    let accessory = if is_json(&entry.text) {
                        "JSON".to_string()
                    } else if is_url(&entry.text) {
                        "Link".to_string()
                    } else if entry.image_path().is_some() {
                        "Image".to_string()
                    } else if preferences().save_colors && is_color_code(&entry.text) {
                        "Color".to_string()
                    } else {
                        entry.source_app.clone()
                    };
                    SearchResult {
                        id: format!("clipboard-manager:entry:{}", entry.id),
                        title: single_line_preview(&entry.text),
                        subtitle: Some(entry.section.clone()),
                        icon,
                        score: 50.0,
                        accessory: Some(accessory),
                    }
                })
                .collect();
        }

        if q.is_empty() {
            return vec![SearchResult {
                id: "clipboard-manager:open".into(),
                title: "Clipboard History".into(),
                subtitle: Some("Commands".into()),
                icon: Icon::Clipboard,
                score: 99.0,
                accessory: Some("⌥⌘C".into()),
            }];
        }

        let mut results = Vec::new();
        if let Some(score) = search_match_score(
            &q,
            &["Clipboard History", "clipboard clip paste history copied"],
        ) {
            results.push(SearchResult {
                id: "clipboard-manager:open".into(),
                title: "Clipboard History".into(),
                subtitle: Some("Commands".into()),
                icon: Icon::Clipboard,
                score: score + 12.0,
                accessory: Some("⌥⌘C".into()),
            });
        }
        results
    }

    async fn execute(&self, result_id: &str, ctx: &ExecutionContext) -> Result<Action, CommandError> {
        if result_id == "clipboard-manager:open" {
            return Ok(Action::ShowToast("Clipboard History".into()));
        }
        if let Some(id) = result_id.strip_prefix("clipboard-manager:entry:") {
            if let Some(entry) = get_entry(id) {
                if let Some(path) = entry.image_path() {
                    return Ok(if ctx.store.as_ref().is_some_and(|store| store.clipboard_auto_paste()) {
                        Action::PasteImage(path)
                    } else {
                        Action::CopyImage(path)
                    });
                } else {
                    return Ok(if ctx.store.as_ref().is_some_and(|store| store.clipboard_auto_paste()) {
                        Action::PasteText(entry.text)
                    } else {
                        Action::Copy(entry.text)
                    });
                }
            }
        }
        Err(CommandError::NotFound)
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        if result_id == "clipboard-manager:open" {
            return vec![CommandAction {
                id: "clipboard-manager-action:open".into(),
                label: "Open Clipboard History".into(),
                action: Action::ShowToast("Clipboard History".into()),
                icon: Icon::Svg(phosphor_svgs::style::regular::CLIPBOARD_TEXT),
                group: ActionGroup::Primary,
                hotkey: Some("↵"),
            }];
        }
        let Some(id) = result_id.strip_prefix("clipboard-manager:entry:") else {
            return Vec::new();
        };
        let Some(entry) = get_entry(id) else {
            return Vec::new();
        };

        if let Some(img_path) = entry.image_path() {
            vec![
                CommandAction {
                    id: "clipboard-manager-action:paste-image".into(),
                    label: "Paste to Active App".into(),
                    action: Action::PasteImage(img_path.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                    group: ActionGroup::Primary,
                    hotkey: Some("↵"),
                },
                CommandAction {
                    id: "clipboard-manager-action:copy-image".into(),
                    label: "Copy Image".into(),
                    action: Action::CopyImage(img_path.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                    group: ActionGroup::Standard,
                    hotkey: Some("⌘↵"),
                },
                CommandAction {
                    id: "clipboard-manager-action:reveal-image".into(),
                    label: "Show in Finder".into(),
                    action: Action::RunShell(format!("open -R \"{}\"", img_path.to_string_lossy())),
                    icon: Icon::Svg(phosphor_svgs::style::regular::FOLDER),
                    group: ActionGroup::Standard,
                    hotkey: Some("⌥↵"),
                },
                CommandAction {
                    id: format!("clipboard-manager-action:delete:{id}"),
                    label: "Delete Entry".into(),
                    action: Action::ShowToast(format!("delete:{id}")),
                    icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
                    group: ActionGroup::Destructive,
                    hotkey: Some("⌘⌫"),
                },
                CommandAction {
                    id: "clipboard-manager-action:clear".into(),
                    label: "Clear History".into(),
                    action: Action::ShowToast("clear".into()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
                    group: ActionGroup::Destructive,
                    hotkey: None,
                },
            ]
        } else {
            vec![
                CommandAction {
                    id: "clipboard-manager-action:paste".into(),
                    label: "Paste to Active App".into(),
                    action: Action::Copy(entry.text.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                    group: ActionGroup::Primary,
                    hotkey: Some("↵"),
                },
                CommandAction {
                    id: "clipboard-manager-action:copy".into(),
                    label: "Copy to Clipboard".into(),
                    action: Action::ShowToast(format!("copy:{}", entry.text)),
                    icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                    group: ActionGroup::Standard,
                    hotkey: Some("⌘↵"),
                },
                CommandAction {
                    id: format!("clipboard-manager-action:delete:{id}"),
                    label: "Delete Entry".into(),
                    action: Action::ShowToast(format!("delete:{id}")),
                    icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
                    group: ActionGroup::Destructive,
                    hotkey: Some("⌘⌫"),
                },
                CommandAction {
                    id: "clipboard-manager-action:clear".into(),
                    label: "Clear History".into(),
                    action: Action::ShowToast("clear".into()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
                    group: ActionGroup::Destructive,
                    hotkey: None,
                },
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_search_empty_returns_command() {
        let command = ClipboardManagerCommand;
        let ctx = SearchContext { max_results: 10, store: None };
        let results = smol::block_on(command.search("", &ctx));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "clipboard-manager:open");
    }

    #[test]
    fn root_search_keyword_returns_command() {
        let command = ClipboardManagerCommand;
        let ctx = SearchContext { max_results: 10, store: None };
        let results = smol::block_on(command.search("clipboard", &ctx));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "clipboard-manager:open");
    }

    #[test]
    fn single_line_preview_truncates() {
        let long_str = "a".repeat(100);
        let preview = single_line_preview(&long_str);
        assert!(preview.ends_with("..."));
        assert_eq!(preview.chars().count(), 25);
    }

    #[test]
    fn test_format_time_and_section() {
        let now = 1710000000;
        let (copied, section) = format_time_and_section(now, now);
        assert_eq!(section, "Today");
        assert!(copied.starts_with("Today at"));

        let yesterday = now - 86400;
        let (copied_yesterday, section_yesterday) = format_time_and_section(now, yesterday);
        assert_eq!(section_yesterday, "Yesterday");
        assert!(copied_yesterday.starts_with("Yesterday at"));
    }

    #[test]
    fn retention_filters_entries_older_than_30_days() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut fresh = create_entry("Fresh Entry".into(), "TestApp".into());
        fresh.timestamp_secs = now - 5 * 86_400; // 5 days old

        let mut expired = create_entry("Expired Entry".into(), "TestApp".into());
        expired.timestamp_secs = now - 35 * 86_400; // 35 days old

        let cutoff = now.saturating_sub(RETENTION_SECS);
        let mut entries = vec![fresh, expired];
        entries.retain(|e| e.timestamp_secs >= cutoff);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "Fresh Entry");
    }

    #[test]
    fn test_image_entry_creation_and_actions() {
        let png_bytes = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D,
            b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x01,
            0x08, 0x06, 0x00, 0x00, 0x00,
            0x1F, 0x15, 0xC4, 0x89,
            0x00, 0x00, 0x00, 0x0A,
            b'I', b'D', b'A', b'T',
            0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01,
            0x0D, 0x0A, 0x2D, 0xB4,
            0x00, 0x00, 0x00, 0x00,
            b'I', b'E', b'N', b'D',
            0xAE, 0x42, 0x60, 0x82,
        ];
        let entry = create_image_entry(png_bytes, "Finder".into()).expect("Failed to create image entry");
        assert!(entry.is_image());
        assert_eq!(entry.image_width, Some(1));
        assert_eq!(entry.image_height, Some(1));
        assert!(entry.image_path().is_some());
        let path = entry.image_path().unwrap();
        assert!(path.exists());

        {
            let s = store();
            let mut h = s.write().unwrap();
            h.entries.insert(0, entry.clone());
        }

        let cmd = ClipboardManagerCommand;
        let actions = cmd.actions(&format!("clipboard-manager:entry:{}", entry.id));
        assert_eq!(actions.len(), 5);
        assert_eq!(actions[0].label, "Paste to Active App");
        assert_eq!(actions[1].label, "Copy Image");
        assert_eq!(actions[2].label, "Show in Finder");

        delete_entry(&entry.id);
        assert!(!path.exists());
    }
}

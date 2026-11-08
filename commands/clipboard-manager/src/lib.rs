//! Clipboard history manager and watcher.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

use chrono::{DateTime, Local};

use corvo_core::{
    phosphor_svgs, search_match_score, Action, ActionGroup, Command, CommandAction, CommandError,
    ExecutionContext, Icon, SearchContext, SearchResult,
};

const MAX_ENTRY_CHARS: usize = 100_000;
#[cfg(test)]
const RETENTION_SECS: u64 = 90 * 86_400;

/// Three months of history is the product target. The entry cap has to be
/// high enough that heavy use does not silently truncate the retention
/// window to a few days.
const DEFAULT_MAX_ENTRIES: usize = 2_000;
const DEFAULT_RETENTION_DAYS: u32 = 90;
const CORVO_SOURCE: &str = "Corvo";
const UNKNOWN_SOURCE: &str = "Unknown";

/// App names that handle secrets. The pasteboard markers in
/// `corvo_platform::clipboard_is_concealed` cover most password managers,
/// but Keychain Access and Apple's Passwords app expose no marker, so the
/// frontmost app is the second guard.
const SECRET_APP_FRAGMENTS: [&str; 15] = [
    "keychain",
    "passwords",
    "1password",
    "bitwarden",
    "keepass",
    "lastpass",
    "dashlane",
    "enpass",
    "nordpass",
    "proton pass",
    "protonpass",
    "roboform",
    "seahorse",
    "gnome-keyring",
    "keeper password",
];

/// True when the app that owns the copy holds secrets. Matching is
/// case-insensitive and substring-based: the OS reports names such as
/// "1Password 8" or a Linux process name such as "keepassxc".
fn is_secret_source_app(source_app: &str) -> bool {
    let name = source_app.trim().to_lowercase();
    !name.is_empty()
        && SECRET_APP_FRAGMENTS
            .iter()
            .any(|fragment| name.contains(fragment))
}

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
        Self {
            enabled: true,
            retention_days: DEFAULT_RETENTION_DAYS,
            max_entries: DEFAULT_MAX_ENTRIES,
            save_images: true,
            save_colors: true,
        }
    }
}

fn preferences_store() -> &'static RwLock<ClipboardPreferences> {
    static PREFERENCES: OnceLock<RwLock<ClipboardPreferences>> = OnceLock::new();
    PREFERENCES.get_or_init(|| RwLock::new(ClipboardPreferences::default()))
}

fn preferences() -> ClipboardPreferences {
    preferences_store()
        .read()
        .map(|current| *current)
        .unwrap_or_default()
}

fn current_clipboard_image() -> Option<Vec<u8>> {
    preferences()
        .save_images
        .then(corvo_platform::read_clipboard_image)
        .flatten()
}

pub fn set_preferences(value: ClipboardPreferences) {
    let value = ClipboardPreferences {
        max_entries: value.max_entries.max(1),
        ..value
    };
    let was_enabled = preferences_store()
        .read()
        .map(|current| current.enabled)
        .unwrap_or(true);
    if !was_enabled && value.enabled {
        let count = corvo_platform::clipboard_change_count();
        if let Some(store) = STORE.get() {
            if let Ok(mut history) = store.write() {
                history.last_change_count = count;
            }
        }
    }
    let should_prune = if let Ok(mut current) = preferences_store().write() {
        let changed = current.retention_days != value.retention_days
            || current.max_entries != value.max_entries;
        *current = value;
        changed
    } else {
        false
    };
    if should_prune {
        prune_history_to_preferences();
    }
}

/// The clipboard data directory, falling back to a scratch path when
/// the OS reports no home directory instead of panicking.
fn project_data_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "corvo")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("corvo"))
}

fn history_file_path() -> PathBuf {
    project_data_dir().join("clipboard_history.json")
}

pub fn images_dir_path() -> PathBuf {
    project_data_dir().join("clipboard_images")
}

fn simple_hash(bytes: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// Total bytes the history may hold. The entry cap alone lets a few large
/// screenshots or long documents grow the file without limit.
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// Bytes one entry keeps on disk: its text plus its PNG file size.
fn entry_bytes(entry: &ClipboardEntry) -> usize {
    entry.text.len() + entry.image_bytes.unwrap_or(0)
}

/// Drops expired entries, then the oldest entries until the count and the
/// byte budget fit. Entries are ordered newest first, so the tail goes
/// first. Returns the removed entries so callers can delete their images.
fn prune_entries(
    entries: &mut Vec<ClipboardEntry>,
    max_entries: usize,
    max_bytes: usize,
    cutoff_secs: u64,
) -> Vec<ClipboardEntry> {
    let mut removed = Vec::new();
    let mut kept = Vec::with_capacity(entries.len());
    for entry in entries.drain(..) {
        if entry.timestamp_secs >= cutoff_secs {
            kept.push(entry);
        } else {
            removed.push(entry);
        }
    }
    let mut total_bytes: usize = kept.iter().map(entry_bytes).sum();
    while kept.len() > max_entries {
        let Some(entry) = kept.pop() else { break };
        total_bytes = total_bytes.saturating_sub(entry_bytes(&entry));
        removed.push(entry);
    }
    while total_bytes > max_bytes {
        let Some(entry) = kept.pop() else { break };
        total_bytes = total_bytes.saturating_sub(entry_bytes(&entry));
        removed.push(entry);
    }
    *entries = kept;
    removed
}

fn remove_entry_files(entries: &[ClipboardEntry]) {
    for entry in entries {
        if let Some(path) = entry.image_path() {
            let _ = fs::remove_file(path);
        }
    }
}

fn load_persisted_entries() -> Vec<ClipboardEntry> {
    let path = history_file_path();
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(mut entries) = serde_json::from_str::<Vec<ClipboardEntry>>(&data) else {
        corvo_platform::diagnostics::record_error("clipboard", "history_parse_failed");
        eprintln!("corvo: clipboard_history.json malformed, starting fresh");
        return Vec::new();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff = now.saturating_sub(u64::from(preferences().retention_days) * 86_400);
    let removed = prune_entries(
        &mut entries,
        preferences().max_entries,
        MAX_TOTAL_BYTES,
        cutoff,
    );
    remove_entry_files(&removed);
    entries
}

fn save_entries_atomic(entries: &[ClipboardEntry]) -> Result<(), String> {
    let path = history_file_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Clipboard history path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create clipboard history directory: {error}"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff = now.saturating_sub(u64::from(preferences().retention_days) * 86_400);

    let mut filtered = entries.to_vec();
    let removed = prune_entries(
        &mut filtered,
        preferences().max_entries,
        MAX_TOTAL_BYTES,
        cutoff,
    );

    let json = serde_json::to_string_pretty(&filtered)
        .map_err(|error| format!("Could not encode clipboard history: {error}"))?;
    let tmp_path = parent.join(format!("clipboard_history.json.tmp.{}", std::process::id()));

    fs::write(&tmp_path, json)
        .map_err(|error| format!("Could not write clipboard history: {error}"))?;
    if let Err(error) = fs::rename(&tmp_path, &path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Could not save clipboard history: {error}"));
    }
    remove_entry_files(&removed);
    Ok(())
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
        self.image_file
            .as_ref()
            .map(|filename| images_dir_path().join(filename))
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
            let source = resolve_capture_source(None);

            // stamp_top: false — a relaunch is not a copy event, so
            // content already sitting at the top keeps its timestamp.
            if ingest_current_clipboard(&mut history, source.as_deref(), false) {
                let _ = save_entries_atomic(&history.entries);
            }
        }
        Arc::new(RwLock::new(history))
    })
}

fn format_time_and_section(now_secs: u64, entry_secs: u64) -> (String, String) {
    let to_local = |secs: u64| {
        i64::try_from(secs)
            .ok()
            .and_then(|s| DateTime::from_timestamp(s, 0))
            .map(|dt| dt.with_timezone(&Local))
    };

    let (Some(now_dt), Some(entry_dt)) = (to_local(now_secs), to_local(entry_secs)) else {
        return ("Previous at --:--:--".to_string(), "Older".to_string());
    };

    let time_str = entry_dt.format("%-I:%M:%S %p").to_string();
    let day_diff = (now_dt.date_naive() - entry_dt.date_naive()).num_days();

    let (section, copied_at_str) = match day_diff {
        d if d <= 0 => ("Today".to_string(), format!("Today at {time_str}")),
        1 => ("Yesterday".to_string(), format!("Yesterday at {time_str}")),
        _ => ("Older".to_string(), format!("Previous at {time_str}")),
    };

    (copied_at_str, section)
}

static ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn create_entry(text: String, source_app: String) -> ClipboardEntry {
    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let timestamp_secs = now.as_secs();
    let seq = ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = format!("{}_{}", now.as_millis(), seq);
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
    let seq = ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = format!("{}_{}", now.as_millis(), seq);
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
    let Ok(mut history) = store_arc.write() else {
        return;
    };
    if count == history.last_change_count {
        return;
    }
    history.last_change_count = count;
    if corvo_platform::clipboard_is_concealed() {
        return;
    }

    let Some(source) = resolve_capture_source(preferred_source) else {
        // The OS can name the frontmost app but could not this time.
        // Recording anyway would store a secret whenever the copy came
        // from Keychain Access or a password manager, so skip it.
        return;
    };

    // stamp_top: true — a pasteboard change is a real copy event (an
    // external copy, or Corvo's own paste/copy writing its content back),
    // so re-copying the top entry refreshes its timestamp.
    if ingest_current_clipboard(&mut history, Some(&source), true) {
        let _ = save_entries_atomic(&history.entries);
    }
}

/// The app that owns the copy, or `None` when the platform can attribute
/// copies but could not name the frontmost app.
///
/// `None` is the fail-closed case: without a name the secret-app guard
/// cannot run, and the pasteboard markers do not cover Keychain Access.
/// Platforms with no attribution at all report `"Unknown"` instead, so
/// their history keeps working.
fn resolve_capture_source(preferred_source: Option<&str>) -> Option<String> {
    resolve_capture_source_with(
        preferred_source,
        corvo_platform::frontmost_app_identity,
        corvo_platform::clipboard_source_attribution_available(),
    )
}

/// The pure decision behind [`resolve_capture_source`].
///
/// The lookup and the attribution capability are parameters so every
/// branch is testable without a desktop session.
fn resolve_capture_source_with(
    preferred_source: Option<&str>,
    identity: impl FnOnce() -> Option<(i32, String)>,
    attribution_available: bool,
) -> Option<String> {
    if let Some(name) = preferred_source
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return Some(name.to_string());
    }
    if let Some((pid, name)) = identity() {
        if pid == std::process::id() as i32 {
            return Some(CORVO_SOURCE.into());
        }
        let name = name.trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    if attribution_available {
        None
    } else {
        Some(UNKNOWN_SOURCE.into())
    }
}

/// Reads the current pasteboard content and merges it into history.
///
/// Content that already has an entry is promoted to the top with a fresh
/// timestamp instead of being recorded twice — this is what keeps
/// Corvo's own paste (Enter) and copy actions from duplicating entries.
/// `stamp_top` decides whether a match already at the top is re-stamped:
/// polling follows real copy events (true), while process start is not a
/// copy event (false).
fn ingest_current_clipboard(
    history: &mut ClipboardHistory,
    source: Option<&str>,
    stamp_top: bool,
) -> bool {
    let Some(source) = source else {
        return false;
    };
    if is_secret_source_app(source) {
        return false;
    }
    if let Some(png_bytes) = current_clipboard_image() {
        return merge_image(history, png_bytes, source, stamp_top);
    }
    match corvo_platform::read_clipboard_text() {
        Some(text) => merge_text(history, text, source, stamp_top),
        None => false,
    }
}

fn merge_image(
    history: &mut ClipboardHistory,
    png_bytes: Vec<u8>,
    source: &str,
    stamp_top: bool,
) -> bool {
    let hash = simple_hash(&png_bytes);
    if let Some(pos) = history
        .entries
        .iter()
        .position(|entry| entry.image_hash == Some(hash))
    {
        if pos == 0 && !stamp_top {
            return false;
        }
        promote_entry(&mut history.entries, pos);
        return true;
    }
    match create_image_entry(png_bytes, source.to_string()) {
        Some(entry) => {
            insert_new_entry(history, entry);
            true
        }
        None => false,
    }
}

fn merge_text(history: &mut ClipboardHistory, text: String, source: &str, stamp_top: bool) -> bool {
    if text.trim().is_empty() || !should_record_text(&text) {
        return false;
    }
    let text = if text.len() > MAX_ENTRY_CHARS {
        text.chars().take(MAX_ENTRY_CHARS).collect()
    } else {
        text
    };
    if let Some(pos) = history
        .entries
        .iter()
        .position(|entry| !entry.is_image() && entry.text == text)
    {
        if pos == 0 && !stamp_top {
            return false;
        }
        promote_entry(&mut history.entries, pos);
        return true;
    }
    insert_new_entry(history, create_entry(text, source.to_string()));
    true
}

/// Inserts a freshly captured entry at the top, evicting the oldest entry
/// (and its image file) once the history exceeds the cap.
fn insert_new_entry(history: &mut ClipboardHistory, entry: ClipboardEntry) {
    history.entries.insert(0, entry);
    let removed = prune_entries(
        &mut history.entries,
        preferences().max_entries,
        MAX_TOTAL_BYTES,
        0,
    );
    remove_entry_files(&removed);
}

/// Moves `entries[pos]` to the front and stamps it as just-captured.
/// The entry keeps its original source app: while pasting, the poll-time
/// frontmost app is the paste target, not where the content came from.
fn promote_entry(entries: &mut Vec<ClipboardEntry>, pos: usize) {
    let mut entry = entries.remove(pos);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    entry.timestamp_secs = now;
    let (copied_at_str, section) = format_time_and_section(now, now);
    entry.copied_at_str = copied_at_str;
    entry.section = section;
    entries.insert(0, entry);
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
    let Ok(history) = store_arc.read() else {
        return Vec::new();
    };
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
        let removed = prune_entries(
            &mut history.entries,
            prefs.max_entries,
            MAX_TOTAL_BYTES,
            cutoff,
        );
        remove_entry_files(&removed);
        let _ = save_entries_atomic(&history.entries);
    }
}

pub fn delete_entry(id: &str) -> Result<(), String> {
    let mut history = store()
        .write()
        .map_err(|_| "Could not lock clipboard history".to_string())?;
    let pos = history
        .entries
        .iter()
        .position(|entry| entry.id == id)
        .ok_or_else(|| "Clipboard item was not found".to_string())?;
    let previous_entries = history.entries.clone();
    let removed = history.entries.remove(pos);
    if let Err(error) = save_entries_atomic(&history.entries) {
        history.entries = previous_entries;
        return Err(error);
    }
    if let Some(path) = removed.image_path() {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

pub fn clear_history() -> Result<(), String> {
    let mut history = store()
        .write()
        .map_err(|_| "Could not lock clipboard history".to_string())?;
    let previous_entries = history.entries.clone();
    history.entries.clear();
    if let Err(error) = save_entries_atomic(&history.entries) {
        history.entries = previous_entries;
        return Err(error);
    }
    for entry in previous_entries {
        if let Some(path) = entry.image_path() {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
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
    let first = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
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
        return matches!(hex.len(), 3 | 4 | 6 | 8)
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    let lower = value.to_ascii_lowercase();
    let Some((name, values)) = lower.split_once('(') else {
        return false;
    };
    let Some(values) = values.strip_suffix(')') else {
        return false;
    };
    if !matches!(name.trim(), "rgb" | "rgba") {
        return false;
    }
    let components: Vec<_> = values.split(',').map(str::trim).collect();
    let expected = if name.trim() == "rgba" { 4 } else { 3 };
    components.len() == expected
        && components.iter().all(|component| {
            component
                .strip_suffix('%')
                .unwrap_or(component)
                .parse::<f32>()
                .is_ok()
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
                "text" => entries
                    .retain(|e| e.image_path().is_none() && !is_url(&e.text) && !is_json(&e.text)),
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
                            score: 50,
                            accessory: Some(accessory),
                            section: None,
                            accessories: Vec::new(),
                        }
                    })
                    .collect();
            }

            let q_text = corvo_core::search::SearchText::new(sub);
            let mut scored: Vec<(&ClipboardEntry, i32)> = Vec::new();
            for entry in &entries {
                let t_text = corvo_core::search::SearchText::new(&entry.text);
                let app_text = corvo_core::search::SearchText::new(&entry.source_app);
                let fields = [
                    (corvo_core::search::FieldRole::Name, &t_text),
                    (corvo_core::search::FieldRole::Subtitle, &app_text),
                ];
                if let Some(quality) = corvo_core::search::quality(&q_text, &fields) {
                    scored.push((entry, quality));
                }
            }
            scored.sort_by_key(|a| std::cmp::Reverse(a.1));

            return scored
                .into_iter()
                .take(ctx.max_results)
                .map(|(entry, score)| {
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
                        score,
                        accessory: Some(accessory),
                        section: None,
                        accessories: Vec::new(),
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
                score: 990,
                accessory: Some("cmd+alt+c".into()),
                section: None,
                accessories: Vec::new(),
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
                score,
                accessory: Some("cmd+alt+c".into()),
                section: None,
                accessories: Vec::new(),
            });
        }
        results
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        if result_id == "clipboard-manager:open" {
            return Ok(Action::ShowToast("Clipboard History".into()));
        }
        if let Some(id) = result_id.strip_prefix("clipboard-manager:entry:") {
            if let Some(entry) = get_entry(id) {
                if let Some(path) = entry.image_path() {
                    return Ok(Action::PasteImage(path));
                } else {
                    return Ok(Action::PasteText(entry.text));
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
                hotkey: Some("enter"),
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
                    hotkey: Some("enter"),
                },
                CommandAction {
                    id: "clipboard-manager-action:copy-image".into(),
                    label: "Copy Image".into(),
                    action: Action::CopyImage(img_path.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                    group: ActionGroup::Standard,
                    hotkey: Some("cmd+enter"),
                },
                CommandAction {
                    id: "clipboard-manager-action:reveal-image".into(),
                    label: "Show in Finder".into(),
                    action: Action::RunShell(format!("open -R \"{}\"", img_path.to_string_lossy())),
                    icon: Icon::Svg(phosphor_svgs::style::regular::FOLDER),
                    group: ActionGroup::Standard,
                    hotkey: Some("alt+enter"),
                },
                CommandAction {
                    id: format!("clipboard-manager-action:delete:{id}"),
                    label: "Delete Entry".into(),
                    action: Action::ShowToast(format!("delete:{id}")),
                    icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
                    group: ActionGroup::Destructive,
                    hotkey: Some("cmd+backspace"),
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
                    action: Action::PasteText(entry.text.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_BEND_DOWN_LEFT),
                    group: ActionGroup::Primary,
                    hotkey: Some("enter"),
                },
                CommandAction {
                    id: "clipboard-manager-action:copy".into(),
                    label: "Copy to Clipboard".into(),
                    action: Action::Copy(entry.text.clone()),
                    icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                    group: ActionGroup::Standard,
                    hotkey: Some("cmd+enter"),
                },
                CommandAction {
                    id: format!("clipboard-manager-action:delete:{id}"),
                    label: "Delete Entry".into(),
                    action: Action::ShowToast(format!("delete:{id}")),
                    icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
                    group: ActionGroup::Destructive,
                    hotkey: Some("cmd+backspace"),
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
        let ctx = SearchContext {
            max_results: 10,
            store: None,
        };
        let results = smol::block_on(command.search("", &ctx));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "clipboard-manager:open");
    }

    #[test]
    fn root_search_keyword_returns_command() {
        let command = ClipboardManagerCommand;
        let ctx = SearchContext {
            max_results: 10,
            store: None,
        };
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

        let future = now + 10;
        let (copied_future, section_future) = format_time_and_section(now, future);
        assert_eq!(section_future, "Today");
        assert!(copied_future.starts_with("Today at"));

        let yesterday = now - 86400;
        let (copied_yesterday, section_yesterday) = format_time_and_section(now, yesterday);
        assert_eq!(section_yesterday, "Yesterday");
        assert!(copied_yesterday.starts_with("Yesterday at"));

        let older = now - 86400 * 5;
        let (copied_older, section_older) = format_time_and_section(now, older);
        assert_eq!(section_older, "Older");
        assert!(copied_older.starts_with("Previous at"));
    }

    #[test]
    fn retention_filters_entries_older_than_three_months() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut fresh = create_entry("Fresh Entry".into(), "TestApp".into());
        fresh.timestamp_secs = now - 60 * 86_400; // 60 days old

        let mut expired = create_entry("Expired Entry".into(), "TestApp".into());
        expired.timestamp_secs = now - 95 * 86_400; // 95 days old

        let cutoff = now.saturating_sub(RETENTION_SECS);
        let mut entries = vec![fresh, expired];
        entries.retain(|e| e.timestamp_secs >= cutoff);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "Fresh Entry");
    }

    #[test]
    fn secret_source_apps_are_excluded() {
        for name in [
            "Keychain Access",
            "Passwords",
            "1Password",
            "1Password 8",
            "Bitwarden",
            "KeePassXC",
            "keepassxc",
            "LastPass",
            "Dashlane",
            "Enpass",
            "NordPass",
            "Proton Pass",
            "RoboForm",
            "Seahorse",
            "gnome-keyring",
            "Keeper Password Manager",
        ] {
            assert!(is_secret_source_app(name), "{name} must be excluded");
        }

        for name in [
            "Safari",
            "Terminal",
            "Visual Studio Code",
            "Notes",
            "Corvo",
            "Unknown",
            "",
        ] {
            assert!(!is_secret_source_app(name), "{name} must be recorded");
        }
    }

    #[test]
    fn capture_skips_secret_source_apps_without_reading_the_clipboard() {
        let mut history = test_history(Vec::new());

        assert!(!ingest_current_clipboard(
            &mut history,
            Some("1Password"),
            true
        ));
        assert!(history.entries.is_empty());
    }

    #[test]
    fn capture_skips_an_unresolved_source() {
        let mut history = test_history(Vec::new());

        assert!(!ingest_current_clipboard(&mut history, None, true));
        assert!(history.entries.is_empty());
    }

    #[test]
    fn resolve_keeps_a_named_preferred_source() {
        let source = resolve_capture_source_with(Some("Safari"), || None, true);

        assert_eq!(source.as_deref(), Some("Safari"));
    }

    #[test]
    fn resolve_names_corvo_for_its_own_pid() {
        let own_pid = std::process::id() as i32;
        let source =
            resolve_capture_source_with(None, || Some((own_pid, "corvo".into())), true);

        assert_eq!(source.as_deref(), Some(CORVO_SOURCE));
    }

    #[test]
    fn resolve_fails_closed_when_attribution_is_available() {
        let source = resolve_capture_source_with(None, || None, true);

        assert_eq!(source, None);
    }

    #[test]
    fn resolve_uses_unknown_when_attribution_is_unavailable() {
        let source = resolve_capture_source_with(None, || None, false);

        assert_eq!(source.as_deref(), Some(UNKNOWN_SOURCE));
    }

    #[test]
    fn resolve_keeps_a_named_frontmost_app() {
        let source = resolve_capture_source_with(None, || Some((42, "Notes".into())), true);

        assert_eq!(source.as_deref(), Some("Notes"));
    }

    #[test]
    fn default_retention_is_three_months() {
        let prefs = ClipboardPreferences::default();
        assert_eq!(prefs.retention_days, 90);
        assert!(prefs.max_entries >= 2000);
    }

    #[test]
    fn prune_drops_oldest_entries_until_the_byte_budget_fits() {
        let now = 1_700_000_000;
        let mut entries = vec![
            create_entry("a".repeat(40), "A".into()),
            create_entry("b".repeat(40), "B".into()),
            create_entry("c".repeat(40), "C".into()),
        ];
        for entry in &mut entries {
            entry.timestamp_secs = now;
        }

        let removed = prune_entries(&mut entries, 10, 90, now);

        assert_eq!(entries.len(), 2, "the tail entry is dropped");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].text, "c".repeat(40));
        let total: usize = entries.iter().map(entry_bytes).sum();
        assert!(total <= 90);
    }

    #[test]
    fn prune_drops_expired_entries_before_the_byte_budget() {
        let now = 1_700_000_000;
        let mut expired = create_entry("old".into(), "A".into());
        expired.timestamp_secs = now - 10;
        let mut fresh = create_entry("new".into(), "B".into());
        fresh.timestamp_secs = now;
        let mut entries = vec![fresh, expired];

        let removed = prune_entries(&mut entries, 10, 10_000, now);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "new");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].text, "old");
    }

    /// Minimal valid 1×1 PNG; no pasteboard is touched.
    fn minimal_png() -> Vec<u8> {
        vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, b'I', b'D', b'A', b'T', 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    fn test_history(entries: Vec<ClipboardEntry>) -> ClipboardHistory {
        ClipboardHistory {
            last_change_count: 0,
            entries,
        }
    }

    #[test]
    fn merge_text_promotes_existing_entry_without_duplicating() {
        let mut history = test_history(vec![
            create_entry("top".into(), "A".into()),
            create_entry("middle".into(), "B".into()),
            create_entry("oldest".into(), "C".into()),
        ]);
        let middle_id = history.entries[1].id.clone();
        history.entries[1].timestamp_secs = 100;
        history.entries[1].section = "Older".into();

        assert!(merge_text(&mut history, "middle".into(), "Chrome", true));

        assert_eq!(history.entries.len(), 3, "no duplicate may be inserted");
        assert_eq!(history.entries[0].id, middle_id);
        assert_eq!(history.entries[0].source_app, "B");
        assert!(history.entries[0].timestamp_secs > 100);
        assert_eq!(history.entries[0].section, "Today");
    }

    #[test]
    fn merge_text_restamps_top_entry_when_polling_but_not_at_startup() {
        let mut history = test_history(vec![create_entry("top".into(), "A".into())]);
        history.entries[0].timestamp_secs = 100;

        assert!(!merge_text(&mut history, "top".into(), "X", false));
        assert_eq!(history.entries[0].timestamp_secs, 100);
        assert_eq!(history.entries.len(), 1);

        assert!(merge_text(&mut history, "top".into(), "X", true));
        assert!(history.entries[0].timestamp_secs > 100);
        assert_eq!(history.entries.len(), 1);
    }

    #[test]
    fn merge_text_records_new_content_at_top() {
        let mut history = test_history(vec![create_entry("top".into(), "A".into())]);

        assert!(merge_text(
            &mut history,
            "fresh copy".into(),
            "Safari",
            true
        ));

        assert_eq!(history.entries.len(), 2);
        assert_eq!(history.entries[0].text, "fresh copy");
        assert_eq!(history.entries[0].source_app, "Safari");
    }

    #[test]
    fn merge_text_never_promotes_image_entries() {
        let png_bytes = minimal_png();
        let entry = create_image_entry(png_bytes, "Finder".into()).expect("image entry");
        let image_path = entry.image_path().unwrap();
        let image_text = entry.text.clone();
        let mut history = test_history(vec![entry]);

        assert!(merge_text(&mut history, image_text, "X", true));

        assert_eq!(history.entries.len(), 2);
        assert!(!history.entries[0].is_image());

        let _ = fs::remove_file(image_path);
    }

    #[test]
    fn merge_image_promotes_existing_entry_without_duplicating() {
        let png_bytes = minimal_png();
        let entry = create_image_entry(png_bytes.clone(), "Finder".into()).expect("image entry");
        let image_path = entry.image_path().unwrap();
        let image_id = entry.id.clone();
        let mut history = test_history(vec![create_entry("note".into(), "A".into()), entry]);

        assert!(merge_image(&mut history, png_bytes, "X", true));

        assert_eq!(history.entries.len(), 2, "no duplicate may be inserted");
        assert_eq!(history.entries[0].id, image_id);
        assert_eq!(history.entries[0].source_app, "Finder");

        let _ = fs::remove_file(image_path);
    }

    #[test]
    fn test_image_entry_creation_and_actions() {
        let png_bytes = minimal_png();
        let entry =
            create_image_entry(png_bytes, "Finder".into()).expect("Failed to create image entry");
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

        let _ = delete_entry(&entry.id);
        assert!(!path.exists());
    }

    #[test]
    fn test_text_entry_creation_and_actions() {
        let entry = create_entry("Hello World".into(), "TextEdit".into());
        {
            let s = store();
            let mut h = s.write().unwrap();
            h.entries.insert(0, entry.clone());
        }

        let cmd = ClipboardManagerCommand;
        let actions = cmd.actions(&format!("clipboard-manager:entry:{}", entry.id));
        assert_eq!(actions.len(), 4);
        assert_eq!(actions[0].label, "Paste to Active App");
        assert_eq!(actions[0].action, Action::PasteText("Hello World".into()));
        assert_eq!(actions[1].label, "Copy to Clipboard");
        assert_eq!(actions[1].action, Action::Copy("Hello World".into()));

        let ctx = ExecutionContext::default();
        let exec_action =
            smol::block_on(cmd.execute(&format!("clipboard-manager:entry:{}", entry.id), &ctx));
        assert_eq!(exec_action, Ok(Action::PasteText("Hello World".into())));

        let _ = delete_entry(&entry.id);
    }
}

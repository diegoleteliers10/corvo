//! Quick notes: one markdown file per note under `data_dir()/notes/`,
//! named after the note's title (slugged), with `title` and `created_at`
//! in a small frontmatter block. Parsing never assumes anything about a
//! file's contents: empty bodies, missing frontmatter, or empty titles
//! all degrade to placeholder values instead of breaking the list.
//!
//! Routing: `note <text>` opens the editor on a new note, `notes` opens
//! the dedicated page; the page namespace `notes-page:` lists and
//! filters saved notes.

use std::path::PathBuf;

use corvo_core::{
    phosphor_svgs, search_match_score, Action, ActionGroup, Command, CommandAction, CommandError,
    ExecutionContext, Icon, SearchContext, SearchResult,
};

/// A saved note as the UI sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// The file stem, the stable id.
    pub id: String,
    pub title: String,
    pub body: String,
    /// `YYYY-MM-DD HH:MM` from the frontmatter, or the file's mtime.
    pub created_at: String,
}

fn notes_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "corvo")
        .map(|dirs| dirs.data_dir().join("notes"))
        .unwrap_or_else(|| std::env::temp_dir().join("corvo-notes"))
}

/// Turns a note title into a file name: lowercase, alphanumeric and
/// dashes, never empty, never starting with a dot.
fn slug(title: &str) -> String {
    let mut slug: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let trimmed = slug.trim_matches('-').to_string();
    let mut out: String = trimmed.chars().take(40).collect();
    if out.is_empty() {
        out = "untitled".into();
    }
    if reserved_file_stem(&out) {
        out = format!("note-{out}");
    }
    out
}

/// Lists saved notes, newest first by frontmatter date.
pub fn list_notes() -> Vec<Note> {
    let Ok(entries) = std::fs::read_dir(notes_dir()) else {
        return Vec::new();
    };
    let mut notes: Vec<Note> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
        .filter_map(|entry| {
            let id = entry.path().file_stem()?.to_string_lossy().into_owned();
            let text = std::fs::read_to_string(entry.path()).ok()?;
            Some(parse_note(&id, &text, &entry.path()))
        })
        .collect();
    notes.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then(left.id.cmp(&right.id))
    });
    notes
}

/// Strips the frontmatter and derives the display fields. Every field
/// degrades to a placeholder: an empty file lists as "(empty note)"
/// instead of breaking anything.
fn parse_note(id: &str, text: &str, path: &std::path::Path) -> Note {
    let (meta_title, meta_created, body) = split_frontmatter(text);
    let body_trimmed = body.trim_start_matches('\n');
    let title_from_body = body_trimmed.lines().next().unwrap_or_default().trim();
    let title = meta_title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| title_from_body.to_owned());
    let created_at = meta_created
        .filter(|stamp| !stamp.trim().is_empty())
        .map(|stamp| stamp.to_owned())
        .unwrap_or_else(|| mtime_stamp(path).unwrap_or_else(|| "—".into()));
    Note {
        id: id.to_owned(),
        title,
        body: body_trimmed.to_owned(),
        created_at,
    }
}

type Frontmatter = (Option<String>, Option<String>, String);

fn split_frontmatter(text: &str) -> Frontmatter {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (None, None, text.to_owned());
    };
    // An empty frontmatter block: "---\n---\n" (or with a trailing
    // newline still to come).
    if let Some(body) = rest.strip_prefix("---\n") {
        return (None, None, body.to_owned());
    }
    if rest.trim_end() == "---" {
        return (None, None, String::new());
    }
    let Some(end) = rest.find("\n---") else {
        return (None, None, text.to_owned());
    };
    let header = &rest[..end];
    let body = rest[end + 4..].to_owned();
    let mut title = None;
    let mut created = None;
    for line in header.lines() {
        if let Some(value) = line.strip_prefix("title:") {
            title = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("created_at:") {
            created = Some(value.trim().to_owned());
        }
    }
    (title, created, body)
}

fn mtime_stamp(path: &std::path::Path) -> Option<String> {
    use std::time::UNIX_EPOCH;
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let seconds = modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let datetime = chrono::DateTime::from_timestamp(seconds, 0)?;
    Some(datetime.format("%Y-%m-%d %H:%M").to_string())
}

fn file_contents(title: &str, created_at: &str, body: &str) -> String {
    let escaped_title = title.replace(['\n', '\r'], " ");
    format!("---\ntitle: {escaped_title}\ncreated_at: {created_at}\n---\n{body}")
}

fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// Saves a new note. The file is named after the title; a collision
/// gets a numeric suffix.
pub fn create_note(title: &str, body: &str) -> std::io::Result<Note> {
    let title = title.trim();
    let body = body.trim();
    if title.is_empty() && body.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the note is empty",
        ));
    }
    let created_at = now_stamp();
    let contents = file_contents(title, &created_at, body);
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    let id = create_unique_note(&dir, &slug(title), &contents)?;
    Ok(parse_note(&id, &contents, &dir.join(format!("{id}.md"))))
}

/// Saves the note and keeps its creation date.
pub fn save_note(id: &str, title: &str, body: &str) -> std::io::Result<String> {
    save_note_in(&notes_dir(), id, title, body)
}

fn save_note_in(
    dir: &std::path::Path,
    id: &str,
    title: &str,
    body: &str,
) -> std::io::Result<String> {
    if !valid_note_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid note id",
        ));
    }
    let old_path = dir.join(format!("{id}.md"));
    let old_text = std::fs::read_to_string(&old_path)?;
    let created_at = split_frontmatter(&old_text).1.unwrap_or_else(now_stamp);
    let title_slug = slug(title);
    let same_title_slug = slug(&parse_note(id, &old_text, &old_path).title) == title_slug;
    let contents = file_contents(title.trim(), &created_at, body);
    std::fs::create_dir_all(dir)?;
    if same_title_slug || title_slug == id {
        publish_note(dir, &old_path, &contents, true)?;
        return Ok(id.to_owned());
    }
    let new_id = create_unique_note(dir, &title_slug, &contents)?;
    std::fs::remove_file(old_path)?;
    Ok(new_id)
}

fn create_unique_note(
    dir: &std::path::Path,
    base: &str,
    contents: &str,
) -> std::io::Result<String> {
    for suffix in 1u64..=u64::MAX {
        let id = if suffix == 1 {
            base.to_owned()
        } else {
            format!("{base}-{suffix}")
        };
        match publish_note(dir, &dir.join(format!("{id}.md")), contents, false) {
            Ok(()) => return Ok(id),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no note filename is available",
    ))
}

fn publish_note(
    dir: &std::path::Path,
    path: &std::path::Path,
    contents: &str,
    replace: bool,
) -> std::io::Result<()> {
    use std::io::Write;
    static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let (temporary, mut file) = loop {
        let sequence = NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = dir.join(format!(".note-{}-{sequence}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| {
        if replace {
            std::fs::rename(&temporary, path)
        } else {
            std::fs::hard_link(&temporary, path)
        }
    });
    match std::fs::remove_file(&temporary) {
        Ok(()) => result,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => result,
        Err(error) => result.and(Err(error)),
    }
}

fn reserved_file_stem(id: &str) -> bool {
    let stem = id
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
}

fn valid_note_id(id: &str) -> bool {
    !id.is_empty()
        && !reserved_file_stem(id)
        && !id.contains("..")
        && !id.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
        && !id.ends_with(['.', ' '])
        && !id.starts_with('.')
}

/// Deletes a note by id. Returns true when a file was removed.
pub fn delete_note(id: &str) -> bool {
    if !valid_note_id(id) {
        return false;
    }
    std::fs::remove_file(notes_dir().join(format!("{id}.md"))).is_ok()
}

/// Collapses newlines and runs of whitespace so a preview always fits
/// one list row.
fn oneline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn note_result(note: &Note, score: i32) -> SearchResult {
    SearchResult {
        id: format!("notes:{}", note.id),
        title: if note.title.is_empty() {
            "(empty note)".into()
        } else {
            truncate(&oneline(&note.title), 60)
        },
        subtitle: Some(if note.body.trim().is_empty() {
            "Empty".into()
        } else {
            truncate(&oneline(note.body.trim()), 90)
        }),
        icon: Icon::Svg(phosphor_svgs::style::regular::NOTE_PENCIL),
        score,
        accessory: Some(note.created_at.clone()),
        section: None,
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(limit).collect();
        format!("{cut}…")
    }
}

fn create_result(input: &str, score: i32) -> SearchResult {
    SearchResult {
        id: format!("notes:create:{input}"),
        title: format!("New Note “{}”", truncate(input.trim(), 50)),
        subtitle: Some("Opens the editor".into()),
        icon: Icon::Svg(phosphor_svgs::style::regular::PLUS),
        score,
        accessory: None,
        section: None,
    }
}

fn open_result(score: i32) -> SearchResult {
    SearchResult {
        id: "notes:open".into(),
        title: "Notes".into(),
        subtitle: Some("Commands".into()),
        icon: Icon::Svg(phosphor_svgs::style::regular::NOTE_PENCIL),
        score,
        accessory: None,
        section: None,
    }
}

#[derive(Default)]
pub struct NotesCommand;

corvo_core::register_command!(NotesCommand);

#[async_trait::async_trait]
impl Command for NotesCommand {
    fn id(&self) -> &'static str {
        "notes"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["note", "notes", "memo"]
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        // Dedicated page: filter saved notes by the typed text, with a
        // create row that captures the filter itself.
        if let Some(filter) = query.strip_prefix("notes-page:") {
            let needle = filter.trim().to_lowercase();
            let notes = list_notes();
            let mut results: Vec<SearchResult> = notes
                .iter()
                .filter_map(|note| {
                    if needle.is_empty() {
                        return Some(note_result(note, 700));
                    }
                    let score =
                        search_match_score(&needle, &[note.title.as_str(), note.body.as_str()])?;
                    Some(note_result(note, score))
                })
                .collect();
            if !filter.trim().is_empty() {
                results.insert(0, create_result(filter.trim(), 1000));
            }
            results.sort_by_key(|result| std::cmp::Reverse(result.score));
            results.truncate(ctx.max_results);
            return results;
        }

        let trimmed = query.trim();
        if trimmed.is_empty() {
            return vec![open_result(1000)];
        }

        let first = trimmed.split_whitespace().next().unwrap_or_default();
        let lowered = first.to_lowercase();
        if ["note", "notes", "memo"].contains(&lowered.as_str()) {
            let rest = trimmed[first.len()..].trim();
            if rest.is_empty() {
                return vec![open_result(1000)];
            }
            // Quick command: the UI opens its editor with the typed
            // text as the title; this row is the non-UI fallback.
            return vec![create_result(rest, 1000)];
        }

        corvo_core::search_match_score(trimmed, &["Notes", "notes memo quick note"])
            .map(|score| vec![open_result(score + 120)])
            .unwrap_or_default()
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some(key) = result_id.strip_prefix("notes:") else {
            return Err(CommandError::NotFound);
        };
        if key == "open" {
            return Ok(Action::ShowToast("Notes".into()));
        }
        // Creation happens through the UI's editor window; this is the
        // headless fallback (tests, future callers).
        if let Some(text) = key.strip_prefix("create:") {
            let (title, body) = match text.split_once('\n') {
                Some((title, body)) => (title, body),
                None => (text, ""),
            };
            return create_note(title, body)
                .map(|_| Action::ShowToast("Note saved".into()))
                .map_err(|error| CommandError::Platform(error.to_string()));
        }
        if let Some(id) = key.strip_prefix("delete:") {
            return if delete_note(id) {
                Ok(Action::ShowToast("Note deleted".into()))
            } else {
                Err(CommandError::NotFound)
            };
        }
        let notes = list_notes();
        let note = notes
            .iter()
            .find(|note| note.id == key)
            .ok_or(CommandError::NotFound)?;
        Ok(Action::Open(notes_dir().join(format!("{}.md", note.id))))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(key) = result_id.strip_prefix("notes:") else {
            return Vec::new();
        };
        if key.starts_with("create:") || key == "open" {
            return Vec::new();
        }
        let body = list_notes()
            .iter()
            .find(|note| note.id == key)
            .map(|note| note.body.clone())
            .unwrap_or_default();
        vec![
            CommandAction {
                id: "notes:primary-open".into(),
                label: "Open in Corvo Editor".into(),
                action: Action::ShowToast(format!("note-editor:{key}")),
                icon: Icon::Svg(phosphor_svgs::style::regular::ARROW_UP_RIGHT),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            },
            CommandAction {
                id: "notes:copy".into(),
                label: "Copy Note".into(),
                action: Action::ShowToast(format!("copy:{body}")),
                icon: Icon::Svg(phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Standard,
                hotkey: Some("cmd+enter"),
            },
            CommandAction {
                id: format!("notes:delete:{key}"),
                label: "Delete Note".into(),
                action: Action::ShowToast(format!("note-delete:{key}")),
                icon: Icon::Svg(phosphor_svgs::style::regular::TRASH),
                group: ActionGroup::Destructive,
                hotkey: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_keeps_collisions_and_rejects_bad_sources() {
        let dir = std::env::temp_dir().join(format!(
            "corvo-note-storage-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let body = format!("body-{index}");
                    let id = create_unique_note(&dir, "collision", &body).unwrap();
                    (id, body)
                })
            })
            .collect();
        for handle in handles {
            let (id, body) = handle.join().unwrap();
            assert_eq!(
                std::fs::read_to_string(dir.join(format!("{id}.md"))).unwrap(),
                body
            );
        }
        let path = dir.join("saved.md");
        std::fs::write(&path, file_contents("Saved", "2026-01-01 00:00", "old")).unwrap();
        assert_eq!(
            save_note_in(&dir, "saved", "Saved", "new").unwrap(),
            "saved"
        );
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("2026-01-01 00:00"));
        assert!(saved.ends_with("new"));
        std::fs::write(&path, [0xff]).unwrap();
        assert!(save_note_in(&dir, "saved", "Saved", "replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), [0xff]);
        assert!(save_note_in(&dir, "missing", "Missing", "replacement").is_err());
        assert!(!dir.join("missing.md").exists());
        let blocked = dir.join("blocked.md");
        std::fs::create_dir(&blocked).unwrap();
        assert!(publish_note(&dir, &blocked, "replacement", true).is_err());
        assert!(blocked.is_dir());
        assert!(std::fs::read_dir(&dir).unwrap().all(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_none_or(|ext| ext != "tmp")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn slugs_titles_into_file_names() {
        assert_eq!(slug("Buy Milk!"), "buy-milk");
        assert_eq!(slug("  Multiple   Spaces  "), "multiple-spaces");
        assert_eq!(slug("..."), "untitled");
        assert_eq!(slug(""), "untitled");
        for title in ["CON", "PRN", "AUX", "NUL", "COM1", "LPT1"] {
            assert_eq!(slug(title), format!("note-{}", title.to_ascii_lowercase()));
        }
        let long = "a".repeat(80);
        assert_eq!(slug(&long).chars().count(), 40);
    }

    #[test]
    fn parses_every_degenerate_file_without_panicking() {
        let dir = notes_dir();
        std::fs::create_dir_all(&dir).unwrap();

        let empty = parse_note("empty", "", &dir.join("empty.md"));
        assert_eq!(empty.title, "");
        assert_eq!(empty.body, "");

        let header_only = parse_note("header", "---\n---\n", &dir.join("header.md"));
        assert_eq!(header_only.title, "");

        let no_meta = parse_note("bare", "Just a body line", &dir.join("bare.md"));
        assert_eq!(no_meta.title, "Just a body line");

        let with_meta = parse_note(
            "titled",
            "---\ntitle: My Note\ncreated_at: 2026-10-03 00:45\n---\nBody",
            &dir.join("titled.md"),
        );
        assert_eq!(with_meta.title, "My Note");
        assert_eq!(with_meta.created_at, "2026-10-03 00:45");
        assert_eq!(with_meta.body, "Body");

        let blank_meta = parse_note(
            "blank",
            "---\ntitle: \ncreated_at: \n---\n\n",
            &dir.join("blank.md"),
        );
        assert_eq!(blank_meta.title, "");
    }

    #[test]
    fn rejects_path_tricks_in_delete() {
        assert!(!delete_note("../escape"));
        assert!(!delete_note("a/b"));
        for id in [
            "",
            "..",
            "a\\b",
            "C:escape",
            "C:\\escape",
            "\\\\server\\note",
            "a\0b",
        ] {
            assert!(!valid_note_id(id), "accepted {id:?}");
            assert!(save_note(id, "Title", "Body").is_err());
            assert!(!delete_note(id));
        }
        for id in ["CON", "nul", "COM1", "LPT9", "con.extra", "COM¹"] {
            assert!(!valid_note_id(id));
        }
        assert!(valid_note_id("normal-note-2"));
    }

    #[test]
    fn root_routing_creates_and_opens() {
        let search = |query: &str| {
            smol::block_on(<NotesCommand as Command>::search(
                &NotesCommand,
                query,
                &SearchContext::default(),
            ))
        };
        let results = search("note buy milk");
        assert_eq!(results.len(), 1);
        assert!(results[0].id.starts_with("notes:create:buy milk"));

        assert_eq!(search("notes")[0].id, "notes:open");
        assert_eq!(search("")[0].id, "notes:open");
        assert!(search("hello world").is_empty());
    }

    #[test]
    fn create_and_save_roundtrip_with_title_names() {
        let note = create_note("Roundtrip Note", "First body").unwrap();
        assert_eq!(note.id, "roundtrip-note");
        assert_eq!(note.title, "Roundtrip Note");
        assert_eq!(
            save_note(&note.id, "Roundtrip Note", "Updated body").unwrap(),
            note.id
        );

        // Saving with a new title renames the file.
        let new_id = save_note(&note.id, "Renamed Note", "Second body").unwrap();
        assert_eq!(new_id, "renamed-note");
        assert!(!list_notes().iter().any(|saved| saved.id == note.id));
        let saved = list_notes()
            .into_iter()
            .find(|saved| saved.id == "renamed-note")
            .unwrap();
        assert_eq!(saved.body, "Second body");
        assert!(delete_note("renamed-note"));

        // Creating the same title twice gets a suffix.
        let first = create_note("Duplicated", "one").unwrap();
        let second = create_note("Duplicated", "two").unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(
            save_note(&second.id, "Duplicated", "Updated").unwrap(),
            second.id
        );
        assert_eq!(
            save_note(&second.id, "Duplicated", "Updated again").unwrap(),
            second.id
        );
        assert!(delete_note(&first.id));
        assert!(delete_note(&second.id));
    }
}

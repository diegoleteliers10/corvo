//! The in-launcher snippet and quicklink editors.
//!
//! The state and the pure rules live here so both callers — the launcher
//! window and the Settings buttons — share one implementation, and so the
//! key rules and the save rules are testable without a window.

use corvo_config::{AppConfig, QuicklinksFile, Settings, SnippetsFile};

/// Which editor is open in the launcher, and what it holds.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum LauncherEditor {
    #[default]
    Snippet,
    Quicklink,
}

/// An editor currently open in the launcher: which kind, the previous
/// name when editing an existing entry, and the field text.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenEditor {
    pub kind: LauncherEditor,
    pub editing: Option<String>,
    pub buffer: FieldBuffer,
}

impl OpenEditor {
    pub fn new_snippet() -> Self {
        Self {
            kind: LauncherEditor::Snippet,
            editing: None,
            buffer: snippet_buffer("", "", ""),
        }
    }

    pub fn edit_snippet(name: String, keyword: String, body: String) -> Self {
        Self {
            kind: LauncherEditor::Snippet,
            editing: Some(name.clone()),
            buffer: snippet_buffer(&name, &keyword, &body),
        }
    }

    pub fn new_quicklink() -> Self {
        Self {
            kind: LauncherEditor::Quicklink,
            editing: None,
            buffer: quicklink_buffer("", ""),
        }
    }

    pub fn edit_quicklink(name: String, url: String) -> Self {
        Self {
            kind: LauncherEditor::Quicklink,
            editing: Some(name.clone()),
            buffer: quicklink_buffer(&name, &url),
        }
    }

    pub fn title(&self) -> &'static str {
        match (&self.kind, self.editing.is_some()) {
            (LauncherEditor::Snippet, true) => "Edit Snippet",
            (LauncherEditor::Snippet, false) => "New Snippet",
            (LauncherEditor::Quicklink, true) => "Edit Quicklink",
            (LauncherEditor::Quicklink, false) => "New Quicklink",
        }
    }

    pub fn save_label(&self) -> &'static str {
        if self.editing.is_some() {
            "Save"
        } else {
            "Create"
        }
    }

    pub fn field_labels(&self) -> &'static [&'static str] {
        match self.kind {
            LauncherEditor::Snippet => &["Name", "Keyword (optional)", "Body"],
            LauncherEditor::Quicklink => &["Name", "Link"],
        }
    }

    pub fn field_hints(&self) -> &'static [&'static str] {
        match self.kind {
            LauncherEditor::Snippet => &[
                "e.g. Meeting Notes",
                "e.g. notes",
                "Supports {clipboard}, {date}, {time}",
            ],
            LauncherEditor::Quicklink => &["e.g. GitHub", "https://github.com/{argument}"],
        }
    }

    pub fn footer_hint(&self) -> &'static str {
        match self.kind {
            LauncherEditor::Snippet => "Tab switches fields. Enter adds a line in the body.",
            LauncherEditor::Quicklink => "Tab switches fields. Enter saves.",
        }
    }

    /// Applies one keystroke; returns `true` when the editor should save.
    pub fn handle_key(&mut self, key: &str, plain: bool) -> bool {
        self.buffer.handle_key(key, plain)
    }

    /// Builds the draft this editor saves.
    pub fn draft(&self) -> SnippetOrQuicklink {
        match self.kind {
            LauncherEditor::Snippet => SnippetOrQuicklink::Snippet(SnippetDraft::from_buffer(
                self.editing.clone(),
                &self.buffer,
            )),
            LauncherEditor::Quicklink => SnippetOrQuicklink::Quicklink(
                QuicklinkDraft::from_buffer(self.editing.clone(), &self.buffer),
            ),
        }
    }
}

/// The draft an open editor saves, without the caller matching on kind.
pub enum SnippetOrQuicklink {
    Snippet(SnippetDraft),
    Quicklink(QuicklinkDraft),
}

/// Text state for an editor made of simple single-line-per-field buffers.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FieldBuffer {
    pub fields: Vec<String>,
    pub active: usize,
    /// Enter inside the last field adds a newline instead of submitting.
    /// The snippet body is such a field; a quicklink URL is not.
    pub newline_in_last: bool,
}

impl FieldBuffer {
    pub fn new(fields: Vec<String>, newline_in_last: bool) -> Self {
        Self {
            fields,
            active: 0,
            newline_in_last,
        }
    }

    fn in_last(&self) -> bool {
        self.active + 1 == self.fields.len()
    }

    /// Applies one keystroke. Returns `true` when the editor should save.
    /// Unmodified single characters and space insert; backspace deletes;
    /// tab cycles; enter submits, except in a trailing body field, where
    /// it adds a newline. Modifier combos and named keys are ignored.
    pub fn handle_key(&mut self, key: &str, plain: bool) -> bool {
        match key {
            "escape" => false,
            "tab" => {
                self.active = (self.active + 1) % self.fields.len();
                false
            }
            "enter" => {
                if self.newline_in_last && self.in_last() {
                    self.fields[self.active].push('\n');
                    false
                } else {
                    true
                }
            }
            "backspace" => {
                self.fields[self.active].pop();
                false
            }
            "space" => {
                self.fields[self.active].push(' ');
                false
            }
            name if plain && name.chars().count() == 1 => {
                if let Some(ch) = name.chars().next() {
                    self.fields[self.active].push(ch);
                }
                false
            }
            _ => false,
        }
    }
}

/// A snippet as the editor holds it. `editing` carries the previous name,
/// so a rename can move the per-name alias, hotkey and visibility over.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SnippetDraft {
    pub editing: Option<String>,
    pub name: String,
    pub keyword: String,
    pub body: String,
}

impl SnippetDraft {
    pub fn from_buffer(editing: Option<String>, buffer: &FieldBuffer) -> Self {
        Self {
            editing,
            name: buffer.fields[0].trim().to_string(),
            keyword: buffer.fields[1].trim().to_string(),
            body: buffer.fields[2].clone(),
        }
    }

    /// Inserts or updates the snippet and carries the per-name config over
    /// on a rename. Returns `false` for an empty name: there is nothing to
    /// save an unnamed snippet under.
    pub fn apply(
        &self,
        snippets: &mut Vec<corvo_config::SnippetEntry>,
        command_items: &mut std::collections::HashMap<String, AppConfig>,
    ) -> bool {
        let name = self.name.trim();
        if name.is_empty() {
            return false;
        }
        let keyword = self.keyword.trim();
        let keyword = (!keyword.is_empty()).then(|| keyword.to_string());

        let previous = self
            .editing
            .clone()
            .filter(|old| snippets.iter().any(|s| &s.name == old));
        let entry_index = previous
            .as_ref()
            .and_then(|old| snippets.iter().position(|s| &s.name == old));

        match entry_index {
            Some(index) => {
                snippets[index].name = name.to_string();
                snippets[index].keyword = keyword.clone();
                snippets[index].body = self.body.clone();
            }
            None => snippets.push(corvo_config::SnippetEntry {
                name: name.to_string(),
                keyword: keyword.clone(),
                body: self.body.clone(),
            }),
        }

        // Alias, hotkey and visibility are keyed by name, so a rename has
        // to move that entry or the row falls back to defaults.
        if let Some(old) = previous.filter(|old| old != name) {
            if let Some(config) = command_items.remove(&old) {
                command_items.entry(name.to_string()).or_insert(config);
            }
        }
        true
    }
}

/// A quicklink as the editor holds it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct QuicklinkDraft {
    pub editing: Option<String>,
    pub name: String,
    pub url: String,
}

impl QuicklinkDraft {
    pub fn from_buffer(editing: Option<String>, buffer: &FieldBuffer) -> Self {
        Self {
            editing,
            name: buffer.fields[0].trim().to_string(),
            url: buffer.fields[1].trim().to_string(),
        }
    }

    /// Inserts or updates the quicklink in the file, carrying the
    /// settings entry over on a rename. Empty names and links are
    /// rejected: a quicklink without either cannot open.
    pub fn apply(&self, file: &mut QuicklinksFile, settings: &mut Settings) -> bool {
        let name = self.name.trim();
        let url = self.url.trim();
        if name.is_empty() || url.is_empty() {
            return false;
        }

        let previous = self
            .editing
            .clone()
            .filter(|old| file.quicklinks.iter().any(|q| &q.name == old));
        let entry_index = previous
            .as_ref()
            .and_then(|old| file.quicklinks.iter().position(|q| &q.name == old));

        match entry_index {
            Some(index) => {
                file.quicklinks[index].name = name.to_string();
                file.quicklinks[index].url = url.to_string();
            }
            None => file.quicklinks.push(corvo_config::QuicklinkEntry {
                name: name.to_string(),
                url: url.to_string(),
                alias: None,
                hotkey: None,
                hidden: false,
            }),
        }

        if let Some(old) = previous.filter(|old| old != name) {
            if let Some(config) = settings.quicklinks.command_items.remove(&old) {
                settings
                    .quicklinks
                    .command_items
                    .entry(name.to_string())
                    .or_insert(config);
            }
        }
        true
    }
}

/// Builds the field buffer a snippet editor edits: name, keyword, body.
/// The body is the trailing field, so Enter adds a newline inside it.
pub fn snippet_buffer(name: &str, keyword: &str, body: &str) -> FieldBuffer {
    FieldBuffer::new(
        vec![name.to_string(), keyword.to_string(), body.to_string()],
        true,
    )
}

/// Builds the field buffer a quicklink editor edits: name, link.
pub fn quicklink_buffer(name: &str, url: &str) -> FieldBuffer {
    FieldBuffer::new(vec![name.to_string(), url.to_string()], false)
}

/// Loads one snippet's fields for editing, matched by name.
pub fn snippet_fields(file: &SnippetsFile, name: &str) -> Option<(String, String, String)> {
    let snippet = file.snippets.iter().find(|s| s.name == name)?;
    Some((
        snippet.name.clone(),
        snippet.keyword.clone().unwrap_or_default(),
        snippet.body.clone(),
    ))
}

/// Loads one quicklink's fields for editing, matched by name.
pub fn quicklink_fields(file: &QuicklinksFile, name: &str) -> Option<(String, String)> {
    let link = file.quicklinks.iter().find(|q| q.name == name)?;
    Some((link.name.clone(), link.url.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_fills_the_active_field_and_tab_cycles() {
        let mut buffer = snippet_buffer("", "", "");
        for key in ["m", "e", "e", "t"] {
            buffer.handle_key(key, true);
        }
        assert_eq!(buffer.fields[0], "meet");
        assert_eq!(buffer.active, 0);
        buffer.handle_key("tab", false);
        buffer.handle_key("n", true);
        assert_eq!(buffer.fields[1], "n");
        assert_eq!(buffer.active, 1);
    }

    /// Enter in the snippet body adds a line; Enter anywhere else saves.
    #[test]
    fn enter_adds_a_line_only_in_the_body() {
        let mut buffer = snippet_buffer("Meet", "", "line one");
        buffer.active = 2;
        assert!(!buffer.handle_key("enter", false));
        assert_eq!(buffer.fields[2], "line one\n");

        buffer.active = 0;
        assert!(buffer.handle_key("enter", false));
    }

    /// A modifier combo must never land in the text: Cmd+R is a real
    /// shortcut, not the letter r.
    #[test]
    fn modified_keys_do_not_type() {
        let mut buffer = snippet_buffer("", "", "");
        assert!(!buffer.handle_key("r", false));
        assert_eq!(buffer.fields[0], "");
    }

    #[test]
    fn backspace_deletes_from_the_active_field() {
        let mut buffer = quicklink_buffer("Git", "");
        buffer.active = 1;
        buffer.handle_key("backspace", false);
        assert_eq!(buffer.fields[1], "");
        buffer.active = 0;
        buffer.handle_key("backspace", false);
        assert_eq!(buffer.fields[0], "Gi");
    }

    #[test]
    fn an_unnamed_snippet_is_rejected() {
        let mut snippets = Vec::new();
        let mut items = std::collections::HashMap::new();
        let draft = SnippetDraft {
            editing: None,
            name: "   ".into(),
            keyword: String::new(),
            body: "text".into(),
        };
        assert!(!draft.apply(&mut snippets, &mut items));
        assert!(snippets.is_empty());
    }

    #[test]
    fn a_new_snippet_is_appended_with_a_trimmed_keyword() {
        let mut snippets = Vec::new();
        let mut items = std::collections::HashMap::new();
        let draft = SnippetDraft {
            editing: None,
            name: "  Meet  ".into(),
            keyword: " mt ".into(),
            body: "hello".into(),
        };
        assert!(draft.apply(&mut snippets, &mut items));
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].name, "Meet");
        assert_eq!(snippets[0].keyword.as_deref(), Some("mt"));
        assert!(!items.contains_key("Meet"), "a new snippet has no config");
    }

    /// The per-name config follows a rename, or the row would silently
    /// lose its alias, hotkey and visibility.
    #[test]
    fn renaming_a_snippet_carries_its_config_over() {
        let mut snippets = vec![corvo_config::SnippetEntry {
            name: "Old".into(),
            keyword: None,
            body: "b".into(),
        }];
        let mut items = std::collections::HashMap::new();
        items.insert(
            "Old".into(),
            AppConfig {
                alias: Some("zz".into()),
                hotkey: None,
                hidden: false,
            },
        );

        let draft = SnippetDraft {
            editing: Some("Old".into()),
            name: "New".into(),
            keyword: String::new(),
            body: "b".into(),
        };
        assert!(draft.apply(&mut snippets, &mut items));

        assert_eq!(snippets[0].name, "New");
        assert_eq!(items.len(), 1);
        assert_eq!(items["New"].alias.as_deref(), Some("zz"));
        assert!(!items.contains_key("Old"));
    }

    /// Saving without a rename must not disturb the entry.
    #[test]
    fn saving_a_snippet_under_the_same_name_keeps_its_config() {
        let mut snippets = vec![corvo_config::SnippetEntry {
            name: "Same".into(),
            keyword: None,
            body: "old body".into(),
        }];
        let mut items = std::collections::HashMap::new();
        items.insert("Same".into(), AppConfig::default());

        let draft = SnippetDraft {
            editing: Some("Same".into()),
            name: "Same".into(),
            keyword: String::new(),
            body: "new body".into(),
        };
        assert!(draft.apply(&mut snippets, &mut items));
        assert_eq!(snippets[0].body, "new body");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn a_quicklink_without_a_url_is_rejected() {
        let mut file = QuicklinksFile::default();
        let before = file.quicklinks.len();
        let mut settings = Settings::default();
        let draft = QuicklinkDraft {
            editing: None,
            name: "GitHub".into(),
            url: "   ".into(),
        };
        assert!(!draft.apply(&mut file, &mut settings));
        assert_eq!(
            file.quicklinks.len(),
            before,
            "a rejected draft must not add"
        );
    }

    #[test]
    fn renaming_a_quicklink_carries_its_config_over() {
        let mut file = QuicklinksFile::default();
        file.quicklinks.push(corvo_config::QuicklinkEntry {
            name: "Old".into(),
            url: "https://old.example".into(),
            alias: None,
            hotkey: None,
            hidden: false,
        });
        let mut settings = Settings::default();
        settings.quicklinks.command_items.insert(
            "Old".into(),
            AppConfig {
                alias: Some("ol".into()),
                hotkey: None,
                hidden: false,
            },
        );

        let draft = QuicklinkDraft {
            editing: Some("Old".into()),
            name: "New".into(),
            url: "https://new.example".into(),
        };
        assert!(draft.apply(&mut file, &mut settings));

        let renamed = file
            .quicklinks
            .iter()
            .find(|q| q.name == "New")
            .expect("the renamed entry must exist");
        assert_eq!(renamed.url, "https://new.example");
        assert!(
            !file.quicklinks.iter().any(|q| q.name == "Old"),
            "the renamed-away name must not linger"
        );
        assert_eq!(settings.quicklinks.command_items.len(), 1);
        assert_eq!(
            settings.quicklinks.command_items["New"].alias.as_deref(),
            Some("ol")
        );
        assert!(
            !settings.quicklinks.command_items.contains_key("Old"),
            "the renamed-away key must not linger"
        );
    }

    #[test]
    fn editing_an_unknown_name_creates_the_quicklink() {
        let mut file = QuicklinksFile::default();
        let mut settings = Settings::default();
        let draft = QuicklinkDraft {
            editing: Some("Vanished".into()),
            name: "Fresh".into(),
            url: "https://fresh.example".into(),
        };
        assert!(draft.apply(&mut file, &mut settings));
        assert_eq!(
            file.quicklinks.iter().filter(|q| q.name == "Fresh").count(),
            1
        );
    }
}

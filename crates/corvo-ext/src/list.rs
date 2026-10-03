//! The List module: builders for result rows and sections.
//!
//! `SearchResult` is the row type the UI renders; these helpers give
//! every command the same row shapes so results look consistent across
//! extensions — the equivalent of List.Item plus List.Section.

use corvo_core::{Icon, SearchResult};

/// One list row under construction. Field names mirror the row
/// layout: title, subtitle, icon, right accessory, section.
pub struct ListItem {
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Icon,
    pub accessory: Option<String>,
    pub section: Option<String>,
}

impl ListItem {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            icon: Icon::App,
            accessory: None,
            section: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = icon;
        self
    }

    /// Right-aligned hint such as a unit, a count, or a state.
    pub fn accessory(mut self, accessory: impl Into<String>) -> Self {
        self.accessory = Some(accessory.into());
        self
    }

    /// Rows sharing a consecutive section render under one header.
    /// Keep each section contiguous when sorting.
    pub fn section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    /// Finishes the row. `id` must be namespaced:
    /// `{command-id}:{action}[:{data}]`.
    pub fn build(self, id: impl Into<String>, score: i32) -> SearchResult {
        SearchResult {
            id: id.into(),
            title: self.title,
            subtitle: self.subtitle,
            icon: self.icon,
            score,
            accessory: self.accessory,
            section: self.section,
        }
    }
}

/// The standard opener row that exposes a command's dedicated page:
/// one per command, matched by the page `{command-id}:open` id.
pub fn open_entry(id: &str, title: &str, subtitle: &str, icon: Icon, score: i32) -> SearchResult {
    SearchResult {
        id: format!("{id}:open"),
        title: title.to_owned(),
        subtitle: Some(subtitle.to_owned()),
        icon,
        score,
        accessory: None,
        section: None,
    }
}

/// A quiet hint row for an empty page — "loading finished, nothing
/// here" instead of a silently blank list. Score 1 keeps it below any
/// real row.
pub fn empty_state(id: &str, hint: &str) -> SearchResult {
    SearchResult {
        id: format!("{id}:empty"),
        title: hint.to_owned(),
        subtitle: None,
        icon: Icon::System,
        score: 1,
        accessory: None,
        section: None,
    }
}

/// The shared fuzzy tail for root search: score the query against the
/// command's aliases, and answer with the opener row when it matches.
/// `base` is the opener's score when the query is a keyword hit but
/// not a fuzzy one.
pub fn fuzzy_open(
    query: &str,
    aliases: &[&str],
    open: &SearchResult,
) -> Vec<SearchResult> {
    corvo_core::search_match_score(query, aliases)
        .map(|score| {
            let mut row = open.clone();
            row.score = open.score.max(score + 120);
            vec![row]
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_builds_a_namespaced_row() {
        let row = ListItem::new("GitHub")
            .subtitle("github.com")
            .icon(Icon::Web)
            .accessory("Tab")
            .section("Open tabs")
            .build("demo:tab:https://github.com", 700);
        assert_eq!(row.id, "demo:tab:https://github.com");
        assert_eq!(row.title, "GitHub");
        assert_eq!(row.section.as_deref(), Some("Open tabs"));
        assert_eq!(row.accessory.as_deref(), Some("Tab"));
    }

    #[test]
    fn open_entry_namespaces_the_id() {
        let row = open_entry("demo", "Demo", "A demo page", Icon::App, 1000);
        assert_eq!(row.id, "demo:open");
        assert_eq!(row.title, "Demo");
    }

    #[test]
    fn fuzzy_open_scores_or_silences() {
        let open = open_entry("weather", "Weather", "Forecast", Icon::System, 900);
        let hit = fuzzy_open("wather", &["Weather", "forecast clima"], &open);
        assert_eq!(hit.len(), 1);
        assert!(hit[0].score >= 120);

        assert!(fuzzy_open("zzz", &["Weather"], &open).is_empty());
    }

    #[test]
    fn empty_state_scores_below_everything() {
        let hint = empty_state("demo", "Nothing here yet");
        assert_eq!(hint.score, 1);
        assert_eq!(hint.id, "demo:empty");
    }
}

//! Search files under the scopes configured in Settings.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use corvo_core::{
    search_match_score, Action, Command, CommandError, ExecutionContext, Icon, SearchContext,
    SearchResult,
};

const MAX_VISITED_ENTRIES: usize = 20_000;

#[derive(Default)]
pub struct FileSearchCommand;

corvo_core::register_command!(FileSearchCommand);

#[async_trait::async_trait]
impl Command for FileSearchCommand {
    fn id(&self) -> &'static str {
        "file-search"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["file", "files", "find", "documents"]
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let Some(store) = ctx.store.as_ref() else {
            return Vec::new();
        };
        let options = store.file_search_options();
        let query = query.trim().to_lowercase();
        if !options.enabled || query.is_empty() {
            return Vec::new();
        }

        let max_results = ctx.max_results;
        smol::unblock(move || search_files(query, options, max_results)).await
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let path = result_id
            .strip_prefix("file-search:")
            .ok_or(CommandError::NotFound)?;
        let path = PathBuf::from(path);
        if path.is_file() {
            Ok(Action::Open(path))
        } else {
            Err(CommandError::NotFound)
        }
    }
}

fn search_files(
    query: String,
    options: corvo_core::FileSearchOptions,
    max_results: usize,
) -> Vec<SearchResult> {
    let scopes = options
        .search_scopes
        .iter()
        .filter_map(|scope| expand_scope(scope));
    let mut pending: VecDeque<PathBuf> = scopes.take(MAX_VISITED_ENTRIES).collect();
    let mut visited = 0;
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    while let Some(path) = pending.pop_front() {
        if visited >= MAX_VISITED_ENTRIES {
            break;
        }
        visited += 1;
        if !seen.insert(path.clone()) {
            continue;
        }
        if is_ignored(&path, &options.ignore_patterns) {
            continue;
        }

        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let Ok(children) = std::fs::read_dir(&path) else {
                continue;
            };
            let remaining_capacity = MAX_VISITED_ENTRIES.saturating_sub(visited + pending.len());
            pending.extend(
                children
                    .flatten()
                    .take(remaining_capacity)
                    .map(|entry| entry.path()),
            );
            continue;
        }
        if !metadata.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let full_path = path.to_string_lossy();
        let name_lower = file_name.to_lowercase();
        let path_lower = full_path.to_lowercase();
        let Some(score) = search_match_score(&query, &[&name_lower, &path_lower]) else {
            continue;
        };
        results.push(SearchResult {
            id: format!("file-search:{}", full_path),
            title: file_name.to_string(),
            subtitle: Some(full_path.to_string()),
            icon: Icon::File,
            score,
            accessory: None,
        });
    }

    results.sort_by(|left, right| right.score.total_cmp(&left.score));
    results.truncate(max_results);
    results
}

fn expand_scope(scope: &str) -> Option<PathBuf> {
    if scope == "~" {
        return std::env::var_os("HOME").map(PathBuf::from);
    }
    if let Some(rest) = scope.strip_prefix("~/") {
        return std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest));
    }
    Some(PathBuf::from(scope))
}

fn is_ignored(path: &Path, patterns: &[String]) -> bool {
    let path = path.to_string_lossy();
    let segments = path.split(std::path::MAIN_SEPARATOR);
    patterns
        .iter()
        .filter(|pattern| !pattern.is_empty())
        .any(|pattern| segments.clone().any(|segment| segment == pattern.as_str()))
}

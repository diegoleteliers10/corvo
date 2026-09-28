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
        let query = query.trim();
        if !options.enabled {
            return Vec::new();
        }

        if let Some(page_query) = query.strip_prefix("file-search-page:") {
            let page_query = page_query.trim().to_lowercase();
            if page_query == "index" {
                return smol::unblock(move || search_file_index(options)).await;
            }
            let max_results = ctx.max_results;
            return smol::unblock(move || search_files(page_query, options, max_results)).await;
        }

        let Some(result) = search_files_command_result(query) else {
            return Vec::new();
        };
        vec![result]
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        if result_id == "file-search:open" {
            return Ok(Action::OpenFileSearch);
        }
        let path = result_id
            .strip_prefix("file-search:")
            .ok_or(CommandError::NotFound)?;
        let path = PathBuf::from(path);
        if path.exists() {
            Ok(Action::Open(path))
        } else {
            Err(CommandError::NotFound)
        }
    }
}

fn search_files_command_result(query: &str) -> Option<SearchResult> {
    let score = search_match_score(
        query,
        &[
            "Search Files",
            "file search",
            "file",
            "files",
            "find files",
            "documents",
        ],
    )?;
    Some(SearchResult {
        id: "file-search:open".into(),
        title: "Search Files".into(),
        subtitle: Some("Find files and folders".into()),
        icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::MAGNIFYING_GLASS),
        score: score + 600,
        accessory: Some("Command".into()),
    })
}

fn search_files(
    query: String,
    options: corvo_core::FileSearchOptions,
    max_results: usize,
) -> Vec<SearchResult> {
    if query.is_empty() {
        return search_files_shallow(&options, max_results);
    }

    search_files_recursive(query, options, max_results)
}

fn search_file_index(options: corvo_core::FileSearchOptions) -> Vec<SearchResult> {
    search_files_recursive(String::new(), options, MAX_VISITED_ENTRIES)
}

fn search_files_recursive(
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
        let is_dir = metadata.is_dir();
        if is_dir {
            if let Ok(children) = std::fs::read_dir(&path) {
                let remaining_capacity =
                    MAX_VISITED_ENTRIES.saturating_sub(visited + pending.len());
                pending.extend(
                    children
                        .flatten()
                        .take(remaining_capacity)
                        .map(|entry| entry.path()),
                );
            }
        } else if !metadata.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let full_path = path.to_string_lossy();
        let name_lower = file_name.to_lowercase();
        let path_lower = full_path.to_lowercase();
        let score = if query.is_empty() {
            0
        } else {
            let Some(score) = search_match_score(&query, &[&name_lower, &path_lower]) else {
                continue;
            };
            score
        };
        results.push(file_search_result(&path, score, is_dir));
    }

    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
    results.truncate(max_results);
    results
}

fn search_files_shallow(
    options: &corvo_core::FileSearchOptions,
    max_results: usize,
) -> Vec<SearchResult> {
    let mut results = Vec::new();
    for scope in options
        .search_scopes
        .iter()
        .filter_map(|scope| expand_scope(scope))
    {
        if results.len() >= max_results {
            break;
        }
        if is_ignored(&scope, &options.ignore_patterns) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&scope) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            results.push(file_search_result(&scope, 0, false));
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(scope) else {
            continue;
        };
        for entry in children.flatten().take(max_results - results.len()) {
            let path = entry.path();
            if is_ignored(&path, &options.ignore_patterns) {
                continue;
            }
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() || metadata.is_file() {
                results.push(file_search_result(&path, 0, metadata.is_dir()));
            }
        }
    }
    results.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    results.truncate(max_results);
    results
}

fn file_search_result(path: &Path, score: i32, is_dir: bool) -> SearchResult {
    let title = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let full_path = path.to_string_lossy().to_string();
    SearchResult {
        id: format!("file-search:{full_path}"),
        title,
        subtitle: Some(full_path),
        icon: if is_dir {
            Icon::Svg(corvo_core::phosphor_svgs::style::fill::FOLDER)
        } else {
            Icon::File
        },
        score,
        accessory: Some(if is_dir { "Folder" } else { "File" }.into()),
    }
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

#[cfg(test)]
mod tests {
    use super::search_files_command_result;

    #[test]
    fn root_search_opens_files_page_for_matching_query() {
        let result = search_files_command_result("files");
        assert!(result.is_some_and(|result| result.title == "Search Files"));
    }

    #[test]
    fn root_search_hides_file_command_for_unrelated_query() {
        assert!(search_files_command_result("zzzz-unrelated").is_none());
    }
}

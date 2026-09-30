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
        // A plain word, not a shortcut: no key opens this page, and a
        // Windows user reading "Command" would look for Cmd.
        accessory: Some(if cfg!(target_os = "macos") {
            "Command".into()
        } else {
            "Files".into()
        }),
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
        if is_link(&metadata) {
            continue;
        }
        let is_dir = metadata.is_dir();
        if is_dir {
            if is_reparse_point(&path) {
                // A junction or mount point can point back into the tree
                // and send the walk in circles, burning the whole budget
                // before any real file is reached.
                continue;
            }
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
        if is_link(&metadata) {
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
            if is_link(&metadata) {
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

/// The user's home directory.
///
/// Windows sets `USERPROFILE` and leaves `HOME` unset for a process
/// started from Explorer, so reading only `HOME` makes the default scope
/// resolve to nothing and the page stays empty. Try `HOME` first because
/// a Linux or macOS shell that overrides it should win.
fn home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        return Some(PathBuf::from(home));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE").filter(|home| !home.is_empty()) {
        return Some(PathBuf::from(profile));
    }
    // HOMEDRIVE and HOMEPATH are the legacy pair Windows still sets.
    let drive = std::env::var_os("HOMEDRIVE").filter(|value| !value.is_empty());
    let path = std::env::var_os("HOMEPATH").filter(|value| !value.is_empty());
    match (drive, path) {
        (Some(drive), Some(path)) => Some(PathBuf::from(drive).join(path)),
        _ => None,
    }
}

/// Expands a scope to a path.
///
/// Handles `~`, `~/name`, `%NAME%` and `$NAME`, then returns the input
/// unchanged when nothing applies.
fn expand_scope(scope: &str) -> Option<PathBuf> {
    let scope = scope.trim();
    if scope.is_empty() {
        return None;
    }
    if scope == "~" {
        return home_dir();
    }
    if let Some(rest) = scope
        .strip_prefix("~/")
        .or_else(|| scope.strip_prefix("~\\"))
    {
        return home_dir().map(|home| home.join(rest));
    }
    if let Some(expanded) = expand_env_vars(scope) {
        return Some(expanded);
    }
    Some(PathBuf::from(scope))
}

/// Expands `%NAME%` and `$NAME` references, or returns `None` when the
/// string names a variable that is not set.
///
/// `$$` and `%%` are literal percent and dollar signs, so a path that
/// really contains one still resolves.
fn expand_env_vars(scope: &str) -> Option<PathBuf> {
    let mut expanded = String::with_capacity(scope.len());
    let mut chars = scope.chars().peekable();
    let mut saw_variable = false;
    while let Some(character) = chars.next() {
        // Look for a variable opener. `%%` and `$$` are escapes.
        let Some(closing) = (match character {
            '%' if chars.peek() == Some(&'%') => {
                chars.next();
                expanded.push('%');
                continue;
            }
            '%' => Some('%'),
            '$' if chars.peek() == Some(&'$') => {
                chars.next();
                expanded.push('$');
                continue;
            }
            // `${NAME}` is the braced shell form. A bare `$NAME` is left
            // alone: it collides with folder names such as
            // `$Recycle.Bin`, and the braced form covers every real use.
            '$' if chars.peek() == Some(&'{') => {
                chars.next();
                Some('}')
            }
            // Not a variable reference: emit the character as written.
            _ => {
                expanded.push(character);
                continue;
            }
        }) else {
            expanded.push(character);
            continue;
        };
        let mut name = String::new();
        let mut closed = false;
        for next in chars.by_ref() {
            if next == closing {
                closed = true;
                break;
            }
            name.push(next);
        }
        if !closed || name.is_empty() {
            // An unterminated reference is a literal path.
            expanded.push(character);
            if closing == '}' {
                expanded.push('{');
            }
            expanded.push_str(&name);
            continue;
        }
        let value = std::env::var_os(&name)?;
        expanded.push_str(&value.to_string_lossy());
        saw_variable = true;
    }
    saw_variable.then(|| PathBuf::from(expanded))
}

fn is_ignored(path: &Path, patterns: &[String]) -> bool {
    let path = path.to_string_lossy();
    let segments = path.split(std::path::MAIN_SEPARATOR);
    patterns
        .iter()
        .filter(|pattern| !pattern.is_empty())
        .any(|pattern| {
            segments
                .clone()
                .any(|segment| segment.eq_ignore_ascii_case(pattern.as_str()))
        })
}

/// True when the entry is a symbolic link.
fn is_link(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

/// True when the directory is a Windows reparse point: a junction, a
/// symbolic link, or a volume mount. `symlink_metadata` reports a
/// junction as a plain directory, so the walk needs this extra check to
/// avoid following one back into itself.
#[cfg(target_os = "windows")]
fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const INVALID_FILE_ATTRIBUTES: u32 = 0xffff_ffff;
    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileAttributesW(name: *const u16) -> u32;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// No reparse points outside Windows.
#[cfg(not(target_os = "windows"))]
fn is_reparse_point(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{expand_env_vars, expand_scope, is_ignored, search_files_command_result};
    use std::path::Path;

    #[test]
    fn root_search_opens_files_page_for_matching_query() {
        let result = search_files_command_result("files");
        assert!(result.is_some_and(|result| result.title == "Search Files"));
    }

    #[test]
    fn root_search_hides_file_command_for_unrelated_query() {
        assert!(search_files_command_result("zzzz-unrelated").is_none());
    }

    #[test]
    fn tilde_resolves_on_every_platform() {
        // Windows sets USERPROFILE, not HOME. Reading HOME alone left the
        // default scope unresolvable and the page permanently empty.
        let scope = expand_scope("~");
        assert!(
            scope.is_some_and(|path| path.is_dir()),
            "the home scope must resolve to a real directory"
        );
    }

    #[test]
    fn tilde_with_a_child_resolves() {
        let path = expand_scope("~/Documents").expect("resolves");
        assert!(
            path.to_string_lossy().contains("Documents"),
            "got {}",
            path.display()
        );
    }

    #[test]
    fn absolute_scope_passes_through() {
        let path = expand_scope("/tmp").expect("resolves");
        assert_eq!(path, Path::new("/tmp"));
    }

    #[test]
    fn blank_scope_resolves_to_nothing() {
        assert!(expand_scope("   ").is_none());
    }

    #[test]
    fn env_vars_expand() {
        // The tests set their own variable, so this holds on every
        // platform without touching the real environment.
        // SAFETY: single-threaded test setup before any parallel read.
        unsafe { std::env::set_var("CORVO_TEST_SCOPE", "/tmp/corvo-scope") };
        assert_eq!(
            expand_env_vars("%CORVO_TEST_SCOPE%/sub").as_deref(),
            Some(Path::new("/tmp/corvo-scope/sub"))
        );
        assert_eq!(
            expand_env_vars("${CORVO_TEST_SCOPE}/sub").as_deref(),
            Some(Path::new("/tmp/corvo-scope/sub"))
        );
    }

    #[test]
    fn bare_dollar_is_literal() {
        // `$NAME` without braces stays literal, so a folder called
        // `$Recycle.Bin` still resolves.
        assert!(expand_env_vars("$Recycle.Bin").is_none());
        assert_eq!(
            expand_scope("$Recycle.Bin").as_deref(),
            Some(Path::new("$Recycle.Bin"))
        );
    }

    #[test]
    fn escaped_percent_is_literal() {
        // A path can contain a percent sign. `%%` must not be read as a
        // variable opener.
        assert!(expand_env_vars("100%%/report").is_none());
        assert_eq!(
            expand_scope("100%%/report").as_deref(),
            Some(Path::new("100%%/report"))
        );
    }

    #[test]
    fn unknown_env_var_does_not_resolve() {
        // A literal path that names no variable must stay untouched, so
        // a folder really called `%FOO%` still works.
        assert!(expand_env_vars("%CORVO_MISSING_SCOPE%/x").is_none());
    }

    #[test]
    fn ignore_matching_ignores_case() {
        // NTFS and APFS are both case insensitive, so a folder named
        // Node_Modules must match the pattern node_modules.
        let patterns = vec!["node_modules".to_string()];
        assert!(is_ignored(Path::new("/home/u/Node_Modules/pkg"), &patterns));
        assert!(is_ignored(Path::new("/home/u/NODE_MODULES/pkg"), &patterns));
        assert!(!is_ignored(Path::new("/home/u/src/pkg"), &patterns));
    }
}

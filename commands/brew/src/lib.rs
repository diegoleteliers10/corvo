use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult,
};

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
enum BrewItemKind {
    Formula,
    Cask,
    Service,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BrewItem {
    name: String,
    version: Option<String>,
    latest_version: Option<String>,
    kind: BrewItemKind,
    status: Option<String>,
}

type BrewOutputParser = fn(&str) -> Option<Vec<BrewItem>>;

#[derive(Clone, Debug)]
struct CachedItems {
    updated_at: Instant,
    items: Vec<BrewItem>,
    error: Option<String>,
}

#[derive(Default)]
struct BrewCache {
    entries: HashMap<String, CachedItems>,
}

fn cache() -> &'static Mutex<BrewCache> {
    static CACHE: OnceLock<Mutex<BrewCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BrewCache::default()))
}

pub fn invalidate_cache() {
    if let Ok(mut state) = cache().lock() {
        state.entries.clear();
    }
}

#[derive(Default)]
pub struct BrewCommand;

#[cfg(target_os = "macos")]
corvo_core::register_command!(BrewCommand);

#[async_trait::async_trait]
impl Command for BrewCommand {
    fn id(&self) -> &'static str {
        "brew"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "brew",
            "homebrew",
            "formula",
            "cask",
            "outdated",
            "installed",
        ]
    }

    fn prefix(&self) -> Option<&'static str> {
        Some("brew")
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let Some((mode, search_query)) = parse_query(query) else {
            return Vec::new();
        };
        if mode == QueryMode::Menu {
            return menu_results();
        }
        let key = cache_key(&mode, &search_query);
        let (items, error) = items_for_query(key, mode, search_query.clone()).await;
        if let Some(error) = error {
            return vec![SearchResult {
                id: "brew:unavailable".into(),
                title: "Homebrew unavailable".into(),
                subtitle: Some(error),
                icon: Icon::System,
                score: 900,
                accessory: None,
                section: None,
            }];
        }
        results_for_items(items, mode, &search_query, ctx.max_results)
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some((_, kind, name)) = result_id.strip_prefix("brew:").and_then(parse_result_id)
        else {
            return Err(CommandError::NotFound);
        };
        if !safe_package_name(name) {
            return Err(CommandError::NotFound);
        }
        let path = match kind {
            "formula" => format!("/formula/{name}"),
            "cask" => format!("/cask/{name}"),
            _ => return Err(CommandError::NotFound),
        };
        Ok(Action::OpenUrl(format!("https://formulae.brew.sh{path}")))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let menu_action = match result_id {
            "brew:upgrade-all" => Some(("Upgrade All", vec!["upgrade"])),
            "brew:clear-cache" => Some(("Clear Cache", vec!["cleanup", "--prune=all"])),
            "brew:clean-up" => Some(("Clean Up", vec!["cleanup"])),
            _ => None,
        };
        if let Some((label, args)) = menu_action {
            return vec![CommandAction {
                id: format!("brew:run:{}", label.to_lowercase().replace(' ', "-")),
                label: label.into(),
                action: Action::RunProcess {
                    program: brew_executable(),
                    args: args.into_iter().map(str::to_owned).collect(),
                    title: format!("Homebrew {label}"),
                },
                icon: Icon::Glyph("🍺"),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            }];
        }
        let Some((mode, kind, name)) = result_id.strip_prefix("brew:").and_then(parse_result_id)
        else {
            return Vec::new();
        };
        if !safe_package_name(name) {
            return Vec::new();
        }
        if mode == "services" {
            let (label, operation) = if kind == "started" {
                ("Stop Service", "stop")
            } else {
                ("Start Service", "start")
            };
            return vec![
                service_action(label, operation, name, ActionGroup::Primary),
                service_action("Restart Service", "restart", name, ActionGroup::Standard),
            ];
        }
        let page_path = match kind {
            "formula" => format!("https://formulae.brew.sh/formula/{name}"),
            "cask" => format!("https://formulae.brew.sh/cask/{name}"),
            _ => return Vec::new(),
        };
        let mut actions = vec![CommandAction {
            id: "brew:open-page".into(),
            label: "Open Homebrew Page".into(),
            action: Action::OpenUrl(page_path),
            icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::ARROW_UP_RIGHT),
            group: ActionGroup::Primary,
            hotkey: Some("enter"),
        }];
        let operation = match mode {
            "outdated" => Some(("Upgrade", "upgrade")),
            "search" => Some(("Install", "install")),
            _ => None,
        };
        if let Some((label, operation)) = operation {
            let mut args = vec![operation.to_string()];
            if kind == "cask" {
                args.push("--cask".into());
            }
            args.push(name.to_string());
            actions.push(CommandAction {
                id: format!("brew:{operation}"),
                label: format!("{label} {name}"),
                action: Action::RunProcess {
                    program: brew_executable(),
                    args,
                    title: format!("Homebrew {label}"),
                },
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::DOWNLOAD_SIMPLE),
                group: ActionGroup::Standard,
                hotkey: None,
            });
        }
        actions
    }
}

fn service_action(
    label: &str,
    operation: &str,
    service: &str,
    group: ActionGroup,
) -> CommandAction {
    CommandAction {
        id: format!("brew:services:{operation}:{service}"),
        label: format!("{label} {service}"),
        action: Action::RunProcess {
            program: brew_executable(),
            args: vec!["services".into(), operation.into(), service.into()],
            title: format!("Homebrew {label}"),
        },
        icon: Icon::System,
        group,
        hotkey: None,
    }
}

fn results_for_items(
    items: Vec<BrewItem>,
    mode: QueryMode,
    query: &str,
    max_results: usize,
) -> Vec<SearchResult> {
    let query_text = query.to_lowercase();
    let mut results: Vec<SearchResult> = items
        .into_iter()
        .filter_map(|item| {
            let score = if query_text.is_empty() {
                700
            } else {
                corvo_core::search_match_score(&query_text, &[item.name.as_str()])?
            };
            let (kind, kind_segment, icon) = match item.kind {
                BrewItemKind::Formula => ("Formula", "formula", Icon::Glyph("📦")),
                BrewItemKind::Cask => ("Cask", "cask", Icon::App),
                BrewItemKind::Service => ("Service", "service", Icon::System),
            };
            let subtitle = if mode == QueryMode::Outdated {
                // The section header names the kind and the accessory
                // carries the version jump; a subtitle would repeat
                // both.
                None
            } else if let Some(status) = item.status.as_deref() {
                Some(format!("Service · {status}"))
            } else {
                match (&item.version, &item.latest_version) {
                    (Some(current), Some(latest)) if current != latest => {
                        Some(format!("{kind} · {current} → {latest}"))
                    }
                    (Some(current), _) => Some(format!("{kind} · {current}")),
                    _ => Some(kind.to_string()),
                }
            };
            // Upgrade lists group formulae before casks; other modes
            // stay flat.
            let section = match (mode, kind_segment) {
                (QueryMode::Outdated, "formula") => Some("Formulae".into()),
                (QueryMode::Outdated, "cask") => Some("Casks".into()),
                _ => None,
            };
            Some(SearchResult {
                id: format!(
                    "brew:{}:{}:{}",
                    mode_name(mode),
                    if mode == QueryMode::Services {
                        item.status.as_deref().unwrap_or("none")
                    } else {
                        kind_segment
                    },
                    item.name
                ),
                title: item.name,
                subtitle,
                icon,
                score,
                accessory: match mode {
                    QueryMode::Outdated => match (&item.version, &item.latest_version) {
                        (Some(current), Some(latest)) => Some(format!("{current} → {latest}")),
                        _ => Some("Upgrade".into()),
                    },
                    QueryMode::Search => Some("Install".into()),
                    QueryMode::Services => Some(
                        if item.status.as_deref() == Some("started") {
                            "Stop"
                        } else {
                            "Start"
                        }
                        .into(),
                    ),
                    _ => None,
                },
                section,
            })
        })
        .collect();
    results.sort_by(|left, right| {
        // Sections stay contiguous so the UI groups them under one
        // header: formulae first, then casks.
        section_rank(left).cmp(&section_rank(right)).then_with(|| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.cmp(&right.title))
        })
    });
    results.truncate(max_results);
    results
}

fn mode_name(mode: QueryMode) -> &'static str {
    match mode {
        QueryMode::Menu => "menu",
        QueryMode::Installed => "installed",
        QueryMode::Outdated => "outdated",
        QueryMode::Search => "search",
        QueryMode::Services => "services",
    }
}

/// Sort order of a section label: formulae before casks, everything
/// else alongside the unsectioned rows.
fn section_rank(result: &SearchResult) -> u8 {
    match result.section.as_deref() {
        Some("Formulae") => 0,
        Some("Casks") => 1,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryMode {
    Menu,
    Installed,
    Outdated,
    Search,
    Services,
}

fn parse_query(query: &str) -> Option<(QueryMode, String)> {
    if let Some(page_query) = query.strip_prefix("brew-page:") {
        let (mode, rest) = page_query.split_once(':')?;
        let mode = match mode {
            "installed" => QueryMode::Installed,
            "upgrades" => QueryMode::Outdated,
            "search" => QueryMode::Search,
            "services" => QueryMode::Services,
            _ => return None,
        };
        return Some((mode, rest.to_string()));
    }
    let command = query.trim().strip_prefix("brew")?.trim();
    if command.is_empty() {
        return Some((QueryMode::Menu, String::new()));
    }
    let (mode, rest) = if let Some(rest) = command
        .strip_prefix("installed")
        .filter(|rest| token_boundary(rest))
    {
        (QueryMode::Installed, rest)
    } else if let Some(rest) = command
        .strip_prefix("outdated")
        .filter(|rest| token_boundary(rest))
    {
        (QueryMode::Outdated, rest)
    } else {
        let rest = command
            .strip_prefix("search")
            .filter(|rest| token_boundary(rest))?;
        (QueryMode::Search, rest)
    };
    Some((mode, rest.trim().to_string()))
}

fn token_boundary(rest: &str) -> bool {
    rest.chars().next().is_none_or(char::is_whitespace)
}

fn cache_key(mode: &QueryMode, query: &str) -> String {
    match mode {
        QueryMode::Menu => "menu".into(),
        QueryMode::Installed => "installed".into(),
        QueryMode::Outdated => "outdated".into(),
        QueryMode::Search => format!("search:{}", query.to_lowercase()),
        QueryMode::Services => "services".into(),
    }
}

fn menu_results() -> Vec<SearchResult> {
    [
        ("brew:clear-cache", "Clear Cache"),
        ("brew:show-upgrades", "Show Upgrades"),
        ("brew:upgrade-all", "Upgrade All"),
        ("brew:clean-up", "Clean Up"),
        ("brew:manage-services", "Manage Services"),
        ("brew:search", "Search"),
        ("brew:show-installed", "Show Installed"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, title))| SearchResult {
        id: id.into(),
        title: title.into(),
        subtitle: None,
        icon: Icon::Glyph("🍺"),
        score: 900 - index as i32,
        accessory: Some("Brew".into()),
        section: None,
    })
    .collect()
}

async fn items_for_query(
    key: String,
    mode: QueryMode,
    query: String,
) -> (Vec<BrewItem>, Option<String>) {
    let cached = cache().lock().ok().and_then(|state| {
        state.entries.get(&key).and_then(|entry| {
            (entry.updated_at.elapsed() < CACHE_TTL)
                .then(|| (entry.items.clone(), entry.error.clone()))
        })
    });
    if let Some(cached) = cached {
        return cached;
    }

    let loaded = smol::unblock(move || load_items(&mode, &query)).await;
    let (items, error) = match loaded {
        Ok(items) => (items, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    if let Ok(mut state) = cache().lock() {
        state.entries.insert(
            key,
            CachedItems {
                updated_at: Instant::now(),
                items: items.clone(),
                error: error.clone(),
            },
        );
    }
    (items, error)
}

fn load_items(mode: &QueryMode, query: &str) -> Result<Vec<BrewItem>, String> {
    let (args, parser): (&[&str], BrewOutputParser) = match mode {
        QueryMode::Menu => return Ok(Vec::new()),
        QueryMode::Installed => (&["info", "--json=v2", "--installed"], parse_installed),
        QueryMode::Outdated => (&["outdated", "--json=v2"], parse_outdated),
        QueryMode::Services => {
            return run_brew(&["services", "list", "--json"]).and_then(|output| {
                parse_services(&output)
                    .ok_or_else(|| "Homebrew returned invalid service data".into())
            });
        }
        QueryMode::Search => {
            if query.is_empty() {
                return Ok(Vec::new());
            }
            let formula_output = run_brew_search("--formula", query)?;
            let cask_output = run_brew_search("--cask", query)?;
            return parse_search(&formula_output, &cask_output)
                .ok_or_else(|| "Homebrew returned invalid search results".into());
        }
    };
    let output = run_brew(args)?;
    parser(&output).ok_or_else(|| "Homebrew returned invalid JSON".into())
}

/// Homebrew queries run while the user types; anything slower than this
/// is treated as hung and killed instead of parking the task forever.
const BREW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

fn run_brew(args: &[&str]) -> Result<String, String> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    corvo_platform::run_process_with_timeout(&brew_executable(), &owned, BREW_TIMEOUT)
        .map_err(|error| format!("Could not run brew: {error}"))
        .and_then(|output| {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(if error.is_empty() {
                    format!("brew exited with {}", output.status)
                } else {
                    error
                })
            }
        })
}

fn run_brew_search(scope: &str, query: &str) -> Result<String, String> {
    run_brew(&["search", scope, query]).or_else(|error| {
        // A search with no hits exits non-zero with this message; that
        // is an empty result set, not a failure.
        if is_empty_search_result(&error) {
            Ok(String::new())
        } else {
            Err(error)
        }
    })
}

fn is_empty_search_result(error: &str) -> bool {
    error
        .to_ascii_lowercase()
        .contains("no formulae or casks found")
}

fn brew_executable() -> String {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .map(str::to_owned)
        .unwrap_or_else(|| "brew".to_string())
}

fn parse_installed(json: &str) -> Option<Vec<BrewItem>> {
    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut items = Vec::new();
    for formula in root.get("formulae")?.as_array()? {
        let name = formula.get("name")?.as_str()?.to_string();
        let version = formula
            .get("installed")
            .and_then(serde_json::Value::as_array)
            .and_then(|installed| installed.first())
            .and_then(|entry| entry.get("version"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        items.push(BrewItem {
            name,
            version,
            latest_version: None,
            kind: BrewItemKind::Formula,
            status: None,
        });
    }
    for cask in root.get("casks")?.as_array()? {
        items.push(BrewItem {
            name: cask.get("token")?.as_str()?.to_string(),
            version: cask
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            latest_version: None,
            kind: BrewItemKind::Cask,
            status: None,
        });
    }
    Some(items)
}

fn parse_outdated(json: &str) -> Option<Vec<BrewItem>> {
    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut items = Vec::new();
    for formula in root.get("formulae")?.as_array()? {
        let installed_versions = formula
            .get("installed_versions")
            .and_then(serde_json::Value::as_array);
        items.push(BrewItem {
            name: formula.get("name")?.as_str()?.to_string(),
            version: installed_versions
                .and_then(|versions| versions.first())
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            latest_version: formula
                .get("current_version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            kind: BrewItemKind::Formula,
            status: None,
        });
    }
    for cask in root.get("casks")?.as_array()? {
        let installed_versions = cask
            .get("installed_versions")
            .and_then(serde_json::Value::as_array);
        items.push(BrewItem {
            name: cask.get("name")?.as_str()?.to_string(),
            version: installed_versions
                .and_then(|versions| versions.first())
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            latest_version: cask
                .get("current_version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            kind: BrewItemKind::Cask,
            status: None,
        });
    }
    Some(items)
}

fn parse_search(formula_output: &str, cask_output: &str) -> Option<Vec<BrewItem>> {
    let mut items = Vec::new();
    for name in search_result_names(formula_output) {
        items.push(BrewItem {
            name,
            version: None,
            latest_version: None,
            kind: BrewItemKind::Formula,
            status: None,
        });
    }
    for name in search_result_names(cask_output) {
        items.push(BrewItem {
            name,
            version: None,
            latest_version: None,
            kind: BrewItemKind::Cask,
            status: None,
        });
    }
    Some(items)
}

fn search_result_names(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("==>")
                && !line.starts_with("No formulae or casks")
        })
        .map(str::to_owned)
        .collect()
}

fn parse_services(json: &str) -> Option<Vec<BrewItem>> {
    let services: serde_json::Value = serde_json::from_str(json).ok()?;
    services
        .as_array()?
        .iter()
        .map(|service| {
            Some(BrewItem {
                name: service.get("name")?.as_str()?.to_string(),
                version: None,
                latest_version: None,
                kind: BrewItemKind::Service,
                status: Some(service.get("status")?.as_str()?.to_string()),
            })
        })
        .collect::<Option<Vec<_>>>()
}

fn parse_result_id(id: &str) -> Option<(&str, &str, &str)> {
    let (mode, item) = id.split_once(':')?;
    let (kind, name) = item.split_once(':')?;
    if !matches!(mode, "installed" | "outdated" | "search" | "services") {
        return None;
    }
    Some((mode, kind, name))
}

fn safe_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b'@')
        })
        && !name.starts_with('/')
        && !name.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_modes_and_rejects_unrelated_queries() {
        assert_eq!(parse_query("brew"), Some((QueryMode::Menu, String::new())));
        assert_eq!(
            parse_query("brew installed"),
            Some((QueryMode::Installed, String::new()))
        );
        assert_eq!(
            parse_query("brew-page:upgrades:firefox"),
            Some((QueryMode::Outdated, "firefox".into()))
        );
        assert_eq!(
            parse_query("brew search firefox"),
            Some((QueryMode::Search, "firefox".into()))
        );
        assert_eq!(parse_query("brew install firefox"), None);
        assert_eq!(parse_query("firefox"), None);
    }

    #[test]
    fn parses_formula_and_cask_search_output() {
        let items =
            parse_search("==> Formulae\nripgrep\n", "==> Casks\nfirefox\n").expect("search output");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "ripgrep");
        assert_eq!(items[0].kind, BrewItemKind::Formula);
        assert_eq!(items[1].name, "firefox");
        assert_eq!(items[1].kind, BrewItemKind::Cask);
    }

    #[test]
    fn parses_outdated_versions() {
        let items = parse_outdated(
            r#"{"formulae":[{"name":"ripgrep","installed_versions":["14.0"],"current_version":"14.1"}],"casks":[]}"#,
        )
        .expect("valid json");
        assert_eq!(items[0].version.as_deref(), Some("14.0"));
        assert_eq!(items[0].latest_version.as_deref(), Some("14.1"));
    }

    #[test]
    fn parses_services_and_their_status() {
        let items = parse_services(r#"[{"name":"postgresql@18","status":"started"}]"#)
            .expect("valid service json");
        assert_eq!(items[0].kind, BrewItemKind::Service);
        assert_eq!(items[0].status.as_deref(), Some("started"));
    }

    #[test]
    fn menu_shows_brew_actions_in_launcher_order() {
        let results = menu_results();
        assert_eq!(results[0].title, "Clear Cache");
        assert!(results
            .iter()
            .any(|result| result.title == "Show Installed"));
        assert!(results.iter().any(|result| result.title == "Upgrade All"));
    }

    #[test]
    fn validates_names_before_building_brew_urls() {
        assert!(safe_package_name("font-thing@2"));
        assert!(!safe_package_name("../../etc/passwd"));
    }

    #[test]
    fn treats_homebrew_no_match_error_as_empty_results() {
        assert!(is_empty_search_result(
            "Error: No formulae or casks found for \"corvo-no-match\"."
        ));
        assert!(!is_empty_search_result("Error: tap unavailable"));
    }

    #[test]
    fn searches_items_with_match_and_non_match() {
        let items = vec![BrewItem {
            name: "ripgrep".into(),
            version: None,
            latest_version: None,
            kind: BrewItemKind::Formula,
            status: None,
        }];
        let matches = results_for_items(items.clone(), QueryMode::Search, "rip", 10);
        let misses = results_for_items(items, QueryMode::Search, "firefox", 10);
        assert_eq!(matches.len(), 1);
        assert!(misses.is_empty());
    }
}

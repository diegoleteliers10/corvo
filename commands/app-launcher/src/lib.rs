//! Launch installed applications.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    search_match_score, SearchContext, SearchResult,
};
use corvo_platform::AppEntry;

#[derive(Default)]
pub struct AppLauncherCommand;

corvo_core::register_command!(AppLauncherCommand);

#[async_trait::async_trait]
impl Command for AppLauncherCommand {
    fn id(&self) -> &'static str {
        "app-launcher"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["app", "apps", "application", "launch", "open"]
    }

    fn priority(&self) -> u8 {
        70
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let settings = corvo_config::Settings::load();
        if !settings.applications.enabled {
            return Vec::new();
        }

        ensure_corpus();
        let all_apps = corpus_apps();
        if all_apps.is_empty() {
            return Vec::new();
        }

        let app_configs = &settings.applications.app_configs;
        let apps: Vec<AppEntry> = all_apps
            .into_iter()
            .filter(|app| {
                let hidden = app_configs
                    .get(&app.name)
                    .or_else(|| app_configs.get(&app.path.display().to_string()))
                    .is_some_and(|cfg| cfg.hidden);
                !hidden
            })
            .collect();
        if apps.is_empty() {
            return Vec::new();
        }

        let mut frequencies = frequencies()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ensure_seed_frequencies(&mut frequencies);
        let ranked = rank(&apps, query, &frequencies, app_configs);

        if query.is_empty() {
            ranked
                .into_iter()
                .map(|entry| SearchResult {
                    id: format!("app-launcher:{}", entry.path.display()),
                    title: entry.name.clone(),
                    subtitle: None,
                    icon: entry
                        .icon_png
                        .clone()
                        .map(Icon::Image)
                        .unwrap_or(Icon::App),
                    score: 1100,
                    accessory: Some("Application".into()),
                })
                .collect()
        } else {
            ranked
                .into_iter()
                .take(ctx.max_results)
                .map(|entry| {
                    let key = entry.path.display().to_string();
                    let is_recent = frequencies.get(&key).is_some_and(|f| f.0 > 0);
                    let alias = app_configs
                        .get(&entry.name)
                        .or_else(|| app_configs.get(&key))
                        .and_then(|config| config.alias.as_deref())
                        .unwrap_or("");
                    let score = if alias.is_empty() {
                        search_match_score(query, &[entry.name.as_str()])
                    } else {
                        search_match_score(query, &[entry.name.as_str(), alias])
                    }
                    .unwrap_or(700)
                        + if is_recent { 20 } else { 0 };
                    SearchResult {
                        id: format!("app-launcher:{}", entry.path.display()),
                        title: entry.name.clone(),
                        subtitle: None,
                        icon: entry
                            .icon_png
                            .clone()
                            .map(Icon::Image)
                            .unwrap_or(Icon::App),
                        score,
                        accessory: Some("Application".into()),
                    }
                })
                .collect()
        }
    }

    async fn execute(&self, result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let path = result_id
            .strip_prefix("app-launcher:recent:")
            .or_else(|| result_id.strip_prefix("app-launcher:"))
            .ok_or(CommandError::NotFound)?;
        bump_frequency(path);
        Ok(Action::Open(PathBuf::from(path)))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(path) = result_id
            .strip_prefix("app-launcher:recent:")
            .or_else(|| result_id.strip_prefix("app-launcher:"))
            .map(PathBuf::from)
        else {
            return Vec::new();
        };
        let mut actions = vec![
            CommandAction {
                id: "app-launcher-action:open".into(),
                label: "Open Application".into(),
                action: Action::Open(path.clone()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::APP_WINDOW),
                group: ActionGroup::Primary,
                hotkey: Some("↵"),
            },
            CommandAction {
                id: "app-launcher-action:reveal".into(),
                label: "Show in Finder".into(),
                action: Action::RunShell(format!("open -R \"{}\"", path.to_string_lossy())),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::FOLDER),
                group: ActionGroup::Standard,
                hotkey: Some("⌘↵"),
            },
        ];
        if corvo_platform::supports_app_uninstall()
            && path.extension().is_some_and(|extension| extension == "app")
            && !path.starts_with("/System/")
        {
            actions.push(CommandAction {
                id: "app-launcher-action:uninstall".into(),
                label: "Uninstall application".into(),
                action: Action::OpenAppUninstaller {
                    name: app_name_for_path(&path),
                    path,
                },
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::TRASH),
                group: ActionGroup::Destructive,
                hotkey: None,
            });
        }
        actions
    }
}

fn app_name_for_path(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Application")
        .to_string()
}

/// Shared corpus state. A background scan fills the list once; searches
/// read whatever is present and never block.
fn corpus() -> &'static RwLock<CorpusState> {
    static CORPUS: OnceLock<RwLock<CorpusState>> = OnceLock::new();
    CORPUS.get_or_init(|| RwLock::new(CorpusState {
        apps: Vec::new(),
        scopes: Vec::new(),
        scanned_at: None,
        scanning: false,
    }))
}

struct CorpusState {
    apps: Vec<AppEntry>,
    scopes: Vec<String>,
    scanned_at: Option<Instant>,
    scanning: bool,
}

pub fn warmup() {
    ensure_corpus();
}

fn ensure_corpus() {
    let mut state = corpus().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    let settings = corvo_config::Settings::load();
    let scan_is_fresh = state
        .scanned_at
        .is_some_and(|scanned_at| scanned_at.elapsed().as_secs() < 30);
    if scan_is_fresh && state.scopes == settings.applications.search_scopes {
        return;
    }
    state.apps = corvo_platform::list_apps_in_scopes(&settings.applications.search_scopes).unwrap_or_default();
    state.scopes = settings.applications.search_scopes;
    state.scanned_at = Some(Instant::now());
    state.scanning = false;
}

pub fn reload_corpus() {
    let mut state = corpus().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    let settings = corvo_config::Settings::load();
    state.apps = corvo_platform::list_apps_in_scopes(&settings.applications.search_scopes).unwrap_or_default();
    state.scopes = settings.applications.search_scopes;
    state.scanned_at = Some(Instant::now());
    state.scanning = false;
}

fn corpus_apps() -> Vec<AppEntry> {
    corpus()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .apps
        .clone()
}

/// Launch counts, in memory. Persistent frecency lands in phase 3 with
/// the LMDB store (SPEC §7).
fn frequencies() -> &'static Mutex<HashMap<String, (u64, u64)>> {
    static FREQUENCIES: OnceLock<Mutex<HashMap<String, (u64, u64)>>> = OnceLock::new();
    FREQUENCIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn bump_frequency(path: &str) {
    let mut frequencies = frequencies().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let entry = frequencies.entry(path.to_string()).or_insert((0, 0));
    entry.0 += 1;
    entry.1 = now;
}

fn ensure_seed_frequencies(frequencies: &mut HashMap<String, (u64, u64)>) {
    if !frequencies.is_empty() {
        return;
    }
    let default_recent = [
        "/System/Applications/System Settings.app",
        "/System/Applications/Utilities/Terminal.app",
        "/System/Applications/Safari.app",
        "/Applications/Safari.app",
    ];
    let mut time = 100;
    for path in default_recent {
        if std::path::Path::new(path).exists() {
            frequencies.insert(path.to_string(), (1, time));
            time += 1;
        }
    }
}

/// Ranks the corpus: fuzzy matches for a query, launch counts for an
/// empty one.
fn rank<'a>(
    apps: &'a [AppEntry],
    query: &str,
    frequencies: &HashMap<String, (u64, u64)>,
    app_configs: &HashMap<String, corvo_config::AppConfig>,
) -> Vec<&'a AppEntry> {
    if query.is_empty() {
        let mut ranked: Vec<(&AppEntry, (u64, u64))> = apps
            .iter()
            .map(|entry| {
                let key = entry.path.display().to_string();
                (entry, *frequencies.get(&key).unwrap_or(&(0, 0)))
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.1 .0.cmp(&a.1 .0)
                .then_with(|| b.1 .1.cmp(&a.1 .1))
                .then_with(|| corvo_core::search::natural_cmp(&a.0.name, &b.0.name))
        });
        return ranked.into_iter().map(|(entry, _)| entry).collect();
    }

    let q_text = corvo_core::search::SearchText::new(query);
    let mut scored: Vec<(&'a AppEntry, i32)> = Vec::new();

    for entry in apps {
        let alias = app_configs
            .get(&entry.name)
            .or_else(|| app_configs.get(&entry.path.display().to_string()))
            .and_then(|cfg| cfg.alias.as_deref())
            .unwrap_or("");

        let name_text = corvo_core::search::SearchText::new(&entry.name);
        let alias_text = corvo_core::search::SearchText::new(alias);
        let mut fields = vec![(corvo_core::search::FieldRole::Name, &name_text)];
        if !alias.is_empty() {
            fields.push((corvo_core::search::FieldRole::Alias, &alias_text));
        }

        if let Some(quality) = corvo_core::search::quality(&q_text, &fields) {
            scored.push((entry, quality));
        } else if let Some(dp_score) = corvo_core::search::match_launcher_dp(
            &q_text,
            &name_text,
            corvo_core::search::SearchSensitivity::Medium,
        ) {
            scored.push((entry, 1000 + dp_score));
        }
    }

    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| corvo_core::search::natural_cmp(&a.0.name, &b.0.name))
    });

    scored.into_iter().map(|(entry, _)| entry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, path: &str) -> AppEntry {
        AppEntry { name: name.into(), path: PathBuf::from(path), icon_png: None }
    }

    #[test]
    fn empty_query_orders_by_launch_count() {
        let apps = vec![app("A", "/A"), app("B", "/B"), app("C", "/C")];
        let mut frequencies = HashMap::new();
        frequencies.insert("/B".to_string(), (3, 100));
        frequencies.insert("/A".to_string(), (1, 200));

        let ranked = rank(&apps, "", &frequencies, &HashMap::new());
        let names: Vec<&str> = ranked.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["B", "A", "C"]);
    }

    #[test]
    fn query_matches_names_fuzzily() {
        let apps = vec![app("Safari", "/Safari"), app("Notes", "/Notes")];

        let ranked = rank(&apps, "safr", &HashMap::new(), &HashMap::new());
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].name, "Safari");
    }

    #[test]
    fn query_matches_alias_first() {
        let apps = vec![app("Google Chrome", "/Chrome"), app("Calculator", "/Calc")];
        let mut app_configs = HashMap::new();
        app_configs.insert(
            "Calculator".to_string(),
            corvo_config::AppConfig {
                alias: Some("c".into()),
                hotkey: None,
                hidden: false,
            },
        );

        let ranked = rank(&apps, "c", &HashMap::new(), &app_configs);
        assert_eq!(ranked[0].name, "Calculator");
    }

    #[test]
    fn query_with_no_match_is_empty() {
        let apps = vec![app("Safari", "/Safari")];
        assert!(rank(&apps, "zzz", &HashMap::new(), &HashMap::new()).is_empty());
    }
}

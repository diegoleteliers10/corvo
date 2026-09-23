//! Launch installed applications.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult,
};
use corvo_platform::{platform_ops, AppEntry};

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
        ensure_corpus();
        let apps = corpus_apps();
        if apps.is_empty() {
            return Vec::new();
        }

        let mut frequencies = frequencies()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ensure_seed_frequencies(&mut frequencies);
        let ranked = rank(&apps, query, &frequencies);

        if query.is_empty() {
            let mut results = Vec::new();
            let recent_limit = 5.min(ranked.len());
            for (idx, entry) in ranked.iter().enumerate() {
                let key = entry.path.display().to_string();
                let has_freq = frequencies.get(&key).map_or(false, |f| f.0 > 0);
                if idx < recent_limit && has_freq {
                    results.push(SearchResult {
                        id: format!("app-launcher:recent:{}", entry.path.display()),
                        title: entry.name.clone(),
                        subtitle: None,
                        icon: entry
                            .icon_png
                            .clone()
                            .map(Icon::Image)
                            .unwrap_or(Icon::App),
                        score: 120.0,
                        accessory: Some("Recent".into()),
                    });
                } else {
                    results.push(SearchResult {
                        id: format!("app-launcher:{}", entry.path.display()),
                        title: entry.name.clone(),
                        subtitle: None,
                        icon: entry
                            .icon_png
                            .clone()
                            .map(Icon::Image)
                            .unwrap_or(Icon::App),
                        score: 110.0,
                        accessory: Some("Application".into()),
                    });
                }
            }
            results
        } else {
            ranked
                .into_iter()
                .take(ctx.max_results)
                .map(|entry| {
                    let key = entry.path.display().to_string();
                    let is_recent = frequencies.get(&key).map_or(false, |f| f.0 > 0);
                    SearchResult {
                        id: format!("app-launcher:{}", entry.path.display()),
                        title: entry.name.clone(),
                        subtitle: None,
                        icon: entry
                            .icon_png
                            .clone()
                            .map(Icon::Image)
                            .unwrap_or(Icon::App),
                        score: if is_recent { 105.0 } else { 100.0 },
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
        let parent = path
            .parent()
            .map(|dir| dir.to_path_buf())
            .unwrap_or_else(|| path.clone());
        vec![
            CommandAction {
                id: "app-launcher-action:open".into(),
                label: "Open Application".into(),
                action: Action::Open(path.clone()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::ARROW_UP_RIGHT),
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
            CommandAction {
                id: "app-launcher-action:terminal".into(),
                label: "Open in Terminal".into(),
                action: Action::RunShell(format!("open -a Terminal \"{}\"", parent.to_string_lossy())),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::TERMINAL_WINDOW),
                group: ActionGroup::Standard,
                hotkey: Some("⌃↵"),
            },
            CommandAction {
                id: "app-launcher-action:copy-path".into(),
                label: "Copy Path".into(),
                action: Action::Copy(path.to_string_lossy().to_string()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Standard,
                hotkey: Some("⌥⌘C"),
            },
        ]
    }
}

/// Shared corpus state. A background scan fills the list once; searches
/// read whatever is present and never block.
fn corpus() -> &'static RwLock<CorpusState> {
    static CORPUS: OnceLock<RwLock<CorpusState>> = OnceLock::new();
    CORPUS.get_or_init(|| RwLock::new(CorpusState { apps: Vec::new(), scanning: false }))
}

struct CorpusState {
    apps: Vec<AppEntry>,
    scanning: bool,
}

pub fn warmup() {
    ensure_corpus();
}

fn ensure_corpus() {
    let mut state = corpus().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !state.apps.is_empty() {
        return;
    }
    state.apps = platform_ops().list_apps().unwrap_or_default();
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
                .then_with(|| a.0.name.to_lowercase().cmp(&b.0.name.to_lowercase()))
        });
        return ranked.into_iter().map(|(entry, _)| entry).collect();
    }

    let names: Vec<&str> = apps.iter().map(|entry| entry.name.as_str()).collect();
    let mut matcher = frizbee::Matcher::new(query, &frizbee::Config::default());
    let mut matches: Vec<frizbee::Match> = matcher.match_list(&names).to_vec();
    matches.sort();
    matches
        .iter()
        .filter_map(|matched| apps.get(matched.index as usize))
        .collect()
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

        let ranked = rank(&apps, "", &frequencies);
        let names: Vec<&str> = ranked.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["B", "A", "C"]);
    }

    #[test]
    fn query_matches_names_fuzzily() {
        let apps = vec![app("Safari", "/Safari"), app("Notes", "/Notes")];

        let ranked = rank(&apps, "safr", &HashMap::new());
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].name, "Safari");
    }

    #[test]
    fn query_with_no_match_is_empty() {
        let apps = vec![app("Safari", "/Safari")];
        assert!(rank(&apps, "zzz", &HashMap::new()).is_empty());
    }
}

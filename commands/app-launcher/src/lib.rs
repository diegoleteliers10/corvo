//! Launch installed applications.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use corvo_core::{
    search_match_score, Action, ActionGroup, Command, CommandAction, CommandError,
    ExecutionContext, Icon, SearchContext, SearchResult,
};
use corvo_platform::AppEntry;

#[cfg(target_os = "windows")]
#[derive(serde::Serialize, serde::Deserialize)]
struct CachedCorpus {
    version: u8,
    scopes: Vec<String>,
    apps: Vec<AppEntry>,
}

#[cfg(target_os = "windows")]
fn corpus_cache_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Corvo/app-corpus.json"))
}

#[cfg(target_os = "windows")]
fn read_cached_corpus() -> Option<CachedCorpus> {
    let bytes = std::fs::read(corpus_cache_path()?).ok()?;
    let mut cached: CachedCorpus = serde_json::from_slice(&bytes).ok()?;
    if cached.version != 1
        || cached.scopes != corvo_config::Settings::load().applications.search_scopes
    {
        return None;
    }
    cached.apps.retain(app_path_is_present);
    for app in &mut cached.apps {
        if app.icon_png.as_ref().is_some_and(|path| !path.is_file()) {
            app.icon_png = None;
        }
    }
    Some(cached)
}

#[cfg(target_os = "windows")]
fn write_cached_corpus(scopes: &[String], apps: &[AppEntry]) {
    let Some(path) = corpus_cache_path() else {
        return;
    };
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let cached = CachedCorpus {
        version: 1,
        scopes: scopes.to_vec(),
        apps: apps.to_vec(),
    };
    let Ok(bytes) = serde_json::to_vec(&cached) else {
        return;
    };
    let _ = std::fs::write(path, bytes);
}

/// An entry the scanner found but whose icon is still being decoded, so it
/// is not ready to publish yet.
///
/// A `shell:AppsFolder` entry is a Store app, and that path is not a file
/// on disk, so it cannot be checked with `is_file`. Store apps are
/// re-derived from `Get-StartApps` on every scan, so they are always
/// current and never need carrying forward.
#[cfg(any(target_os = "windows", test))]
fn is_pending_icon_decode(app: &AppEntry) -> bool {
    app.icon_png.is_none() && !is_store_app_entry(app)
}

#[cfg(any(target_os = "windows", test))]
fn is_store_app_entry(app: &AppEntry) -> bool {
    app.path.to_string_lossy().starts_with("shell:AppsFolder\\")
}

#[cfg(target_os = "windows")]
fn publish_ready_apps(scopes: &[String], apps: &[AppEntry]) {
    let mut state = corpus()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.apps = ready_apps(apps, &state.apps);
    state.scopes = scopes.to_vec();
    drop(state);
    notify_corpus_subscribers();
}

#[cfg(any(target_os = "windows", test))]
fn ready_apps(apps: &[AppEntry], previous: &[AppEntry]) -> Vec<AppEntry> {
    let mut ready = apps
        .iter()
        .filter(|app| app.icon_png.as_ref().is_some_and(|path| path.is_file()))
        .cloned()
        .collect::<Vec<_>>();
    let ready_paths = ready
        .iter()
        .map(|app| app.path.clone())
        .collect::<std::collections::HashSet<_>>();
    let pending_paths = apps
        .iter()
        .filter(|app| is_pending_icon_decode(app))
        .map(|app| &app.path)
        .collect::<std::collections::HashSet<_>>();
    ready.extend(
        previous
            .iter()
            .filter(|app| !ready_paths.contains(&app.path))
            .filter(|app| pending_paths.contains(&app.path))
            .filter(|app| app_path_is_present(app))
            .cloned(),
    );
    ready
}

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
                    icon: entry.icon_png.clone().map(Icon::Image).unwrap_or(Icon::App),
                    score: 1100,
                    accessory: Some("Application".into()),
                    section: None,
                    accessories: Vec::new(),
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
                        icon: entry.icon_png.clone().map(Icon::Image).unwrap_or(Icon::App),
                        score,
                        accessory: Some("Application".into()),
                        section: None,
                        accessories: Vec::new(),
                    }
                })
                .collect()
        }
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
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
        let mut actions = vec![CommandAction {
            id: "app-launcher-action:open".into(),
            label: "Open Application".into(),
            action: Action::Open(path.clone()),
            icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::APP_WINDOW),
            group: ActionGroup::Primary,
            hotkey: Some("enter"),
        }];
        if can_reveal(&path) {
            actions.push(CommandAction {
                id: "app-launcher-action:reveal".into(),
                label: reveal_label().into(),
                action: reveal_action(&path),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::FOLDER),
                group: ActionGroup::Primary,
                hotkey: Some(reveal_hotkey()),
            });
        }
        if corvo_platform::supports_app_uninstall_path(&path) {
            actions.push(CommandAction {
                id: "app-launcher-action:uninstall".into(),
                label: "Uninstall application".into(),
                action: Action::OpenAppUninstaller {
                    name: app_name_for_path(&path),
                    path,
                },
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::TRASH),
                group: ActionGroup::Destructive,
                // No hint: Enter belongs to the primary action, and a
                // destructive row should not advertise it.
                hotkey: None,
            });
        }
        actions
    }
}

/// A Start app id has no file on disk, so there is nothing to reveal.
fn can_reveal(path: &std::path::Path) -> bool {
    !path.to_string_lossy().starts_with("shell:AppsFolder\\")
}

/// Reveal runs the secondary action, which the launcher binds to the
/// platform's primary modifier plus Enter. Written once and translated
/// when drawn, so Windows and Linux read `Ctrl+Enter`.
fn reveal_hotkey() -> &'static str {
    "cmd+enter"
}

#[cfg(target_os = "macos")]
fn reveal_label() -> &'static str {
    "Show in Finder"
}

#[cfg(target_os = "windows")]
fn reveal_label() -> &'static str {
    "Show in Explorer"
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal_label() -> &'static str {
    "Show in Files"
}

#[cfg(target_os = "macos")]
fn reveal_action(path: &std::path::Path) -> Action {
    Action::RunShell(format!("open -R \"{}\"", path.to_string_lossy()))
}

/// `explorer /select` highlights the file and opens its folder in one step.
#[cfg(target_os = "windows")]
fn reveal_action(path: &std::path::Path) -> Action {
    Action::RunShell(format!(
        "explorer.exe /select,\"{}\"",
        path.to_string_lossy()
    ))
}

/// No file manager on Linux takes a reveal argument, so open the folder.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal_action(path: &std::path::Path) -> Action {
    let folder = path.parent().unwrap_or(path);
    Action::RunShell(format!("xdg-open \"{}\"", folder.to_string_lossy()))
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
    CORPUS.get_or_init(|| {
        #[cfg(target_os = "windows")]
        let cached = read_cached_corpus();
        RwLock::new(CorpusState {
            #[cfg(target_os = "windows")]
            apps: cached
                .as_ref()
                .map(|entry| entry.apps.clone())
                .unwrap_or_default(),
            #[cfg(not(target_os = "windows"))]
            apps: Vec::new(),
            #[cfg(target_os = "windows")]
            scopes: cached.map(|entry| entry.scopes).unwrap_or_default(),
            #[cfg(not(target_os = "windows"))]
            scopes: Vec::new(),
            scanned_at: None,
            scanning: false,
            pending_reload: false,
        })
    })
}

fn corpus_subscribers() -> &'static Mutex<Vec<smol::channel::Sender<()>>> {
    static SUBSCRIBERS: OnceLock<Mutex<Vec<smol::channel::Sender<()>>>> = OnceLock::new();
    SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn subscribe_corpus_changes() -> smol::channel::Receiver<()> {
    let (sender, receiver) = smol::channel::unbounded();
    let mut subscribers = corpus_subscribers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    subscribers.retain(|subscriber| !subscriber.is_closed());
    subscribers.push(sender.clone());
    drop(subscribers);
    if corpus()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .scanned_at
        .is_some()
    {
        let _ = sender.try_send(());
    }
    receiver
}

struct CorpusState {
    apps: Vec<AppEntry>,
    scopes: Vec<String>,
    scanned_at: Option<Instant>,
    scanning: bool,
    pending_reload: bool,
}

pub fn warmup() {
    ensure_corpus();
}

fn ensure_corpus() {
    start_scan(false);
}

fn start_scan(force: bool) {
    let scopes = corvo_config::Settings::load().applications.search_scopes;
    let mut state = corpus()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let scan_is_fresh = state
        .scanned_at
        .is_some_and(|scanned_at| scanned_at.elapsed().as_secs() < 300);
    if state.scanning {
        state.pending_reload |= force || state.scopes != scopes;
        return;
    }
    if !force && scan_is_fresh && state.scopes == scopes {
        return;
    }
    state.scanning = true;
    drop(state);
    std::thread::spawn(move || {
        let scanned = corvo_platform::list_apps_in_scopes(&scopes);
        #[cfg(target_os = "windows")]
        {
            let previous_apps = corpus_apps();
            let mut apps = match scanned {
                Ok(apps) => apps,
                Err(error) => {
                    corvo_platform::diagnostics::record_error("applications", "scan_failed");
                    eprintln!("corvo: could not scan applications: {error}");
                    previous_apps
                }
            };
            // Store apps are not carried forward from the previous scan.
            // `append_start_apps` re-reads them from `Get-StartApps` below,
            // which is authoritative, and re-adding the old ones kept an
            // uninstalled Store app in the list forever.
            publish_ready_apps(&scopes, &apps);
            notify_corpus_subscribers();
            corvo_platform::hydrate_shortcut_icons(&mut apps);
            publish_ready_apps(&scopes, &apps);
            notify_corpus_subscribers();
            corvo_platform::append_start_apps(&mut apps);
            corvo_platform::hydrate_start_app_icons(&mut apps);
            publish_ready_apps(&scopes, &apps);
            let mut state = corpus()
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.apps = apps.clone();
            state.scopes = scopes.clone();
            state.scanned_at = Some(Instant::now());
            drop(state);
            write_cached_corpus(&scopes, &apps);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut state = corpus()
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match scanned {
                Ok(apps) => state.apps = apps,
                Err(error) => {
                    corvo_platform::diagnostics::record_error("applications", "scan_failed");
                    eprintln!("corvo: could not scan applications: {error}");
                }
            }
            state.scopes = scopes.clone();
            state.scanned_at = Some(Instant::now());
            drop(state);
        }
        notify_corpus_subscribers();

        let mut state = corpus()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.scanning = false;
        let pending_reload = std::mem::take(&mut state.pending_reload);
        drop(state);
        if pending_reload {
            start_scan(true);
        } else {
            ensure_corpus();
        }
    });
}

fn notify_corpus_subscribers() {
    let mut subscribers = corpus_subscribers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    subscribers.retain(|sender| sender.try_send(()).is_ok());
}

pub fn reload_corpus() {
    start_scan(true);
}

pub fn cached_apps() -> Vec<AppEntry> {
    corpus_apps()
}

/// Removes app entries after the platform removes their files.
pub fn remove_cached_apps(paths: &[PathBuf]) {
    let mut state = corpus()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.apps.retain(|app| !paths.contains(&app.path));
    #[cfg(target_os = "windows")]
    write_cached_corpus(&state.scopes, &state.apps);
    drop(state);
    notify_corpus_subscribers();
}

fn app_path_is_present(app: &AppEntry) -> bool {
    if app.path.to_string_lossy().starts_with("shell:AppsFolder\\") {
        return true;
    }
    app.path.try_exists().unwrap_or(true)
}

fn corpus_apps() -> Vec<AppEntry> {
    let mut state = corpus()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.apps.retain(app_path_is_present);
    state.apps.clone()
}

/// Launch counts, in memory. Persistent frecency lands in phase 3 with
/// the LMDB store (SPEC §7).
fn frequencies() -> &'static Mutex<HashMap<String, (u64, u64)>> {
    static FREQUENCIES: OnceLock<Mutex<HashMap<String, (u64, u64)>>> = OnceLock::new();
    FREQUENCIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn bump_frequency(path: &str) {
    let mut frequencies = frequencies()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let entry = frequencies.entry(path.to_string()).or_insert((0, 0));
    entry.0 += 1;
    entry.1 = now;
}

pub fn record_launch(result_id: &str) {
    if let Some(path) = result_id
        .strip_prefix("app-launcher:recent:")
        .or_else(|| result_id.strip_prefix("app-launcher:"))
    {
        bump_frequency(path);
    }
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
            b.1 .0
                .cmp(&a.1 .0)
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
        AppEntry {
            name: name.into(),
            path: PathBuf::from(path),
            icon_png: None,
        }
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
    fn deleted_app_paths_are_not_present() {
        let directory = std::env::temp_dir().join(format!(
            "corvo-app-launcher-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        for name in ["Removed.app", "removed.desktop", "Removed.lnk"] {
            let path = directory.join(name);
            std::fs::write(&path, []).unwrap();
            let entry = AppEntry {
                name: name.into(),
                path: path.clone(),
                icon_png: None,
            };
            assert!(app_path_is_present(&entry));
            std::fs::remove_file(&path).unwrap();
            assert!(!app_path_is_present(&entry));
        }
        std::fs::remove_dir(&directory).unwrap();
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn corpus_reads_and_removal_drop_deleted_app_entries() {
        let path = std::env::temp_dir().join(format!(
            "corvo-corpus-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, []).unwrap();
        let entry = AppEntry {
            name: "Removed".into(),
            path: path.clone(),
            icon_png: None,
        };
        let saved = {
            let mut state = corpus().write().unwrap();
            std::mem::replace(&mut state.apps, vec![entry.clone()])
        };
        assert_eq!(cached_apps().len(), 1);
        std::fs::remove_file(&path).unwrap();
        assert!(cached_apps().is_empty());
        corpus().write().unwrap().apps = vec![entry];
        remove_cached_apps(&[path]);
        assert!(corpus().read().unwrap().apps.is_empty());
        corpus().write().unwrap().apps = saved;
    }

    #[test]
    fn partial_publish_drops_apps_absent_from_the_scan() {
        let previous = app("Old", ".");
        assert!(ready_apps(&[], &[previous]).is_empty());
    }

    #[test]
    fn partial_publish_keeps_a_scanned_app_until_its_icon_is_ready() {
        let previous = app("Present", ".");
        let scanned = previous.clone();
        let ready = ready_apps(&[scanned], &[previous]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "Present");
    }

    #[test]
    fn store_app_ids_do_not_require_a_file() {
        let entry = app(
            "Store",
            r"shell:AppsFolder\Microsoft.Store_8wekyb3d8bbwe!App",
        );
        assert!(is_store_app_entry(&entry));
        assert!(app_path_is_present(&entry));
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

    #[test]
    fn reveal_is_hidden_for_start_apps() {
        // A Start app id has no file on disk, so a file manager cannot
        // reveal it. The action must not appear rather than fail at run
        // time.
        assert!(!can_reveal(std::path::Path::new(
            r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        )));
        assert!(can_reveal(std::path::Path::new(
            r"C:\Program Files\7-Zip\7zFM.exe"
        )));
    }

    #[test]
    fn reveal_action_targets_the_file_manager() {
        #[cfg(target_os = "windows")]
        let path = std::path::Path::new(r"C:\Program Files\7-Zip\7zFM.exe");
        #[cfg(target_os = "macos")]
        let path = std::path::Path::new("/Applications/7zFM.app");
        #[cfg(target_os = "linux")]
        let path = std::path::Path::new("/usr/share/applications/7zFM.desktop");
        let Action::RunShell(command) = reveal_action(path) else {
            panic!("reveal must run a shell command");
        };
        #[cfg(target_os = "macos")]
        assert!(
            command.starts_with("open -R ") && command.contains("7zFM.app"),
            "got {command}"
        );
        #[cfg(target_os = "windows")]
        assert!(
            command.contains("explorer.exe /select,") && command.contains("7zFM.exe"),
            "got {command}"
        );
        #[cfg(target_os = "linux")]
        assert!(
            command.starts_with("xdg-open ") && command.contains("/usr/share/applications"),
            "got {command}"
        );
    }

    #[test]
    fn reveal_label_and_hotkey_are_not_macos_only() {
        // These strings render in the actions menu, so a macOS-only word
        // on another platform is a visible bug.
        let label = reveal_label();
        #[cfg(target_os = "macos")]
        assert_eq!(label, "Show in Finder");
        #[cfg(target_os = "windows")]
        assert_eq!(label, "Show in Explorer");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(label, "Show in Files");
    }

    #[test]
    fn reveal_hotkey_names_the_bound_secondary_action() {
        // The secondary action is bound to the primary modifier plus
        // Enter, so the label has to say the same thing and has to render
        // as one keycap per key.
        let caps = corvo_core::shortcut::keycaps(reveal_hotkey());
        let expected_primary = corvo_core::Primary::current().keycap_label();
        assert_eq!(caps, vec![expected_primary.to_string(), "↵".to_string()]);
    }

    /// An app that was uninstalled stops being reported by the scan, and
    /// that is the only signal that it is gone. Carrying the previous
    /// entry forward re-added it, so it stayed in the launcher forever.
    #[test]
    fn an_uninstalled_store_app_is_not_carried_forward() {
        let uninstalled = app("Gone", r"shell:AppsFolder\Microsoft.Gone_8wekyb3d8bbwe!App");
        // A Store app that has no decoded icon is not "pending", it is
        // simply gone. `Get-StartApps` is the authority on what is
        // installed, and it no longer lists this one.
        assert!(!is_pending_icon_decode(&uninstalled));
    }

    /// A shortcut whose icon has not been decoded yet has to survive the
    /// intermediate publishes, otherwise every app blinks out while the
    /// icons stream in.
    #[test]
    fn a_shortcut_waiting_for_its_icon_is_carried_forward() {
        let pending = app("Pending", r"C:\Program Files\Pending\Pending.lnk");
        assert!(is_pending_icon_decode(&pending));
    }
}

//! Settings, snippets, and quicklinks from TOML files, plus the store
//! locations for later phases. The only crate that knows about paths and
//! file formats (SPEC §7).

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use corvo_core::{
    CommandAvailability, DataStore, FileSearchOptions, Quicklink, Snippet,
    UpdateSettings as CoreUpdateSettings,
};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub struct ConfigService {
    dirs: ProjectDirs,
    settings: Settings,
    snippets: RwLock<Vec<SnippetEntry>>,
    quicklinks: RwLock<Vec<QuicklinkEntry>>,
    command_availability: RwLock<Vec<CommandAvailability>>,
    emoji_preferences: RwLock<(usize, usize)>,
    clipboard_auto_paste: RwLock<bool>,
    file_search_options: RwLock<FileSearchOptions>,
    escape_closes_window: RwLock<bool>,
    interface_appearance: RwLock<(usize, usize)>,
    compact_mode: RwLock<bool>,
    updates: RwLock<CoreUpdateSettings>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey: String,
    pub theme: String,
    pub launch_at_login: bool,
    pub show_menu_bar: bool,
    pub compact_mode: bool,
    pub pop_to_root_option: usize,
    pub escape_behavior_option: usize,
    pub auto_switch_input: usize,
    pub theme_option: usize,
    pub interface_size_option: usize,
    pub transparency_level: usize,
    pub applications: ApplicationsSettings,
    pub system_settings: SectionConfig,
    pub system_actions: SectionConfig,
    pub commands: SectionConfig,
    pub quicklinks: QuicklinksSettings,
    /// Empty means every registered command is enabled.
    pub enabled_commands: Vec<String>,
    pub onboarding: Onboarding,
    pub favorite_items: Vec<String>,
    pub clipboard: ClipboardSettings,
    pub snippets: SnippetsSettings,
    pub file_search: FileSearchSettings,
    pub window_management: WindowManagementSettings,
    pub navigation: NavigationSettings,
    pub calendar: CalendarSettings,
    pub emojis: EmojisSettings,
    /// 0 = Low, 1 = Medium (default), 2 = High
    pub search_sensitivity: usize,
    pub updates: UpdateSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub check_updates: bool,
    pub channel: String,
    pub auto_download: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_updates: true,
            channel: "stable".to_string(),
            auto_download: false,
        }
    }
}

pub fn update_settings(settings: &UpdateSettings) -> CoreUpdateSettings {
    CoreUpdateSettings {
        check_updates: settings.check_updates,
        channel: settings.channel.clone(),
        auto_download: settings.auto_download,
    }
}

pub mod ranking;
pub use ranking::{FrecencyStore, LearnedTerm};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub alias: Option<String>,
    pub hotkey: Option<String>,
    pub hidden: bool,
}

pub type CommandItemConfig = AppConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SectionConfig {
    pub enabled: bool,
    pub items: std::collections::HashMap<String, AppConfig>,
}

impl Default for SectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            items: std::collections::HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationsSettings {
    pub enabled: bool,
    pub search_scopes: Vec<String>,
    pub app_configs: std::collections::HashMap<String, AppConfig>,
}

impl Default for ApplicationsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            search_scopes: vec![
                "/Applications".into(),
                "/Applications/Utilities".into(),
                "/System/Applications".into(),
                "/System/Applications/Utilities".into(),
                "/System/Library/CoreServices/Applications".into(),
                "/System/Volumes/Preboot/Cryptexes/App/System/Applications".into(),
                "/System/Library/CoreServices/Finder.app".into(),
                "~/Applications".into(),
            ],
            app_configs: std::collections::HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct QuicklinksSettings {
    pub enabled: bool,
    pub show_in_launcher: bool,
    pub open_in_new_window: bool,
    pub command_items: std::collections::HashMap<String, AppConfig>,
}

impl Default for QuicklinksSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_in_launcher: true,
            open_in_new_window: false,
            command_items: std::collections::HashMap::new(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "cmd+space".into(),
            theme: "system".into(),
            launch_at_login: false,
            show_menu_bar: true,
            compact_mode: false,
            pop_to_root_option: 1, // After 90 seconds
            escape_behavior_option: 0, // Navigate back or close window
            auto_switch_input: 0, // None
            theme_option: 0, // System
            interface_size_option: 1, // Medium
            transparency_level: 2,
            applications: ApplicationsSettings::default(),
            system_settings: SectionConfig::default(),
            system_actions: SectionConfig::default(),
            commands: SectionConfig::default(),
            quicklinks: QuicklinksSettings::default(),
            enabled_commands: Vec::new(),
            onboarding: Onboarding::default(),
            favorite_items: Vec::new(),
            clipboard: ClipboardSettings::default(),
            snippets: SnippetsSettings::default(),
            file_search: FileSearchSettings::default(),
            window_management: WindowManagementSettings::default(),
            navigation: NavigationSettings::default(),
            calendar: CalendarSettings::default(),
            emojis: EmojisSettings::default(),
            search_sensitivity: 1,
            updates: UpdateSettings::default(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let dirs = ProjectDirs::from("", "", "corvo");
        if let Some(dirs) = dirs {
            load_or_create(&dirs, "settings.toml", &Settings::default())
        } else {
            Settings::default()
        }
    }

    pub fn config_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "corvo").map(|d| d.config_dir().to_path_buf())
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dirs = ProjectDirs::from("", "", "corvo")
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?;
        let path = dirs.config_dir().join("settings.toml");
        let toml_str = toml::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml_str)
    }

    pub fn result_item(&self, result_id: &str, title: &str) -> Option<&AppConfig> {
        let (items, key) = self.result_items_and_key(result_id)?;
        items.get(key).or_else(|| items.get(title))
    }

    pub fn result_is_favorite(&self, result_id: &str) -> bool {
        let identity = result_identity(result_id);
        self.favorite_items.iter().any(|item| item == identity)
    }

    pub fn set_result_item_flags(
        &mut self,
        result_id: &str,
        title: &str,
        hidden: Option<bool>,
        favorite: Option<bool>,
    ) -> bool {
        if let Some(favorite) = favorite {
            let identity = result_identity(result_id);
            if favorite && !self.favorite_items.iter().any(|item| item == identity) {
                self.favorite_items.push(identity.to_string());
            } else if !favorite {
                self.favorite_items.retain(|item| item != identity);
            }
        }
        if hidden.is_none() {
            return true;
        }
        let Some((items, key)) = self.result_items_and_key_mut(result_id) else {
            return false;
        };
        let key = if items.contains_key(key) {
            key.to_string()
        } else if items.contains_key(title) {
            title.to_string()
        } else {
            key.to_string()
        };
        let item = items.entry(key).or_default();
        item.hidden = hidden.unwrap_or(item.hidden);
        true
    }

    fn result_items_and_key<'a>(
        &self,
        result_id: &'a str,
    ) -> Option<(&std::collections::HashMap<String, AppConfig>, &'a str)> {
        let (items, key) = self.result_items_and_key_for_id(result_id)?;
        Some((items, key))
    }

    fn result_items_and_key_mut<'a>(
        &mut self,
        result_id: &'a str,
    ) -> Option<(&mut std::collections::HashMap<String, AppConfig>, &'a str)> {
        let (section, key) = result_section_and_key(result_id)?;
        let items = match section {
            "app-launcher" => &mut self.applications.app_configs,
            "system-actions" if result_id.starts_with("system-actions:setting:") => {
                &mut self.system_settings.items
            }
            "system-actions" => &mut self.system_actions.items,
            "quicklinks" => &mut self.quicklinks.command_items,
            "clipboard-manager" => &mut self.clipboard.command_items,
            "snippets" => &mut self.snippets.command_items,
            "file-search" => &mut self.file_search.command_items,
            "window-management" => &mut self.window_management.command_items,
            "emoji-picker" => &mut self.emojis.command_items,
            _ => &mut self.commands.items,
        };
        Some((items, key))
    }

    fn result_items_and_key_for_id<'a>(
        &self,
        result_id: &'a str,
    ) -> Option<(&std::collections::HashMap<String, AppConfig>, &'a str)> {
        let (section, key) = result_section_and_key(result_id)?;
        let items = match section {
            "app-launcher" => &self.applications.app_configs,
            "system-actions" if result_id.starts_with("system-actions:setting:") => {
                &self.system_settings.items
            }
            "system-actions" => &self.system_actions.items,
            "quicklinks" => &self.quicklinks.command_items,
            "clipboard-manager" => &self.clipboard.command_items,
            "snippets" => &self.snippets.command_items,
            "file-search" => &self.file_search.command_items,
            "window-management" => &self.window_management.command_items,
            "emoji-picker" => &self.emojis.command_items,
            _ => &self.commands.items,
        };
        Some((items, key))
    }
}

fn result_section_and_key(result_id: &str) -> Option<(&str, &str)> {
    let (section, suffix) = result_id.split_once(':')?;
    let key = if section == "app-launcher" {
        suffix.strip_prefix("recent:").unwrap_or(suffix)
    } else if section == "system-actions" {
        suffix
            .strip_prefix("action:")
            .or_else(|| suffix.strip_prefix("setting:"))
            .unwrap_or(suffix)
    } else {
        suffix
    };
    Some((section, key))
}

fn result_identity(result_id: &str) -> &str {
    result_id
        .strip_prefix("app-launcher:")
        .map(|path| path.strip_prefix("recent:").unwrap_or(path))
        .unwrap_or(result_id)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Onboarding {
    pub accessibility_granted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardSettings {
    pub enabled: bool,
    pub max_entries: usize,
    pub retention_days: u32,
    pub retention_option: usize,
    pub auto_paste: bool,
    pub save_images: bool,
    pub save_colors: bool,
    pub command_items: std::collections::HashMap<String, AppConfig>,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        let mut command_items = std::collections::HashMap::new();
        command_items.insert(
            "Clipboard History".into(),
            AppConfig {
                alias: None,
                hotkey: Some("cmd+shift+v".into()),
                hidden: false,
            },
        );
        command_items.insert(
            "Clear Clipboard History".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        Self {
            enabled: true,
            max_entries: 500,
            retention_days: 30,
            retention_option: 2,
            auto_paste: true,
            save_images: true,
            save_colors: true,
            command_items,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SnippetsSettings {
    pub enabled: bool,
    pub show_in_launcher: bool,
    pub command_items: std::collections::HashMap<String, AppConfig>,
}

impl Default for SnippetsSettings {
    fn default() -> Self {
        let mut command_items = std::collections::HashMap::new();
        command_items.insert(
            "Search Snippets".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        command_items.insert(
            "Create Snippet".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        Self {
            enabled: true,
            show_in_launcher: true,
            command_items,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSearchSettings {
    pub enabled: bool,
    pub search_scopes: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub command_items: std::collections::HashMap<String, AppConfig>,
}

impl Default for FileSearchSettings {
    fn default() -> Self {
        let mut command_items = std::collections::HashMap::new();
        command_items.insert(
            "Search Files".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        Self {
            enabled: true,
            search_scopes: vec!["~".into()],
            ignore_patterns: vec![
                "node_modules".into(),
                "DerivedData".into(),
                "build".into(),
                "dist".into(),
                "target".into(),
                "Pods".into(),
            ],
            command_items,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowPlacement {
    pub app_name: String,
    pub position: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowLayoutTemplate {
    pub id: String,
    pub name: String,
    pub hotkey: Option<String>,
    pub placements: Vec<WindowPlacement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowManagementSettings {
    pub enabled: bool,
    pub show_in_launcher: bool,
    pub cycling_option: usize,
    pub gap_between_windows: usize,
    pub show_layouts_in_launcher: bool,
    pub command_items: std::collections::HashMap<String, AppConfig>,
    pub layouts: Vec<WindowLayoutTemplate>,
}

impl Default for WindowManagementSettings {
    fn default() -> Self {
        let mut command_items = std::collections::HashMap::new();
        command_items.insert(
            "Create Window Layout".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        command_items.insert(
            "Create Layout from Current Windows".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        Self {
            enabled: true,
            show_in_launcher: true,
            cycling_option: 0,
            gap_between_windows: 0,
            show_layouts_in_launcher: true,
            command_items,
            layouts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NavigationSettings {
    pub enabled: bool,
    pub show_apple_menu_items: bool,
    pub command_items: std::collections::HashMap<String, AppConfig>,
    pub disabled_applications: Vec<String>,
}

impl Default for NavigationSettings {
    fn default() -> Self {
        let mut command_items = std::collections::HashMap::new();
        command_items.insert(
            "Switch Windows".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        command_items.insert(
            "Search Menu Bar Items".into(),
            AppConfig {
                alias: None,
                hotkey: None,
                hidden: false,
            },
        );
        Self {
            enabled: false,
            show_apple_menu_items: false,
            command_items,
            disabled_applications: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSettings {
    pub join_meetings: bool,
    pub show_in_launcher: bool,
    pub upcoming_meetings_option: usize,
    pub include_tomorrow: bool,
    pub join_card_option: usize,
    pub auto_join_meetings: bool,
    pub camera_preview: bool,
    pub open_meeting_links_in: usize,
}

impl Default for CalendarSettings {
    fn default() -> Self {
        Self {
            join_meetings: true,
            show_in_launcher: true,
            upcoming_meetings_option: 2, // "5 next"
            include_tomorrow: true,
            join_card_option: 1, // "5 minutes"
            auto_join_meetings: false,
            camera_preview: false,
            open_meeting_links_in: 0, // "Default Browser"
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EmojisSettings {
    pub column_count: usize,
    pub skin_tone: usize,
    pub command_items: std::collections::HashMap<String, AppConfig>,
}

impl Default for EmojisSettings {
    fn default() -> Self {
        let mut command_items = std::collections::HashMap::new();
        command_items.insert(
            "Search Emoji & Symbols".into(),
            AppConfig {
                alias: None,
                hotkey: Some("ctrl+cmd+space".into()),
                hidden: false,
            },
        );
        Self {
            column_count: 8,
            skin_tone: 0,
            command_items,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SnippetEntry {
    pub name: String,
    pub keyword: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SnippetsFile {
    #[serde(default)]
    pub snippets: Vec<SnippetEntry>,
}

impl SnippetsFile {
    pub fn load() -> Self {
        let dirs = ProjectDirs::from("", "", "corvo");
        if let Some(dirs) = dirs {
            load_or_create(&dirs, "snippets.toml", &SnippetsFile::default())
        } else {
            SnippetsFile::default()
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dirs = ProjectDirs::from("", "", "corvo")
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?;
        let path = dirs.config_dir().join("snippets.toml");
        let toml_str = toml::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml_str)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QuicklinkEntry {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuicklinksFile {
    #[serde(default)]
    pub quicklinks: Vec<QuicklinkEntry>,
}

impl Default for QuicklinksFile {
    fn default() -> Self {
        Self {
            quicklinks: vec![
                QuicklinkEntry {
                    name: "Search DuckDuckGo".into(),
                    url: "https://duckduckgo.com/?q={argument}".into(),
                    alias: None,
                    hotkey: None,
                    hidden: false,
                },
                QuicklinkEntry {
                    name: "Search Google".into(),
                    url: "https://google.com/search?q={argument}".into(),
                    alias: None,
                    hotkey: None,
                    hidden: false,
                },
            ],
        }
    }
}

impl QuicklinksFile {
    pub fn load() -> Self {
        let dirs = ProjectDirs::from("", "", "corvo");
        if let Some(dirs) = dirs {
            load_or_create(&dirs, "quicklinks.toml", &QuicklinksFile::default())
        } else {
            QuicklinksFile::default()
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dirs = ProjectDirs::from("", "", "corvo")
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?;
        let path = dirs.config_dir().join("quicklinks.toml");
        let toml_str = toml::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml_str)
    }
}

impl ConfigService {
    /// Loads the TOML files and writes defaults for the missing ones. A
    /// malformed file falls back to defaults so the app stays usable.
    pub fn load() -> Self {
        let dirs = ProjectDirs::from("", "", "corvo").expect("corvo: no home directory");
        let settings = load_or_create(&dirs, "settings.toml", &Settings::default());
        let snippets = load_or_create(&dirs, "snippets.toml", &SnippetsFile::default());
        let quicklinks = load_or_create(&dirs, "quicklinks.toml", &QuicklinksFile::default());
        let availability = command_availability(&settings);
        let emoji_preferences = (settings.emojis.column_count, settings.emojis.skin_tone);
        let clipboard_auto_paste = settings.clipboard.auto_paste;
        let file_search_options = file_search_options(&settings.file_search);
        let escape_closes_window = settings.escape_behavior_option == 1;
        let interface_appearance = (settings.interface_size_option, settings.transparency_level);
        let compact_mode = settings.compact_mode;
        let updates = update_settings(&settings.updates);
        Self {
            dirs,
            settings,
            snippets: RwLock::new(snippets.snippets),
            quicklinks: RwLock::new(quicklinks.quicklinks),
            command_availability: RwLock::new(availability),
            emoji_preferences: RwLock::new(emoji_preferences),
            clipboard_auto_paste: RwLock::new(clipboard_auto_paste),
            file_search_options: RwLock::new(file_search_options),
            escape_closes_window: RwLock::new(escape_closes_window),
            interface_appearance: RwLock::new(interface_appearance),
            compact_mode: RwLock::new(compact_mode),
            updates: RwLock::new(updates),
        }
    }

    pub fn save_settings(&mut self, settings: &Settings) -> std::io::Result<()> {
        settings.save()?;
        self.settings = settings.clone();
        self.replace_command_availability(command_availability(settings));
        self.replace_emoji_preferences(settings.emojis.column_count, settings.emojis.skin_tone);
        self.replace_clipboard_auto_paste(settings.clipboard.auto_paste);
        self.replace_file_search_options(file_search_options(&settings.file_search));
        self.replace_escape_behavior(settings.escape_behavior_option == 1);
        self.replace_interface_appearance(settings.interface_size_option, settings.transparency_level);
        self.replace_compact_mode(settings.compact_mode);
        self.replace_update_settings(update_settings(&settings.updates));
        Ok(())
    }

    pub fn save_snippets(&mut self, snippets: Vec<SnippetEntry>) -> std::io::Result<()> {
        SnippetsFile { snippets: snippets.clone() }.save()?;
        self.replace_snippets(
            snippets
                .into_iter()
                .map(|snippet| Snippet { name: snippet.name, keyword: snippet.keyword, body: snippet.body })
                .collect(),
        );
        Ok(())
    }

    pub fn save_quicklinks(&mut self, quicklinks: Vec<QuicklinkEntry>) -> std::io::Result<()> {
        QuicklinksFile { quicklinks: quicklinks.clone() }.save()?;
        self.replace_quicklinks(
            quicklinks
                .into_iter()
                .map(|link| Quicklink {
                    name: link.name,
                    url: link.url,
                    alias: link.alias,
                    hotkey: link.hotkey,
                    hidden: link.hidden,
                })
                .collect(),
        );
        Ok(())
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn config_dir(&self) -> PathBuf {
        self.dirs.config_dir().to_owned()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dirs.data_dir().to_owned()
    }

    /// Frecency store for app-launcher and file-search (phase 1, SPEC §7).
    pub fn frecency_dir(&self) -> PathBuf {
        self.dirs.data_dir().join("frecency")
    }

    /// Clipboard history LMDB (phase 3, SPEC §7).
    pub fn clipboard_db_path(&self) -> PathBuf {
        self.dirs.data_dir().join("clipboard.mdb")
    }

    /// Clipboard image files referenced from the LMDB entries.
    pub fn clipboard_images_dir(&self) -> PathBuf {
        self.dirs.data_dir().join("clipboard").join("images")
    }

    /// Hands the service to command contexts as `Arc<dyn DataStore>`.
    pub fn shared(self) -> Arc<dyn DataStore> {
        Arc::new(self)
    }
}

fn load_or_create<T>(dirs: &ProjectDirs, file: &str, default: &T) -> T
where
    T: Serialize + Clone + for<'de> Deserialize<'de>,
{
    let path = dirs.config_dir().join(file);
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str::<T>(&text).ok().unwrap_or_else(|| {
            eprintln!("corvo: {file} is malformed, using defaults");
            default.clone()
        }),
        Err(_) => {
            let _ = fs::create_dir_all(dirs.config_dir());
            match toml::to_string_pretty(default) {
                Ok(text) => {
                    let _ = fs::write(&path, &text);
                }
                Err(err) => eprintln!("corvo: cannot serialize default {file}: {err}"),
            }
            default.clone()
        }
    }
}

impl DataStore for ConfigService {
    fn snippets(&self) -> Vec<Snippet> {
        self.snippets.read().map(|items| items.clone()).unwrap_or_default()
            .iter()
            .map(|s| Snippet {
                name: s.name.clone(),
                keyword: s.keyword.clone(),
                body: s.body.clone(),
            })
            .collect()
    }

    fn quicklinks(&self) -> Vec<Quicklink> {
        self.quicklinks.read().map(|items| items.clone()).unwrap_or_default()
            .iter()
            .map(|q| Quicklink {
                name: q.name.clone(),
                url: q.url.clone(),
                alias: q.alias.clone(),
                hotkey: q.hotkey.clone(),
                hidden: q.hidden,
            })
            .collect()
    }

    fn replace_snippets(&self, snippets: Vec<Snippet>) {
        if let Ok(mut current) = self.snippets.write() {
            *current = snippets
                .into_iter()
                .map(|snippet| SnippetEntry {
                    name: snippet.name,
                    keyword: snippet.keyword,
                    body: snippet.body,
                })
                .collect();
        }
    }

    fn replace_quicklinks(&self, quicklinks: Vec<Quicklink>) {
        if let Ok(mut current) = self.quicklinks.write() {
            *current = quicklinks
                .into_iter()
                .map(|link| QuicklinkEntry {
                    name: link.name,
                    url: link.url,
                    alias: link.alias,
                    hotkey: link.hotkey,
                    hidden: link.hidden,
                })
                .collect();
        }
    }

    fn command_enabled(&self, command_id: &str) -> bool {
        self.command_availability
            .read()
            .ok()
            .and_then(|items| items.iter().find(|item| item.command_id == command_id).map(|item| item.enabled))
            .unwrap_or(true)
    }

    fn show_command_in_launcher(&self, command_id: &str) -> bool {
        self.command_availability
            .read()
            .ok()
            .and_then(|items| {
                items.iter().find(|item| item.command_id == command_id).map(|item| item.show_in_launcher)
            })
            .unwrap_or(true)
    }

    fn replace_command_availability(&self, availability: Vec<CommandAvailability>) {
        if let Ok(mut current) = self.command_availability.write() {
            *current = availability;
        }
    }

    fn emoji_column_count(&self) -> usize {
        self.emoji_preferences.read().map(|preferences| preferences.0).unwrap_or(8).clamp(6, 10)
    }

    fn emoji_skin_tone(&self) -> usize {
        self.emoji_preferences.read().map(|preferences| preferences.1).unwrap_or(0).min(5)
    }

    fn replace_emoji_preferences(&self, column_count: usize, skin_tone: usize) {
        if let Ok(mut preferences) = self.emoji_preferences.write() {
            *preferences = (column_count.clamp(6, 10), skin_tone.min(5));
        }
    }

    fn clipboard_auto_paste(&self) -> bool {
        self.clipboard_auto_paste.read().map(|enabled| *enabled).unwrap_or(true)
    }

    fn replace_clipboard_auto_paste(&self, enabled: bool) {
        if let Ok(mut current) = self.clipboard_auto_paste.write() {
            *current = enabled;
        }
    }

    fn file_search_options(&self) -> FileSearchOptions {
        self.file_search_options.read().map(|options| options.clone()).unwrap_or_default()
    }

    fn replace_file_search_options(&self, options: FileSearchOptions) {
        if let Ok(mut current) = self.file_search_options.write() {
            *current = options;
        }
    }

    fn escape_closes_window(&self) -> bool {
        self.escape_closes_window.read().map(|value| *value).unwrap_or(false)
    }

    fn replace_escape_behavior(&self, close_window: bool) {
        if let Ok(mut current) = self.escape_closes_window.write() {
            *current = close_window;
        }
    }

    fn interface_size_option(&self) -> usize {
        self.interface_appearance.read().map(|value| value.0).unwrap_or(1).min(2)
    }

    fn transparency_level(&self) -> usize {
        self.interface_appearance.read().map(|value| value.1).unwrap_or(2).min(4)
    }

    fn replace_interface_appearance(&self, size_option: usize, transparency_level: usize) {
        if let Ok(mut current) = self.interface_appearance.write() {
            *current = (size_option.min(2), transparency_level.min(4));
        }
    }

    fn compact_mode(&self) -> bool {
        self.compact_mode.read().map(|enabled| *enabled).unwrap_or(false)
    }

    fn replace_compact_mode(&self, enabled: bool) {
        if let Ok(mut current) = self.compact_mode.write() {
            *current = enabled;
        }
    }

    fn update_settings(&self) -> CoreUpdateSettings {
        self.updates.read().map(|u| u.clone()).unwrap_or_default()
    }

    fn replace_update_settings(&self, settings: CoreUpdateSettings) {
        if let Ok(mut current) = self.updates.write() {
            *current = settings;
        }
    }
}

pub fn command_availability(settings: &Settings) -> Vec<CommandAvailability> {
    vec![
        CommandAvailability { command_id: "app-launcher".into(), enabled: settings.applications.enabled, show_in_launcher: true },
        CommandAvailability { command_id: "system-actions".into(), enabled: settings.system_actions.enabled, show_in_launcher: true },
        CommandAvailability { command_id: "clipboard-manager".into(), enabled: settings.clipboard.enabled, show_in_launcher: true },
        CommandAvailability { command_id: "quicklinks".into(), enabled: settings.quicklinks.enabled, show_in_launcher: settings.quicklinks.show_in_launcher },
        CommandAvailability { command_id: "snippets".into(), enabled: settings.snippets.enabled, show_in_launcher: settings.snippets.show_in_launcher },
        CommandAvailability { command_id: "file-search".into(), enabled: settings.file_search.enabled, show_in_launcher: true },
        CommandAvailability { command_id: "window-management".into(), enabled: settings.window_management.enabled, show_in_launcher: settings.window_management.show_in_launcher },
        CommandAvailability { command_id: "emoji-picker".into(), enabled: true, show_in_launcher: true },
        CommandAvailability { command_id: "calculator".into(), enabled: true, show_in_launcher: true },
        CommandAvailability { command_id: "web-search-fallback".into(), enabled: true, show_in_launcher: true },
    ]
}

fn file_search_options(settings: &FileSearchSettings) -> FileSearchOptions {
    FileSearchOptions {
        enabled: settings.enabled,
        search_scopes: settings.search_scopes.clone(),
        ignore_patterns: settings.ignore_patterns.clone(),
    }
}

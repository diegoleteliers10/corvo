//! Settings, snippets, and quicklinks from TOML files, plus the store
//! locations for later phases. The only crate that knows about paths and
//! file formats (SPEC §7).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use corvo_core::{DataStore, Quicklink, Snippet};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub struct ConfigService {
    dirs: ProjectDirs,
    settings: Settings,
    snippets: Vec<SnippetEntry>,
    quicklinks: Vec<QuicklinkEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey: String,
    pub theme: String,
    /// Empty means every registered command is enabled.
    pub enabled_commands: Vec<String>,
    pub onboarding: Onboarding,
    pub clipboard: ClipboardSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "alt+space".into(),
            theme: "dark".into(),
            enabled_commands: Vec::new(),
            onboarding: Onboarding::default(),
            clipboard: ClipboardSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Onboarding {
    pub accessibility_granted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardSettings {
    pub max_entries: usize,
    pub retention_days: u32,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self { max_entries: 500, retention_days: 30 }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SnippetEntry {
    pub name: String,
    pub keyword: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QuicklinkEntry {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SnippetsFile {
    #[serde(default)]
    pub snippets: Vec<SnippetEntry>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QuicklinksFile {
    #[serde(default)]
    pub quicklinks: Vec<QuicklinkEntry>,
}

impl ConfigService {
    /// Loads the TOML files and writes defaults for the missing ones. A
    /// malformed file falls back to defaults so the app stays usable.
    pub fn load() -> Self {
        let dirs = ProjectDirs::from("", "", "corvo").expect("corvo: no home directory");
        let settings = load_or_create(&dirs, "settings.toml", &Settings::default());
        let snippets = load_or_create(&dirs, "snippets.toml", &SnippetsFile::default());
        let quicklinks = load_or_create(&dirs, "quicklinks.toml", &QuicklinksFile::default());
        Self {
            dirs,
            settings,
            snippets: snippets.snippets,
            quicklinks: quicklinks.quicklinks,
        }
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
        self.snippets
            .iter()
            .map(|s| Snippet {
                name: s.name.clone(),
                keyword: s.keyword.clone(),
                body: s.body.clone(),
            })
            .collect()
    }

    fn quicklinks(&self) -> Vec<Quicklink> {
        self.quicklinks
            .iter()
            .map(|q| Quicklink { name: q.name.clone(), url: q.url.clone() })
            .collect()
    }
}

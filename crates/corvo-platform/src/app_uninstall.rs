use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use objc2_foundation::{NSFileManager, NSString, NSURL};

use crate::{AppFileEntry, AppFileScan, PlatformError, PlatformResult};
const MAX_SIZE_ENTRIES: usize = 100_000;
const MAX_SCAN_ENTRIES: usize = 20_000;

pub(super) fn associated_app_files(app_path: &Path) -> PlatformResult<AppFileScan> {
    let app_path = safe_app_path(app_path)?;
    let info_plist = app_path.join("Contents/Info.plist");
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier"])
        .arg(&info_plist)
        .output()
        .map_err(|error| PlatformError::Os(format!("could not read app bundle id: {error}")))?;
    if !output.status.success() {
        return Err(PlatformError::Os("app bundle id was not found".into()));
    }
    let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !valid_bundle_id(&bundle_id) {
        return Err(PlatformError::Os("app bundle id is invalid".into()));
    }

    let app_name = app_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_string();

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| PlatformError::Os("home folder was not found".into()))?;
    let library = home.join("Library");
    let mut entries = vec![AppFileEntry {
        size_bytes: path_size(&app_path),
        location: display_location(app_path.parent().unwrap_or(&app_path), &home),
        path: app_path.clone(),
        is_application: true,
        matched_by_name: false,
    }];
    let mut seen = HashSet::from([app_path]);
    let mut reached_scan_limit = false;

    for folder in [
        "Application Support",
        "Caches",
        "Preferences",
        "Containers",
        "Group Containers",
        "Application Scripts",
        "WebKit",
        "Cookies",
        "Saved Application State",
        "Autosave Information",
        "Logs",
        "HTTPStorages",
    ] {
        let root = library.join(folder);
        let Ok(root_metadata) = fs::symlink_metadata(&root) else {
            continue;
        };
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            continue;
        }
        let mut pending = vec![(root.clone(), 0usize)];
        let mut scanned = 0usize;
        while let Some((directory, depth)) = pending.pop() {
            let Ok(children) = fs::read_dir(&directory) else {
                continue;
            };
            for child in children.flatten() {
                scanned += 1;
                if scanned >= MAX_SCAN_ENTRIES {
                    reached_scan_limit = true;
                    break;
                }
                let path = child.path();
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if metadata.file_type().is_symlink() || !metadata.is_dir() && !metadata.is_file() {
                    continue;
                }
                let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let is_bundle_match = matches_app_data(file_name, &bundle_id);
                let is_name_match = !is_bundle_match
                    && !app_name.is_empty()
                    && matches_app_name(file_name, &app_name);

                if is_bundle_match || is_name_match {
                    let Ok(path) = path.canonicalize() else {
                        continue;
                    };
                    if path.starts_with(&library) && seen.insert(path.clone()) {
                        entries.push(AppFileEntry {
                            size_bytes: path_size(&path),
                            location: display_location(path.parent().unwrap_or(&root), &home),
                            path,
                            is_application: false,
                            matched_by_name: is_name_match,
                        });
                    }
                } else if metadata.is_dir() && depth < 2 {
                    pending.push((path, depth + 1));
                }
            }
            if scanned >= MAX_SCAN_ENTRIES {
                reached_scan_limit = true;
                break;
            }
        }
    }

    entries[1..].sort_by(|left, right| left.path.cmp(&right.path));
    Ok(AppFileScan {
        files: entries,
        reached_scan_limit,
    })
}

pub(super) fn move_app_files_to_trash(app_path: &Path, paths: &[PathBuf]) -> PlatformResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let app_path = safe_app_path(app_path)?;
    let safe_scan = associated_app_files(&app_path)?;
    let safe_paths: HashSet<PathBuf> = safe_scan
        .files
        .into_iter()
        .map(|entry| entry.path)
        .collect();

    let mut canonical_paths = Vec::<PathBuf>::with_capacity(paths.len());
    for path in paths {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            PlatformError::Os(format!("could not access {}: {error}", path.display()))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(PlatformError::Os(format!(
                "{} changed after the scan",
                path.display()
            )));
        }
        let canonical = path.canonicalize().map_err(|error| {
            PlatformError::Os(format!("could not access {}: {error}", path.display()))
        })?;
        if path != &canonical || !safe_paths.contains(&canonical) {
            return Err(PlatformError::Os(format!(
                "{} changed after the scan",
                path.display()
            )));
        }
        canonical_paths.push(canonical);
    }
    canonical_paths.sort();
    canonical_paths.dedup();
    canonical_paths.sort_by_key(|path| path == &app_path);
    let file_manager = NSFileManager::defaultManager();
    for canonical in &canonical_paths {
        let path = NSString::from_str(&canonical.to_string_lossy());
        let url = NSURL::fileURLWithPath(&path);
        file_manager
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|error| {
                PlatformError::Os(format!(
                    "could not move {} to Trash: {}",
                    canonical.display(),
                    error.localizedDescription()
                ))
            })?;
    }
    Ok(())
}

fn safe_app_path(app_path: &Path) -> PlatformResult<PathBuf> {
    let canonical = app_path
        .canonicalize()
        .map_err(|error| PlatformError::Os(format!("could not access app: {error}")))?;
    if canonical
        .extension()
        .is_none_or(|extension| extension != "app")
        || canonical.starts_with("/System")
        || canonical.starts_with("/Library/Apple")
    {
        return Err(PlatformError::Os("this app cannot be uninstalled".into()));
    }
    Ok(canonical)
}

fn valid_bundle_id(bundle_id: &str) -> bool {
    bundle_id.contains('.')
        && bundle_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
}

fn display_location(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .map(|relative| format!("~/{}", relative.display()))
        .unwrap_or_else(|_| path.display().to_string())
}

fn matches_app_data(name: &str, bundle_id: &str) -> bool {
    let name = name.to_lowercase();
    let bundle_id = bundle_id.to_lowercase();
    let without_plist = name.strip_suffix(".plist").unwrap_or(&name);
    let without_saved_state = without_plist
        .strip_suffix(".savedstate")
        .unwrap_or(without_plist);
    without_plist == bundle_id
        || without_saved_state == bundle_id
        || name
            .strip_prefix("group.")
            .is_some_and(|group| group == bundle_id)
}

fn matches_app_name(name: &str, app_name: &str) -> bool {
    let name = name.to_lowercase();
    let app_name = app_name.to_lowercase();
    let without_plist = name.strip_suffix(".plist").unwrap_or(&name);
    let without_saved_state = without_plist
        .strip_suffix(".savedstate")
        .unwrap_or(without_plist);
    without_plist == app_name
        || without_saved_state == app_name
        || name.starts_with(&format!("{}.", app_name))
        || name.starts_with(&format!("{}-", app_name))
}

fn path_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return metadata.len();
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    let mut visited = 0usize;
    while let Some(directory) = stack.pop() {
        let Ok(children) = fs::read_dir(directory) else {
            continue;
        };
        for child in children.flatten() {
            visited += 1;
            if visited >= MAX_SIZE_ENTRIES {
                return total;
            }
            let child_path = child.path();
            let Ok(metadata) = fs::symlink_metadata(&child_path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(child_path);
            } else {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

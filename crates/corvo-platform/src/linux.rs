//! Linux desktop integration through standard XDG and desktop utilities.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use super::{
    AppEntry, AppFileEntry, AppFileScan, PlatformError, PlatformOps, PlatformResult, WindowHandle,
};

pub struct LinuxPlatform;

impl PlatformOps for LinuxPlatform {
    fn lock(&self) -> PlatformResult<()> {
        run_first_success(
            &[
                (&"loginctl", &["lock-session"]),
                (&"gnome-screensaver-command", &["--lock"]),
                (&"xdg-screensaver", &["lock"]),
                (&"dm-tool", &["lock"]),
            ],
            "lock the session",
        )
    }

    fn sleep(&self) -> PlatformResult<()> {
        run_first_success(
            &[(&"systemctl", &["suspend"]), (&"loginctl", &["suspend"])],
            "suspend the system",
        )
    }

    fn shutdown(&self) -> PlatformResult<()> {
        run_first_success(
            &[(&"systemctl", &["poweroff"]), (&"loginctl", &["poweroff"])],
            "shut down the system",
        )
    }

    fn list_windows(&self) -> PlatformResult<Vec<WindowHandle>> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Err(PlatformError::Unsupported(
                "full window listing is not available on Wayland. wmctrl only sees X11 windows, including a partial set on XWayland".into(),
            ));
        }
        if std::env::var_os("DISPLAY").is_none() {
            return Err(PlatformError::Unsupported(
                "window listing requires an active X11 or Wayland desktop session".into(),
            ));
        }

        let output = Command::new("wmctrl")
            .args(["-lp"])
            .output()
            .map_err(|error| command_error("run wmctrl", error))?;
        if !output.status.success() {
            return Err(PlatformError::Os(format!(
                "wmctrl could not list windows: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        let windows = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(parse_wmctrl_line)
            .collect();
        Ok(windows)
    }

    fn focus_window(&self, handle: &WindowHandle) -> PlatformResult<()> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Err(PlatformError::Unsupported(
                "window focus through wmctrl is not available on Wayland".into(),
            ));
        }
        if std::env::var_os("DISPLAY").is_none() {
            return Err(PlatformError::Unsupported(
                "window focus requires an active X11 session and wmctrl".into(),
            ));
        }
        run_command(
            "wmctrl",
            [OsStr::new("-ia"), OsStr::new(&format!("0x{:x}", handle.id))],
        )
        .map(|_| ())
        .map_err(|error| PlatformError::Os(format!("could not focus window: {error}")))
    }

    fn list_apps(&self) -> PlatformResult<Vec<AppEntry>> {
        let mut apps = Vec::new();
        let mut seen_ids = HashSet::new();
        for directory in application_directories() {
            for path in desktop_files(&directory) {
                if path.extension() != Some(OsStr::new("desktop")) {
                    continue;
                }
                let Ok(contents) = fs::read_to_string(&path) else {
                    continue;
                };
                let Some(desktop_entry) = parse_desktop_entry(&contents) else {
                    continue;
                };
                let Ok(relative_path) = path.strip_prefix(&directory) else {
                    continue;
                };
                let id = relative_path
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "-");
                if !seen_ids.insert(id) {
                    continue;
                }
                if desktop_entry.hidden || desktop_entry.name.is_empty() {
                    continue;
                }
                apps.push(AppEntry {
                    name: desktop_entry.name,
                    path,
                    icon_png: desktop_entry.icon.as_deref().and_then(resolve_desktop_icon),
                });
            }
        }

        apps.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(apps)
    }

    fn open_path(&self, path: &Path) -> PlatformResult<()> {
        if path.extension() == Some(OsStr::new("desktop")) {
            match Command::new("gio").arg("launch").arg(path).output() {
                Ok(output) if output.status.success() => return Ok(()),
                Ok(output) => {
                    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    if let Some(id) = path.file_stem() {
                        if run_command("gtk-launch", [id]).is_ok() {
                            return Ok(());
                        }
                    }
                    return Err(PlatformError::Os(format!(
                        "could not launch {} with gio: {detail}",
                        path.display()
                    )));
                }
                Err(error) => {
                    if let Some(id) = path.file_stem() {
                        if run_command("gtk-launch", [id]).is_ok() {
                            return Ok(());
                        }
                    }
                    return Err(PlatformError::Unsupported(format!(
                        "launching desktop entries requires gio or gtk-launch: {error}"
                    )));
                }
            }
        }

        spawn_command("xdg-open", [path.as_os_str()]).map(|_| ())
    }

    fn copy_text(&self, text: &str) -> PlatformResult<()> {
        let mut last_failure = None;
        for program in ["wl-copy", "xclip", "xsel"] {
            let args: &[&str] = match program {
                "wl-copy" => &[],
                "xclip" => &["-selection", "clipboard"],
                _ => &["--clipboard", "--input"],
            };
            match copy_to_clipboard(program, args, text) {
                Ok(()) => return Ok(()),
                Err(ClipboardAttemptError::Unavailable) => continue,
                Err(ClipboardAttemptError::Failed(error)) => {
                    last_failure = Some(format!("{program}: {error}"));
                }
            }
        }
        if let Some(error) = last_failure {
            return Err(PlatformError::Os(format!(
                "could not write to the clipboard: {error}"
            )));
        }
        Err(PlatformError::Unsupported(
            "clipboard copy needs wl-clipboard on Wayland, or xclip/xsel on X11".into(),
        ))
    }
}

pub fn supports_app_uninstall_path(path: &Path) -> bool {
    linux_uninstall_target(path).is_some()
}

pub fn associated_app_files(app_path: &Path) -> PlatformResult<AppFileScan> {
    let target = linux_uninstall_target(app_path).ok_or_else(|| {
        PlatformError::Unsupported(
            "this app does not have a safe, interactive uninstall path".into(),
        )
    })?;
    let location = match &target {
        LinuxUninstallTarget::Flatpak { app_id, .. } => format!("Flatpak · {app_id}"),
        LinuxUninstallTarget::Snap { name } => format!("Snap · {name}"),
        LinuxUninstallTarget::Package { manager, package } => {
            format!("{} · {package}", manager.label())
        }
        LinuxUninstallTarget::AppImage { path } => {
            format!("AppImage · {}", path.parent().unwrap_or(path).display())
        }
    };
    let row_path = match &target {
        LinuxUninstallTarget::AppImage { path } => path.clone(),
        _ => app_path.to_path_buf(),
    };
    Ok(AppFileScan {
        files: vec![AppFileEntry {
            path: row_path,
            location,
            size_bytes: match &target {
                LinuxUninstallTarget::AppImage { path } => fs::metadata(path)
                    .map(|metadata| metadata.len())
                    .unwrap_or_default(),
                _ => 0,
            },
            is_application: true,
            matched_by_name: false,
        }],
        reached_scan_limit: false,
    })
}

pub fn move_app_files_to_trash(app_path: &Path, paths: &[PathBuf]) -> PlatformResult<()> {
    let target = linux_uninstall_target(app_path).ok_or_else(|| {
        PlatformError::Unsupported(
            "this app does not have a safe, interactive uninstall path".into(),
        )
    })?;
    let selected_path = match &target {
        LinuxUninstallTarget::AppImage { path } => path.as_path(),
        _ => app_path,
    };
    if paths.len() != 1 || paths[0].as_path() != selected_path {
        return Err(PlatformError::Os(
            "select only the app package before uninstalling".into(),
        ));
    }
    match target {
        LinuxUninstallTarget::Flatpak {
            app_id,
            installation,
        } => {
            let current_installation = flatpak_installation(&app_id).ok_or_else(|| {
                PlatformError::Unsupported("this Flatpak installation is ambiguous".into())
            })?;
            if current_installation != installation {
                return Err(PlatformError::Unsupported(
                    "the Flatpak installation changed after the preview".into(),
                ));
            }
            let args = match installation {
                FlatpakInstallation::User => {
                    vec!["uninstall", "--user", "--app", app_id.as_str()]
                }
                FlatpakInstallation::System => {
                    vec!["uninstall", "--system", "--app", app_id.as_str()]
                }
            };
            run_interactive_manager("flatpak", &args, false)
                .map_err(|error| PlatformError::Os(format!("could not uninstall Flatpak: {error}")))
        }
        LinuxUninstallTarget::Snap { name } => {
            run_interactive_manager("snap", &["remove", name.as_str()], true)
                .map_err(|error| PlatformError::Os(format!("could not uninstall Snap: {error}")))
        }
        LinuxUninstallTarget::Package { manager, package } => {
            let (program, args) = manager.remove_command(&package);
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            run_interactive_manager(program, &args, true).map_err(|error| {
                PlatformError::Os(format!("could not uninstall package {package}: {error}"))
            })
        }
        LinuxUninstallTarget::AppImage { path } => trash_appimage(&path),
    }
}

enum LinuxUninstallTarget {
    Flatpak {
        app_id: String,
        installation: FlatpakInstallation,
    },
    Snap {
        name: String,
    },
    Package {
        manager: LinuxPackageManager,
        package: String,
    },
    AppImage {
        path: PathBuf,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum FlatpakInstallation {
    User,
    System,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinuxPackageManager {
    Apt,
    Dnf,
    Zypper,
    Pacman,
}

impl LinuxPackageManager {
    fn label(self) -> &'static str {
        match self {
            Self::Apt => "APT",
            Self::Dnf => "DNF",
            Self::Zypper => "Zypper",
            Self::Pacman => "Pacman",
        }
    }

    fn remove_command(self, package: &str) -> (&'static str, Vec<String>) {
        let package = package.to_owned();
        match self {
            Self::Apt => ("apt-get", vec!["remove".into(), "--".into(), package]),
            Self::Dnf => (
                if command_in_path("dnf") {
                    "dnf"
                } else {
                    "dnf5"
                },
                vec!["remove".into(), "--".into(), package],
            ),
            Self::Zypper => ("zypper", vec!["remove".into(), "--".into(), package]),
            Self::Pacman => ("pacman", vec!["-R".into(), "--".into(), package]),
        }
    }
}

fn linux_uninstall_target(path: &Path) -> Option<LinuxUninstallTarget> {
    let contents = fs::read_to_string(path).ok()?;
    let desktop_entry = parse_desktop_entry(&contents)?;
    let path_string = path.to_string_lossy();
    if let Some(app_id) = path.file_stem().and_then(OsStr::to_str) {
        if desktop_entry
            .flatpak_id
            .as_deref()
            .is_some_and(|metadata_id| metadata_id != app_id)
        {
            return None;
        }
        if let Some(installation) = flatpak_export_installation(path, app_id) {
            return interactive_route_available("flatpak", false).then_some(
                LinuxUninstallTarget::Flatpak {
                    app_id: app_id.to_owned(),
                    installation,
                },
            );
        }
    }
    let executable = desktop_entry.exec.as_deref()?;
    let command = desktop_exec_binary(executable)?;
    if path_string.contains("/snapd/desktop/applications/")
        || command.parent()? == Path::new("/snap/bin")
    {
        if command.parent()? != Path::new("/snap/bin") {
            return None;
        }
        let name = command.file_name()?.to_str()?.split('.').next()?.to_owned();
        return (valid_package_id(&name)
            && snap_is_installed(&name)
            && interactive_route_available("snap", true))
        .then_some(LinuxUninstallTarget::Snap { name });
    }
    if is_appimage_executable(&command) {
        return appimage_can_move_to_trash(&command)
            .then_some(LinuxUninstallTarget::AppImage { path: command });
    }
    let desktop_path = canonical_managed_path(path)?;
    let executable_path = canonical_managed_path(&command)?;
    distro_package_target(&desktop_path, &executable_path)
}

fn flatpak_export_installation(path: &Path, app_id: &str) -> Option<FlatpakInstallation> {
    if !valid_package_id(app_id) || path.extension() != Some(OsStr::new("desktop")) {
        return None;
    }
    let filename_id = path.file_stem()?.to_str()?;
    if filename_id != app_id {
        return None;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let roots = [
        (
            home.join(".local/share/flatpak/exports/share/applications"),
            FlatpakInstallation::User,
        ),
        (
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
            FlatpakInstallation::System,
        ),
    ];
    let installation = roots.iter().find_map(|(root, installation)| {
        let relative = path.strip_prefix(root).ok()?;
        (relative.components().count() == 1).then_some(*installation)
    })?;
    if flatpak_installation(app_id)? != installation {
        return None;
    }
    let args = match installation {
        FlatpakInstallation::User => vec!["info", "--show-location", "--user", app_id],
        FlatpakInstallation::System => vec!["info", "--show-location", "--system", app_id],
    };
    let output = Command::new("flatpak").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let location = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    let location = location.canonicalize().ok()?;
    let export_root = location.join("export/share/applications");
    let canonical_path = path.canonicalize().ok()?;
    let relative = canonical_path.strip_prefix(export_root).ok()?;
    (relative.components().count() == 1).then_some(installation)
}

fn distro_package_target(
    desktop_path: &Path,
    executable_path: &Path,
) -> Option<LinuxUninstallTarget> {
    let mut matches = Vec::new();
    if command_in_path("dpkg-query") && command_in_path("apt-get") {
        if let (Some(desktop_owner), Some(executable_owner)) = (
            dpkg_file_owner(desktop_path),
            dpkg_file_owner(executable_path),
        ) {
            if desktop_owner == executable_owner && valid_dpkg_package(&desktop_owner) {
                matches.push(LinuxUninstallTarget::Package {
                    manager: LinuxPackageManager::Apt,
                    package: desktop_owner,
                });
            }
        }
    }
    if command_in_path("rpm") {
        if let (Some(desktop_owner), Some(executable_owner)) = (
            rpm_file_owner(desktop_path),
            rpm_file_owner(executable_path),
        ) {
            if desktop_owner == executable_owner && valid_package_id(&desktop_owner) {
                if let Some(manager) = rpm_removal_manager() {
                    matches.push(LinuxUninstallTarget::Package {
                        manager,
                        package: desktop_owner,
                    });
                }
            }
        }
    }
    if command_in_path("pacman") {
        if let (Some(desktop_owner), Some(executable_owner)) = (
            pacman_file_owner(desktop_path),
            pacman_file_owner(executable_path),
        ) {
            if desktop_owner == executable_owner && valid_package_id(&desktop_owner) {
                matches.push(LinuxUninstallTarget::Package {
                    manager: LinuxPackageManager::Pacman,
                    package: desktop_owner,
                });
            }
        }
    }
    if matches.len() != 1 {
        return None;
    }
    let target = matches.pop()?;
    let LinuxUninstallTarget::Package { manager, .. } = target else {
        return None;
    };
    interactive_route_available(manager.remove_command("probe").0, true).then_some(target)
}

fn canonical_managed_path(path: &Path) -> Option<PathBuf> {
    let text = path.to_str()?;
    if !path.is_absolute() || has_glob_chars(text) || text.contains('\0') {
        return None;
    }
    let canonical = path.canonicalize().ok()?;
    let canonical_text = canonical.to_str()?;
    (!has_glob_chars(canonical_text)).then_some(canonical)
}

fn is_appimage_executable(path: &Path) -> bool {
    path.is_absolute()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("AppImage"))
        && !path.to_string_lossy().contains('%')
        && canonical_managed_path(path).is_some()
        && fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn appimage_can_move_to_trash(path: &Path) -> bool {
    let Some(uid) = effective_uid() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(file_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    let Ok(parent_metadata) = fs::metadata(parent) else {
        return false;
    };
    file_metadata.uid() == uid
        && parent_metadata.uid() == uid
        && parent_metadata.permissions().mode() & 0o200 != 0
        && (command_in_path("gio") || command_in_path("trash-put"))
}

fn trash_appimage(path: &Path) -> PlatformResult<()> {
    if !is_appimage_executable(path) || !appimage_can_move_to_trash(path) {
        return Err(PlatformError::Unsupported(
            "this AppImage cannot be moved to the current user's Trash".into(),
        ));
    }
    let result = if command_in_path("gio") {
        run_command("gio", [OsStr::new("trash"), path.as_os_str()])
    } else {
        run_command("trash-put", [path.as_os_str()])
    };
    result.map_err(|error| PlatformError::Os(format!("could not move AppImage to Trash: {error}")))
}

fn has_glob_chars(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, '*' | '?' | '[' | ']' | '{' | '}'))
}

fn desktop_exec_binary(exec: &str) -> Option<PathBuf> {
    if exec.contains('%') || exec.contains('\0') || exec.trim().is_empty() {
        return None;
    }
    let command = exec.trim_start();
    let binary = if let Some(quoted) = command.strip_prefix('"') {
        let end = quoted.find('"')?;
        let tail = &quoted[end + 1..];
        if !tail.is_empty() && !tail.chars().next().is_some_and(char::is_whitespace) {
            return None;
        }
        let binary = &quoted[..end];
        if binary.contains('\\') || binary.contains('"') {
            return None;
        }
        binary
    } else {
        command.split_whitespace().next()?
    };
    if binary.is_empty()
        || has_glob_chars(binary)
        || binary.chars().any(|character| {
            matches!(
                character,
                '$' | '`' | ';' | '|' | '&' | '<' | '>' | '\\' | '\'' | '"'
            )
        })
    {
        return None;
    }
    let path = Path::new(binary);
    if !path.is_absolute() || is_exec_wrapper(path) {
        return None;
    }
    Some(path.to_path_buf())
}

fn is_exec_wrapper(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return true;
    };
    matches!(
        name,
        "env"
            | "sh"
            | "bash"
            | "dash"
            | "zsh"
            | "fish"
            | "csh"
            | "tcsh"
            | "busybox"
            | "flatpak"
            | "snap"
            | "sudo"
            | "pkexec"
            | "systemd-run"
            | "xdg-open"
    ) || ["python", "python2", "python3", "perl", "ruby", "node"]
        .iter()
        .any(|prefix| {
            name == *prefix
                || name.starts_with(&format!("{prefix}"))
                    && name
                        .as_bytes()
                        .get(prefix.len())
                        .is_some_and(|byte| byte.is_ascii_digit())
        })
}

fn dpkg_file_owner(path: &Path) -> Option<String> {
    let output = Command::new("dpkg-query")
        .args([OsStr::new("-S"), OsStr::new("--"), path.as_os_str()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8(output.stdout).ok()?;
    let mut owners = output.lines().filter_map(|line| {
        let (package, owned_path) = line.split_once(": ")?;
        (owned_path == path.to_str()?).then(|| package.to_owned())
    });
    let owner = owners.next()?;
    owners.next().is_none().then_some(owner)
}

fn rpm_file_owner(path: &Path) -> Option<String> {
    let output = Command::new("rpm")
        .args([
            OsStr::new("-qf"),
            OsStr::new("--qf"),
            OsStr::new("%{NAME}\\n"),
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8(output.stdout).ok()?;
    let mut owners = output.lines().filter(|line| !line.is_empty());
    let owner = owners.next()?.to_owned();
    owners.next().is_none().then_some(owner)
}

fn pacman_file_owner(path: &Path) -> Option<String> {
    let output = Command::new("pacman")
        .args([OsStr::new("-Qoq"), OsStr::new("--"), path.as_os_str()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8(output.stdout).ok()?;
    let mut owners = output.lines().filter(|line| !line.is_empty());
    let owner = owners.next()?.to_owned();
    owners.next().is_none().then_some(owner)
}

fn flatpak_installation(app_id: &str) -> Option<FlatpakInstallation> {
    let output = Command::new("flatpak")
        .args(["list", "--columns=application,installation"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let installations = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?))
        })
        .filter_map(|(installed_id, installation)| {
            if installed_id != app_id {
                return None;
            }
            match installation {
                "user" => Some(FlatpakInstallation::User),
                "system" => Some(FlatpakInstallation::System),
                _ => None,
            }
        })
        .collect::<HashSet<_>>();
    (installations.len() == 1).then(|| *installations.iter().next().unwrap())
}

fn snap_is_installed(name: &str) -> bool {
    let output = Command::new("snap").args(["list", name]).output();
    let Ok(output) = output else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .skip(1)
            .any(|line| line.split_whitespace().next() == Some(name))
}

fn valid_dpkg_package(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '_' | '-' | ':')
        })
}

fn rpm_removal_manager() -> Option<LinuxPackageManager> {
    let os_release = fs::read_to_string("/etc/os-release").ok()?;
    let values = os_release
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key, value.trim_matches('"')))
        .collect::<std::collections::HashMap<_, _>>();
    let ids = [values.get("ID"), values.get("ID_LIKE")]
        .into_iter()
        .flatten()
        .flat_map(|value| value.split_whitespace())
        .collect::<Vec<_>>();
    let manager = if ids.iter().any(|id| {
        matches!(
            *id,
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "ol"
        )
    }) {
        if command_in_path("dnf") {
            Some(LinuxPackageManager::Dnf)
        } else if command_in_path("dnf5") {
            Some(LinuxPackageManager::Dnf)
        } else {
            None
        }
    } else if ids.iter().any(|id| {
        matches!(
            *id,
            "suse" | "opensuse" | "opensuse-leap" | "opensuse-tumbleweed"
        )
    }) {
        command_in_path("zypper").then_some(LinuxPackageManager::Zypper)
    } else {
        None
    }?;
    Some(manager)
}

fn interactive_route_available(manager_command: &str, needs_privilege: bool) -> bool {
    command_in_path(manager_command)
        && terminal_emulator().is_some()
        && (!needs_privilege || command_in_path("pkexec") || effective_uid() == Some(0))
}

fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let ids = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .collect::<Vec<_>>();
    ids.get(1)?.parse().ok()
}

fn terminal_emulator() -> Option<(&'static str, TerminalKind)> {
    [
        ("gnome-terminal", TerminalKind::Gnome),
        ("xterm", TerminalKind::Xterm),
    ]
    .into_iter()
    .find(|(program, _)| command_in_path(program))
}

#[derive(Clone, Copy)]
enum TerminalKind {
    Gnome,
    Xterm,
}

fn command_in_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(program))
        .any(|path| {
            path.is_file()
                && fs::metadata(path)
                    .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
        })
}

fn run_interactive_manager(
    program: &str,
    args: &[&str],
    needs_privilege: bool,
) -> Result<(), String> {
    let (terminal_program, terminal_kind) = terminal_emulator()
        .ok_or_else(|| "no supported terminal emulator is available".to_owned())?;
    let command = if needs_privilege && effective_uid() != Some(0) {
        if !command_in_path("pkexec") {
            return Err("this action needs pkexec or a root session".into());
        }
        let mut command = vec!["pkexec".to_owned(), program.to_owned()];
        command.extend(args.iter().map(|arg| (*arg).to_owned()));
        command
    } else {
        let mut command = vec![program.to_owned()];
        command.extend(args.iter().map(|arg| (*arg).to_owned()));
        command
    };
    let terminal_args: Vec<String> = match terminal_kind {
        TerminalKind::Gnome => vec!["--wait".into(), "--".into()],
        TerminalKind::Xterm => vec!["-e".into()],
    };
    let mut terminal = Command::new(terminal_program);
    terminal.args(terminal_args);
    terminal.args(command);
    let status = terminal
        .status()
        .map_err(|error| format!("could not start {terminal_program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{terminal_program} exited with {status}"))
    }
}

fn valid_package_id(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

pub fn frontmost_app_info() -> Option<(i32, String)> {
    if std::env::var_os("DISPLAY").is_none() || std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return None;
    }
    let focused_window = Command::new("xdotool")
        .arg("getwindowfocus")
        .output()
        .ok()?;
    if !focused_window.status.success() {
        return None;
    }
    let window_id = String::from_utf8_lossy(&focused_window.stdout)
        .trim()
        .to_owned();
    let output = Command::new("xdotool")
        .args(["getwindowpid", window_id.as_str()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i32>()
        .ok()?;
    if pid == std::process::id() as i32 {
        return None;
    }
    let process_name = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some((pid, process_name.trim().to_owned()))
}

pub fn activate_app(pid: i32) -> PlatformResult<()> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_none() {
        return Err(PlatformError::Unsupported(
            "activating a previous app requires an X11 session".into(),
        ));
    }
    let output = Command::new("wmctrl")
        .args(["-lp"])
        .output()
        .map_err(|error| command_error("run wmctrl", error))?;
    if !output.status.success() {
        return Err(PlatformError::Os(format!(
            "wmctrl could not list windows: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let window_id = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let window_id = fields.next()?;
            let _desktop = fields.next()?;
            let window_pid = fields.next()?.parse::<i32>().ok()?;
            (window_pid == pid).then(|| window_id.to_owned())
        })
        .ok_or_else(|| PlatformError::Os("the previous app has no activatable window".into()))?;
    run_command("wmctrl", [OsStr::new("-ia"), OsStr::new(&window_id)])
        .map_err(|error| PlatformError::Os(format!("could not activate the previous app: {error}")))
}

pub fn send_paste_keystroke() -> PlatformResult<()> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        match run_command(
            "wtype",
            [
                OsStr::new("-M"),
                OsStr::new("ctrl"),
                OsStr::new("-k"),
                OsStr::new("v"),
                OsStr::new("-m"),
                OsStr::new("ctrl"),
            ],
        ) {
            Ok(()) => return Ok(()),
            Err(wtype_error) => {
                return run_command(
                    "ydotool",
                    [
                        OsStr::new("key"),
                        OsStr::new("29:1"),
                        OsStr::new("47:1"),
                        OsStr::new("47:0"),
                        OsStr::new("29:0"),
                    ],
                )
                .map_err(|ydotool_error| {
                    PlatformError::Unsupported(format!(
                        "Wayland paste needs wtype or a running ydotoold service: {wtype_error}; {ydotool_error}"
                    ))
                });
            }
        }
    }
    run_command(
        "xdotool",
        [
            OsStr::new("key"),
            OsStr::new("--clearmodifiers"),
            OsStr::new("ctrl+v"),
        ],
    )
    .map_err(|error| PlatformError::Unsupported(format!("X11 paste needs xdotool: {error}")))
}

pub fn adjust_brightness_with_level(delta: f32) -> PlatformResult<f32> {
    if !delta.is_finite() {
        return Err(PlatformError::Os("brightness change must be finite".into()));
    }
    let before = brightnessctl_level();
    let percent = ((delta.abs() * 100.0).round() as u32).max(1);
    let change = format!("{percent}%{}", if delta >= 0.0 { "+" } else { "-" });
    let brightnessctl_status = Command::new("brightnessctl")
        .args(["set", change.as_str()])
        .status()
        .ok()
        .filter(|status| status.success());
    if brightnessctl_status.is_some() {
        if let Some(level) = brightnessctl_level() {
            return Ok(level);
        }
        if let Some(previous) = before {
            return Ok((previous + delta * 100.0).clamp(0.0, 100.0));
        }
        return Err(PlatformError::Os(
            "brightnessctl changed brightness but did not return a level".into(),
        ));
    }

    let devices = fs::read_dir("/sys/class/backlight").map_err(|error| {
        PlatformError::Unsupported(format!(
            "brightness control needs brightnessctl or a writable /sys/class/backlight device: {error}"
        ))
    })?;
    let mut last_error = None;
    for device in devices.flatten() {
        let directory = device.path();
        let current_path = directory.join("brightness");
        let max_path = directory.join("max_brightness");
        let parsed = fs::read_to_string(&current_path)
            .and_then(|value| value.trim().parse::<u64>().map_err(std::io::Error::other))
            .and_then(|current| {
                fs::read_to_string(&max_path)
                    .and_then(|value| value.trim().parse::<u64>().map_err(std::io::Error::other))
                    .map(|max| (current, max))
            });
        let (current, max) = match parsed {
            Ok(values) if values.1 > 0 => values,
            Ok(_) => continue,
            Err(error) => {
                last_error = Some(error.to_string());
                continue;
            }
        };
        let target = (current as f64 + f64::from(delta) * max as f64)
            .round()
            .clamp(0.0, max as f64) as u64;
        match fs::write(&current_path, target.to_string()) {
            Ok(()) => return Ok(target as f32 * 100.0 / max as f32),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(PlatformError::Unsupported(format!(
        "brightness control is not available: {}",
        last_error.unwrap_or_else(|| "no backlight device was found".into())
    )))
}

pub fn adjust_audio_output_with_level(delta: f32) -> PlatformResult<f32> {
    if !delta.is_finite() {
        return Err(PlatformError::Os("volume change must be finite".into()));
    }
    let percent = (delta.clamp(-1.0, 1.0).abs() * 100.0).round() as u32;
    let change = format!("{percent}%{}", if delta >= 0.0 { "+" } else { "-" });
    let pulse_change = format!("{}{percent}%", if delta >= 0.0 { "+" } else { "-" });
    let mixer_change = format!("{percent}%{}", if delta >= 0.0 { "+" } else { "-" });
    let attempts = [
        (
            "wpctl",
            vec![
                "set-volume",
                "-l",
                "1.5",
                "@DEFAULT_AUDIO_SINK@",
                change.as_str(),
            ],
        ),
        (
            "pactl",
            vec!["set-sink-volume", "@DEFAULT_SINK@", pulse_change.as_str()],
        ),
        ("amixer", vec!["set", "Master", mixer_change.as_str()]),
    ];
    let mut last_error = None;
    let before = super::audio_output_level();
    for (program, args) in attempts {
        match run_command(program, args) {
            Ok(()) => {
                if let Some(level) = super::audio_output_level() {
                    return Ok(level);
                }
                if let Some(previous) = before {
                    return Ok((previous + delta.clamp(-1.0, 1.0) * 100.0).clamp(0.0, 100.0));
                }
                return Err(PlatformError::Os(
                    "volume changed but the new level is unavailable".into(),
                ));
            }
            Err(error) => last_error = Some(format!("{program}: {error}")),
        }
    }
    Err(PlatformError::Unsupported(format!(
        "could not change audio output volume: {}",
        last_error.unwrap_or_else(|| "no supported audio utility was found".into())
    )))
}

fn brightnessctl_level() -> Option<f32> {
    let current = Command::new("brightnessctl").arg("get").output().ok()?;
    let maximum = Command::new("brightnessctl").arg("max").output().ok()?;
    if !current.status.success() || !maximum.status.success() {
        return None;
    }
    let current = String::from_utf8_lossy(&current.stdout)
        .trim()
        .parse::<f32>()
        .ok()?;
    let maximum = String::from_utf8_lossy(&maximum.stdout)
        .trim()
        .parse::<f32>()
        .ok()?;
    (maximum > 0.0).then_some((current * 100.0 / maximum).clamp(0.0, 100.0))
}

fn run_first_success(candidates: &[(&str, &[&str])], action: &str) -> PlatformResult<()> {
    let mut last_error = None;
    for (program, args) in candidates {
        match Command::new(program).args(*args).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => last_error = Some(format!("{program} exited with {status}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => last_error = Some(format!("{program}: {error}")),
        }
    }
    Err(PlatformError::Unsupported(format!(
        "could not {action}: {}",
        last_error.unwrap_or_else(|| "no supported desktop utility was found".into())
    )))
}

fn run_command<I, S>(program: &str, args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn spawn_command<I, S>(program: &str, args: I) -> PlatformResult<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| PlatformError::Unsupported(format!("could not start {program}: {error}")))
}

enum ClipboardAttemptError {
    Unavailable,
    Failed(String),
}

fn copy_to_clipboard(
    program: &str,
    args: &[&str],
    text: &str,
) -> Result<(), ClipboardAttemptError> {
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ClipboardAttemptError::Unavailable);
        }
        Err(error) => return Err(ClipboardAttemptError::Failed(error.to_string())),
    };

    let write_error = child.stdin.take().and_then(|mut stdin| {
        stdin
            .write_all(text.as_bytes())
            .err()
            .map(|error| error.to_string())
    });
    let output = child
        .wait_with_output()
        .map_err(|error| ClipboardAttemptError::Failed(error.to_string()))?;
    if let Some(error) = write_error {
        return Err(ClipboardAttemptError::Failed(error));
    }
    if output.status.success() {
        Ok(())
    } else {
        Err(ClipboardAttemptError::Failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

fn application_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        directories.push(PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = std::env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let data_dirs =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for directory in std::env::split_paths(&data_dirs) {
        directories.push(directory.join("applications"));
    }
    directories.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    if let Some(home) = std::env::var_os("HOME") {
        directories
            .push(PathBuf::from(home).join(".local/share/flatpak/exports/share/applications"));
    }
    directories.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
    directories
}

fn desktop_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = fs::read_dir(current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
            } else if path.extension() == Some(OsStr::new("desktop")) {
                files.push(path);
            }
        }
    }
    files
}

struct DesktopEntry {
    name: String,
    hidden: bool,
    icon: Option<String>,
    exec: Option<String>,
    flatpak_id: Option<String>,
}

fn parse_desktop_entry(contents: &str) -> Option<DesktopEntry> {
    let mut in_desktop_entry = false;
    let mut is_application = false;
    let mut hidden = false;
    let mut name = None;
    let mut icon = None;
    let mut exec = None;
    let mut flatpak_id = None;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => is_application = value == "Application",
            "Name" => name = Some(unescape_desktop_value(value)),
            "Icon" => icon = Some(unescape_desktop_value(value)),
            "Exec" => exec = Some(unescape_desktop_value(value)),
            "X-Flatpak" => flatpak_id = Some(unescape_desktop_value(value)),
            "Hidden" | "NoDisplay" => hidden |= value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    is_application.then(|| DesktopEntry {
        name: name.unwrap_or_default(),
        hidden,
        icon,
        exec,
        flatpak_id,
    })
}

fn resolve_desktop_icon(icon: &str) -> Option<PathBuf> {
    let icon = icon.trim();
    if icon.is_empty() {
        return None;
    }

    let icon_path = Path::new(icon);
    if icon_path.is_absolute() {
        return png_file(icon_path).or_else(|| rasterize_svg(icon_path));
    }

    let file_name = Path::new(icon).file_name()?.to_string_lossy();
    let base_name = Path::new(file_name.as_ref())
        .file_stem()
        .filter(|_| {
            Path::new(file_name.as_ref())
                .extension()
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("png") || extension.eq_ignore_ascii_case("svg")
                })
        })
        .unwrap_or_else(|| OsStr::new(file_name.as_ref()))
        .to_string_lossy();
    let png_names = [format!("{base_name}.png"), file_name.to_string()];
    let svg_names = [format!("{base_name}.svg")];

    let roots = icon_theme_roots();
    let themes = icon_themes();

    for root in &roots {
        for theme in themes {
            for size in [
                "48x48", "64x64", "128x128", "256x256", "32x32", "scalable", "24x24", "16x16",
            ] {
                for category in ["apps", "mimetypes", "places"] {
                    for name in &png_names {
                        let path = root.join(theme).join(size).join(category).join(name);
                        if let Some(path) = png_file(&path) {
                            return Some(path);
                        }
                    }
                }
            }
            for name in &png_names {
                let path = root.join(theme).join(name);
                if let Some(path) = png_file(&path) {
                    return Some(path);
                }
            }
        }
    }

    for root in &roots {
        for theme in themes {
            for size in [
                "48x48", "64x64", "128x128", "256x256", "32x32", "scalable", "24x24", "16x16",
            ] {
                for category in ["apps", "mimetypes", "places"] {
                    for name in &svg_names {
                        let path = root.join(theme).join(size).join(category).join(name);
                        if let Some(path) = rasterize_svg(&path) {
                            return Some(path);
                        }
                    }
                }
            }
            for name in &svg_names {
                let path = root.join(theme).join(name);
                if let Some(path) = rasterize_svg(&path) {
                    return Some(path);
                }
            }
        }
    }
    for root in pixmap_roots() {
        for name in &png_names {
            let path = root.join(name);
            if let Some(path) = png_file(&path) {
                return Some(path);
            }
        }
    }
    for root in pixmap_roots() {
        for name in &svg_names {
            let path = root.join(name);
            if let Some(path) = rasterize_svg(&path) {
                return Some(path);
            }
        }
    }
    None
}

fn rasterize_svg(path: &Path) -> Option<PathBuf> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        || !path.is_file()
    {
        return None;
    }

    let metadata = fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    modified.hash(&mut hasher);
    let cache_name = format!("{:016x}.png", hasher.finish());
    let cache_directory = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?
        .join("corvo/icons");
    fs::create_dir_all(&cache_directory).ok()?;
    let output = cache_directory.join(cache_name);
    if output.is_file() {
        return Some(output);
    }
    let temporary = cache_directory.join(format!(".icon-{}.png", std::process::id()));
    let Some(renderer) = svg_renderer() else {
        return None;
    };
    let rendered = match renderer {
        SvgRenderer::RsvgConvert => Command::new("rsvg-convert")
            .args(["-w", "128", "-h", "128", "-o"])
            .arg(&temporary)
            .arg(path)
            .status(),
        SvgRenderer::Inkscape => Command::new("inkscape")
            .arg(path)
            .arg(format!("--export-filename={}", temporary.display()))
            .args(["--export-width=128", "--export-height=128"])
            .status(),
        SvgRenderer::Magick => Command::new("magick")
            .arg("convert")
            .arg("-background")
            .arg("none")
            .arg(path)
            .arg("-resize")
            .arg("128x128")
            .arg(&temporary)
            .status(),
        SvgRenderer::Convert => Command::new("convert")
            .arg("-background")
            .arg("none")
            .arg(path)
            .arg("-resize")
            .arg("128x128")
            .arg(&temporary)
            .status(),
    }
    .is_ok_and(|status| status.success());
    if !rendered || !temporary.is_file() {
        let _ = fs::remove_file(&temporary);
        return None;
    }
    if fs::rename(&temporary, &output).is_err() {
        let _ = fs::remove_file(&temporary);
    }
    output.is_file().then_some(output)
}

#[derive(Clone, Copy)]
enum SvgRenderer {
    RsvgConvert,
    Inkscape,
    Magick,
    Convert,
}

fn svg_renderer() -> Option<SvgRenderer> {
    static RENDERER: OnceLock<Option<SvgRenderer>> = OnceLock::new();
    *RENDERER.get_or_init(|| {
        [
            ("rsvg-convert", SvgRenderer::RsvgConvert),
            ("inkscape", SvgRenderer::Inkscape),
            ("magick", SvgRenderer::Magick),
            ("convert", SvgRenderer::Convert),
        ]
        .into_iter()
        .find_map(|(program, renderer)| {
            Command::new(program)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .ok()
                .filter(|status| status.success())
                .map(|_| renderer)
        })
    })
}

fn png_file(path: &Path) -> Option<PathBuf> {
    (path.extension()?.eq_ignore_ascii_case("png") && path.is_file()).then(|| path.to_path_buf())
}

fn icon_theme_roots() -> Vec<PathBuf> {
    static ROOTS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    ROOTS
        .get_or_init(|| {
            let mut roots = Vec::new();
            if let Some(home) = std::env::var_os("XDG_DATA_HOME") {
                roots.push(PathBuf::from(home).join("icons"));
            } else if let Some(home) = std::env::var_os("HOME") {
                roots.push(PathBuf::from(home).join(".local/share/icons"));
            }
            if let Some(home) = std::env::var_os("HOME") {
                roots.push(PathBuf::from(home).join(".icons"));
            }
            let data_dirs = std::env::var_os("XDG_DATA_DIRS")
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
            roots
                .extend(std::env::split_paths(&data_dirs).map(|directory| directory.join("icons")));
            roots
        })
        .clone()
}

fn icon_themes() -> &'static [String] {
    static THEMES: OnceLock<Vec<String>> = OnceLock::new();
    THEMES.get_or_init(|| {
        let roots = icon_theme_roots();
        let mut themes = selected_icon_themes();
        themes.push("hicolor".to_owned());
        for root in &roots {
            if let Ok(entries) = fs::read_dir(root) {
                themes.extend(entries.flatten().filter_map(|entry| {
                    entry
                        .file_type()
                        .ok()
                        .filter(|kind| kind.is_dir())
                        .map(|_| entry.file_name().to_string_lossy().into_owned())
                }));
            }
        }
        let mut seen = HashSet::new();
        themes.retain(|theme| seen.insert(theme.clone()));
        themes
    })
}

fn pixmap_roots() -> Vec<PathBuf> {
    static ROOTS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    ROOTS
        .get_or_init(|| {
            let mut roots = vec![
                PathBuf::from("/usr/local/share/pixmaps"),
                PathBuf::from("/usr/share/pixmaps"),
            ];
            if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
                roots.push(PathBuf::from(data_home).join("pixmaps"));
            }
            let data_dirs = std::env::var_os("XDG_DATA_DIRS")
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
            roots.extend(
                std::env::split_paths(&data_dirs).map(|directory| directory.join("pixmaps")),
            );
            roots
        })
        .clone()
}

fn selected_icon_themes() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    ["gtk-4.0/settings.ini", "gtk-3.0/settings.ini"]
        .into_iter()
        .filter_map(|relative| fs::read_to_string(home.join(".config").join(relative)).ok())
        .flat_map(|contents| {
            contents
                .lines()
                .filter_map(|line| {
                    let (key, value) = line.trim().split_once('=')?;
                    (key.trim() == "gtk-icon-theme-name")
                        .then(|| value.trim().trim_matches(['"', '\'']).to_owned())
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn unescape_desktop_value(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            match chars.next() {
                Some('s') => decoded.push(' '),
                Some('n') => decoded.push('\n'),
                Some('t') => decoded.push('\t'),
                Some('r') => decoded.push('\r'),
                Some('\\') => decoded.push('\\'),
                Some(other) => {
                    decoded.push('\\');
                    decoded.push(other);
                }
                None => decoded.push('\\'),
            }
        } else {
            decoded.push(character);
        }
    }
    decoded
}

fn parse_wmctrl_line(line: &str) -> Option<WindowHandle> {
    let mut fields = line.split_whitespace();
    let id = u64::from_str_radix(fields.next()?.trim_start_matches("0x"), 16).ok()?;
    let _desktop = fields.next()?;
    let _pid = fields.next()?;
    let _host = fields.next()?;
    let title = fields.collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return None;
    }
    Some(WindowHandle {
        id,
        title,
        app_name: None,
    })
}

fn command_error(context: &str, error: std::io::Error) -> PlatformError {
    if error.kind() == std::io::ErrorKind::NotFound {
        PlatformError::Unsupported(format!("{context} requires wmctrl"))
    } else {
        PlatformError::Os(format!("could not {context}: {error}"))
    }
}

/// Queries whether launch at login is enabled via ~/.config/autostart/corvo.desktop.
pub fn is_launch_at_login_enabled() -> bool {
    if let Some(config_dir) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    {
        config_dir.join("autostart/corvo.desktop").exists()
    } else {
        false
    }
}

/// Enables or disables launch at login via ~/.config/autostart/corvo.desktop on Linux.
pub fn set_launch_at_login(enabled: bool) -> PlatformResult<()> {
    let autostart_dir = if let Some(config_dir) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    {
        config_dir.join("autostart")
    } else {
        return Err(PlatformError::Os("could not resolve autostart directory".into()));
    };

    let desktop_path = autostart_dir.join("corvo.desktop");
    if enabled {
        let _ = std::fs::create_dir_all(&autostart_dir);
        let exe = std::env::current_exe().map_err(|e| PlatformError::Os(e.to_string()))?;
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=Corvo\nComment=Fast keyboard launcher\nExec=\"{}\"\nTerminal=false\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        std::fs::write(&desktop_path, content)
            .map_err(|e| PlatformError::Os(format!("failed to write autostart desktop entry: {e}")))
    } else {
        if desktop_path.exists() {
            let _ = std::fs::remove_file(desktop_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_desktop_entry, parse_wmctrl_line};

    #[test]
    fn parses_visible_desktop_application() {
        let entry = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Example\\sApp\nExec=example\n",
        )
        .unwrap();
        assert_eq!(entry.name, "Example App");
    }

    #[test]
    fn ignores_hidden_desktop_application() {
        assert!(
            parse_desktop_entry("[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\n")
                .is_some_and(|entry| entry.hidden)
        );
    }

    #[test]
    fn parses_x11_window_id_and_title() {
        let window = parse_wmctrl_line("0x04600007  0  1234 host Example Window").unwrap();
        assert_eq!(window.id, 0x04600007);
        assert_eq!(window.title, "Example Window");
    }

    #[test]
    fn launch_at_login_toggles_autostart_entry() {
        let dir = std::env::temp_dir().join(format!(
            "corvo-autostart-test-{}",
            std::process::id()
        ));
        let prior = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!super::is_launch_at_login_enabled());
        super::set_launch_at_login(true).expect("enables launch at login");
        assert!(super::is_launch_at_login_enabled());
        super::set_launch_at_login(false).expect("disables launch at login");
        assert!(!super::is_launch_at_login_enabled());
        let _ = std::fs::remove_dir_all(&dir);
        match prior {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}

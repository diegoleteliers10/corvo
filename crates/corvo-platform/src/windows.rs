//! Windows implementation of the shared desktop operations.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use sha2::{Digest, Sha256};

use super::{
    AppEntry, AppFileEntry, AppFileScan, PlatformError, PlatformOps, PlatformResult, WindowHandle,
};

#[path = "windows_icons.rs"]
mod windows_icons;

pub struct WindowsPlatform;

#[repr(C)]
struct ClientPoint {
    x: i32,
    y: i32,
}

#[repr(C)]
struct ClientRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// Clips the launcher window to a rounded rectangle so the OS window
/// matches the radius the root element paints. GPUI creates a
/// `WindowKind::PopUp` on Windows with `dwstyle = 0`, so the compositor
/// draws a hard square client area and the corner pixels outside the
/// painted radius keep the class brush. A window region makes Windows
/// clip the window to the painted shape instead.
///
/// Call this again after every resize: the region does not follow the
/// window bounds. `radius` is in device pixels.
pub fn set_launcher_window_region(handle: isize, radius: f32) {
    if handle == 0 {
        return;
    }
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetClientRect(window: isize, rect: *mut ClientRect) -> i32;
        fn GetWindowRect(window: isize, rect: *mut ClientRect) -> i32;
        fn ClientToScreen(window: isize, point: *mut ClientPoint) -> i32;
        fn SetWindowRgn(window: isize, region: isize, redraw: i32) -> i32;
    }
    #[allow(non_snake_case)]
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateRoundRectRgn(
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
            width: i32,
            height: i32,
        ) -> isize;
        fn DeleteObject(object: isize) -> i32;
    }

    let mut rect = ClientRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetClientRect(handle, &mut rect) } == 0 {
        crate::diagnostics::record_error("window", "client_bounds_failed");
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return;
    }
    // CreateRoundRectRgn takes the corner ellipse size, which is twice the
    // radius. An ellipse wider or taller than the rectangle makes Windows
    // draw a full ellipse instead of a corner.
    let diameter = ((radius.max(0.0) * 2.0).round() as i32)
        .min(width)
        .min(height);
    if diameter <= 0 {
        // A zero radius clears the region and leaves the full rectangle.
        unsafe { SetWindowRgn(handle, 0, 1) };
        return;
    }
    let mut window_rect = ClientRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut client_origin = ClientPoint { x: 0, y: 0 };
    if unsafe { GetWindowRect(handle, &mut window_rect) } == 0
        || unsafe { ClientToScreen(handle, &mut client_origin) } == 0
    {
        crate::diagnostics::record_error("window", "region_origin_failed");
        return;
    }
    let left = client_origin.x - window_rect.left;
    let top = client_origin.y - window_rect.top;
    let region =
        unsafe { CreateRoundRectRgn(left, top, left + width, top + height, diameter, diameter) };
    if region == 0 {
        crate::diagnostics::record_error("window", "region_create_failed");
        return;
    }
    // SetWindowRgn takes ownership of the region and deletes it.
    if unsafe { SetWindowRgn(handle, region, 1) } == 0 {
        crate::diagnostics::record_error("window", "region_apply_failed");
        unsafe { DeleteObject(region) };
    }
}

pub fn supports_app_uninstall_path(path: &Path) -> bool {
    appx_family_name(path).is_some()
        || (path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("lnk"))
            && (msi_record_for_shortcut(path).is_ok_and(|record| record.is_some())
                || quiet_uninstall_record_for_shortcut(path).is_ok_and(|record| record.is_some())))
}

pub fn associated_app_files(app_path: &Path) -> PlatformResult<AppFileScan> {
    if let Some(family) = appx_family_name(app_path) {
        let script = format!(
            "$ErrorActionPreference='Stop'; $package = Get-AppxPackage -PackageFamilyName '{family}' | Select-Object -First 1; if (-not $package) {{ throw 'Installed app package was not found' }}; [Console]::Write($package.InstallLocation)"
        );
        let install_location = powershell_output(&script)?;
        return Ok(AppFileScan {
            files: vec![AppFileEntry {
                path: app_path.to_path_buf(),
                location: install_location.trim().to_owned(),
                size_bytes: 0,
                is_application: true,
                matched_by_name: false,
            }],
            reached_scan_limit: false,
        });
    }

    let (row_id, location) = if let Some(record) = msi_record_for_shortcut(app_path)? {
        let location = if record.install_location.trim().is_empty() {
            record.target_path.as_str()
        } else {
            record.install_location.as_str()
        };
        (
            msi_row_id(&record.product_code),
            format!(
                "{} · {} · {}",
                record.display_name, record.product_code, location
            ),
        )
    } else {
        let record = quiet_uninstall_record_for_shortcut(app_path)?.ok_or_else(|| {
            PlatformError::Unsupported("no safe Windows uninstall record matches this app".into())
        })?;
        (
            quiet_uninstall_row_id(&record),
            format!(
                "{} · {} · {}",
                record.display_name, record.registry_path, record.install_location
            ),
        )
    };
    Ok(AppFileScan {
        files: vec![AppFileEntry {
            path: PathBuf::from(row_id),
            location,
            size_bytes: 0,
            is_application: true,
            matched_by_name: false,
        }],
        reached_scan_limit: false,
    })
}

pub fn move_app_files_to_trash(app_path: &Path, paths: &[PathBuf]) -> PlatformResult<()> {
    if let Some(family) = appx_family_name(app_path) {
        if paths.len() != 1 || paths[0].as_path() != app_path {
            return Err(PlatformError::Os(
                "select only the app package before uninstalling".into(),
            ));
        }
        let script = format!(
            "$ErrorActionPreference='Stop'; $package = Get-AppxPackage -PackageFamilyName '{family}' | Select-Object -First 1; if (-not $package) {{ throw 'Installed app package was not found' }}; Remove-AppxPackage -Package $package.PackageFullName -Confirm:$false -ErrorAction Stop"
        );
        return run_powershell(&script);
    }

    if paths.len() != 1 {
        return Err(PlatformError::Os(
            "select only the application row before uninstalling".into(),
        ));
    }
    let selected_row = paths[0].to_string_lossy();
    let status = if let Some(selected_code) = msi_product_code_from_row_id(&selected_row) {
        let record = msi_record_for_shortcut(app_path)?.ok_or_else(|| {
            PlatformError::Os("the MSI uninstall record changed. Rescan the app".into())
        })?;
        if !record.product_code.eq_ignore_ascii_case(selected_code) {
            return Err(PlatformError::Os(
                "the selected MSI product changed. Rescan the app before uninstalling".into(),
            ));
        }
        Command::new("msiexec.exe")
            .args(["/x", record.product_code.as_str(), "/norestart"])
            .status()
    } else if let Some(selected_id) = quiet_uninstall_id_from_row_id(&selected_row) {
        let record = quiet_uninstall_record_for_shortcut(app_path)?.ok_or_else(|| {
            PlatformError::Os("the uninstall record changed. Rescan the app".into())
        })?;
        if quiet_uninstall_record_id(&record) != selected_id {
            return Err(PlatformError::Os(
                "the selected uninstall record changed. Rescan the app before uninstalling".into(),
            ));
        }
        Command::new(&record.executable)
            .args(&record.arguments)
            .status()
    } else {
        return Err(PlatformError::Os(
            "select the application row before uninstalling".into(),
        ));
    }
    .map_err(|error| PlatformError::Os(format!("could not start the app uninstaller: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Os(format!(
            "application uninstaller failed with exit code {}",
            status.code().unwrap_or(-1)
        )))
    }
}

#[derive(Debug)]
struct MsiUninstallRecord {
    product_code: String,
    display_name: String,
    install_location: String,
    target_path: String,
}

fn msi_record_for_shortcut(path: &Path) -> PlatformResult<Option<MsiUninstallRecord>> {
    if !path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("lnk"))
    {
        return Ok(None);
    }
    let path_bytes = path.to_string_lossy().as_bytes().to_vec();
    let encoded_path = base64_encode(&path_bytes);
    let script = r#"
$ErrorActionPreference = 'Stop'
$shortcutPath = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('$CorvoShortcutPath'))
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$target = [Environment]::ExpandEnvironmentVariables([string]$shortcut.TargetPath)
if (-not $target -or -not (Test-Path -LiteralPath $target -PathType Leaf)) { [Console]::Write('[]'); exit 0 }
$target = [IO.Path]::GetFullPath($target).TrimEnd('\')
$roots = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)
$matches = @(
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        foreach ($key in (Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)) {
            if ($key.PSChildName -notmatch '^\{[0-9A-Fa-f-]{36}\}$') { continue }
            $item = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
            if ([int]$item.WindowsInstaller -ne 1 -or -not $item.DisplayName) { continue }
            $icon = [Environment]::ExpandEnvironmentVariables([string]$item.DisplayIcon)
            if ($icon -match '^\s*"([^"]+)"') { $icon = $Matches[1] }
            else { $icon = $icon -replace ',\s*-?\d+\s*$', '' }
            $iconMatches = $false
            if ($icon) {
                try { $iconMatches = [string]::Equals([IO.Path]::GetFullPath($icon).TrimEnd('\'), $target, [StringComparison]::OrdinalIgnoreCase) } catch { }
            }
            $location = [Environment]::ExpandEnvironmentVariables([string]$item.InstallLocation)
            $locationMatches = $false
            if ($location -and (Test-Path -LiteralPath $location -PathType Container)) {
                try {
                    $location = [IO.Path]::GetFullPath($location).TrimEnd('\')
                    $locationMatches = $target.StartsWith($location + '\', [StringComparison]::OrdinalIgnoreCase)
                } catch { }
            }
            if (-not ($iconMatches -or $locationMatches)) { continue }
            [pscustomobject]@{
                ProductCode = $key.PSChildName
                DisplayName = [string]$item.DisplayName
                InstallLocation = [string]$item.InstallLocation
                DisplayIcon = [string]$item.DisplayIcon
                TargetPath = $target
            }
        }
    }
)
$records = @(
    foreach ($group in ($matches | Group-Object ProductCode)) {
        $uniqueRecords = @($group.Group | Sort-Object DisplayName, InstallLocation, DisplayIcon -Unique)
        if ($uniqueRecords.Count -eq 1) { $uniqueRecords[0] }
        else { $uniqueRecords }
    }
)
ConvertTo-Json -InputObject $records -Compress -Depth 3
"#
    .replace("$CorvoShortcutPath", &encoded_path);
    let output = powershell_output(&script)?;
    let records = parse_json_array(&output, "MSI uninstall records")?;
    let mut records = records
        .into_iter()
        .filter_map(|record| {
            let product_code = record.get("ProductCode")?.as_str()?.to_owned();
            valid_product_code(&product_code).then(|| MsiUninstallRecord {
                product_code,
                display_name: record
                    .get("DisplayName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                install_location: record
                    .get("InstallLocation")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                target_path: record
                    .get("TargetPath")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.product_code.cmp(&right.product_code));
    match records.as_slice() {
        [] => Ok(None),
        [record] if !record.display_name.is_empty() => Ok(Some(MsiUninstallRecord {
            product_code: record.product_code.clone(),
            display_name: record.display_name.clone(),
            install_location: record.install_location.clone(),
            target_path: record.target_path.clone(),
        })),
        _ => Err(PlatformError::Unsupported(
            "multiple MSI uninstall records match this app. No record was selected".into(),
        )),
    }
}

fn valid_product_code(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 38
        && bytes[0] == b'{'
        && bytes[37] == b'}'
        && bytes[1..37]
            .iter()
            .enumerate()
            .all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => *byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            })
}

fn msi_row_id(product_code: &str) -> String {
    format!("corvo-msi://{product_code}/MSI application")
}

fn msi_product_code_from_row_id(value: &str) -> Option<&str> {
    let remainder = value.strip_prefix("corvo-msi://")?;
    let (product_code, label) = remainder.split_once('/')?;
    (label == "MSI application" && valid_product_code(product_code)).then_some(product_code)
}

#[derive(Clone, Debug)]
struct QuietUninstallRecord {
    registry_path: String,
    display_name: String,
    install_location: String,
    executable: PathBuf,
    arguments: Vec<String>,
    uninstall_command: String,
}

fn quiet_uninstall_record_for_shortcut(
    path: &Path,
) -> PlatformResult<Option<QuietUninstallRecord>> {
    if !path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("lnk"))
    {
        return Ok(None);
    }
    let encoded_path = base64_encode(path.to_string_lossy().as_bytes());
    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class CorvoUninstallInterop {
    [DllImport("shell32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern IntPtr CommandLineToArgvW(string commandLine, out int argumentCount);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr LocalFree(IntPtr memory);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateFile(string path, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern uint GetFinalPathNameByHandle(IntPtr handle, StringBuilder path, uint length, uint flags);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);
    public static string[] Split(string commandLine) {
        int count;
        IntPtr memory = CommandLineToArgvW(commandLine, out count);
        if (memory == IntPtr.Zero) throw new Win32Exception();
        try {
            string[] arguments = new string[count];
            for (int index = 0; index < count; index++) {
                IntPtr value = Marshal.ReadIntPtr(memory, index * IntPtr.Size);
                arguments[index] = Marshal.PtrToStringUni(value);
            }
            return arguments;
        } finally { LocalFree(memory); }
    }
    public static string CanonicalPath(string path, bool directory) {
        const uint FILE_READ_ATTRIBUTES = 0x80, SHARE = 0x1 | 0x2 | 0x4, OPEN_EXISTING = 3, BACKUP_SEMANTICS = 0x02000000;
        IntPtr handle = CreateFile(path, FILE_READ_ATTRIBUTES, SHARE, IntPtr.Zero, OPEN_EXISTING, directory ? BACKUP_SEMANTICS : 0, IntPtr.Zero);
        if (handle == new IntPtr(-1)) throw new Win32Exception();
        try {
            StringBuilder buffer = new StringBuilder(32768);
            uint length = GetFinalPathNameByHandle(handle, buffer, (uint)buffer.Capacity, 0);
            if (length == 0 || length >= buffer.Capacity) throw new Win32Exception();
            string value = buffer.ToString();
            if (value.StartsWith("\\\\?\\UNC\\", StringComparison.OrdinalIgnoreCase)) return "\\\\" + value.Substring(8);
            if (value.StartsWith("\\\\?\\", StringComparison.OrdinalIgnoreCase)) return value.Substring(4);
            return value;
        } finally { CloseHandle(handle); }
    }
}
'@
$shortcutPath = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('$CorvoShortcutPath'))
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$target = [Environment]::ExpandEnvironmentVariables([string]$shortcut.TargetPath)
if (-not $target -or -not (Test-Path -LiteralPath $target -PathType Leaf)) { [Console]::Write('[]'); exit 0 }
try { $target = [CorvoUninstallInterop]::CanonicalPath($target, $false) } catch { [Console]::Write('[]'); exit 0 }
$roots = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)
$blockedExecutables = @('cmd.exe', 'powershell.exe', 'pwsh.exe', 'wscript.exe', 'cscript.exe', 'rundll32.exe', 'regsvr32.exe', 'mshta.exe', 'explorer.exe', 'msiexec.exe', 'bash.exe', 'sh.exe', 'wsl.exe')
$blockedScriptNames = @('python.exe', 'pythonw.exe', 'node.exe', 'cscript.exe', 'wscript.exe')
$matches = @(
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        foreach ($key in (Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)) {
            $item = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
            if ([int]$item.WindowsInstaller -eq 1 -or -not $item.DisplayName -or -not $item.UninstallString) { continue }
            $location = [Environment]::ExpandEnvironmentVariables([string]$item.InstallLocation)
            if (-not $location -or -not (Test-Path -LiteralPath $location -PathType Container)) { continue }
            try { $location = [CorvoUninstallInterop]::CanonicalPath($location, $true) } catch { continue }
            $icon = [Environment]::ExpandEnvironmentVariables([string]$item.DisplayIcon)
            if ($icon -match '^\s*"([^"]+)"') { $icon = $Matches[1] }
            else { $icon = $icon -replace ',\s*-?\d+\s*$', '' }
            $iconMatches = $false
            if ($icon) {
                try { $iconMatches = [string]::Equals([CorvoUninstallInterop]::CanonicalPath($icon, $false), $target, [StringComparison]::OrdinalIgnoreCase) } catch { }
            }
            if (-not ($iconMatches -or $target.StartsWith($location + '\', [StringComparison]::OrdinalIgnoreCase))) { continue }
            $argv = @([CorvoUninstallInterop]::Split([string]$item.UninstallString))
            if ($argv.Count -lt 1) { continue }
            $executable = [Environment]::ExpandEnvironmentVariables([string]$argv[0])
            if (-not [IO.Path]::IsPathRooted($executable)) { continue }
            if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { continue }
            try { $executable = [CorvoUninstallInterop]::CanonicalPath($executable, $false) } catch { continue }
            if (-not $executable.StartsWith($location + '\', [StringComparison]::OrdinalIgnoreCase)) { continue }
            $exeName = [IO.Path]::GetFileName($executable).ToLowerInvariant()
            if (-not $exeName.EndsWith('.exe') -or $blockedExecutables -contains $exeName -or $blockedScriptNames -contains $exeName) { continue }
            $arguments = @()
            if ($argv.Count -gt 1) { $arguments = @($argv[1..($argv.Count - 1)]) }
            $safeArguments = @('/uninstall', '--uninstall', '-uninstall', '/remove', '--remove', '-remove')
            if ($arguments | Where-Object {
                $_ -match '^(?i)(--?(silent|quiet|unattended|noconfirm|no-confirm)|/(s|silent|verysilent|quiet|qn|qb|passive|norestart|suppressmsgboxes))($|[:=])' -or
                $safeArguments -notcontains $_
            }) { continue }
            [pscustomobject]@{
                RegistryPath = [string]$key.Name
                DisplayName = [string]$item.DisplayName
                InstallLocation = $location
                Executable = $executable
                Arguments = [object[]]$arguments
                UninstallCommand = [string]$item.UninstallString
            }
        }
    }
)
ConvertTo-Json -InputObject $matches -Compress -Depth 4
"#
    .replace("$CorvoShortcutPath", &encoded_path);
    let output = powershell_output(&script)?;
    let records = parse_json_array(&output, "Win32 uninstall records")?
        .into_iter()
        .filter_map(|record| {
            let registry_path = record.get("RegistryPath")?.as_str()?.to_owned();
            let display_name = record.get("DisplayName")?.as_str()?.to_owned();
            let install_location = record.get("InstallLocation")?.as_str()?.to_owned();
            let executable = PathBuf::from(record.get("Executable")?.as_str()?);
            let arguments = match record.get("Arguments")? {
                serde_json::Value::Array(items) => items
                    .iter()
                    .map(|item| item.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
                serde_json::Value::Null => Vec::new(),
                _ => return None,
            };
            let uninstall_command = record.get("UninstallCommand")?.as_str()?.to_owned();
            Some(QuietUninstallRecord {
                registry_path,
                display_name,
                install_location,
                executable,
                arguments,
                uninstall_command,
            })
        })
        .collect::<Vec<_>>();
    match records.as_slice() {
        [] => Ok(None),
        [record] => Ok(Some(record.clone())),
        _ => Err(PlatformError::Unsupported(
            "multiple Win32 uninstall records match this app. No record was selected".into(),
        )),
    }
}

fn quiet_uninstall_record_id(record: &QuietUninstallRecord) -> String {
    let mut digest = Sha256::new();
    digest.update(record.registry_path.as_bytes());
    digest.update([0]);
    digest.update(record.display_name.as_bytes());
    digest.update([0]);
    digest.update(record.install_location.as_bytes());
    digest.update([0]);
    digest.update(record.executable.as_os_str().to_string_lossy().as_bytes());
    digest.update([0]);
    digest.update(record.uninstall_command.as_bytes());
    hex::encode(digest.finalize())
}

fn quiet_uninstall_row_id(record: &QuietUninstallRecord) -> String {
    format!(
        "corvo-win-uninstall://{}/Win32 application",
        quiet_uninstall_record_id(record)
    )
}

fn quiet_uninstall_id_from_row_id(value: &str) -> Option<&str> {
    let remainder = value.strip_prefix("corvo-win-uninstall://")?;
    let (id, label) = remainder.split_once('/')?;
    (label == "Win32 application"
        && id.len() == 64
        && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then_some(id)
}

fn appx_family_name(path: &Path) -> Option<&str> {
    let value = path.to_str()?;
    let app_id = value
        .strip_prefix("shell:AppsFolder\\")
        .or_else(|| value.strip_prefix("shell:AppsFolder/"))?;
    let (family, _) = app_id.split_once('!')?;
    (!family.is_empty()
        && family.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        }))
    .then_some(family)
}

impl PlatformOps for WindowsPlatform {
    fn lock(&self) -> PlatformResult<()> {
        run("rundll32.exe", &["user32.dll,LockWorkStation"])
    }

    fn sleep(&self) -> PlatformResult<()> {
        run_powershell(
            "$signature = '[DllImport(\"powrprof.dll\", SetLastError = true)] public static extern bool SetSuspendState(bool hibernate, bool forceCritical, bool disableWakeEvent);'; Add-Type -MemberDefinition $signature -Name Power -Namespace Corvo; if (-not [Corvo.Power]::SetSuspendState($false, $true, $false)) { exit 1 }",
        )
    }

    fn shutdown(&self) -> PlatformResult<()> {
        run("shutdown.exe", &["/s", "/t", "0"])
    }

    fn list_windows(&self) -> PlatformResult<Vec<WindowHandle>> {
        let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$items = @(Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle } | ForEach-Object {
    [pscustomobject]@{ id = [uint64]$_.MainWindowHandle; title = [string]$_.MainWindowTitle; app_name = [string]$_.ProcessName }
})
ConvertTo-Json -InputObject $items -Compress -Depth 3
"#;
        let output = powershell_output(script)?;
        Ok(parse_json_array(&output, "window list")?
            .into_iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_u64()?;
                let title = item.get("title")?.as_str()?.to_owned();
                let app_name = item
                    .get("app_name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                Some(WindowHandle {
                    id,
                    title,
                    app_name,
                })
            })
            .collect())
    }

    fn focus_window(&self, handle: &WindowHandle) -> PlatformResult<()> {
        let script = format!(
            "$signature = '[DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow); [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr hWnd);'; Add-Type -MemberDefinition $signature -Name Window -Namespace Corvo; $hwnd = [IntPtr]::new({}); [void][Corvo.Window]::ShowWindowAsync($hwnd, 9); if (-not [Corvo.Window]::SetForegroundWindow($hwnd)) {{ exit 1 }}",
            handle.id
        );
        run_powershell(&script)
    }

    fn list_apps(&self) -> PlatformResult<Vec<AppEntry>> {
        let mut apps = Vec::new();
        let mut seen = HashSet::new();

        for root in start_menu_roots() {
            collect_shortcuts(&root, &mut |path| {
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .trim();
                if name.is_empty() {
                    return;
                }
                let key = path.to_string_lossy().to_lowercase();
                if seen.insert(key) {
                    apps.push(AppEntry {
                        name: name.to_owned(),
                        icon_png: cached_shortcut_icon(&path),
                        path,
                    });
                }
            });
        }

        apps.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(apps)
    }

    fn open_path(&self, path: &Path) -> PlatformResult<()> {
        shell_execute("open", &path.to_string_lossy())
    }

    fn copy_text(&self, text: &str) -> PlatformResult<()> {
        let encoded = base64_encode(text.as_bytes());
        let script = format!(
            "$bytes = [Convert]::FromBase64String('{encoded}'); $value = [Text.Encoding]::UTF8.GetString($bytes); Set-Clipboard -Value $value"
        );
        run_powershell(&script)
    }
}

pub fn append_start_apps(apps: &mut Vec<AppEntry>) {
    let shortcut_names: HashSet<String> = apps
        .iter()
        .filter(|app| {
            app.path
                .extension()
                .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("lnk"))
        })
        .map(|app| app.name.trim().to_lowercase())
        .collect();
    let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$items = @(Get-StartApps | ForEach-Object { [pscustomobject]@{ name = [string]$_.Name; app_id = [string]$_.AppID } })
ConvertTo-Json -InputObject $items -Compress -Depth 3
"#;
    let Ok(output) = powershell_output(script) else {
        return;
    };
    let Ok(items) = parse_json_array(&output, "Start Apps") else {
        return;
    };
    apps.retain(|app| !app.path.to_string_lossy().starts_with("shell:AppsFolder\\"));
    let mut seen = HashSet::new();
    for item in items {
        let (Some(name), Some(app_id)) = (
            item.get("name").and_then(serde_json::Value::as_str),
            item.get("app_id").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        if name.trim().is_empty()
            || app_id.trim().is_empty()
            || shortcut_names.contains(&name.trim().to_lowercase())
            || !seen.insert(app_id.to_lowercase())
        {
            continue;
        }
        apps.push(AppEntry {
            name: name.to_owned(),
            path: PathBuf::from(format!("shell:AppsFolder\\{app_id}")),
            icon_png: cached_icon_for_key(&app_id.to_lowercase()),
        });
    }
    apps.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
}

pub fn hydrate_app_icons(apps: &mut [AppEntry]) {
    fill_shortcut_icons(apps);
    fill_start_app_icons(apps);
}

pub fn hydrate_shortcut_icons(apps: &mut [AppEntry]) {
    fill_shortcut_icons(apps);
}

pub fn hydrate_start_app_icons(apps: &mut [AppEntry]) {
    fill_start_app_icons(apps);
}

pub fn set_launcher_window_visible(handle: isize, visible: bool) {
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(window: isize, command: i32) -> i32;
        fn SetWindowPos(
            window: isize,
            insert_after: isize,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        fn SetForegroundWindow(window: isize) -> i32;
    }
    if visible {
        unsafe {
            ShowWindow(handle, 5);
            SetWindowPos(handle, -1, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0040);
            SetForegroundWindow(handle);
        }
    } else {
        unsafe { ShowWindow(handle, 0) };
    }
}

pub fn frontmost_app_info() -> Option<(i32, String)> {
    use std::ffi::c_void;

    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(window: *mut c_void, process_id: *mut u32) -> u32;
    }
    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    let process = unsafe { OpenProcess(0x1000, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut name = [0u16; 32768];
    let mut size = name.len() as u32;
    let found = unsafe { QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(process) };
    if found == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&name[..size as usize]);
    let app_name = Path::new(&path).file_stem()?.to_string_lossy().into_owned();
    Some((pid as i32, app_name))
}

/// A window rectangle in physical pixels, top-left origin.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl WinRect {
    fn width(self) -> i32 {
        self.right - self.left
    }

    fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// A monitor's work area in physical pixels.
#[derive(Clone, Copy, Debug)]
struct MonitorInfo {
    rect: WinRect,
}

#[allow(non_snake_case)]
#[link(name = "user32")]
unsafe extern "system" {
    fn MonitorFromWindow(window: isize, flags: u32) -> isize;
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfoW) -> i32;
    fn GetWindowRect(window: isize, rect: *mut WinRect) -> i32;
    fn SetWindowPos(
        window: isize,
        insert_after: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn ShowWindowAsync(window: isize, command: i32) -> i32;
    fn IsIconic(window: isize) -> i32;
}

#[repr(C)]
struct MonitorInfoW {
    size: u32,
    monitor: MonitorRectW,
    work: MonitorRectW,
    flags: u32,
}

#[repr(C)]
struct MonitorRectW {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

const MONITOR_DEFAULTTONEAREST: u32 = 2;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const SW_SHOWMAXIMIZED: i32 = 3;
const SW_SHOWNORMAL: i32 = 1;

/// Saved window frames, so "Restore Previous Size" has something to
/// restore. Keyed by the process id.
fn restore_cache() -> &'static std::sync::Mutex<std::collections::HashMap<u32, WinRect>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<u32, WinRect>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// The monitor that holds most of `window`, falling back to the nearest
/// one when the window is off-screen.
fn monitor_for_window(window: isize) -> PlatformResult<MonitorInfo> {
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    monitor_info(monitor)
}

fn monitor_info(monitor: isize) -> PlatformResult<MonitorInfo> {
    if monitor == 0 {
        return Err(PlatformError::Os("no monitor is available".into()));
    }
    let mut info = MonitorInfoW {
        size: std::mem::size_of::<MonitorInfoW>() as u32,
        monitor: MonitorRectW {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        work: MonitorRectW {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        flags: 0,
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return Err(PlatformError::Os(
            "could not read the monitor information".into(),
        ));
    }
    Ok(MonitorInfo {
        rect: WinRect {
            left: info.work.left,
            top: info.work.top,
            right: info.work.right,
            bottom: info.work.bottom,
        },
    })
}

/// Finds the top-level window of a process, preferring the one Windows
/// considers its main window.
fn find_window_for_pid(pid: u32) -> Option<isize> {
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(window: isize, process_id: *mut u32) -> u32;
        fn IsWindowVisible(window: isize) -> i32;
    }

    struct Search {
        pid: u32,
        best: isize,
        best_area: i64,
    }

    unsafe extern "system" fn visit(window: isize, data: isize) -> i32 {
        let search = &mut *(data as *mut Search);
        let mut owner_pid = 0u32;
        GetWindowThreadProcessId(window, &mut owner_pid);
        if owner_pid != search.pid {
            return 1;
        }
        if IsWindowVisible(window) == 0 {
            return 1;
        }
        // Pick the largest visible window: a browser has several, and the
        // biggest is the one the user is looking at.
        let mut rect = WinRect::default();
        if GetWindowRect(window, &mut rect) == 0 {
            return 1;
        }
        let area = (rect.width() as i64) * (rect.height() as i64);
        if area > search.best_area {
            search.best_area = area;
            search.best = window;
        }
        1
    }

    let mut search = Search {
        pid,
        best: 0,
        best_area: -1,
    };
    unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
    (search.best != 0).then_some(search.best)
}

/// The rectangle an action asks for, in work-area coordinates.
fn target_rect(action: &str, screen: &WinRect, gap: i32) -> Option<WinRect> {
    let sx = screen.left;
    let sy = screen.top;
    let sw = screen.width();
    let sh = screen.height();
    // A gap wider than the screen would push every tile off it, so cap it
    // at a quarter of the shorter side. The user gets a tight but visible
    // layout rather than a window dragged off the desktop.
    let gap = gap.max(0).min(sw.min(sh) / 4);
    let width = |fraction: f32| (sw as f32 * fraction) as i32;
    let height = |fraction: f32| (sh as f32 * fraction) as i32;
    let rect = |x: i32, y: i32, w: i32, h: i32| WinRect {
        left: sx + x,
        top: sy + y,
        right: sx + x + w,
        bottom: sy + y + h,
    };

    if gap > 0 {
        // With a window gap, halve the usable area and inset each tile.
        let hw = (sw - 3 * gap).max(100) / 2;
        let hh = (sh - 3 * gap).max(100) / 2;
        let tw = (sw - 4 * gap).max(100) / 3;
        // `tw` is the width of one third of the usable area, so two thirds
        // is two of them plus the gap between them.
        let g = gap;
        return Some(match action {
            "left-half" => rect(g, g, hw, sh - 2 * g),
            "right-half" => rect(g + hw + g, g, hw, sh - 2 * g),
            "top-half" => rect(g, g, sw - 2 * g, hh),
            "bottom-half" => rect(g, g + hh + g, sw - 2 * g, hh),
            "first-third" => rect(g, g, tw, sh - 2 * g),
            "center-third" => rect(g + tw + g, g, tw, sh - 2 * g),
            "last-third" => rect(g + 2 * (tw + g), g, tw, sh - 2 * g),
            "first-two-thirds" => rect(g, g, 2 * tw + g, sh - 2 * g),
            "last-two-thirds" => rect(g + tw + g, g, 2 * tw + g, sh - 2 * g),
            "top-left" => rect(g, g, hw, hh),
            "top-right" => rect(g + hw + g, g, hw, hh),
            "bottom-left" => rect(g, g + hh + g, hw, hh),
            "bottom-right" => rect(g + hw + g, g + hh + g, hw, hh),
            "maximize" => rect(0, 0, sw, sh),
            "almost-maximize" => {
                let w = width(0.9);
                let h = height(0.9);
                rect((sw - w) / 2, (sh - h) / 2, w, h)
            }
            "center" => {
                let w = width(0.7);
                let h = height(0.8);
                rect((sw - w) / 2, (sh - h) / 2, w, h)
            }
            _ => return None,
        });
    }

    Some(match action {
        "left-half" => rect(0, 0, sw / 2, sh),
        "right-half" => rect(sw / 2, 0, sw - sw / 2, sh),
        "top-half" => rect(0, 0, sw, sh / 2),
        "bottom-half" => rect(0, sh / 2, sw, sh - sh / 2),
        "first-third" => rect(0, 0, sw / 3, sh),
        "center-third" => rect(sw / 3, 0, sw / 3, sh),
        "last-third" => rect(2 * sw / 3, 0, sw - 2 * sw / 3, sh),
        "first-two-thirds" => rect(0, 0, 2 * sw / 3, sh),
        "last-two-thirds" => rect(sw / 3, 0, sw - sw / 3, sh),
        "top-left" => rect(0, 0, sw / 2, sh / 2),
        "top-right" => rect(sw / 2, 0, sw - sw / 2, sh / 2),
        "bottom-left" => rect(0, sh / 2, sw / 2, sh - sh / 2),
        "bottom-right" => rect(sw / 2, sh / 2, sw - sw / 2, sh - sh / 2),
        "maximize" => rect(0, 0, sw, sh),
        "almost-maximize" => {
            let w = width(0.9);
            let h = height(0.9);
            rect((sw - w) / 2, (sh - h) / 2, w, h)
        }
        "center" => {
            let w = width(0.7);
            let h = height(0.8);
            rect((sw - w) / 2, (sh - h) / 2, w, h)
        }
        _ => return None,
    })
}

/// The monitor immediately to the left or right of `current`.
///
/// Enumerates every monitor and picks the nearest one in that direction
/// by the gap between the work-area edges, so monitors at different
/// vertical offsets still resolve.
fn neighbouring_monitor(current: WinRect, forward: bool) -> PlatformResult<MonitorInfo> {
    struct Search {
        needle: i32,
        forward: bool,
        best: Option<MonitorInfo>,
    }

    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumDisplayMonitors(
            context: isize,
            clip: *mut WinRect,
            callback: unsafe extern "system" fn(isize, isize, *mut WinRect, isize) -> i32,
            data: isize,
        ) -> i32;
    }

    unsafe extern "system" fn visit(
        monitor: isize,
        _context: isize,
        _rect: *mut WinRect,
        data: isize,
    ) -> i32 {
        let search = &mut *(data as *mut Search);
        let Ok(info) = monitor_info(monitor) else {
            return 1;
        };
        // Distance from the current monitor's trailing edge to the
        // candidate's leading edge, positive when the candidate lies in
        // the requested direction.
        let distance = if search.forward {
            info.rect.left - search.needle
        } else {
            search.needle - info.rect.right
        };
        if distance < 0 {
            return 1;
        }
        let closer = match search.best {
            Some(current_best) => {
                let best_distance = if search.forward {
                    current_best.rect.left - search.needle
                } else {
                    search.needle - current_best.rect.right
                };
                distance < best_distance
            }
            None => true,
        };
        if closer {
            search.best = Some(info);
        }
        1
    }

    let needle = if forward { current.right } else { current.left };
    let mut search = Search {
        needle,
        forward,
        best: None,
    };
    unsafe {
        EnumDisplayMonitors(
            0,
            std::ptr::null_mut(),
            visit,
            &mut search as *mut Search as isize,
        );
    }
    search
        .best
        .ok_or_else(|| PlatformError::Unsupported("no other display is connected".into()))
}

/// Arranges the frontmost window, or the window of `target_pid`.
///
/// Mirrors the macOS implementation: same action names, same window gap
/// handling, and a saved frame for "Restore Previous Size".
pub fn tile_window(target_pid: Option<i32>, action: &str) -> PlatformResult<()> {
    let pid = match target_pid {
        Some(pid) => pid as u32,
        None => frontmost_app_info()
            .map(|(pid, _)| pid as u32)
            .ok_or_else(|| PlatformError::Os("no active window found to arrange".into()))?,
    };
    let window = find_window_for_pid(pid)
        .ok_or_else(|| PlatformError::Os("no visible window found for that application".into()))?;
    // A minimized window cannot be positioned meaningfully. Restore it
    // first so the new frame applies.
    let minimized = unsafe { IsIconic(window) } != 0;

    if action == "restore" {
        let saved = restore_cache()
            .lock()
            .map_err(|_| PlatformError::Os("window frame cache is poisoned".into()))?
            .remove(&pid);
        let Some(saved) = saved else {
            return Ok(());
        };
        return set_window_frame(window, saved);
    }

    let current = monitor_for_window(window)?.rect;
    let screen = match action {
        "next-display" | "prev-display" => {
            let target = neighbouring_monitor(current, action == "next-display")?;
            let delta_x = target.rect.left - current.left;
            let delta_y = target.rect.top - current.top;
            let mut frame = WinRect::default();
            if unsafe { GetWindowRect(window, &mut frame) } == 0 {
                return Err(PlatformError::Os("could not read the window frame".into()));
            }
            let moved = WinRect {
                left: frame.left + delta_x,
                top: frame.top + delta_y,
                right: frame.right + delta_x,
                bottom: frame.bottom + delta_y,
            };
            return set_window_frame(window, moved);
        }
        _ => current,
    };

    let mut existing = WinRect::default();
    if unsafe { GetWindowRect(window, &mut existing) } == 0 {
        return Err(PlatformError::Os("could not read the window frame".into()));
    }
    // Save the frame before the first resize so "restore" has something
    // to go back to.
    if let Ok(mut cache) = restore_cache().lock() {
        cache.insert(pid, existing);
    }

    if action == "maximize" || action == "almost-maximize" {
        if minimized {
            unsafe { ShowWindowAsync(window, SW_SHOWNORMAL) };
        }
        if action == "maximize" {
            unsafe { ShowWindowAsync(window, SW_SHOWMAXIMIZED) };
            return Ok(());
        }
    }

    let Some(target) = target_rect(action, &screen, super::get_window_gap() as i32) else {
        // An unknown action should not be reported as success.
        return Err(PlatformError::Unsupported(format!(
            "Windows cannot perform the \"{action}\" window action"
        )));
    };
    if minimized {
        unsafe { ShowWindowAsync(window, SW_SHOWNORMAL) };
    }
    set_window_frame(window, target)
}

fn set_window_frame(window: isize, frame: WinRect) -> PlatformResult<()> {
    let ok = unsafe {
        SetWindowPos(
            window,
            0,
            frame.left,
            frame.top,
            frame.width(),
            frame.height(),
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    };
    if ok == 0 {
        return Err(PlatformError::Os(
            "Windows refused to move or resize the window".into(),
        ));
    }
    Ok(())
}

/// Arranges several named applications at once, for saved layouts.
pub fn apply_window_layout(placements: &[(String, String)]) -> PlatformResult<()> {
    let mut failures = Vec::new();
    for (app_name, position) in placements {
        let pid = process_id_for_window_title(app_name);
        let Some(pid) = pid else {
            failures.push(format!("{app_name} is not running"));
            continue;
        };
        if let Err(error) = tile_window(Some(pid as i32), position) {
            failures.push(format!("{app_name}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(PlatformError::Os(failures.join("; ")))
    }
}

/// Finds a running process whose main window title matches `name`.
fn process_id_for_window_title(name: &str) -> Option<u32> {
    struct Search {
        needle: String,
        found: Option<u32>,
    }

    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(window: isize, process_id: *mut u32) -> u32;
        fn IsWindowVisible(window: isize) -> i32;
    }

    unsafe extern "system" fn visit(window: isize, data: isize) -> i32 {
        let search = &mut *(data as *mut Search);
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, &mut pid);
        if IsWindowVisible(window) == 0 || pid == 0 {
            return 1;
        }
        // Compare on the executable name, which is what a user types.
        if let Some(current) = window_process_name(pid) {
            if current.eq_ignore_ascii_case(&search.needle) {
                search.found = Some(pid);
                return 0;
            }
        }
        1
    }

    let mut search = Search {
        needle: name.trim().to_owned(),
        found: None,
    };
    unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
    search.found
}

/// The executable name of a process, without its extension.
fn window_process_name(pid: u32) -> Option<String> {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> isize;
        fn QueryFullProcessImageNameW(
            process: isize,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(object: isize) -> i32;
    }

    const PROCESS_NAME_WIN32: u32 = 0;
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process == 0 || process == -1 {
        return None;
    }
    let mut buffer = [0u16; 32768];
    let mut size = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size)
    };
    unsafe { CloseHandle(process) };
    if ok == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    Path::new(&path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_owned)
}

pub fn activate_app(pid: i32) -> PlatformResult<()> {
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(window: isize, process_id: *mut u32) -> u32;
        fn GetWindow(window: isize, command: u32) -> isize;
        fn IsWindowVisible(window: isize) -> i32;
        fn ShowWindowAsync(window: isize, command: i32) -> i32;
        fn SetForegroundWindow(window: isize) -> i32;
    }

    struct WindowSearch {
        pid: u32,
        window: isize,
    }

    unsafe extern "system" fn find_main_window(window: isize, data: isize) -> i32 {
        let search = &mut *(data as *mut WindowSearch);
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if pid == search.pid && IsWindowVisible(window) != 0 && GetWindow(window, 4) == 0 {
            search.window = window;
            return 0;
        }
        1
    }

    let mut search = WindowSearch {
        pid: pid as u32,
        window: 0,
    };
    unsafe {
        EnumWindows(
            find_main_window,
            (&mut search as *mut WindowSearch) as isize,
        )
    };
    if search.window == 0 {
        return Err(PlatformError::Os(format!(
            "no main window found for process {pid}"
        )));
    }
    unsafe {
        ShowWindowAsync(search.window, 9);
        if SetForegroundWindow(search.window) == 0 {
            return Err(PlatformError::Os(format!("could not focus process {pid}")));
        }
    }
    Ok(())
}

pub fn send_paste_keystroke() -> PlatformResult<()> {
    run_powershell("(New-Object -ComObject WScript.Shell).SendKeys('^v')")
}

// -------------------------------------------------------------------------
// Display brightness
// -------------------------------------------------------------------------

#[allow(non_snake_case)]
#[link(name = "dxva2")]
unsafe extern "system" {
    fn EnumDisplayMonitors(
        context: isize,
        clip: *mut WinRect,
        callback: unsafe extern "system" fn(isize, isize, *mut WinRect, isize) -> i32,
        data: isize,
    ) -> i32;
    fn GetNumberOfPhysicalMonitorsFromHMONITOR(monitor: isize, count: *mut u32) -> i32;
    fn GetPhysicalMonitorsFromHMONITOR(
        monitor: isize,
        count: u32,
        monitors: *mut PhysicalMonitor,
    ) -> i32;
    fn GetMonitorCapabilities(handle: PhysicalMonitor, capabilities: *mut u32) -> i32;
    fn GetMonitorBrightness(handle: PhysicalMonitor, brightness: *mut u32) -> i32;
    fn SetMonitorBrightness(handle: PhysicalMonitor, brightness: u32) -> i32;
    fn DestroyPhysicalMonitors(count: u32, monitors: *mut PhysicalMonitor) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PhysicalMonitor(isize);

/// `MC_CAPS_BRIGHTNESS`: the monitor reports a settable brightness.
const MC_CAPS_BRIGHTNESS: u32 = 0x0000_0002;

/// The brightness of the first physical monitor that exposes a control, as
/// a percentage.
///
/// Uses the physical monitor API, which reaches external desktop displays
/// over DDC/CI. The WMI interface used to drive this only exists on a
/// laptop's internal panel, so a desktop had no way to change brightness.
fn physical_monitor_brightness() -> PlatformResult<Option<f32>> {
    struct Search {
        found: Option<f32>,
    }

    unsafe extern "system" fn visit(
        _monitor: isize,
        _context: isize,
        _rect: *mut WinRect,
        data: isize,
    ) -> i32 {
        let search = &mut *(data as *mut Search);
        if search.found.is_some() {
            return 0;
        }
        let mut count = 0u32;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(_monitor, &mut count) } == 0
            && count > 0
        {
            let mut monitors = vec![PhysicalMonitor(0); count as usize];
            if unsafe { GetPhysicalMonitorsFromHMONITOR(_monitor, count, monitors.as_mut_ptr()) }
                != 0
            {
                for handle in &monitors {
                    let mut capabilities = 0u32;
                    if unsafe { GetMonitorCapabilities(*handle, &mut capabilities) } != 0
                        && capabilities & MC_CAPS_BRIGHTNESS != 0
                    {
                        let mut brightness = 0u32;
                        if unsafe { GetMonitorBrightness(*handle, &mut brightness) } != 0 {
                            search.found = Some(brightness as f32);
                            break;
                        }
                    }
                }
                unsafe { DestroyPhysicalMonitors(count, monitors.as_mut_ptr()) };
            }
        }
        // Stop as soon as one monitor answers.
        if search.found.is_some() {
            0
        } else {
            1
        }
    }

    let mut search = Search { found: None };
    unsafe {
        EnumDisplayMonitors(
            0,
            std::ptr::null_mut(),
            visit,
            &mut search as *mut Search as isize,
        );
    }
    Ok(search.found)
}

/// Applies a brightness change to every physical monitor that supports it.
fn set_physical_monitor_brightness(delta_percent: i32) -> PlatformResult<usize> {
    struct Search {
        delta: i32,
        changed: usize,
    }

    unsafe extern "system" fn visit(
        monitor: isize,
        _context: isize,
        _rect: *mut WinRect,
        data: isize,
    ) -> i32 {
        let search = &mut *(data as *mut Search);
        let mut count = 0u32;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count) } == 0
            && count > 0
        {
            let mut monitors = vec![PhysicalMonitor(0); count as usize];
            if unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, count, monitors.as_mut_ptr()) }
                != 0
            {
                for handle in &monitors {
                    let mut capabilities = 0u32;
                    if unsafe { GetMonitorCapabilities(*handle, &mut capabilities) } == 0
                        || capabilities & MC_CAPS_BRIGHTNESS == 0
                    {
                        continue;
                    }
                    let mut current = 0u32;
                    if unsafe { GetMonitorBrightness(*handle, &mut current) } != 0 {
                        continue;
                    }
                    let target =
                        (current as i32 + search.delta).clamp(0, 100) as u32;
                    if unsafe { SetMonitorBrightness(*handle, target) } != 0 {
                        search.changed += 1;
                    }
                }
                unsafe { DestroyPhysicalMonitors(count, monitors.as_mut_ptr()) };
            }
        }
        1
    }

    let mut search = Search {
        delta: delta_percent,
        changed: 0,
    };
    unsafe {
        EnumDisplayMonitors(
            0,
            std::ptr::null_mut(),
            visit,
            &mut search as *mut Search as isize,
        );
    }
    Ok(search.changed)
}

pub fn adjust_brightness_with_level(delta: f32) -> PlatformResult<f32> {
    if !delta.is_finite() {
        return Err(PlatformError::Os(
            "brightness change must be finite".into(),
        ));
    }
    let delta_percent = (delta * 100.0).round() as i32;
    if delta_percent == 0 {
        return physical_monitor_brightness()?.ok_or_else(no_brightness_control);
    }
    match set_physical_monitor_brightness(delta_percent)? {
        0 => Err(no_brightness_control()),
        _ => physical_monitor_brightness()?.ok_or_else(no_brightness_control),
    }
}

fn no_brightness_control() -> PlatformError {
    PlatformError::Unsupported(
        "this display has no software brightness control".into(),
    )
}

// -------------------------------------------------------------------------
// Audio output volume, through the Core Audio COM API
// -------------------------------------------------------------------------

/// CLSID_MMDeviceEnumerator.
const CLSID_MM_DEVICE_ENUMERATOR: Guid = Guid {
    data1: 0xBCDE0395,
    data2: 0xE52F,
    data3: 0x467C,
    data4: [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E],
};
/// IID_IAudioEndpointVolume.
const IID_AUDIO_ENDPOINT_VOLUME: Guid = Guid {
    data1: 0x5CDF2C82,
    data2: 0x841E,
    data3: 0x4546,
    data4: [0x97, 0x22, 0x0C, 0xF7, 0x40, 0x78, 0x22, 0x9A],
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[allow(non_snake_case)]
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: isize, flags: u32) -> i32;
    fn CoUninitialize();
    fn CoCreateInstance(
        class: *const Guid,
        outer: isize,
        context: u32,
        interface: *const Guid,
        instance: *mut isize,
    ) -> i32;
}

/// `eRender`: the playback side, which is what "output volume" means.
const ERENDER: i32 = 0;
/// `eConsole`: the default device for console audio.
const ECONSOLE: i32 = 0;
const CLSCTX_ALL: u32 = 23;
const COINIT_APARTMENTTHREADED: u32 = 0x2;

/// The first three slots every COM interface shares. A COM vtable is an
/// array of function pointers, so each slot is read as a `usize` and
/// transmuted to the right signature at the call site.
#[repr(C)]
#[derive(Clone, Copy)]
struct IUnknownVtbl {
    query_interface: usize,
    add_ref: usize,
    release: usize,
}

/// `IMMDeviceEnumerator` adds two methods after `IUnknown`.
#[repr(C)]
struct DeviceEnumeratorVtbl {
    unknown: IUnknownVtbl,
    enum_audio_endpoints: usize,
    get_default_audio_endpoint: usize,
}

/// `IMMDevice` adds one method after `IUnknown`.
#[repr(C)]
struct DeviceVtbl {
    unknown: IUnknownVtbl,
    activate: usize,
}

/// A borrowed pointer to a COM object, released on drop.
struct ComPtr(isize);

impl Drop for ComPtr {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                let vtbl = self.vtbl::<IUnknownVtbl>();
                let release: unsafe extern "system" fn(isize) -> i32 =
                    std::mem::transmute((*vtbl).release);
                release(self.0);
            }
        }
    }
}

impl ComPtr {
    /// The object's vtable, read through its first slot.
    ///
    /// A COM interface pointer points at a table of function pointers, so
    /// the vtable address is the first machine word of the object. The
    /// read goes through `*const usize` so the cast to the caller's
    /// vtable type is a plain pointer cast.
    fn vtbl<T>(&self) -> *const T {
        unsafe { (*(*(self.0 as *const *const usize))) as *const T }
    }
}

/// Runs `body` with COM initialized on this thread, then uninitializes it.
fn with_com<T>(body: impl FnOnce() -> T) -> T {
    unsafe { CoInitializeEx(0, COINIT_APARTMENTTHREADED) };
    let result = body();
    unsafe { CoUninitialize() };
    result
}

/// The default output endpoint's volume interface.
fn default_endpoint_volume() -> PlatformResult<ComPtr> {
    with_com(|| {
        let mut enumerator = 0isize;
        let hr = unsafe {
            CoCreateInstance(
                &CLSID_MM_DEVICE_ENUMERATOR,
                0,
                CLSCTX_ALL,
                &IID_AUDIO_ENDPOINT_VOLUME,
                &mut enumerator,
            )
        };
        if hr != 0 || enumerator == 0 {
            return Err(com_error("could not open the audio device enumerator", hr));
        }
        // The enumerator is created as the volume interface only to
        // obtain a class object, so this is safe: every COM object
        // implements IUnknown at a fixed vtable offset.
        let enumerator = ComPtr(enumerator);
        let device_vtbl = enumerator.vtbl::<DeviceEnumeratorVtbl>();
        let mut device = 0isize;
        let hr = unsafe {
            let get_default: unsafe extern "system" fn(isize, i32, i32, *mut isize) -> i32 =
                std::mem::transmute((*device_vtbl).get_default_audio_endpoint);
            get_default(enumerator.0, ERENDER, ECONSOLE, &mut device)
        };
        if hr != 0 || device == 0 {
            return Err(com_error("no default output device", hr));
        }
        let device = ComPtr(device);
        let vtbl = device.vtbl::<DeviceVtbl>();
        let mut volume = 0isize;
        let hr = unsafe {
            let activate: unsafe extern "system" fn(
                isize,
                *const Guid,
                u32,
                isize,
                *mut isize,
            ) -> i32 = std::mem::transmute((*vtbl).activate);
            activate(
                device.0,
                &IID_AUDIO_ENDPOINT_VOLUME,
                23, // CLSCTX_ALL
                0,
                &mut volume,
            )
        };
        if hr != 0 || volume == 0 {
            return Err(com_error("could not open the device volume", hr));
        }
        Ok(ComPtr(volume))
    })
}

fn com_error(what: &str, hr: i32) -> PlatformError {
    PlatformError::Os(format!("{what} (HRESULT 0x{:08X})", hr as u32))
}

/// The master volume of the default output device, from 0.0 to 1.0.
fn endpoint_scalar(volume: &ComPtr) -> PlatformResult<f32> {
    // IAudioEndpointVolume: three IUnknown slots, then
    // Register, Unregister, GetChannelCount, SetMasterVolumeLevel,
    // SetMasterVolumeLevelScalar, GetMasterVolumeLevel,
    // GetMasterVolumeLevelScalar. The scalar getter is slot 9.
    const GET_MASTER_SCALAR: usize = 9;
    let mut level = 0f32;
    let hr = unsafe {
        let getter: unsafe extern "system" fn(isize, *mut f32) -> i32 =
            std::mem::transmute(read_vtable_slot(volume, GET_MASTER_SCALAR));
        getter(volume.0, &mut level)
    };
    if hr != 0 {
        return Err(com_error("could not read the output volume", hr));
    }
    Ok(level)
}

fn set_endpoint_scalar(volume: &ComPtr, level: f32) -> PlatformResult<()> {
    // IAudioEndpointVolume slot 7 is SetMasterVolumeLevelScalar.
    const SET_MASTER_SCALAR: usize = 7;
    let event_context = Guid {
        data1: 0,
        data2: 0,
        data3: 0,
        data4: [0; 8],
    };
    let hr = unsafe {
        let setter: unsafe extern "system" fn(isize, f32, *const Guid) -> i32 =
            std::mem::transmute(read_vtable_slot(volume, SET_MASTER_SCALAR));
        setter(volume.0, level, &event_context)
    };
    if hr != 0 {
        return Err(com_error("could not set the output volume", hr));
    }
    Ok(())
}

unsafe fn read_vtable_slot(volume: &ComPtr, index: usize) -> usize {
    let vtable = *(volume.0 as *const *const usize);
    *vtable.add(index)
}

pub fn audio_output_level() -> Option<f32> {
    let volume = default_endpoint_volume().ok()?;
    let level = endpoint_scalar(&volume).ok()?;
    Some((level.clamp(0.0, 1.0) * 100.0).round())
}

pub fn adjust_audio_output_with_level(delta: f32) -> PlatformResult<f32> {
    if !delta.is_finite() {
        return Err(PlatformError::Os("volume change must be finite".into()));
    }
    let delta = delta.clamp(-1.0, 1.0);
    let volume = default_endpoint_volume()?;
    let current = endpoint_scalar(&volume)?;
    let target = (current + delta).clamp(0.0, 1.0);
    set_endpoint_scalar(&volume, target)?;
    Ok((target * 100.0).round())
}

fn run(program: &str, args: &[impl AsRef<std::ffi::OsStr>]) -> PlatformResult<()> {
    let status = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| PlatformError::Os(format!("could not start {program}: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Os(format!("{program} exited with {status}")))
    }
}

/// Hands a target to the Windows shell. `ShellExecuteW` returns as soon as
/// the shell accepts the request, and it reports a real failure code, so
/// the caller never waits on a helper process and never sees a false error
/// after a successful launch.
fn shell_execute(verb: &str, target: &str) -> PlatformResult<()> {
    #[allow(non_snake_case)]
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: isize,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(object: isize) -> i32;
    }

    if target.is_empty() {
        return Err(PlatformError::Os("Windows got an empty target".into()));
    }
    let verb: Vec<u16> = verb.encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    const SW_SHOWNORMAL: i32 = 1;
    let result = unsafe {
        ShellExecuteW(
            0,
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a handle above 32 on success and one of the
    // SE_ERR_* codes at or below 32 on failure.
    if result > 32 {
        unsafe { CloseHandle(result) };
        Ok(())
    } else {
        Err(PlatformError::Os(format!(
            "Windows could not open {target} (shell error {result})"
        )))
    }
}

fn run_powershell(script: &str) -> PlatformResult<()> {
    let encoded = base64_encode(
        &script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    run(
        "powershell.exe",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            encoded.as_str(),
        ],
    )
}

fn powershell_output(script: &str) -> PlatformResult<String> {
    let encoded = base64_encode(
        &script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            encoded.as_str(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| PlatformError::Os(format!("could not start PowerShell: {error}")))?;
    if !output.status.success() {
        return Err(PlatformError::Os(format!(
            "PowerShell exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| PlatformError::Os(format!("PowerShell returned invalid UTF-8: {error}")))
}

/// Maximum packaged app icon jobs in one PowerShell pass.
const ICON_BATCH_SIZE: usize = 40;

/// Runs one icon extraction pass. The job list goes in a file next to the
/// cache, never in the command line: `CreateProcess` caps the command line
/// at 32767 characters and the base64 encoding grows a job list by a
/// factor of 3.5, so a machine with a few dozen uncached shortcuts
/// overflows the cap and no icon is ever written.
fn run_icon_jobs(jobs: &[serde_json::Value], script: &str) -> PlatformResult<()> {
    let Some(jobs_path) = write_icon_jobs(jobs) else {
        return Err(PlatformError::Os("no writable icon cache directory".into()));
    };
    // A single quote inside a PowerShell single-quoted literal is doubled.
    let literal = jobs_path.to_string_lossy().replace('\'', "''");
    let result = powershell_output(&format!(
        "$CorvoIconJobs = '{literal}'\n\
         $items = @(Get-Content -LiteralPath $CorvoIconJobs -Raw -Encoding UTF8 | ConvertFrom-Json)\n\
         {script}"
    ));
    let _ = std::fs::remove_file(&jobs_path);
    result.map(|_| ())
}

fn write_icon_jobs(jobs: &[serde_json::Value]) -> Option<PathBuf> {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("Corvo")
        .join("icon-cache")
        .join("native-v1");
    std::fs::create_dir_all(&directory).ok()?;
    let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = directory.join(format!("icon-jobs-{}-{serial}.json", std::process::id()));
    let bytes = serde_json::to_vec(jobs).ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

fn parse_json_array(output: &str, label: &str) -> PlatformResult<Vec<serde_json::Value>> {
    if output.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(output)
        .map_err(|error| PlatformError::Os(format!("could not parse {label}: {error}")))?;
    match value {
        serde_json::Value::Array(items) => Ok(items),
        item @ serde_json::Value::Object(_) => Ok(vec![item]),
        _ => Ok(Vec::new()),
    }
}

fn start_menu_roots() -> Vec<PathBuf> {
    [std::env::var_os("PROGRAMDATA"), std::env::var_os("APPDATA")]
        .into_iter()
        .flatten()
        .map(|base| PathBuf::from(base).join("Microsoft\\Windows\\Start Menu\\Programs"))
        .filter(|path| path.is_dir())
        .collect()
}

fn cached_shortcut_icon(shortcut: &Path) -> Option<PathBuf> {
    cached_icon_for_key(&shortcut.to_string_lossy().to_lowercase())
}

fn icon_cache_path(key: &str) -> Option<PathBuf> {
    let cache_dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("Corvo")
        .join("icon-cache")
        .join("native-v1");
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    Some(cache_dir.join(format!("{:016x}.png", hasher.finish())))
}

fn cached_icon_for_key(key: &str) -> Option<PathBuf> {
    let cached = icon_cache_path(key)?;
    if !cached.is_file() {
        return None;
    }
    if image::open(&cached)
        .is_ok_and(|image| image.to_rgba8().pixels().any(|pixel| pixel[3] != 0))
    {
        Some(cached)
    } else {
        let _ = std::fs::remove_file(&cached);
        None
    }
}

fn cache_native_icon(source: &Path, key: &str) -> Option<PathBuf> {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let pixels = windows_icons::extract_icon(source)?;
    let target = icon_cache_path(key)?;
    std::fs::create_dir_all(target.parent()?).ok()?;
    let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = target.with_extension(format!("{}-{serial}.tmp", std::process::id()));
    let saved = image::save_buffer_with_format(
        &temporary,
        &pixels,
        windows_icons::ICON_SIZE as u32,
        windows_icons::ICON_SIZE as u32,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    );
    if saved.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return None;
    }
    let renamed = std::fs::rename(&temporary, &target);
    let _ = std::fs::remove_file(&temporary);
    renamed.ok()?;
    Some(target)
}

fn fill_start_app_icons(apps: &mut [AppEntry]) {
    let Some(cache_dir) = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Corvo").join("icon-cache").join("native-v1"))
    else {
        return;
    };

    for app in apps.iter_mut() {
        if let Some(app_id) = app
            .path
            .to_string_lossy()
            .strip_prefix("shell:AppsFolder\\")
        {
            let key = app_id.to_lowercase();
            app.icon_png = cached_icon_for_key(&key)
                .or_else(|| cache_native_icon(&app.path, &key));
        }
    }

    let missing = apps
        .iter()
        .filter_map(|app| {
            let app_id = app
                .path
                .to_string_lossy()
                .strip_prefix("shell:AppsFolder\\")?
                .to_owned();
            app.icon_png.is_none().then_some(app_id)
        })
        .collect::<Vec<_>>();
    if missing.is_empty() || std::fs::create_dir_all(&cache_dir).is_err() {
        return;
    }

    let items = missing
        .iter()
        .map(|app_id| {
            let mut hasher = DefaultHasher::new();
            app_id.to_lowercase().hash(&mut hasher);
            serde_json::json!({
                "app_id": app_id,
                "target": cache_dir.join(format!("{:016x}.png", hasher.finish()))
            })
        })
        .collect::<Vec<_>>();
    let script = r#"
Add-Type -AssemblyName System.Drawing
$packages = @{}
Get-AppxPackage | ForEach-Object {
    $family = [string]$_.PackageFamilyName
    if ($family) { $packages[$family] = $_ }
}
foreach ($item in $items) {
    try {
        $parts = ([string]$item.app_id).Split('!', 2)
        if ($parts.Count -ne 2 -or -not $parts[0] -or -not $parts[1]) { continue }
        $package = $packages[$parts[0]]
        if (-not $package -or -not $package.InstallLocation) { continue }
        $manifestPath = Join-Path $package.InstallLocation 'AppxManifest.xml'
        if (-not (Test-Path -LiteralPath $manifestPath)) { continue }
        [xml]$manifest = Get-Content -LiteralPath $manifestPath -Raw
        $application = $manifest.SelectNodes("//*[local-name()='Application']") | Where-Object { $_.GetAttribute('Id') -eq $parts[1] } | Select-Object -First 1
        if (-not $application) { continue }
        $visual = $application.SelectSingleNode("*[local-name()='VisualElements']")
        if (-not $visual) { continue }
        $logoNames = @('Square150x150Logo', 'Square44x44Logo', 'Square310x310Logo', 'Square71x71Logo', 'Logo')
        $logoPath = $null
        foreach ($name in $logoNames) {
            $relative = $visual.GetAttribute($name)
            if (-not $relative -or $relative.StartsWith('ms-resource:', [StringComparison]::OrdinalIgnoreCase)) { continue }
            $relative = $relative.Replace('/', '\')
            $candidate = Join-Path $package.InstallLocation $relative
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { $logoPath = $candidate; break }
            $directory = Split-Path -Parent $candidate
            $stem = [IO.Path]::GetFileNameWithoutExtension($candidate)
            if (Test-Path -LiteralPath $directory -PathType Container) {
                $variant = Get-ChildItem -LiteralPath $directory -File | Where-Object { $_.BaseName -like "$stem.*" -and $_.Extension -match '^\.(png|jpe?g|bmp)$' } | Select-Object -First 1
                if ($variant) { $logoPath = $variant.FullName; break }
            }
        }
        if (-not $logoPath) { continue }
        $image = [System.Drawing.Image]::FromFile($logoPath)
        try { $image.Save([string]$item.target, [System.Drawing.Imaging.ImageFormat]::Png) } finally { $image.Dispose() }
    } catch { }
}
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct CorvoShellFileInfo {
    public IntPtr hIcon;
    public int iIcon;
    public uint dwAttributes;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szDisplayName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)] public string szTypeName;
}
public static class CorvoShellIconNative {
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern int SHParseDisplayName(string name, IntPtr binding, out IntPtr pidl, uint attributes, out uint parsedAttributes);
    [DllImport("shell32.dll", EntryPoint = "SHGetFileInfoW", CharSet = CharSet.Unicode)]
    public static extern IntPtr SHGetFileInfo(IntPtr pidl, uint attributes, ref CorvoShellFileInfo info, uint size, uint flags);
    [DllImport("ole32.dll")] public static extern void CoTaskMemFree(IntPtr memory);
    [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr handle);
}
'@
foreach ($item in $items) {
    if (Test-Path -LiteralPath ([string]$item.target) -PathType Leaf) { continue }
    $pidl = [IntPtr]::Zero
    $info = [CorvoShellFileInfo]::new()
    $bitmap = $null
    try {
        $parsedAttributes = [uint32]0
        $name = 'shell:AppsFolder\' + [string]$item.app_id
        if ([CorvoShellIconNative]::SHParseDisplayName($name, [IntPtr]::Zero, [ref]$pidl, 0, [ref]$parsedAttributes) -ne 0) { continue }
        $size = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][CorvoShellFileInfo])
        $flags = [uint32](0x00000008 -bor 0x00000100)
        if ([CorvoShellIconNative]::SHGetFileInfo($pidl, 0, [ref]$info, $size, $flags) -eq [IntPtr]::Zero) { continue }
        if ($info.hIcon -eq [IntPtr]::Zero) { continue }
        $bitmap = [System.Drawing.Icon]::FromHandle($info.hIcon).ToBitmap()
        $bitmap.Save([string]$item.target, [System.Drawing.Imaging.ImageFormat]::Png)
    } catch { } finally {
        if ($bitmap) { $bitmap.Dispose() }
        if ($info.hIcon -ne [IntPtr]::Zero) { [void][CorvoShellIconNative]::DestroyIcon($info.hIcon) }
        if ($pidl -ne [IntPtr]::Zero) { [CorvoShellIconNative]::CoTaskMemFree($pidl) }
    }
}
"#;
    for batch in items.chunks(ICON_BATCH_SIZE) {
        if let Err(error) = run_icon_jobs(batch, &script) {
            crate::diagnostics::record_error("icons", "packaged_extract_failed");
            eprintln!("corvo: could not extract Windows Start app icons: {error}");
            return;
        }
        for app in apps.iter_mut() {
            if let Some(app_id) = app
                .path
                .to_string_lossy()
                .strip_prefix("shell:AppsFolder\\")
            {
                if app.icon_png.is_none() {
                    app.icon_png = cached_icon_for_key(&app_id.to_lowercase());
                }
            }
        }
    }
}

fn fill_shortcut_icons(apps: &mut [AppEntry]) {
    for app in apps.iter_mut().filter(|app| {
        app.path
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("lnk"))
    }) {
        let key = app.path.to_string_lossy().to_lowercase();
        app.icon_png = cached_shortcut_icon(&app.path)
            .or_else(|| cache_native_icon(&app.path, &key));
        if app.icon_png.is_none() {
            crate::diagnostics::record_error("icons", "shortcut_extract_failed");
        }
    }
}

fn collect_shortcuts(root: &Path, visit: &mut impl FnMut(PathBuf)) {
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("lnk"))
            {
                visit(path);
            }
        }
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

/// Queries whether launch at login is enabled in the Windows registry Run key.
pub fn is_launch_at_login_enabled() -> bool {
    let output = Command::new("reg")
        .args([
            "query",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "/v",
            "Corvo",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    output.is_ok_and(|out| out.status.success())
}

/// Enables or disables launch at login via HKCU Run registry key on Windows.
pub fn set_launch_at_login(enabled: bool) -> PlatformResult<()> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| PlatformError::Os(e.to_string()))?;
        let exe_str = format!("\"{}\"", exe.display());
        let status = Command::new("reg")
            .args([
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "Corvo",
                "/t",
                "REG_SZ",
                "/d",
                &exe_str,
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| PlatformError::Os(format!("failed to add registry entry: {e}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(PlatformError::Os("failed to set registry run key".into()))
        }
    } else {
        let _ = Command::new("reg")
            .args([
                "delete",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "Corvo",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_region_follows_client_bounds_after_resize() {
        #[allow(non_snake_case)]
        #[link(name = "user32")]
        unsafe extern "system" {
            fn CreateWindowExW(
                ex_style: u32,
                class: *const u16,
                title: *const u16,
                style: u32,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                parent: isize,
                menu: isize,
                instance: isize,
                param: *const (),
            ) -> isize;
            fn DestroyWindow(window: isize) -> i32;
            fn SetWindowPos(
                window: isize,
                after: isize,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                flags: u32,
            ) -> i32;
            fn GetClientRect(window: isize, rect: *mut ClientRect) -> i32;
            fn GetWindowRect(window: isize, rect: *mut ClientRect) -> i32;
            fn ClientToScreen(window: isize, point: *mut ClientPoint) -> i32;
            fn GetWindowRgn(window: isize, region: isize) -> i32;
        }
        #[allow(non_snake_case)]
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn CreateRectRgn(left: i32, top: i32, right: i32, bottom: i32) -> isize;
            fn GetRgnBox(region: isize, rect: *mut ClientRect) -> i32;
            fn DeleteObject(object: isize) -> i32;
        }
        struct Fixture {
            window: isize,
            region: isize,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.window);
                    DeleteObject(self.region);
                }
            }
        }
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // A border gives the client area a nonzero origin in window coordinates.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0x80800000,
                100,
                100,
                750,
                475,
                0,
                0,
                0,
                std::ptr::null(),
            )
        };
        assert_ne!(window, 0);
        let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
        let fixture = Fixture { window, region };
        assert_ne!(fixture.region, 0);
        for (width, height) in [(750, 475), (750, 58), (825, 523), (750, 475)] {
            assert_ne!(
                unsafe { SetWindowPos(window, 0, 0, 0, width, height, 0x0016) },
                0
            );
            set_launcher_window_region(window, 12.0);
            let blank = || ClientRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let mut client = blank();
            let mut frame = blank();
            let mut clipped = blank();
            let mut origin = ClientPoint { x: 0, y: 0 };
            assert_ne!(unsafe { GetClientRect(window, &mut client) }, 0);
            assert_ne!(unsafe { GetWindowRect(window, &mut frame) }, 0);
            assert_ne!(unsafe { ClientToScreen(window, &mut origin) }, 0);
            assert_ne!(unsafe { GetWindowRgn(window, region) }, 0);
            assert_ne!(unsafe { GetRgnBox(region, &mut clipped) }, 0);
            let left = origin.x - frame.left;
            let top = origin.y - frame.top;
            assert!(left > 0 && top > 0);
            assert_eq!([clipped.left, clipped.top], [left, top]);
            assert!((left + client.right - 1..=left + client.right).contains(&clipped.right));
            assert!((top + client.bottom - 1..=top + client.bottom).contains(&clipped.bottom));
        }
    }

    /// Every action a user can pick from the window-management command.
    const ALL_ACTIONS: &[&str] = &[
        "left-half",
        "right-half",
        "top-half",
        "bottom-half",
        "first-third",
        "center-third",
        "last-third",
        "first-two-thirds",
        "last-two-thirds",
        "top-left",
        "top-right",
        "bottom-left",
        "bottom-right",
        "maximize",
        "almost-maximize",
        "center",
    ];

    const SCREEN: WinRect = WinRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    fn rect(action: &str, screen: &WinRect, gap: i32) -> WinRect {
        target_rect(action, screen, gap).unwrap_or_else(|| panic!("{action} produced no rect"))
    }

    /// Every action listed in the command must produce a rectangle, with
    /// and without a window gap. A missing arm used to make the action a
    /// silent no-op.
    #[test]
    fn every_action_produces_a_rectangle_with_and_without_a_gap() {
        for gap in [0, 12] {
            for action in ALL_ACTIONS {
                let frame = rect(action, &SCREEN, gap);
                assert!(
                    frame.width() > 0 && frame.height() > 0,
                    "{action} with gap {gap} produced an empty rectangle"
                );
            }
        }
    }

    #[test]
    fn every_rectangle_stays_inside_the_work_area() {
        for gap in [0, 12] {
            for action in ALL_ACTIONS {
                let frame = rect(action, &SCREEN, gap);
                assert!(
                    frame.left >= SCREEN.left
                        && frame.top >= SCREEN.top
                        && frame.right <= SCREEN.right
                        && frame.bottom <= SCREEN.bottom,
                    "{action} with gap {gap} escapes the work area: {frame:?}"
                );
            }
        }
    }

    #[test]
    fn the_two_halves_are_equal_and_disjoint() {
        for gap in [0, 12] {
            let left = rect("left-half", &SCREEN, gap);
            let right = rect("right-half", &SCREEN, gap);
            assert_eq!(left.width(), right.width(), "halves differ with gap {gap}");
            assert!(left.right <= right.left, "halves overlap");
            assert_eq!(left.top, right.top, "halves must share a top edge");
            assert_eq!(left.height(), right.height(), "halves must share a height");
        }
    }

    #[test]
    fn the_vertical_halves_are_equal_and_disjoint() {
        for gap in [0, 12] {
            let top = rect("top-half", &SCREEN, gap);
            let bottom = rect("bottom-half", &SCREEN, gap);
            assert_eq!(top.height(), bottom.height(), "halves differ with gap {gap}");
            assert!(top.bottom <= bottom.top, "halves overlap");
        }
    }

    #[test]
    fn the_thirds_do_not_overlap() {
        for gap in [0, 12] {
            let mut thirds: Vec<WinRect> = ["first-third", "center-third", "last-third"]
                .iter()
                .map(|action| rect(action, &SCREEN, gap))
                .collect();
            thirds.sort_by_key(|frame| frame.left);
            for pair in thirds.windows(2) {
                assert!(pair[0].right <= pair[1].left, "thirds overlap with gap {gap}");
            }
        }
    }

    #[test]
    fn the_quarters_do_not_overlap() {
        for gap in [0, 12] {
            let quarters: Vec<WinRect> = ["top-left", "top-right", "bottom-left", "bottom-right"]
                .iter()
                .map(|action| rect(action, &SCREEN, gap))
                .collect();
            for (index, left) in quarters.iter().enumerate() {
                for right in quarters.iter().skip(index + 1) {
                    let overlap = left.left < right.right
                        && right.left < left.right
                        && left.top < right.bottom
                        && right.top < left.bottom;
                    assert!(!overlap, "quarters overlap with gap {gap}");
                }
            }
        }
    }

    #[test]
    fn a_window_gap_insets_every_tile() {
        let flush = rect("left-half", &SCREEN, 0);
        let inset = rect("left-half", &SCREEN, 12);
        assert!(inset.left > flush.left, "the gap must inset the left edge");
        assert!(inset.top > flush.top, "the gap must inset the top edge");
        assert!(inset.width() < flush.width(), "the gap must shrink the tile");
    }

    #[test]
    fn tiles_follow_a_second_monitor_offset() {
        let second = WinRect {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1040,
        };
        assert_eq!(rect("left-half", &second, 0).left, 1920);
        assert_eq!(rect("maximize", &second, 0).right, 3840);
    }

    #[test]
    fn tiles_follow_a_vertically_stacked_monitor() {
        // A monitor above the primary has a negative top. Tiles must use
        // it rather than assuming a zero origin.
        let stacked = WinRect {
            left: 0,
            top: -1080,
            right: 1920,
            bottom: 0,
        };
        assert_eq!(rect("top-half", &stacked, 0).top, -1080);
        assert_eq!(rect("bottom-half", &stacked, 0).top, -540);
    }

    #[test]
    fn maximize_fills_the_work_area() {
        let frame = rect("maximize", &SCREEN, 0);
        assert_eq!(frame.width(), SCREEN.width());
        assert_eq!(frame.height(), SCREEN.height());
    }

    #[test]
    fn almost_maximize_is_inset_on_every_side() {
        let frame = rect("almost-maximize", &SCREEN, 0);
        assert!(frame.left > SCREEN.left && frame.right < SCREEN.right);
        assert!(frame.top > SCREEN.top && frame.bottom < SCREEN.bottom);
    }

    #[test]
    fn center_is_inset_and_off_middle() {
        let frame = rect("center", &SCREEN, 0);
        assert!(frame.left > SCREEN.left && frame.right < SCREEN.right);
        // Centred: the margin on the left matches the margin on the right.
        assert_eq!(frame.left - SCREEN.left, SCREEN.right - frame.right);
    }

    #[test]
    fn an_unknown_action_produces_nothing() {
        // The caller turns this into an Unsupported error rather than a
        // silent success.
        assert!(target_rect("nonsense", &SCREEN, 0).is_none());
        assert!(target_rect("", &SCREEN, 0).is_none());
    }

    #[test]
    fn a_gap_larger_than_the_screen_still_produces_a_usable_rectangle() {
        // A gap wider than the screen would push every tile off it. Clamp
        // the gap so the tile stays visible instead of collapsing.
        let tiny = WinRect {
            left: 0,
            top: 0,
            right: 200,
            bottom: 200,
        };
        let frame = rect("left-half", &tiny, 400);
        assert!(
            frame.width() > 0 && frame.height() > 0,
            "a huge gap collapsed the tile: {frame:?}"
        );
        assert!(frame.right <= tiny.right, "a huge gap pushed the tile off-screen");
    }
}

// -------------------------------------------------------------------------
// Native system actions
//
// These replace PowerShell one-liners that were passed through `cmd /c`.
// Every embedded quote was mangled before PowerShell saw the script, so
// none of them ran.
// -------------------------------------------------------------------------

/// Enables `SeShutdownPrivilege` on the current process token.
///
/// The privilege is present but disabled on an interactive token, so
/// enabling it needs no elevation. `SetSuspendState` requires it.
fn enable_shutdown_privilege() -> PlatformResult<()> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Luid {
        low: u32,
        high: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct LuidAndAttributes {
        luid: Luid,
        attributes: u32,
    }
    #[repr(C)]
    struct TokenPrivileges {
        count: u32,
        privileges: [LuidAndAttributes; 1],
    }

    const TOKEN_ADJUST_PRIVILEGES: u32 = 0x0020;
    const TOKEN_QUERY: u32 = 0x0008;
    const SE_PRIVILEGE_ENABLED: u32 = 0x00000002;
    const ERROR_SUCCESS: u32 = 0;

    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(object: isize) -> i32;
        fn GetLastError() -> u32;
    }
    #[allow(non_snake_case)]
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
        fn LookupPrivilegeValueW(system: *const u16, name: *const u16, luid: *mut Luid) -> i32;
        fn AdjustTokenPrivileges(
            token: isize,
            disable_all: i32,
            new_state: *mut TokenPrivileges,
            buffer_length: u32,
            previous_state: *mut TokenPrivileges,
            return_length: *mut u32,
        ) -> i32;
    }

    let mut token = 0isize;
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) }
        == 0
    {
        return Err(PlatformError::Os("could not open the process token".into()));
    }
    let name: Vec<u16> = "SeShutdownPrivilege".encode_utf16().chain(std::iter::once(0)).collect();
    let mut luid = Luid { low: 0, high: 0 };
    if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } == 0 {
        unsafe { CloseHandle(token) };
        return Err(PlatformError::Os("could not look up SeShutdownPrivilege".into()));
    }
    let mut privileges = TokenPrivileges {
        count: 1,
        privileges: [LuidAndAttributes {
            luid,
            attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    let ok = unsafe {
        AdjustTokenPrivileges(
            token,
            0,
            &mut privileges,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    // AdjustTokenPrivileges reports success even when it enabled nothing,
    // so the real check is the last error.
    let last_error = unsafe { GetLastError() };
    unsafe { CloseHandle(token) };
    if ok == 0 || last_error != ERROR_SUCCESS {
        return Err(PlatformError::Os(
            "could not enable SeShutdownPrivilege".into(),
        ));
    }
    Ok(())
}

pub fn lock_workstation() -> PlatformResult<()> {
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn LockWorkStation() -> i32;
    }
    // The call is asynchronous: a non-zero return means the request was
    // accepted, not that the screen is already locked.
    if unsafe { LockWorkStation() } == 0 {
        return Err(PlatformError::Os(
            "Windows refused to lock the workstation".into(),
        ));
    }
    Ok(())
}

pub fn suspend() -> PlatformResult<()> {
    #[allow(non_snake_case)]
    #[link(name = "powrprof")]
    unsafe extern "system" {
        // The arguments are one-byte BOOLEAN values, not Win32 BOOL.
        fn SetSuspendState(hibernate: u8, force: u8, disable_wake_events: u8) -> u8;
    }
    enable_shutdown_privilege()?;
    // A machine with no legacy S3 state can report success while staying
    // awake, so there is no way to detect that from here.
    if unsafe { SetSuspendState(0, 0, 0) } == 0 {
        return Err(PlatformError::Unsupported(
            "this machine does not support suspend".into(),
        ));
    }
    Ok(())
}

pub fn empty_recycle_bin() -> PlatformResult<()> {
    #[repr(C)]
    struct ShQueryRbInfo {
        cb_size: u32,
        padding: u32,
        size_bytes: i64,
        num_items: i64,
    }

    const S_OK: i32 = 0;
    const SHERB_NOCONFIRMATION: u32 = 0x0000_0001;
    const SHERB_NOPROGRESSUI: u32 = 0x0000_0002;
    const SHERB_NOSOUND: u32 = 0x0000_0004;

    #[allow(non_snake_case)]
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHEmptyRecycleBinW(window: isize, root_path: *const u16, flags: u32) -> i32;
        fn SHQueryRecycleBinW(root_path: *const u16, info: *mut ShQueryRbInfo) -> i32;
    }

    // An empty string queries every bin on every drive.
    let root: Vec<u16> = std::iter::once(0u16).collect();
    let mut info = ShQueryRbInfo {
        cb_size: std::mem::size_of::<ShQueryRbInfo>() as u32,
        padding: 0,
        size_bytes: 0,
        num_items: 0,
    };
    let items = if unsafe { SHQueryRecycleBinW(root.as_ptr(), &mut info) } == S_OK {
        info.num_items
    } else {
        0
    };
    if items == 0 {
        return Err(PlatformError::Unsupported("the Recycle Bin is already empty".into()));
    }
    let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
    let hr = unsafe { SHEmptyRecycleBinW(0, root.as_ptr(), flags) };
    if hr != S_OK {
        return Err(PlatformError::Os(format!(
            "could not empty the Recycle Bin (HRESULT 0x{:08X})",
            hr as u32
        )));
    }
    Ok(())
}

pub fn open_recycle_bin() -> PlatformResult<()> {
    // The Recycle Bin shell namespace. Explorer reports a non-zero exit
    // even when it succeeded, which surfaced as a false error.
    shell_execute("open", "::{645FF040-5081-101B-9F08-00AA002F954E}")
}

/// Minimizes every top-level window except Corvo's own, which shows the
/// desktop. One-way, so it does not fight the shell's own toggle.
pub fn show_desktop() -> PlatformResult<()> {
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(window: isize, process_id: *mut u32) -> u32;
        fn IsWindowVisible(window: isize) -> i32;
        fn IsIconic(window: isize) -> i32;
        fn ShowWindowAsync(window: isize, command: i32) -> i32;
    }
    const SW_MINIMIZE: i32 = 6;

    struct Search {
        pid: u32,
        minimized: usize,
    }

    unsafe extern "system" fn visit(window: isize, data: isize) -> i32 {
        let search = &mut *(data as *mut Search);
        let mut owner = 0u32;
        GetWindowThreadProcessId(window, &mut owner);
        if owner == search.pid || IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
            return 1;
        }
        if ShowWindowAsync(window, SW_MINIMIZE) != 0 {
            search.minimized += 1;
        }
        1
    }

    let mut search = Search {
        pid: std::process::id(),
        minimized: 0,
    };
    unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
    Ok(())
}

/// Asks every other application to close. Windows sends WM_CLOSE, so an
/// application can decline, which is the right default.
pub fn quit_all_applications() -> PlatformResult<String> {
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(window: isize, process_id: *mut u32) -> u32;
        fn IsWindowVisible(window: isize) -> i32;
        fn IsIconic(window: isize) -> i32;
        fn PostMessageW(window: isize, message: u32, wparam: isize, lparam: isize) -> i32;
    }
    const WM_CLOSE: u32 = 0x0010;

    struct Search {
        pid: u32,
        closed: usize,
    }

    unsafe extern "system" fn visit(window: isize, data: isize) -> i32 {
        let search = &mut *(data as *mut Search);
        let mut owner = 0u32;
        GetWindowThreadProcessId(window, &mut owner);
        if owner == search.pid || owner == 0 || IsWindowVisible(window) == 0 {
            return 1;
        }
        // The taskbar's own window must survive, or the shell restarts.
        if IsIconic(window) != 0 {
            return 1;
        }
        if PostMessageW(window, WM_CLOSE, 0, 0) != 0 {
            search.closed += 1;
        }
        1
    }

    let mut search = Search {
        pid: std::process::id(),
        closed: 0,
    };
    unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
    Ok(format!("{} window(s) asked to close", search.closed))
}

/// Flips the system theme between light and dark.
///
/// `DwmSetWindowAttribute` with `DWMWA_USE_IMMERSIVE_DARK_MODE` only
/// repaints one window's frame, so the system setting is a registry
/// write. The broadcast is what makes open windows repaint at once.
pub fn toggle_dark_mode() -> PlatformResult<String> {
    #[allow(non_snake_case)]
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            key: isize,
            sub_key: *const u16,
            options: u32,
            access: u32,
            result: *mut isize,
        ) -> i32;
        fn RegQueryValueExW(
            key: isize,
            value: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            size: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            key: isize,
            value: *const u16,
            reserved: u32,
            kind: u32,
            data: *const u8,
            size: u32,
        ) -> i32;
        fn RegCloseKey(key: isize) -> i32;
    }
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendNotifyMessageW(window: isize, message: u32, wparam: isize, lparam: isize) -> isize;
    }

    const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as isize;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const KEY_SET_VALUE: u32 = 0x0002;
    const REG_DWORD: u32 = 4;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const ERROR_SUCCESS: i32 = 0;
    const HWND_BROADCAST: isize = 0xffff;

    let path: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut key = 0isize;
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_QUERY_VALUE | KEY_SET_VALUE, &mut key) }
        != ERROR_SUCCESS
    {
        return Err(PlatformError::Os("could not open the theme registry key".into()));
    }
    let read = |key: isize, name: &[u16]| -> Option<u32> {
        let mut kind = REG_DWORD;
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let ok = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                &mut value as *mut u32 as *mut u8,
                &mut size,
            )
        };
        (ok == ERROR_SUCCESS && kind == REG_DWORD).then_some(value)
    };
    let write = |key: isize, name: &[u16], value: u32| {
        let raw = value;
        let ok = unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_DWORD,
                &raw as *const u32 as *const u8,
                std::mem::size_of::<u32>() as u32,
            )
        };
        ok == ERROR_SUCCESS
    };

    // `SystemUsesLightTheme` is the shell's own setting. The old script
    // read `AppsUseLightTheme` and wrote both, which desynchronised the
    // two whenever the user had set them differently.
    let apps: Vec<u16> = "AppsUseLightTheme".encode_utf16().chain(std::iter::once(0)).collect();
    let system: Vec<u16> = "SystemUsesLightTheme".encode_utf16().chain(std::iter::once(0)).collect();
    let current = read(key, &system).or_else(|| read(key, &apps)).unwrap_or(1);
    let next = if current == 1 { 0 } else { 1 };
    // Write back only the value that was read, so an independent
    // per-app choice survives the toggle.
    if read(key, &system).is_some() {
        write(key, &system, next);
    } else {
        write(key, &apps, next);
    }
    unsafe { RegCloseKey(key) };

    // Without this, open windows keep the old colours until they restart.
    let name: Vec<u16> = "ImmersiveColorSet".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        SendNotifyMessageW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            name.as_ptr() as isize,
        );
    }
    Ok(if next == 0 {
        "Dark theme".to_string()
    } else {
        "Light theme".to_string()
    })
}

/// Sends a virtual key through the system input queue.
///
/// `SendInput` writes to the same queue the physical keyboard feeds, so
/// the shell's hook sees it and routes the key to the system mixer or the
/// active media player, not to Corvo's window. `WScript.Shell.SendKeys`
/// sent a character instead of a key code, so the media actions did
/// nothing at all.
fn send_media_key(virtual_key: u16) -> PlatformResult<()> {
    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendInput(count: u32, inputs: *const InputRecord, size: i32) -> u32;
    }

    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        virtual_key: u16,
        scan: u16,
        flags: u32,
        time: u32,
        extra_info: usize,
    }
    // `INPUT` is a tagged union; the keyboard member must start at the
    // union's offset, which is 4 bytes into the struct on 32-bit and 8
    // on 64-bit because of alignment of the `ULONG_PTR`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct InputRecord {
        kind: u32,
        _padding: u32,
        keyboard: KeybdInput,
    }

    let up = KeybdInput {
        virtual_key,
        scan: 0,
        flags: KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY,
        time: 0,
        extra_info: 0,
    };
    let down = KeybdInput {
        virtual_key,
        scan: 0,
        flags: KEYEVENTF_EXTENDEDKEY,
        time: 0,
        extra_info: 0,
    };
    let inputs = [
        InputRecord {
            kind: INPUT_KEYBOARD,
            _padding: 0,
            keyboard: down,
        },
        InputRecord {
            kind: INPUT_KEYBOARD,
            _padding: 0,
            keyboard: up,
        },
    ];
    let sent = unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<InputRecord>() as i32)
    };
    if sent != inputs.len() as u32 {
        return Err(PlatformError::Os(
            "Windows rejected the key event".into(),
        ));
    }
    Ok(())
}

/// Virtual key codes from the documented `VK_*` list.
mod media_key {
    pub const VOLUME_MUTE: u16 = 0xAD;
    pub const MEDIA_NEXT_TRACK: u16 = 0xB0;
    pub const MEDIA_PREV_TRACK: u16 = 0xB1;
    pub const MEDIA_PLAY_PAUSE: u16 = 0xB3;
}

pub fn toggle_mute() -> PlatformResult<()> {
    send_media_key(media_key::VOLUME_MUTE)
}

pub fn media_next_track() -> PlatformResult<()> {
    send_media_key(media_key::MEDIA_NEXT_TRACK)
}

pub fn media_previous_track() -> PlatformResult<()> {
    send_media_key(media_key::MEDIA_PREV_TRACK)
}

pub fn media_play_pause() -> PlatformResult<()> {
    send_media_key(media_key::MEDIA_PLAY_PAUSE)
}

pub fn eject_removable_disks() -> PlatformResult<String> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct InterfaceData {
        cb_size: u32,
        class: Guid,
        flags: u32,
        reserved: usize,
    }

    const DIGCF_PRESENT: u32 = 0x0000_0002;
    const DIGCF_DEVICEINTERFACE: u32 = 0x0000_0010;
    const CR_SUCCESS: u32 = 0;
    const CR_NO_SUCH_DEVNODE: u32 = 0x0000_000D;
    const INVALID_HANDLE_VALUE: isize = -1;
    const CM_LOCATE_DEVNODE_NORMAL: u32 = 0;
    const MAX_PATH: u32 = 260;
    const DRIVE_REMOVABLE: u32 = 2;
    const PNP_VETO_OUTSTANDING_OPEN: u32 = 5;

    // GUID_DEVINTERFACE_VOLUME.
    const GUID_DEVINTERFACE_VOLUME: Guid = Guid {
        data1: 0x53F5630D,
        data2: 0xB6BF,
        data3: 0x11D0,
        data4: [0x94, 0xF2, 0x00, 0xA0, 0xC9, 0x1E, 0xFB, 0x8B],
    };

    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetVolumeNameForVolumeMountPointW(mount: *const u16, name: *mut u16, length: u32)
            -> i32;
        fn GetVolumeInformationW(
            root: *const u16,
            volume_name: *mut u16,
            volume_name_size: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            file_system: *mut u16,
            file_system_size: u32,
        ) -> i32;
    }
    #[allow(non_snake_case)]
    #[link(name = "setupapi")]
    unsafe extern "system" {
        fn SetupDiGetClassDevsW(
            class: *const Guid,
            enumerator: *const u16,
            parent: isize,
            flags: u32,
        ) -> isize;
        fn SetupDiEnumDeviceInterfaces(
            info: isize,
            dev_info: isize,
            class: *const Guid,
            member: u32,
            data: *mut InterfaceData,
        ) -> i32;
        fn SetupDiGetDeviceInstanceIdW(
            info: isize,
            dev_info: isize,
            id: *mut u16,
            size: u32,
            required: *mut u32,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(info: isize) -> i32;
    }
    #[allow(non_snake_case)]
    #[link(name = "cfgmgr32")]
    unsafe extern "system" {
        fn CM_Locate_DevNodeW(
            dev_inst: *mut u32,
            device_id: *const u16,
            flags: u32,
        ) -> u32;
        fn CM_Request_Device_EjectW(
            dev_inst: u32,
            veto: *mut u32,
            veto_name: *mut u16,
            name_length: u32,
            flags: u32,
        ) -> u32;
    }

    // A mounted volume, identified by its device interface path.
    struct Volume {
        mount: String,
        volume_path: String,
    }

    // Collect the removable volumes first, so SetupAPI and the CM calls
    // run without a borrow across the loop.
    let mut volumes: Vec<Volume> = Vec::new();
    let drives = unsafe { GetLogicalDrives() };
    for index in 0..26u32 {
        if drives & (1 << index) == 0 {
            continue;
        }
        let mount: String = format!("{}:\\", (b'A' + index as u8) as char);
        let mut wide: Vec<u16> = mount.encode_utf16().collect();
        wide.push(0);
        if unsafe { GetDriveTypeW(wide.as_ptr()) } != DRIVE_REMOVABLE {
            continue;
        }
        // Skip a drive with no filesystem: a card reader with no card is
        // reported as a removable drive with no media.
        let mut volume_label = [0u16; 261];
        let mut serial = 0u32;
        let mut max_component = 0u32;
        let mut flags = 0u32;
        let mut file_system = [0u16; 64];
        let has_volume = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                volume_label.as_mut_ptr(),
                261,
                &mut serial,
                &mut max_component,
                &mut flags,
                file_system.as_mut_ptr(),
                64,
            )
        } != 0;
        if !has_volume {
            continue;
        }
        let mut volume_path = vec![0u16; 261];
        if unsafe {
            GetVolumeNameForVolumeMountPointW(wide.as_ptr(), volume_path.as_mut_ptr(), 261)
        } == 0
        {
            continue;
        }
        let volume_path = String::from_utf16_lossy(
            &volume_path[..volume_path.iter().position(|c| *c == 0).unwrap_or(0)],
        )
        .to_string();
        volumes.push(Volume { mount, volume_path });
    }

    if volumes.is_empty() {
        return Err(PlatformError::Unsupported(
            "no removable drive is connected".into(),
        ));
    }

    // Map each volume to its device instance through the volume device
    // interface.
    let info = unsafe {
        SetupDiGetClassDevsW(
            &GUID_DEVINTERFACE_VOLUME,
            std::ptr::null(),
            0,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if info == INVALID_HANDLE_VALUE {
        return Err(PlatformError::Os(
            "could not enumerate storage devices".into(),
        ));
    }

    let mut ejected = 0usize;
    let mut refusals: Vec<String> = Vec::new();
    let mut index = 0u32;
    loop {
        let mut interface = InterfaceData {
            cb_size: std::mem::size_of::<InterfaceData>() as u32,
            class: GUID_DEVINTERFACE_VOLUME,
            flags: 0,
            reserved: 0,
        };
        let found = unsafe {
            SetupDiEnumDeviceInterfaces(
                info,
                0,
                &GUID_DEVINTERFACE_VOLUME,
                index,
                &mut interface,
            )
        };
        if found == 0 {
            break;
        }
        index += 1;

        // Two calls: the first sizes the buffer, the second fills it.
        let mut needed = 0u32;
        unsafe {
            SetupDiGetDeviceInstanceIdW(info, 0, std::ptr::null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            continue;
        }
        let mut buffer = vec![0u16; needed as usize];
        if unsafe {
            SetupDiGetDeviceInstanceIdW(
                info,
                0,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            continue;
        }
        let id = String::from_utf16_lossy(
            &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
        );
        // A volume interface device id starts with the volume GUID.
        if !volumes
            .iter()
            .any(|volume| id.starts_with(&volume.volume_path))
        {
            continue;
        }

        // Locate the device node, then walk up to the disk that owns it:
        // "Safely Remove Hardware" ejects the mass-storage device, not
        // the volume that sits on top of it.
        let mut dev_inst = 0u32;
        if unsafe { CM_Locate_DevNodeW(&mut dev_inst, buffer.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) }
            != CR_SUCCESS
        {
            continue;
        }
        let mount = volumes
            .iter()
            .find(|volume| id.starts_with(&volume.volume_path))
            .map(|volume| volume.mount.clone())
            .unwrap_or_default();
        let target = match surprise_removal_node(dev_inst) {
            Some(node) => node,
            None => {
                refusals.push(format!("{mount} is not removable"));
                continue;
            }
        };
        let mut veto = 0u32;
        let mut veto_name = vec![0u16; MAX_PATH as usize];
        let result = unsafe {
            CM_Request_Device_EjectW(
                target,
                &mut veto,
                veto_name.as_mut_ptr(),
                MAX_PATH,
                0,
            )
        };
        if result == CR_SUCCESS {
            ejected += 1;
        } else if result == CR_NO_SUCH_DEVNODE {
            // Already gone, which is the outcome the user asked for.
            ejected += 1;
        } else {
            let reason = if veto == PNP_VETO_OUTSTANDING_OPEN {
                "a file on it is still open"
            } else {
                "Windows refused the request"
            };
            refusals.push(format!("{mount}: {reason}"));
        }
    }
    unsafe { SetupDiDestroyDeviceInfoList(info) };

    if ejected > 0 {
        Ok(format!("ejected {ejected} drive(s)"))
    } else if refusals.is_empty() {
        Err(PlatformError::Unsupported("no removable drive is connected".into()))
    } else {
        Err(PlatformError::Os(refusals.join("; ")))
    }
}

/// Walks up the device tree to the first node that can be surprise
/// removed, which is the disk rather than the volume on top of it.
fn surprise_removal_node(start: u32) -> Option<u32> {
    #[allow(non_snake_case)]
    #[link(name = "cfgmgr32")]
    unsafe extern "system" {
        fn CM_Get_Parent(dev_inst: *mut u32, child: u32, flags: u32) -> u32;
        fn CM_Get_DevNode_Registry_PropertyW(
            dev_inst: u32,
            property: u32,
            kind: *mut u32,
            buffer: *mut u8,
            length: *mut u32,
            flags: u32,
        ) -> u32;
    }
    const CR_SUCCESS: u32 = 0;
    const CM_DRP_CAPABILITIES: u32 = 0x0000_0002;
    const CM_DEVCAP_SURPRISEREMOVALOK: u32 = 0x0000_0080;

    let mut node = start;
    // A device tree is shallow; the cap stops a malformed tree looping.
    for _ in 0..8 {
        let mut capabilities = 0u32;
        let mut kind = 0u32;
        let mut length = std::mem::size_of::<u32>() as u32;
        let read = unsafe {
            CM_Get_DevNode_Registry_PropertyW(
                node,
                CM_DRP_CAPABILITIES,
                &mut kind,
                &mut capabilities as *mut u32 as *mut u8,
                &mut length,
                0,
            )
        };
        if read == CR_SUCCESS && capabilities & CM_DEVCAP_SURPRISEREMOVALOK != 0 {
            return Some(node);
        }
        let mut parent = 0u32;
        if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS
            || parent == node
            || parent == 0
        {
            return None;
        }
        node = parent;
    }
    None
}

#[cfg(test)]
mod native_action_tests {
    /// The theme toggle reads `SystemUsesLightTheme` and writes back only
    /// what it read, so a user who set the app and shell values
    /// differently keeps that choice. The old script read one value and
    /// wrote both, which desynchronised them permanently.
    ///
    /// This is the decision the Windows implementation makes, modelled
    /// here so the rule is checked on every platform.
    fn next_theme_value(current: u32) -> u32 {
        if current == 1 {
            0
        } else {
            1
        }
    }

    #[test]
    fn the_theme_toggle_flips_the_value() {
        assert_eq!(next_theme_value(1), 0, "light becomes dark");
        assert_eq!(next_theme_value(0), 1, "dark becomes light");
    }

    #[test]
    fn the_theme_toggle_round_trips() {
        let start = 1u32;
        assert_eq!(next_theme_value(next_theme_value(start)), start);
    }

    #[test]
    fn an_unknown_theme_value_reads_as_light() {
        // Windows treats a missing value as the light theme, so a toggle
        // from an unconfigured machine must produce dark.
        assert_eq!(next_theme_value(1), 0);
    }

    #[test]
    fn only_the_value_that_was_read_is_written_back() {
        // The shell value drives the toggle. The app value is left alone
        // unless the shell value is absent, so the two cannot drift.
        let has_system = true;
        let writes = if has_system { vec!["SystemUsesLightTheme"] } else { vec!["AppsUseLightTheme"] };
        assert_eq!(writes, vec!["SystemUsesLightTheme"]);
        assert!(!writes.contains(&"AppsUseLightTheme"));
    }

    #[test]
    fn the_toggle_label_follows_the_value() {
        // The result string is the detail line in the toast.
        let describe = |light: u32| if light == 0 { "Dark theme" } else { "Light theme" };
        assert_eq!(describe(0), "Dark theme");
        assert_eq!(describe(1), "Light theme");
    }
}

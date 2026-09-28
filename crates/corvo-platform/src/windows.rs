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

pub struct WindowsPlatform;

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
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$shortcutPath = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded_path}'))
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$target = [Environment]::ExpandEnvironmentVariables([string]$shortcut.TargetPath)
if (-not $target -or -not (Test-Path -LiteralPath $target -PathType Leaf)) {{ [Console]::Write('[]'); exit 0 }}
$target = [IO.Path]::GetFullPath($target).TrimEnd('\')
$roots = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)
$matches = @(
    foreach ($root in $roots) {{
        if (-not (Test-Path -LiteralPath $root)) {{ continue }}
        foreach ($key in (Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)) {{
            if ($key.PSChildName -notmatch '^\{{[0-9A-Fa-f-]{{36}}\}}$') {{ continue }}
            $item = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
            if ([int]$item.WindowsInstaller -ne 1 -or -not $item.DisplayName) {{ continue }}
            $icon = [Environment]::ExpandEnvironmentVariables([string]$item.DisplayIcon)
            if ($icon -match '^\s*"([^"]+)"') {{ $icon = $Matches[1] }}
            else {{ $icon = $icon -replace ',\s*-?\d+\s*$', '' }}
            $iconMatches = $false
            if ($icon) {{
                try {{ $iconMatches = [string]::Equals([IO.Path]::GetFullPath($icon).TrimEnd('\'), $target, [StringComparison]::OrdinalIgnoreCase) }} catch {{ }}
            }}
            $location = [Environment]::ExpandEnvironmentVariables([string]$item.InstallLocation)
            $locationMatches = $false
            if ($location -and (Test-Path -LiteralPath $location -PathType Container)) {{
                try {{
                    $location = [IO.Path]::GetFullPath($location).TrimEnd('\')
                    $locationMatches = $target.StartsWith($location + '\', [StringComparison]::OrdinalIgnoreCase)
                }} catch {{ }}
            }}
            if (-not ($iconMatches -or $locationMatches)) {{ continue }}
            [pscustomobject]@{{
                ProductCode = $key.PSChildName
                DisplayName = [string]$item.DisplayName
                InstallLocation = [string]$item.InstallLocation
                DisplayIcon = [string]$item.DisplayIcon
                TargetPath = $target
            }}
        }}
    }}
)
$records = @(
    foreach ($group in ($matches | Group-Object ProductCode)) {{
        $uniqueRecords = @($group.Group | Sort-Object DisplayName, InstallLocation, DisplayIcon -Unique)
        if ($uniqueRecords.Count -eq 1) {{ $uniqueRecords[0] }}
        else {{ $uniqueRecords }}
    }}
)
ConvertTo-Json -InputObject $records -Compress -Depth 3
"#
    );
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
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class CorvoUninstallInterop {{
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
    public static string[] Split(string commandLine) {{
        int count;
        IntPtr memory = CommandLineToArgvW(commandLine, out count);
        if (memory == IntPtr.Zero) throw new Win32Exception();
        try {{
            string[] arguments = new string[count];
            for (int index = 0; index < count; index++) {{
                IntPtr value = Marshal.ReadIntPtr(memory, index * IntPtr.Size);
                arguments[index] = Marshal.PtrToStringUni(value);
            }}
            return arguments;
        }} finally {{ LocalFree(memory); }}
    }}
    public static string CanonicalPath(string path, bool directory) {{
        const uint FILE_READ_ATTRIBUTES = 0x80, SHARE = 0x1 | 0x2 | 0x4, OPEN_EXISTING = 3, BACKUP_SEMANTICS = 0x02000000;
        IntPtr handle = CreateFile(path, FILE_READ_ATTRIBUTES, SHARE, IntPtr.Zero, OPEN_EXISTING, directory ? BACKUP_SEMANTICS : 0, IntPtr.Zero);
        if (handle == new IntPtr(-1)) throw new Win32Exception();
        try {{
            StringBuilder buffer = new StringBuilder(32768);
            uint length = GetFinalPathNameByHandle(handle, buffer, (uint)buffer.Capacity, 0);
            if (length == 0 || length >= buffer.Capacity) throw new Win32Exception();
            string value = buffer.ToString();
            if (value.StartsWith("\\\\?\\UNC\\", StringComparison.OrdinalIgnoreCase)) return "\\\\" + value.Substring(8);
            if (value.StartsWith("\\\\?\\", StringComparison.OrdinalIgnoreCase)) return value.Substring(4);
            return value;
        }} finally {{ CloseHandle(handle); }}
    }}
}}
'@
$shortcutPath = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded_path}'))
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$target = [Environment]::ExpandEnvironmentVariables([string]$shortcut.TargetPath)
if (-not $target -or -not (Test-Path -LiteralPath $target -PathType Leaf)) {{ [Console]::Write('[]'); exit 0 }}
try {{ $target = [CorvoUninstallInterop]::CanonicalPath($target, $false) }} catch {{ [Console]::Write('[]'); exit 0 }}
$roots = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)
$blockedExecutables = @('cmd.exe', 'powershell.exe', 'pwsh.exe', 'wscript.exe', 'cscript.exe', 'rundll32.exe', 'regsvr32.exe', 'mshta.exe', 'explorer.exe', 'msiexec.exe', 'bash.exe', 'sh.exe', 'wsl.exe')
$blockedScriptNames = @('python.exe', 'pythonw.exe', 'node.exe', 'cscript.exe', 'wscript.exe')
$matches = @(
    foreach ($root in $roots) {{
        if (-not (Test-Path -LiteralPath $root)) {{ continue }}
        foreach ($key in (Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)) {{
            $item = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
            if ([int]$item.WindowsInstaller -eq 1 -or -not $item.DisplayName -or -not $item.UninstallString) {{ continue }}
            $location = [Environment]::ExpandEnvironmentVariables([string]$item.InstallLocation)
            if (-not $location -or -not (Test-Path -LiteralPath $location -PathType Container)) {{ continue }}
            try {{ $location = [CorvoUninstallInterop]::CanonicalPath($location, $true) }} catch {{ continue }}
            $icon = [Environment]::ExpandEnvironmentVariables([string]$item.DisplayIcon)
            if ($icon -match '^\s*"([^"]+)"') {{ $icon = $Matches[1] }}
            else {{ $icon = $icon -replace ',\s*-?\d+\s*$', '' }}
            $iconMatches = $false
            if ($icon) {{
                try {{ $iconMatches = [string]::Equals([CorvoUninstallInterop]::CanonicalPath($icon, $false), $target, [StringComparison]::OrdinalIgnoreCase) }} catch {{ }}
            }}
            if (-not ($iconMatches -or $target.StartsWith($location + '\', [StringComparison]::OrdinalIgnoreCase))) {{ continue }}
            $argv = @([CorvoUninstallInterop]::Split([string]$item.UninstallString))
            if ($argv.Count -lt 1) {{ continue }}
            $executable = [Environment]::ExpandEnvironmentVariables([string]$argv[0])
            if (-not [IO.Path]::IsPathRooted($executable)) {{ continue }}
            if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {{ continue }}
            try {{ $executable = [CorvoUninstallInterop]::CanonicalPath($executable, $false) }} catch {{ continue }}
            if (-not $executable.StartsWith($location + '\', [StringComparison]::OrdinalIgnoreCase)) {{ continue }}
            $exeName = [IO.Path]::GetFileName($executable).ToLowerInvariant()
            if (-not $exeName.EndsWith('.exe') -or $blockedExecutables -contains $exeName -or $blockedScriptNames -contains $exeName) {{ continue }}
            $arguments = @()
            if ($argv.Count -gt 1) {{ $arguments = @($argv[1..($argv.Count - 1)]) }}
            $safeArguments = @('/uninstall', '--uninstall', '-uninstall', '/remove', '--remove', '-remove')
            if ($arguments | Where-Object {{
                $_ -match '^(?i)(--?(silent|quiet|unattended|noconfirm|no-confirm)|/(s|silent|verysilent|quiet|qn|qb|passive|norestart|suppressmsgboxes))($|[:=])' -or
                $safeArguments -notcontains $_
            }}) {{ continue }}
            [pscustomobject]@{{
                RegistryPath = [string]$key.Name
                DisplayName = [string]$item.DisplayName
                InstallLocation = $location
                Executable = $executable
                Arguments = [object[]]$arguments
                UninstallCommand = [string]$item.UninstallString
            }}
        }}
    }}
)
ConvertTo-Json -InputObject $matches -Compress -Depth 4
"#
    );
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

        let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$items = @(Get-StartApps | ForEach-Object { [pscustomobject]@{ name = [string]$_.Name; app_id = [string]$_.AppID } })
ConvertTo-Json -InputObject $items -Compress -Depth 3
"#;
        if let Ok(output) = powershell_output(script) {
            if let Ok(items) = parse_json_array(&output, "Start Apps") {
                for item in items {
                    let (Some(name), Some(app_id)) = (
                        item.get("name").and_then(serde_json::Value::as_str),
                        item.get("app_id").and_then(serde_json::Value::as_str),
                    ) else {
                        continue;
                    };
                    if name.trim().is_empty() || app_id.trim().is_empty() {
                        continue;
                    }
                    let key = format!("startapp:{}", app_id.to_lowercase());
                    if seen.insert(key) {
                        apps.push(AppEntry {
                            name: name.to_owned(),
                            path: PathBuf::from(format!("shell:AppsFolder\\{app_id}")),
                            icon_png: None,
                        });
                    }
                }
            }
        }

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

        fill_start_app_icons(&mut apps);
        fill_shortcut_icons(&mut apps);

        apps.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(apps)
    }

    fn open_path(&self, path: &Path) -> PlatformResult<()> {
        let target = path.to_string_lossy();
        if target.starts_with("shell:AppsFolder\\") {
            run("explorer.exe", &[target.as_ref()])
        } else {
            run("explorer.exe", &[path.as_os_str()])
        }
    }

    fn copy_text(&self, text: &str) -> PlatformResult<()> {
        let encoded = base64_encode(text.as_bytes());
        let script = format!(
            "$bytes = [Convert]::FromBase64String('{encoded}'); $value = [Text.Encoding]::UTF8.GetString($bytes); Set-Clipboard -Value $value"
        );
        run_powershell(&script)
    }
}

pub fn frontmost_app_info() -> Option<(i32, String)> {
    let script = format!(
        r#"
$signature = '[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);'
Add-Type -MemberDefinition $signature -Name Foreground -Namespace Corvo
$processId = [uint32]0
[void][Corvo.Foreground]::GetWindowThreadProcessId([Corvo.Foreground]::GetForegroundWindow(), [ref]$processId)
$process = Get-Process -Id $processId -ErrorAction SilentlyContinue
if ($process -and $process.Id -ne {}) {{ [Console]::Write("{{0}}|{{1}}" -f $process.Id, $process.ProcessName) }}
"#,
        std::process::id()
    );
    let output = powershell_output(&script).ok()?;
    let (pid, name) = output.trim().split_once('|')?;
    let pid = pid.parse::<i32>().ok()?;
    (pid != std::process::id() as i32).then(|| (pid, name.to_owned()))
}

pub fn activate_app(pid: i32) -> PlatformResult<()> {
    let script = format!(
        "$process = Get-Process -Id {pid} -ErrorAction Stop; $hwnd = $process.MainWindowHandle; if ($hwnd -eq 0) {{ exit 1 }}; $signature = '[DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow); [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr hWnd);'; Add-Type -MemberDefinition $signature -Name Window -Namespace Corvo; [void][Corvo.Window]::ShowWindowAsync($hwnd, 9); if (-not [Corvo.Window]::SetForegroundWindow($hwnd)) {{ exit 1 }}"
    );
    run_powershell(&script)
}

pub fn send_paste_keystroke() -> PlatformResult<()> {
    run_powershell("(New-Object -ComObject WScript.Shell).SendKeys('^v')")
}

pub fn adjust_brightness_with_level(delta: f32) -> PlatformResult<f32> {
    let delta_percent = (delta * 100.0).round() as i32;
    let script = format!(
        "$states = @(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction Stop); $methods = @(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods -ErrorAction Stop); if ($states.Count -eq 0 -or $methods.Count -eq 0) {{ throw 'No brightness control is available for this display' }}; $current = [math]::Round(($states | Measure-Object -Property CurrentBrightness -Average).Average); $target = [byte][math]::Min(100, [math]::Max(0, $current + {delta_percent})); foreach ($method in $methods) {{ $result = Invoke-CimMethod -InputObject $method -MethodName WmiSetBrightness -Arguments @{{ Timeout = [uint32]1; Brightness = $target }} -ErrorAction Stop; if ($result.ReturnValue -ne 0) {{ throw \"Brightness API returned $($result.ReturnValue)\" }} }}; Start-Sleep -Milliseconds 100; $after = @(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction Stop); [Console]::Write([math]::Round(($after | Measure-Object -Property CurrentBrightness -Average).Average))"
    );
    let output = powershell_output(&script)?;
    output.trim().parse::<f32>().map_err(|error| {
        PlatformError::Os(format!(
            "could not read the resulting brightness level: {error}"
        ))
    })
}

const CORE_AUDIO_INTEROP: &str = r#"
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E"), ClassInterface(ClassInterfaceType.None)]
public class CorvoAudioDeviceEnumerator {}
[ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface CorvoIMMDeviceEnumerator {
    void EnumAudioEndpoints(int dataFlow, int stateMask, out object devices);
    void GetDefaultAudioEndpoint(int dataFlow, int role, [MarshalAs(UnmanagedType.Interface)] out CorvoIMMDevice endpoint);
}
[ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface CorvoIMMDevice {
    void Activate(ref Guid iid, int context, IntPtr activationParams, [MarshalAs(UnmanagedType.Interface)] out object endpointVolume);
}
[ComImport, Guid("5CDF2C82-841E-4546-9722-0CF74078229A"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface CorvoIAudioEndpointVolume {
    void RegisterControlChangeNotify(IntPtr notify);
    void UnregisterControlChangeNotify(IntPtr notify);
    void GetChannelCount(out uint channels);
    void SetMasterVolumeLevel(float level, ref Guid eventContext);
    void SetMasterVolumeLevelScalar(float level, ref Guid eventContext);
    void GetMasterVolumeLevel(out float level);
    void GetMasterVolumeLevelScalar(out float level);
}
"#;

pub fn audio_output_level() -> Option<f32> {
    let script = format!(
        "$ErrorActionPreference='Stop'; Add-Type -TypeDefinition @'\n{CORE_AUDIO_INTEROP}\n'@; $enumerator = [CorvoAudioDeviceEnumerator]::new(); $deviceEnumerator = [CorvoIMMDeviceEnumerator]$enumerator; $device = $null; $deviceEnumerator.GetDefaultAudioEndpoint(0, 1, [ref]$device); $iid = [CorvoIAudioEndpointVolume].GUID; $endpointObject = $null; $device.Activate([ref]$iid, 23, [IntPtr]::Zero, [ref]$endpointObject); $endpoint = [CorvoIAudioEndpointVolume]$endpointObject; $level = [single]0; $endpoint.GetMasterVolumeLevelScalar([ref]$level); [Console]::Write([Math]::Round($level * 100))"
    );
    powershell_output(&script).ok()?.trim().parse::<f32>().ok()
}

pub fn adjust_audio_output_with_level(delta: f32) -> PlatformResult<f32> {
    if !delta.is_finite() {
        return Err(PlatformError::Os("volume change must be finite".into()));
    }
    let delta = delta.clamp(-1.0, 1.0);
    let script = format!(
        "$ErrorActionPreference='Stop'; Add-Type -TypeDefinition @'\n{CORE_AUDIO_INTEROP}\n'@; $enumerator = [CorvoAudioDeviceEnumerator]::new(); $deviceEnumerator = [CorvoIMMDeviceEnumerator]$enumerator; $device = $null; $deviceEnumerator.GetDefaultAudioEndpoint(0, 1, [ref]$device); $iid = [CorvoIAudioEndpointVolume].GUID; $endpointObject = $null; $device.Activate([ref]$iid, 23, [IntPtr]::Zero, [ref]$endpointObject); $endpoint = [CorvoIAudioEndpointVolume]$endpointObject; $level = [single]0; $endpoint.GetMasterVolumeLevelScalar([ref]$level); $target = [single][Math]::Min(1.0, [Math]::Max(0.0, $level + ({delta}))); $eventContext = [Guid]::Empty; $endpoint.SetMasterVolumeLevelScalar($target, [ref]$eventContext); $endpoint.GetMasterVolumeLevelScalar([ref]$level); [Console]::Write([Math]::Round($level * 100))"
    );
    let output = powershell_output(&script)?;
    output.trim().parse::<f32>().map_err(|error| {
        PlatformError::Os(format!(
            "could not read the resulting volume level: {error}"
        ))
    })
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

fn cached_icon_for_key(key: &str) -> Option<PathBuf> {
    let cache_dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("Corvo")
        .join("icon-cache");
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let cached = cache_dir.join(format!("{:016x}.png", hasher.finish()));
    cached.is_file().then_some(cached)
}

fn fill_start_app_icons(apps: &mut [AppEntry]) {
    let Some(cache_dir) = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Corvo").join("icon-cache"))
    else {
        return;
    };

    let missing = apps
        .iter()
        .filter_map(|app| {
            let app_id = app
                .path
                .to_string_lossy()
                .strip_prefix("shell:AppsFolder\\")?
                .to_owned();
            (app.icon_png.is_none() && cached_icon_for_key(&app_id.to_lowercase()).is_none())
                .then_some(app_id)
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
    let Ok(json) = serde_json::to_vec(&items) else {
        return;
    };
    let encoded = base64_encode(&json);
    let script = format!(
        r#"
$items = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')) | ConvertFrom-Json
Add-Type -AssemblyName System.Drawing
foreach ($item in $items) {{
    try {{
        $parts = ([string]$item.app_id).Split('!', 2)
        if ($parts.Count -ne 2 -or -not $parts[0] -or -not $parts[1]) {{ continue }}
        $package = Get-AppxPackage -PackageFamilyName $parts[0] -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $package -or -not $package.InstallLocation) {{ continue }}
        $manifestPath = Join-Path $package.InstallLocation 'AppxManifest.xml'
        if (-not (Test-Path -LiteralPath $manifestPath)) {{ continue }}
        [xml]$manifest = Get-Content -LiteralPath $manifestPath -Raw
        $application = $manifest.SelectNodes("//*[local-name()='Application']") | Where-Object {{ $_.GetAttribute('Id') -eq $parts[1] }} | Select-Object -First 1
        if (-not $application) {{ continue }}
        $visual = $application.SelectSingleNode("*[local-name()='VisualElements']")
        if (-not $visual) {{ continue }}
        $logoNames = @('Square150x150Logo', 'Square44x44Logo', 'Square310x310Logo', 'Square71x71Logo', 'Logo')
        $logoPath = $null
        foreach ($name in $logoNames) {{
            $relative = $visual.GetAttribute($name)
            if (-not $relative -or $relative.StartsWith('ms-resource:', [StringComparison]::OrdinalIgnoreCase)) {{ continue }}
            $relative = $relative.Replace('/', '\')
            $candidate = Join-Path $package.InstallLocation $relative
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {{ $logoPath = $candidate; break }}
            $directory = Split-Path -Parent $candidate
            $stem = [IO.Path]::GetFileNameWithoutExtension($candidate)
            if (Test-Path -LiteralPath $directory -PathType Container) {{
                $variant = Get-ChildItem -LiteralPath $directory -File | Where-Object {{ $_.BaseName -like "$stem.*" -and $_.Extension -match '^\.(png|jpe?g|bmp)$' }} | Select-Object -First 1
                if ($variant) {{ $logoPath = $variant.FullName; break }}
            }}
        }}
        if (-not $logoPath) {{ continue }}
        $image = [System.Drawing.Image]::FromFile($logoPath)
        try {{ $image.Save([string]$item.target, [System.Drawing.Imaging.ImageFormat]::Png) }} finally {{ $image.Dispose() }}
    }} catch {{ }}
}}
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct CorvoShellFileInfo {{
    public IntPtr hIcon;
    public int iIcon;
    public uint dwAttributes;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szDisplayName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)] public string szTypeName;
}}
public static class CorvoShellIconNative {{
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern int SHParseDisplayName(string name, IntPtr binding, out IntPtr pidl, uint attributes, out uint parsedAttributes);
    [DllImport("shell32.dll", EntryPoint = "SHGetFileInfoW", CharSet = CharSet.Unicode)]
    public static extern IntPtr SHGetFileInfo(IntPtr pidl, uint attributes, ref CorvoShellFileInfo info, uint size, uint flags);
    [DllImport("ole32.dll")] public static extern void CoTaskMemFree(IntPtr memory);
    [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr handle);
}}
'@
foreach ($item in $items) {{
    if (Test-Path -LiteralPath ([string]$item.target) -PathType Leaf) {{ continue }}
    $pidl = [IntPtr]::Zero
    $info = [CorvoShellFileInfo]::new()
    $bitmap = $null
    try {{
        $parsedAttributes = [uint32]0
        $name = 'shell:AppsFolder\' + [string]$item.app_id
        if ([CorvoShellIconNative]::SHParseDisplayName($name, [IntPtr]::Zero, [ref]$pidl, 0, [ref]$parsedAttributes) -ne 0) {{ continue }}
        $size = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][CorvoShellFileInfo])
        $flags = [uint32](0x00000008 -bor 0x00000100)
        if ([CorvoShellIconNative]::SHGetFileInfo($pidl, 0, [ref]$info, $size, $flags) -eq [IntPtr]::Zero) {{ continue }}
        if ($info.hIcon -eq [IntPtr]::Zero) {{ continue }}
        $bitmap = [System.Drawing.Icon]::FromHandle($info.hIcon).ToBitmap()
        $bitmap.Save([string]$item.target, [System.Drawing.Imaging.ImageFormat]::Png)
    }} catch {{ }} finally {{
        if ($bitmap) {{ $bitmap.Dispose() }}
        if ($info.hIcon -ne [IntPtr]::Zero) {{ [void][CorvoShellIconNative]::DestroyIcon($info.hIcon) }}
        if ($pidl -ne [IntPtr]::Zero) {{ [CorvoShellIconNative]::CoTaskMemFree($pidl) }}
    }}
}}
"#
    );
    if powershell_output(&script).is_err() {
        return;
    }
    for app in apps {
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

fn fill_shortcut_icons(apps: &mut [AppEntry]) {
    let Some(cache_dir) = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Corvo").join("icon-cache"))
    else {
        return;
    };

    let missing = apps
        .iter()
        .filter(|app| {
            app.icon_png.is_none()
                && app
                    .path
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("lnk"))
        })
        .map(|app| app.path.clone())
        .filter(|path| cached_shortcut_icon(path).is_none())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return;
    }
    if std::fs::create_dir_all(&cache_dir).is_err() {
        return;
    }

    let cache_paths = missing
        .iter()
        .map(|path| {
            let mut hasher = DefaultHasher::new();
            path.to_string_lossy().to_lowercase().hash(&mut hasher);
            (
                path.clone(),
                cache_dir.join(format!("{:016x}.png", hasher.finish())),
            )
        })
        .collect::<Vec<_>>();
    let script_items = cache_paths
        .iter()
        .map(|(source, target)| (source.to_string_lossy(), target.to_string_lossy()))
        .map(|(source, target)| serde_json::json!({ "source": source, "target": target }))
        .collect::<Vec<_>>();
    let Ok(json) = serde_json::to_vec(&script_items) else {
        return;
    };
    let encoded = base64_encode(&json);
    let script = format!(
        r#"
$items = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')) | ConvertFrom-Json
$shell = New-Object -ComObject WScript.Shell
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class CorvoIconNative {{
    [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern uint ExtractIconEx(string file, int index, IntPtr[] large, IntPtr[] small, uint count);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool DestroyIcon(IntPtr handle);
}}
'@
foreach ($item in $items) {{
    $handle = [IntPtr]::Zero
    $bitmap = $null
    $large = $null
    $small = $null
    try {{
        $shortcut = $shell.CreateShortcut([string]$item.source)
        $location = [string]$shortcut.IconLocation
        $iconPath = [string]$shortcut.TargetPath
        $iconIndex = 0
        if ($location) {{
            $match = [regex]::Match($location, '^(.*?)(?:,\s*(-?\d+))?$')
            if ($match.Success) {{
                $iconPath = $match.Groups[1].Value.Trim().Trim('"')
                if ($match.Groups[2].Success) {{ $iconIndex = [int]$match.Groups[2].Value }}
            }}
        }}
        $iconPath = [Environment]::ExpandEnvironmentVariables($iconPath)
        if (-not (Test-Path -LiteralPath $iconPath)) {{ $iconPath = [string]$shortcut.TargetPath; $iconIndex = 0 }}
        $iconPath = [Environment]::ExpandEnvironmentVariables($iconPath)
        if (-not $iconPath -or -not (Test-Path -LiteralPath $iconPath)) {{ continue }}
        $large = New-Object IntPtr[] 1
        $small = New-Object IntPtr[] 1
        if ([CorvoIconNative]::ExtractIconEx($iconPath, $iconIndex, $large, $small, 1) -gt 0) {{
            $handle = $large[0]
            if ($handle -eq [IntPtr]::Zero) {{ $handle = $small[0] }}
        }}
        if ($handle -ne [IntPtr]::Zero) {{
            $icon = [System.Drawing.Icon]::FromHandle($handle)
        }} else {{
            $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($iconPath)
        }}
        if (-not $icon) {{ continue }}
        $bitmap = $icon.ToBitmap()
        $bitmap.Save([string]$item.target, [System.Drawing.Imaging.ImageFormat]::Png)
    }} catch {{ }}
    finally {{
        if ($bitmap) {{ $bitmap.Dispose() }}
        if ($large -and $large[0] -ne [IntPtr]::Zero) {{ [void][CorvoIconNative]::DestroyIcon($large[0]) }}
        if ($small -and $small[0] -ne [IntPtr]::Zero) {{ [void][CorvoIconNative]::DestroyIcon($small[0]) }}
    }}
}}
"#
    );
    if powershell_output(&script).is_err() {
        return;
    }
    for app in apps {
        if app.icon_png.is_none() {
            app.icon_png = cached_shortcut_icon(&app.path);
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

# ==============================================================================
# Corvo installation script for Windows (PowerShell)
# ==============================================================================
# Run directly from PowerShell with:
# irm https://raw.githubusercontent.com/diegoleteliers10/corvo/main/instalar.ps1 | iex
# ==============================================================================

$ErrorActionPreference = 'Stop'
$GitHubUser = "diegoleteliers10"
$GitHubRepo = "corvo"
$AppName    = "corvo"
$BinaryName = "corvo.exe"

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

Write-Host "=== Starting $AppName installation on Windows ===" -ForegroundColor Green

# ── 1. Resolve architecture and paths ──────────────────────────────────────────
$Arch = $env:PROCESSOR_ARCHITECTURE
if ($Arch -eq "ARM64") {
    $Target = "aarch64-pc-windows-msvc"
} else {
    $Target = "x86_64-pc-windows-msvc"
}

$ConfigDir = Join-Path $env:APPDATA     "corvo\config"
$DataDir   = Join-Path $env:APPDATA     "corvo\data"
$StateDir  = Join-Path $env:LOCALAPPDATA "corvo\state"
$CacheDir  = Join-Path $env:LOCALAPPDATA "corvo\cache"
$BinDir    = Join-Path $env:LOCALAPPDATA "corvo\bin"

# ── 1.5. Prevent conflict with MSI installer ──────────────────────────────────
$ForceInstall = $env:CORVO_FORCE_INSTALL -eq "1"
$MsiExePath = Join-Path $env:LOCALAPPDATA "Programs\Corvo\corvo.exe"
if ((Test-Path $MsiExePath) -and -not $ForceInstall) {
    Write-Host "NOTICE: Corvo is already installed via the .msi installer ($MsiExePath)." -ForegroundColor Yellow
    Write-Host "        Download and run the latest Corvo_*.msi from GitHub Releases to update instead." -ForegroundColor Yellow
    Write-Host '        Re-run with $env:CORVO_FORCE_INSTALL="1" to install alongside it anyway.' -ForegroundColor Yellow
    exit 1
}

# ── 2. Create directories ──────────────────────────────────────────────────────
foreach ($dir in @($ConfigDir, $DataDir, $StateDir, $CacheDir, $BinDir)) {
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
}

# ── 3. Query GitHub API for latest release ────────────────────────────────────
Write-Host "Fetching latest release version from GitHub API..." -ForegroundColor Cyan
$ApiUrl = "https://api.github.com/repos/$GitHubUser/$GitHubRepo/releases/latest"

try {
    $oldProgressPreference = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'

    $Response = Invoke-RestMethod -Uri $ApiUrl -Method Get
    $LatestTag = $Response.tag_name
} catch {
    Write-Error "ERROR: Could not fetch the latest version from GitHub ($ApiUrl)."
    $ProgressPreference = $oldProgressPreference
    exit 1
}

Write-Host "Latest version found: $LatestTag (Target: $Target)" -ForegroundColor Green

# ── 4. Download Windows archive asset ─────────────────────────────────────────
$ZipName = "$AppName-$Target.zip"
$DownloadUrl = "https://github.com/$GitHubUser/$GitHubRepo/releases/download/$LatestTag/$ZipName"

$TempDir = Join-Path $env:TEMP "install-$AppName"
if (Test-Path $TempDir) {
    Remove-Item -Recurse -Force $TempDir
}
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

$ZipPath = Join-Path $TempDir $ZipName
$ExtractDir = Join-Path $TempDir "extracted"

Write-Host "Downloading $ZipName..." -ForegroundColor Cyan
try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing
} catch {
    Write-Error "ERROR: Failed to download the release asset from $DownloadUrl"
    $ProgressPreference = $oldProgressPreference
    exit 1
}

# ── 5. Extract files ───────────────────────────────────────────────────────────
Write-Host "Extracting files..." -ForegroundColor Cyan
try {
    Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force
} catch {
    Write-Error "ERROR: Failed to extract the ZIP archive."
    $ProgressPreference = $oldProgressPreference
    exit 1
}

$ProgressPreference = $oldProgressPreference

# ── 6. Install executable ─────────────────────────────────────────────────────
$ExtractedExe = Get-ChildItem -Path $ExtractDir -Filter $BinaryName -Recurse | Select-Object -First 1

if (-not $ExtractedExe) {
    Write-Error "ERROR: Could not find $BinaryName inside the downloaded archive."
    exit 1
}

$DestinationExe = Join-Path $BinDir $BinaryName
Write-Host "Installing $BinaryName to $BinDir..." -ForegroundColor Cyan

# Stop any running resident instance before overwriting
Get-Process -Name $AppName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 250

Copy-Item -Path $ExtractedExe.FullName -Destination $DestinationExe -Force

# ── 7. Configure PATH ─────────────────────────────────────────────────────────
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
$PathEntries = $UserPath -split ";" | Where-Object { $_ -ne "" }

if ($PathEntries -notcontains $BinDir) {
    Write-Host "Adding $BinDir to User PATH..." -ForegroundColor Cyan
    $NewPath = if ($UserPath) { "$UserPath;$BinDir" } else { $BinDir }
    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
    $env:Path = "$env:Path;$BinDir"
    Write-Host "PATH updated successfully." -ForegroundColor Green
}

# ── 8. Create Start Menu Shortcut ─────────────────────────────────────────────
try {
    $WshShell = New-Object -ComObject WScript.Shell
    $StartMenuDir = [Environment]::GetFolderPath('Programs')
    $ShortcutPath = Join-Path $StartMenuDir "Corvo.lnk"
    $Shortcut = $WshShell.CreateShortcut($ShortcutPath)
    $Shortcut.TargetPath = $DestinationExe
    $Shortcut.WorkingDirectory = $BinDir
    $Shortcut.Description = "Lightweight native application launcher"
    $Shortcut.Save()
    Write-Host "Start Menu shortcut created: $ShortcutPath" -ForegroundColor Green
} catch {
    Write-Warning "Could not create Start Menu shortcut: $_"
}

# Clean temporary files
Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "=== Corvo $LatestTag installed successfully! ===" -ForegroundColor Green
Write-Host "Launch Corvo by typing 'corvo' in a new terminal or from the Start Menu." -ForegroundColor Green

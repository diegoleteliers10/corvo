//! Auto-update system following the Tinycast pattern.
//!
//! Uses GitHub Releases as feed, stream download with progress,
//! cryptographic verification, staging on the target filesystem volume,
//! and a detached waiter process for atomic swap and relaunch.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEFAULT_GITHUB_REPO: &str = "diegoleteliers10/corvo";
pub const MINISIGN_PUBKEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl UpdateChannel {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "beta" | "prerelease" => Self::Beta,
            _ => Self::Stable,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateCheckState {
    pub last_checked_at: u64,
    pub latest_seen: Option<String>,
    pub dismissed_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRelease {
    pub version: semver::Version,
    pub tag_name: String,
    pub title: String,
    pub release_notes: String,
    pub published_at: String,
    pub asset: ReleaseAsset,
    pub checksum_url: Option<String>,
    pub signature_url: Option<String>,
}

#[derive(Debug)]
pub enum UpdateError {
    Network(String),
    Serialization(String),
    Io(io::Error),
    InvalidVersion(String),
    NoMatchingAsset(String),
    ChecksumMismatch { expected: String, actual: String },
    SignatureVerificationFailed(String),
    Translocated(String),
    PermissionDenied(String),
    Cancelled,
    UnsupportedPlatform(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Serialization(msg) => write!(f, "Serialization error: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::InvalidVersion(msg) => write!(f, "Invalid version: {msg}"),
            Self::NoMatchingAsset(msg) => write!(f, "No matching release asset: {msg}"),
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "Checksum mismatch (expected {expected}, got {actual})")
            }
            Self::SignatureVerificationFailed(msg) => {
                write!(f, "Signature verification failed: {msg}")
            }
            Self::Translocated(msg) => write!(f, "App is translocated: {msg}"),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {msg}"),
            Self::Cancelled => write!(f, "Update cancelled"),
            Self::UnsupportedPlatform(msg) => write!(f, "Unsupported platform: {msg}"),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<io::Error> for UpdateError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Returns the cache directory for Corvo updates: `~/.cache/corvo/updates` (Linux),
/// `~/Library/Caches/corvo/updates` (macOS), `%LOCALAPPDATA%\corvo\cache\updates` (Windows).
pub fn update_cache_dir() -> io::Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "corvo")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No cache directory found"))?;
    let dir = proj.cache_dir().join("updates");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn install_error_path() -> io::Result<PathBuf> {
    Ok(update_cache_dir()?.join("install-error.txt"))
}

pub fn take_install_error() -> Option<String> {
    let path = install_error_path().ok()?;
    let message = fs::read_to_string(&path).ok()?;
    let _ = fs::remove_file(path);
    Some(message.trim().to_string())
}

pub fn cached_update_archive(release: &UpdateRelease) -> Option<PathBuf> {
    let path = update_cache_dir().ok()?.join(&release.asset.name);
    path.is_file().then_some(path)
}

fn state_file_path() -> Option<PathBuf> {
    let proj = ProjectDirs::from("", "", "corvo")?;
    Some(proj.cache_dir().join("update-check.json"))
}

pub fn load_check_state() -> UpdateCheckState {
    let Some(path) = state_file_path() else {
        return UpdateCheckState::default();
    };
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(state) = serde_json::from_slice(&bytes) {
            return state;
        }
    }
    UpdateCheckState::default()
}

pub fn save_check_state(state: &UpdateCheckState) {
    let Some(path) = state_file_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, json);
    }
}

pub fn dismiss_version(version: &str) {
    let mut state = load_check_state();
    state.dismissed_version = Some(version.to_string());
    save_check_state(&state);
    clear_pending_release();
}

fn pending_release_path() -> Option<PathBuf> {
    let proj = ProjectDirs::from("", "", "corvo")?;
    Some(proj.cache_dir().join("pending-update.json"))
}

/// Persists the latest known release so the Settings About tab can show
/// the changelog with Skip / Download buttons without a new network check.
pub fn save_pending_release(release: &UpdateRelease) {
    let Some(path) = pending_release_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(release) {
        let _ = fs::write(path, json);
    }
}

/// Returns the persisted release when it is still newer than this build.
/// Clears stale entries (same or older version) so the UI never prompts
/// for an update that no longer applies.
pub fn load_pending_release() -> Option<UpdateRelease> {
    let path = pending_release_path()?;
    let bytes = fs::read(&path).ok()?;
    let release: UpdateRelease = serde_json::from_slice(&bytes).ok()?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).ok()?;
    if release.version > current {
        Some(release)
    } else {
        let _ = fs::remove_file(&path);
        None
    }
}

pub fn clear_pending_release() {
    if let Some(path) = pending_release_path() {
        let _ = fs::remove_file(path);
    }
}

pub fn is_version_dismissed(version: &str) -> bool {
    let state = load_check_state();
    state.dismissed_version.as_deref() == Some(version)
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parses the current system's target asset from GitHub release assets.
fn match_target_asset(assets: &[ReleaseAsset]) -> Result<ReleaseAsset, UpdateError> {
    #[cfg(target_os = "macos")]
    {
        let is_arm = cfg!(target_arch = "aarch64");
        // Prefer specific architecture archive (.zip or .tar.gz), fall back to universal or general macos
        let exact_triple = if is_arm {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };

        if let Some(asset) = assets.iter().find(|a| {
            (a.name.contains(exact_triple) || a.name.contains("universal-apple-darwin"))
                && (a.name.ends_with(".zip") || a.name.ends_with(".tar.gz"))
        }) {
            return Ok(asset.clone());
        }

        // Fallback for macOS zip
        if let Some(asset) = assets
            .iter()
            .find(|a| a.name.ends_with(".zip") && a.name.contains("darwin"))
        {
            return Ok(asset.clone());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let exact_triple = "x86_64-pc-windows-msvc";
        if let Some(asset) = assets
            .iter()
            .find(|a| a.name.contains(exact_triple) && a.name.ends_with(".zip"))
        {
            return Ok(asset.clone());
        }
        if let Some(asset) = assets
            .iter()
            .find(|a| a.name.ends_with(".zip") && a.name.contains("windows"))
        {
            return Ok(asset.clone());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let is_appimage = std::env::var_os("APPIMAGE").is_some();
        if is_appimage {
            if let Some(asset) = assets.iter().find(|a| a.name.ends_with(".AppImage")) {
                return Ok(asset.clone());
            }
        }
        let exact_triple = "x86_64-unknown-linux-gnu";
        if let Some(asset) = assets
            .iter()
            .find(|a| a.name.contains(exact_triple) && a.name.ends_with(".tar.gz"))
        {
            return Ok(asset.clone());
        }
        if let Some(asset) = assets.iter().find(|a| a.name.ends_with(".AppImage")) {
            return Ok(asset.clone());
        }
    }

    Err(UpdateError::NoMatchingAsset(
        "No matching asset found for target OS/Arch in release assets".to_owned(),
    ))
}

/// Cuts release notes at the `<!-- corvo:install -->` or `<!-- tinycast:install -->` marker.
pub fn clean_release_notes(raw: &str) -> String {
    let mut text = raw;
    if let Some((clean, _)) = text.split_once("<!-- corvo:install -->") {
        text = clean;
    } else if let Some((clean, _)) = text.split_once("<!-- tinycast:install -->") {
        text = clean;
    }
    text.trim().to_string()
}

/// Checks GitHub Releases for a newer version than current `CARGO_PKG_VERSION`.
/// Respects `channel` (Stable vs Beta) and `force` (bypasses 24h freshness & dismissed check).
pub fn check_for_updates(
    channel: UpdateChannel,
    force: bool,
) -> Result<Option<UpdateRelease>, UpdateError> {
    let current_ver_str = env!("CARGO_PKG_VERSION");
    let current_ver = semver::Version::parse(current_ver_str)
        .map_err(|e| UpdateError::InvalidVersion(format!("Cannot parse current version: {e}")))?;

    let now = current_timestamp_secs();
    let mut state = load_check_state();

    // 24-hour freshness check unless forced
    if !force && now.saturating_sub(state.last_checked_at) < 24 * 3600 {
        return Ok(None);
    }

    let url = format!("https://api.github.com/repos/{DEFAULT_GITHUB_REPO}/releases");
    let user_agent = format!("corvo/{current_ver_str}");

    let response = ureq::get(&url)
        .set("User-Agent", &user_agent)
        .set("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|e| UpdateError::Network(format!("GitHub API request failed: {e}")))?;

    let releases: Vec<GitHubRelease> = response.into_json().map_err(|e| {
        UpdateError::Serialization(format!("Failed to parse GitHub releases JSON: {e}"))
    })?;

    // Record check timestamp
    state.last_checked_at = now;

    // Filter releases matching the channel and newer than current
    let mut eligible_releases: Vec<(semver::Version, GitHubRelease)> = Vec::new();
    for rel in releases {
        let tag_cleaned = rel.tag_name.trim_start_matches('v');
        let Ok(ver) = semver::Version::parse(tag_cleaned) else {
            continue;
        };

        // Channel constraint: Stable channel rejects prereleases
        if channel == UpdateChannel::Stable && (rel.prerelease || !ver.pre.is_empty()) {
            continue;
        }

        if ver > current_ver {
            eligible_releases.push((ver, rel));
        }
    }

    // Sort by semver descending
    eligible_releases.sort_by(|a, b| b.0.cmp(&a.0));

    let Some((latest_ver, latest_rel)) = eligible_releases.into_iter().next() else {
        save_check_state(&state);
        clear_pending_release();
        return Ok(None);
    };

    let latest_tag = latest_rel.tag_name.clone();
    state.latest_seen = Some(latest_tag.clone());
    save_check_state(&state);

    // If not forced and user dismissed this version, skip prompt
    if !force && state.dismissed_version.as_deref() == Some(&latest_tag) {
        return Ok(None);
    }

    let target_asset = match_target_asset(&latest_rel.assets)?;

    // Check for companion checksum (.sha256) or signature (.minisig) asset
    let checksum_name = format!("{}.sha256", target_asset.name);
    let checksum_url = latest_rel
        .assets
        .iter()
        .find(|a| a.name == checksum_name || a.name == "SHA256SUMS" || a.name == "checksums.txt")
        .map(|a| a.browser_download_url.clone());

    let signature_name = format!("{}.minisig", target_asset.name);
    let signature_url = latest_rel
        .assets
        .iter()
        .find(|a| a.name == signature_name)
        .map(|a| a.browser_download_url.clone());

    let notes = latest_rel
        .body
        .as_deref()
        .unwrap_or("No release notes provided.");
    let clean_notes = clean_release_notes(notes);

    let release = UpdateRelease {
        version: latest_ver,
        tag_name: latest_tag,
        title: latest_rel
            .name
            .unwrap_or_else(|| format!("Version {}", latest_rel.tag_name)),
        release_notes: clean_notes,
        published_at: latest_rel.published_at.unwrap_or_default(),
        asset: target_asset,
        checksum_url,
        signature_url,
    };
    save_pending_release(&release);
    Ok(Some(release))
}

/// Downloads the release asset into `update_cache_dir()` while streaming SHA-256 computation.
/// Verifies SHA-256 against release checksum asset and Minisign signature if available.
pub fn download_and_verify(
    release: &UpdateRelease,
    cancel_flag: &AtomicBool,
    on_progress: Option<&dyn Fn(u64, u64)>,
) -> Result<PathBuf, UpdateError> {
    let cache_dir = update_cache_dir()?;
    let dest_path = cache_dir.join(&release.asset.name);

    let user_agent = format!("corvo/{}", env!("CARGO_PKG_VERSION"));
    let resp = ureq::get(&release.asset.browser_download_url)
        .set("User-Agent", &user_agent)
        .call()
        .map_err(|e| UpdateError::Network(format!("Failed to download asset: {e}")))?;

    let total_size = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(release.asset.size);

    let mut reader = resp.into_reader();
    let mut file = File::create(&dest_path)?;
    let mut hasher = Sha256::new();

    let mut downloaded = 0u64;
    let mut buffer = [0u8; 16384];

    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&dest_path);
            return Err(UpdateError::Cancelled);
        }

        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])?;
        hasher.update(&buffer[..bytes_read]);
        downloaded += bytes_read as u64;

        if let Some(cb) = on_progress {
            cb(downloaded, total_size);
        }
    }

    file.flush()?;
    drop(file);

    let computed_hash = hex::encode(hasher.finalize());

    // 1. Verify SHA-256 Checksum if companion checksum asset exists
    if let Some(ref checksum_url) = release.checksum_url {
        let cs_resp = ureq::get(checksum_url)
            .set("User-Agent", &user_agent)
            .call()
            .map_err(|e| UpdateError::Network(format!("Failed to download checksum: {e}")))?;

        let mut cs_text = String::new();
        cs_resp.into_reader().read_to_string(&mut cs_text)?;

        let mut expected_hash = None;
        for line in cs_text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            if parts.len() == 1 && parts[0].len() == 64 {
                expected_hash = Some(parts[0].to_lowercase());
                break;
            } else if parts.len() >= 2 {
                let hash = parts[0].to_lowercase();
                let file_name = parts[1].trim_start_matches('*');
                if file_name == release.asset.name {
                    expected_hash = Some(hash);
                    break;
                }
            }
        }

        if let Some(expected) = expected_hash {
            if computed_hash != expected {
                let _ = fs::remove_file(&dest_path);
                return Err(UpdateError::ChecksumMismatch {
                    expected,
                    actual: computed_hash,
                });
            }
        }
    }

    // 2. Verify Minisign signature if companion .minisig exists
    if let Some(ref sig_url) = release.signature_url {
        let sig_resp = ureq::get(sig_url)
            .set("User-Agent", &user_agent)
            .call()
            .map_err(|e| UpdateError::Network(format!("Failed to download signature: {e}")))?;

        let mut sig_str = String::new();
        sig_resp.into_reader().read_to_string(&mut sig_str)?;

        let pubkey = minisign_verify::PublicKey::from_base64(MINISIGN_PUBKEY).map_err(|e| {
            UpdateError::SignatureVerificationFailed(format!("Invalid public key: {e}"))
        })?;

        let sig = minisign_verify::Signature::decode(&sig_str).map_err(|e| {
            UpdateError::SignatureVerificationFailed(format!("Invalid signature format: {e}"))
        })?;

        let file_bytes = fs::read(&dest_path)?;
        if let Err(e) = pubkey.verify(&file_bytes, &sig, false) {
            let _ = fs::remove_file(&dest_path);
            return Err(UpdateError::SignatureVerificationFailed(format!(
                "Minisign signature check failed: {e}"
            )));
        }
    }

    Ok(dest_path)
}

/// Applies the update and restarts Corvo via a detached waiter process.
pub fn install_and_restart(staged_archive: &Path) -> Result<(), UpdateError> {
    if !staged_archive.is_file() {
        return Err(UpdateError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "The downloaded update file is missing",
        )));
    }
    let _ = fs::remove_file(install_error_path()?);
    #[cfg(target_os = "macos")]
    {
        install_macos(staged_archive)
    }

    #[cfg(target_os = "windows")]
    {
        install_windows(staged_archive)
    }

    #[cfg(target_os = "linux")]
    {
        install_linux(staged_archive)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(UpdateError::UnsupportedPlatform(
            "Self-update not implemented for this OS".into(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn install_macos(archive_path: &Path) -> Result<(), UpdateError> {
    // 1. Resolve current bundle path
    let current_exe = std::env::current_exe()?;
    // Usually: /Applications/Corvo.app/Contents/MacOS/corvo
    let mut bundle_dir = current_exe.clone();
    while let Some(parent) = bundle_dir.parent() {
        if bundle_dir.extension().and_then(|e| e.to_str()) == Some("app") {
            break;
        }
        bundle_dir = parent.to_path_buf();
    }

    if bundle_dir.extension().and_then(|e| e.to_str()) != Some("app") {
        return Err(UpdateError::UnsupportedPlatform(
            "Corvo is not running from a macOS .app bundle".into(),
        ));
    }

    let bundle_str = bundle_dir.to_string_lossy();
    if bundle_str.contains("/AppTranslocation/") {
        return Err(UpdateError::Translocated(
            "Corvo is running in macOS App Translocation. Please move Corvo.app to /Applications before updating.".into(),
        ));
    }

    let Some(install_parent) = bundle_dir.parent() else {
        return Err(UpdateError::PermissionDenied(
            "Cannot determine parent directory of Corvo.app".into(),
        ));
    };

    let staging_app = install_parent.join("Corvo.app.staging");
    let _ = fs::remove_dir_all(&staging_app);

    let temp_staging_dir = install_parent.join(".corvo_staging_temp");
    let _ = fs::remove_dir_all(&temp_staging_dir);
    fs::create_dir_all(&temp_staging_dir)?;

    // Extract archive into isolated temporary directory on the same filesystem
    let output = Command::new("ditto")
        .arg("-x")
        .arg("-k")
        .arg(archive_path)
        .arg(&temp_staging_dir)
        .output();

    let ditto_failed = match output {
        Ok(out) => !out.status.success(),
        Err(_) => true,
    };

    if ditto_failed {
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(&temp_staging_dir)
            .status()?;
        if !status.success() {
            let _ = fs::remove_dir_all(&temp_staging_dir);
            return Err(UpdateError::Io(io::Error::other(
                "Failed to extract update bundle",
            )));
        }
    }

    // Move extracted Corvo.app to Corvo.app.staging
    let extracted_app = temp_staging_dir.join("Corvo.app");
    if !extracted_app.exists() {
        let _ = fs::remove_dir_all(&temp_staging_dir);
        return Err(UpdateError::NoMatchingAsset(
            "Corvo.app not found inside update archive".into(),
        ));
    }

    fs::rename(&extracted_app, &staging_app)?;
    let _ = fs::remove_dir_all(&temp_staging_dir);

    // Strip quarantine on staging bundle
    let _ = Command::new("xattr").arg("-cr").arg(&staging_app).status();

    if !staging_app.exists() {
        return Err(UpdateError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "Staged application bundle does not exist",
        )));
    }

    verify_macos_update_identity(&bundle_dir, &staging_app)?;

    let pid = std::process::id();
    let old_bundle = install_parent.join("Corvo.app.old");
    let error_log = install_error_path()?;
    let script = r#"
pid="$1"; target="$2"; staging="$3"; old="$4"; error_log="$5"
fail() { printf '%s\n' "$1" > "$error_log"; exit 1; }
for i in $(seq 1 150); do
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
    sleep 0.1
done
if kill -0 "$pid" 2>/dev/null; then fail "Corvo did not quit before the update."; fi
rm -rf "$old" || { open -n "$target"; fail "Cannot remove the previous macOS backup."; }
mv "$target" "$old" || { open -n "$target"; fail "Cannot move the installed Corvo app."; }
if ! mv "$staging" "$target"; then
    if mv "$old" "$target"; then
        open -n "$target"
        fail "Cannot install the downloaded Corvo app. The previous version was restored."
    fi
    fail "Cannot install or restore the Corvo app."
fi
if ! open -n "$target"; then
    rm -rf "$target"
    if mv "$old" "$target"; then
        open -n "$target"
        fail "Cannot open the updated Corvo app. The previous version was restored."
    fi
    fail "Cannot open or restore the Corvo app."
fi
"#;
    Command::new("sh")
        .arg("-c")
        .arg(script)
        .arg("corvo-update")
        .arg(pid.to_string())
        .arg(&bundle_dir)
        .arg(&staging_app)
        .arg(&old_bundle)
        .arg(&error_log)
        .spawn()?;

    std::process::exit(0);
}

#[cfg(target_os = "macos")]
fn verify_macos_update_identity(current: &Path, staged: &Path) -> Result<(), UpdateError> {
    let status = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(staged)
        .status()?;
    if !status.success() {
        return Err(UpdateError::SignatureVerificationFailed(
            "The updated macOS app has an invalid code signature".into(),
        ));
    }

    let bundle_id = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier"])
        .arg(staged.join("Contents/Info.plist"))
        .output()?;
    if !bundle_id.status.success()
        || String::from_utf8_lossy(&bundle_id.stdout).trim() != "sh.corvo.corvo"
    {
        return Err(UpdateError::SignatureVerificationFailed(
            "The updated macOS app has a different bundle identifier".into(),
        ));
    }

    let staged_requirement = macos_designated_requirement(staged)?;
    let expected_requirement = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/macos-signing-requirement.txt"
    ))
    .trim();
    if macos_has_adhoc_signature(staged)? || staged_requirement != expected_requirement {
        return Err(UpdateError::SignatureVerificationFailed(
            "The updated macOS app has an unexpected signing identity".into(),
        ));
    }
    if !macos_has_adhoc_signature(current)?
        && macos_designated_requirement(current)? != staged_requirement
    {
        return Err(UpdateError::SignatureVerificationFailed(
            "The updated macOS app uses a different signing identity".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_has_adhoc_signature(app: &Path) -> Result<bool, UpdateError> {
    let output = Command::new("codesign")
        .args(["-dv", "--verbose=4"])
        .arg(app)
        .output()?;
    if !output.status.success() {
        return Err(UpdateError::SignatureVerificationFailed(
            "Cannot read the macOS app signing identity".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stderr)
        .lines()
        .any(|line| line.trim() == "Signature=adhoc"))
}

#[cfg(target_os = "macos")]
fn macos_designated_requirement(app: &Path) -> Result<String, UpdateError> {
    let output = Command::new("codesign")
        .args(["-dr", "-"])
        .arg(app)
        .output()?;
    let requirement = String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| line.strip_prefix("designated => ").map(str::to_owned));
    if !output.status.success() || requirement.is_none() {
        return Err(UpdateError::SignatureVerificationFailed(
            "Cannot read the macOS app code requirement".into(),
        ));
    }
    Ok(requirement.unwrap_or_default())
}

#[cfg(target_os = "windows")]
fn install_windows(archive_path: &Path) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let current_exe = std::env::current_exe()?;
    let Some(target_dir) = current_exe.parent() else {
        return Err(UpdateError::PermissionDenied(
            "Cannot determine executable directory".into(),
        ));
    };

    let temp_staging_dir = target_dir.join(".corvo_staging_temp");
    let _ = fs::remove_dir_all(&temp_staging_dir);
    fs::create_dir_all(&temp_staging_dir)?;

    let status = Command::new("tar")
        .arg("-xf")
        .arg(archive_path)
        .arg("-C")
        .arg(&temp_staging_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if status.map(|s| !s.success()).unwrap_or(true) {
        let _ = fs::remove_dir_all(&temp_staging_dir);
        return Err(UpdateError::Io(io::Error::new(
            io::ErrorKind::Other,
            "Failed to extract Windows update archive",
        )));
    }

    let extracted_exe = temp_staging_dir.join("corvo.exe");
    let staging_exe = target_dir.join("corvo.exe.new");
    let old_exe = target_dir.join("corvo.exe.old");

    if !extracted_exe.exists() {
        let _ = fs::remove_dir_all(&temp_staging_dir);
        return Err(UpdateError::NoMatchingAsset(
            "corvo.exe not found in extracted archive".into(),
        ));
    }

    let _ = fs::remove_file(&staging_exe);
    fs::copy(&extracted_exe, &staging_exe)?;
    let _ = fs::remove_dir_all(&temp_staging_dir);

    if !staging_exe.exists() {
        return Err(UpdateError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "Staged executable does not exist",
        )));
    }

    let script_path = target_dir.join(format!(".corvo-update-{}.ps1", std::process::id()));
    fs::write(&script_path, WINDOWS_UPDATE_SCRIPT)?;
    let error_log = install_error_path()?;

    let result = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(&script_path)
        .arg(std::process::id().to_string())
        .arg(&current_exe)
        .arg(&staging_exe)
        .arg(&old_exe)
        .arg(&error_log)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    drop(result);

    std::process::exit(0);
}

#[cfg(target_os = "windows")]
const WINDOWS_UPDATE_SCRIPT: &str = r#"
param(
    [int]$ParentPid,
    [string]$Target,
    [string]$Staged,
    [string]$Old,
    [string]$ErrorLog
)
$ErrorActionPreference = 'Stop'

try {
    $parent = [System.Diagnostics.Process]::GetProcessById($ParentPid)
    $parent.WaitForExit()
    $parent.Dispose()
} catch [System.ArgumentException] {
}

$swapped = $false
for ($attempt = 0; $attempt -lt 100; $attempt++) {
    try {
        if (-not $swapped) {
            if ([System.IO.File]::Exists($Old)) {
                [System.IO.File]::Delete($Old)
            }
            [System.IO.File]::Move($Target, $Old)
            try {
                [System.IO.File]::Move($Staged, $Target)
                $swapped = $true
            } catch {
                $reason = $_.Exception.Message
                try {
                    [System.IO.File]::Move($Old, $Target)
                } catch {
                    [System.IO.File]::WriteAllText($ErrorLog, "Swap failed: $reason. Restore failed: $($_.Exception.Message)")
                    exit 1
                }
                throw
            }
        }

        $newProcess = Start-Process -FilePath $Target -WorkingDirectory ([System.IO.Path]::GetDirectoryName($Target)) -PassThru
        Start-Sleep -Milliseconds 500
        $newProcess.Refresh()
        if ($newProcess.HasExited) { throw 'The updated Corvo exited after relaunch.' }
        Remove-Item -LiteralPath $Old -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
        exit 0
    } catch {
        if ($swapped) { break }
        Start-Sleep -Milliseconds 100
    }
}
if ($swapped -and [System.IO.File]::Exists($Old)) {
    try {
        [System.IO.File]::Delete($Target)
        [System.IO.File]::Move($Old, $Target)
        [System.IO.File]::WriteAllText($ErrorLog, "The updated Corvo did not start. The previous version was restored.")
        Start-Process -FilePath $Target -WorkingDirectory ([System.IO.Path]::GetDirectoryName($Target))
    } catch {
        [System.IO.File]::WriteAllText($ErrorLog, "Cannot relaunch or restore Corvo: $($_.Exception.Message)")
        exit 1
    }
}
if (-not $swapped -and [System.IO.File]::Exists($Target)) {
    try {
        [System.IO.File]::WriteAllText($ErrorLog, "The Corvo update could not replace the executable.")
        Start-Process -FilePath $Target -WorkingDirectory ([System.IO.Path]::GetDirectoryName($Target))
    } catch {
        [System.IO.File]::WriteAllText($ErrorLog, "The update and relaunch failed: $($_.Exception.Message)")
        exit 1
    }
}
"#;

#[cfg(target_os = "linux")]
fn install_linux(archive_path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;

    let target_path = if let Ok(appimage) = std::env::var("APPIMAGE") {
        PathBuf::from(appimage)
    } else {
        std::env::current_exe()?
    };

    let Some(parent) = target_path.parent() else {
        return Err(UpdateError::PermissionDenied(
            "Cannot determine target directory".into(),
        ));
    };

    if target_path.starts_with("/usr/bin")
        || target_path.starts_with("/bin")
        || target_path.starts_with("/snap")
        || target_path.starts_with("/var/lib/flatpak")
    {
        return Err(UpdateError::PermissionDenied(
            "This Linux installation uses a system package. Update Corvo with the package manager."
                .into(),
        ));
    }

    let target_name = target_path.file_name().unwrap().to_string_lossy();
    let staged_file = parent.join(format!("{target_name}.new"));
    let _ = fs::remove_file(&staged_file);

    if archive_path.extension().and_then(|e| e.to_str()) == Some("AppImage") {
        fs::copy(archive_path, &staged_file)?;
    } else {
        let temp_staging_dir = parent.join(".corvo_staging_temp");
        let _ = fs::remove_dir_all(&temp_staging_dir);
        fs::create_dir_all(&temp_staging_dir)?;

        let status = Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(&temp_staging_dir)
            .status()?;
        if !status.success() {
            let _ = fs::remove_dir_all(&temp_staging_dir);
            return Err(UpdateError::Io(io::Error::new(
                io::ErrorKind::Other,
                "Failed to extract Linux update archive",
            )));
        }

        let extracted_bin = temp_staging_dir.join("corvo");
        if !extracted_bin.exists() {
            let _ = fs::remove_dir_all(&temp_staging_dir);
            return Err(UpdateError::NoMatchingAsset(
                "corvo binary not found in update archive".into(),
            ));
        }

        fs::copy(&extracted_bin, &staged_file)?;
        let _ = fs::remove_dir_all(&temp_staging_dir);
    }

    if !staged_file.exists() {
        return Err(UpdateError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "Staged binary does not exist",
        )));
    }

    let mut perms = fs::metadata(&staged_file)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&staged_file, perms)?;

    let old_file = parent.join(format!("{target_name}.old"));
    let error_log = install_error_path()?;
    let script = r#"
pid="$1"; target="$2"; staged="$3"; old="$4"; error_log="$5"
fail() { printf '%s\n' "$1" > "$error_log"; exit 1; }
start_old() { nohup "$target" </dev/null >/dev/null 2>&1 & }
restore() {
    rm -f "$target"
    if mv "$old" "$target"; then
        start_old
        fail "$1 The previous version was restored."
    fi
    fail "$1 The previous version could not be restored."
}
for i in $(seq 1 150); do
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
    sleep 0.1
done
if kill -0 "$pid" 2>/dev/null; then fail "Corvo did not quit before the update."; fi
rm -f "$old" || { start_old; fail "Cannot remove the previous Linux backup."; }
if ! mv "$target" "$old"; then
    start_old
    fail "Cannot move the installed Corvo executable."
fi
if ! mv "$staged" "$target"; then restore "Cannot install the downloaded executable."; fi
if ! chmod +x "$target"; then restore "Cannot set executable permissions."; fi
nohup "$target" </dev/null >/dev/null 2>&1 &
child=$!
sleep 1
if ! kill -0 "$child" 2>/dev/null; then restore "The updated Corvo stopped after relaunch."; fi
rm -f "$old"
"#;

    Command::new("sh")
        .arg("-c")
        .arg(script)
        .arg("corvo-update")
        .arg(std::process::id().to_string())
        .arg(&target_path)
        .arg(&staged_file)
        .arg(&old_file)
        .arg(&error_log)
        .spawn()?;

    std::process::exit(0);
}

/// Cleans up any leftover `.old` executables or staging bundles on startup.
pub fn cleanup_old_installations() {
    #[cfg(target_os = "macos")]
    {
        if let Ok(exe) = std::env::current_exe() {
            let mut bundle_dir = exe;
            while let Some(parent) = bundle_dir.parent() {
                if bundle_dir.extension().and_then(|e| e.to_str()) == Some("app") {
                    break;
                }
                bundle_dir = parent.to_path_buf();
            }
            if let Some(parent) = bundle_dir.parent() {
                let old_bundle = parent.join("Corvo.app.old");
                if old_bundle.exists() {
                    let _ = fs::remove_dir_all(old_bundle);
                }
                let staging_bundle = parent.join("Corvo.app.staging");
                if staging_bundle.exists() {
                    let _ = fs::remove_dir_all(staging_bundle);
                }
            }
        }
    }
}

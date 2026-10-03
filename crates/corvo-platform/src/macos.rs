//! macOS implementation. Session control arrives in phase 4 (SPEC §11);
//! app-launcher inputs and the agent policy are live in phase 1.

use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use objc2::AnyThread;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy,
    NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSGraphicsContext,
    NSImageInterpolation, NSPasteboard, NSPasteboardTypeString, NSRunningApplication,
    NSWindowAnimationBehavior, NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSData, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use super::{AppEntry, PlatformError, PlatformOps, PlatformResult, WindowHandle};

thread_local! {
    static LAUNCHER_PANEL: RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSWindow>>> = const { RefCell::new(None) };
}

// CoreGraphics and CoreFoundation FFI
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceCreate(state_id: i32) -> *mut std::ffi::c_void;
    fn CGEventCreateKeyboardEvent(
        source: *mut std::ffi::c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut std::ffi::c_void;
    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CFEqual(cf1: *const std::ffi::c_void, cf2: *const std::ffi::c_void) -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGEventCreate(source: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
    fn CGGetDisplaysWithPoint(
        point: CGPoint,
        max_displays: u32,
        displays: *mut u32,
        matching_display_count: *mut u32,
    ) -> i32;
    fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut u32,
        display_count: *mut u32,
    ) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGDisplayIsMain(display: u32) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFBooleanTrue: *const std::ffi::c_void;
    pub static kCFBooleanFalse: *const std::ffi::c_void;
}

// ApplicationServices FFI for Accessibility trust check and AXUIElement window manipulation
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> *mut std::ffi::c_void;
    fn AXUIElementCopyAttributeValue(
        element: *mut std::ffi::c_void,
        attribute: *const std::ffi::c_void,
        value: *mut *const std::ffi::c_void,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: *mut std::ffi::c_void,
        attribute: *const std::ffi::c_void,
        value: *const std::ffi::c_void,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(
        element: *mut std::ffi::c_void,
        timeout_in_seconds: f32,
    ) -> i32;
    fn AXValueCreate(value_type: u32, value_ptr: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn AXValueGetValue(
        value: *const std::ffi::c_void,
        value_type: u32,
        value_ptr: *mut std::ffi::c_void,
    ) -> bool;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

const K_AX_VALUE_CGPOINT_TYPE: u32 = 1;
const K_AX_VALUE_CGSIZE_TYPE: u32 = 2;

const K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE: i32 = 0;
const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_VK_ANSI_V: u16 = 0x09;
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x0010_0000;

pub struct MacPlatform;

impl PlatformOps for MacPlatform {
    fn lock(&self) -> PlatformResult<()> {
        let status = std::process::Command::new(
            "/System/Library/CoreServices/Menu Extras/User.menu/Contents/Resources/CGSession",
        )
        .arg("-suspend")
        .status();
        if status.is_ok() {
            Ok(())
        } else {
            let _ = std::process::Command::new("pmset")
                .arg("displaysleepnow")
                .spawn();
            Ok(())
        }
    }

    fn sleep(&self) -> PlatformResult<()> {
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to sleep")
            .spawn();
        Ok(())
    }

    fn shutdown(&self) -> PlatformResult<()> {
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to shut down")
            .spawn();
        Ok(())
    }

    fn list_windows(&self) -> PlatformResult<Vec<WindowHandle>> {
        Err(unimplemented_os("list_windows"))
    }

    fn focus_window(&self, _handle: &WindowHandle) -> PlatformResult<()> {
        Err(unimplemented_os("focus_window"))
    }

    fn list_apps(&self) -> PlatformResult<Vec<AppEntry>> {
        let default_scopes = vec![
            "/Applications".into(),
            "/Applications/Utilities".into(),
            "/System/Applications".into(),
            "/System/Applications/Utilities".into(),
            "/System/Library/CoreServices/Applications".into(),
            "/System/Volumes/Preboot/Cryptexes/App/System/Applications".into(),
            "/System/Library/CoreServices/Finder.app".into(),
            "~/Applications".into(),
        ];
        list_apps_in_scopes(&default_scopes)
    }

    fn open_path(&self, path: &Path) -> PlatformResult<()> {
        let ns_path = NSString::from_str(&path.to_string_lossy());
        let url = NSURL::fileURLWithPath_isDirectory(&ns_path, true);
        if NSWorkspace::sharedWorkspace().openURL(&url) {
            Ok(())
        } else {
            Err(PlatformError::Os(format!("cannot open {}", path.display())))
        }
    }

    fn copy_text(&self, text: &str) -> PlatformResult<()> {
        copy_to_pasteboard(text);
        Ok(())
    }
}

/// In-process clipboard copy using NSPasteboard.
pub fn copy_to_pasteboard(text: &str) {
    write_string_to_pasteboard(&NSPasteboard::generalPasteboard(), text);
}

/// Writes `text` to the given pasteboard, replacing its contents.
///
/// Tests drive this with a uniquely named pasteboard so the suite never
/// clobbers what the user actually copied.
pub fn write_string_to_pasteboard(pboard: &NSPasteboard, text: &str) {
    pboard.clearContents();
    let ns_string = NSString::from_str(text);
    unsafe {
        pboard.setString_forType(&ns_string, NSPasteboardTypeString);
    }
}

/// Reads the current string from NSPasteboard.
pub fn read_clipboard_text() -> Option<String> {
    let pboard = NSPasteboard::generalPasteboard();
    unsafe {
        pboard
            .stringForType(NSPasteboardTypeString)
            .map(|s| s.to_string())
    }
}

/// Reads image bytes (PNG) from the system pasteboard.
/// Fast path: reads native PNG directly.
/// Fallback: transcodes TIFF to PNG via NSBitmapImageRep.
pub fn read_clipboard_image() -> Option<Vec<u8>> {
    let pboard = NSPasteboard::generalPasteboard();
    let png_type = NSString::from_str("public.png");
    let tiff_type = NSString::from_str("public.tiff");

    unsafe {
        if let Some(data) = pboard.dataForType(&png_type) {
            return Some(data.to_vec());
        }

        if let Some(tiff_data) = pboard.dataForType(&tiff_type) {
            if let Some(rep) = NSBitmapImageRep::imageRepWithData(&tiff_data) {
                let empty_props = NSDictionary::new();
                if let Some(png_data) =
                    rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_props)
                {
                    return Some(png_data.to_vec());
                }
            }
        }
    }

    None
}

/// Writes PNG bytes to NSPasteboard with dual PNG + TIFF types.
pub fn copy_image_to_pasteboard(png_bytes: &[u8]) -> PlatformResult<()> {
    let pboard = NSPasteboard::generalPasteboard();
    pboard.clearContents();

    let ns_data = NSData::with_bytes(png_bytes);
    let png_type = NSString::from_str("public.png");
    let tiff_type = NSString::from_str("public.tiff");

    if !pboard.setData_forType(Some(&ns_data), &png_type) {
        return Err(PlatformError::Os(
            "Could not write image to the clipboard".into(),
        ));
    }

    if let Some(rep) = NSBitmapImageRep::imageRepWithData(&ns_data) {
        if let Some(tiff_data) = rep.TIFFRepresentation() {
            pboard.setData_forType(Some(&tiff_data), &tiff_type);
        }
    }
    Ok(())
}

/// Extracts (width, height) from a PNG byte slice without decoding pixels.
#[inline]
pub fn parse_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 {
        return None;
    }
    if bytes[0..8] != PNG_SIGNATURE || bytes[12..16] != *b"IHDR" {
        return None;
    }

    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);

    if width == 0 || height == 0 {
        None
    } else {
        Some((width, height))
    }
}

/// Returns the current change count of the system pasteboard.
pub fn clipboard_change_count() -> isize {
    NSPasteboard::generalPasteboard().changeCount()
}

/// Checks whether the system pasteboard contains concealed or auto-generated
/// types (e.g. 1Password, Bitwarden, KeePassXC, Apple Keychain) to prevent
/// persisting secrets in cleartext (SPEC §8).
pub fn clipboard_is_concealed() -> bool {
    let pboard = NSPasteboard::generalPasteboard();
    let Some(types) = pboard.types() else {
        return false;
    };
    for i in 0..types.count() {
        let item = types.objectAtIndex(i);
        let s = item.to_string();
        if s == "org.nspasteboard.ConcealedType"
            || s == "org.nspasteboard.AutoGeneratedType"
            || s == "com.agilebits.onepassword"
        {
            return true;
        }
    }
    false
}

/// Lists installed applications within custom search scopes.
pub fn list_apps_in_scopes(scopes: &[String]) -> PlatformResult<Vec<AppEntry>> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut seen = std::collections::HashSet::new();
    let mut apps = Vec::new();

    for scope in scopes {
        let path = if let Some(stripped) = scope.strip_prefix("~/") {
            if let Some(ref h) = home {
                h.join(stripped)
            } else {
                PathBuf::from(scope)
            }
        } else {
            PathBuf::from(scope)
        };

        if !path.exists() {
            continue;
        }

        // Check if scope itself is an app bundle (like Finder.app)
        if let Some(name) = bundle_app_name(&path) {
            if seen.insert(name.clone()) {
                let icon_png = extract_app_icon(&path);
                apps.push(AppEntry {
                    name,
                    path,
                    icon_png,
                });
            }
            continue;
        }

        // If it's a directory, scan for .app bundles (1 level deep or direct)
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if let Some(name) = bundle_app_name(&entry_path) {
                if seen.insert(name.clone()) {
                    let icon_png = extract_app_icon(&entry_path);
                    apps.push(AppEntry {
                        name,
                        path: entry_path,
                        icon_png,
                    });
                }
            } else if entry_path.is_dir() {
                let file_name = entry_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if !file_name.starts_with('.') && !file_name.ends_with(".app") {
                    if let Ok(sub_entries) = std::fs::read_dir(&entry_path) {
                        for sub in sub_entries.flatten() {
                            let sub_path = sub.path();
                            if let Some(name) = bundle_app_name(&sub_path) {
                                if seen.insert(name.clone()) {
                                    let icon_png = extract_app_icon(&sub_path);
                                    apps.push(AppEntry {
                                        name,
                                        path: sub_path,
                                        icon_png,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    apps.sort_by_key(|app| app.name.to_lowercase());
    Ok(apps)
}

/// The display name of an `.app` bundle. The bundle directory name is a
/// stable stand-in for `CFBundleName`; localization lands with phase 3
/// config work if it is ever missed.
fn bundle_app_name(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let stem = file_name.strip_suffix(".app")?;
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

/// Extracts a high-definition 256x256 PNG icon directly via AppKit.
/// This queries LaunchServices via NSWorkspace, which has direct access to
/// the system master icon catalog (including system apps where .icns is only a 16px stub).
fn extract_app_icon_appkit(bundle: &Path, png_path: &Path) -> Option<()> {
    let ns_path = NSString::from_str(bundle.to_str()?);
    let ws = NSWorkspace::sharedWorkspace();
    let image = ws.iconForFile(&ns_path);
    image.setSize(NSSize::new(256.0, 256.0));

    unsafe {
        let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            256,
            256,
            8,
            4,
            true,
            false,
            objc2_app_kit::NSCalibratedRGBColorSpace,
            0,
            0,
        )?;

        NSGraphicsContext::saveGraphicsState_class();
        let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        NSGraphicsContext::setCurrentContext(Some(&ctx));
        ctx.setImageInterpolation(NSImageInterpolation::High);

        let target_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(256.0, 256.0));
        image.drawInRect_fromRect_operation_fraction(
            target_rect,
            target_rect,
            NSCompositingOperation::Copy,
            1.0,
        );

        NSGraphicsContext::restoreGraphicsState_class();

        let empty_props = NSDictionary::new();
        let png_data =
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_props)?;

        std::fs::write(png_path, png_data.to_vec()).ok()?;
    }

    Some(())
}

/// Renders a bundle icon to a validated, atomically written 256x256 PNG.
/// Read app bundle assets before querying LaunchServices for newly installed apps.
pub(crate) fn extract_app_icon(bundle: &Path) -> Option<PathBuf> {
    let stem = bundle_app_name(bundle)?;
    let cache_dir = std::env::temp_dir().join("corvo-icons-v4");
    let png = cache_dir.join(format!("{}.png", app_icon_cache_key(bundle, &stem)));
    if is_valid_png(&png) {
        return Some(png);
    }
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::create_dir_all(&cache_dir);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary_png = cache_dir.join(format!(
        "{}.{}.{}.tmp.png",
        app_icon_cache_key(bundle, &stem),
        std::process::id(),
        nonce
    ));

    let is_system_app = bundle.starts_with("/System/");
    if is_system_app && extract_app_icon_appkit(bundle, &temporary_png).is_some() {
        if let Some(path) = commit_cached_icon(&temporary_png, &png) {
            return Some(path);
        }
    }

    // Read the bundle icon first. LaunchServices can return a generic icon
    // while it registers a newly installed application.
    let resources = bundle.join("Contents/Resources");
    if let Some(icns) = icns_in(bundle, &resources, &stem) {
        let res = std::process::Command::new("/usr/bin/sips")
            .args(["-Z", "256", "-s", "format", "png"])
            .arg(&icns)
            .arg("--out")
            .arg(&temporary_png)
            .output();
        if res.is_ok_and(|output| output.status.success()) {
            if let Some(path) = commit_cached_icon(&temporary_png, &png) {
                return Some(path);
            }
        }
    }

    if extract_app_icon_appkit(bundle, &temporary_png).is_some() {
        return commit_cached_icon(&temporary_png, &png);
    }
    let _ = std::fs::remove_file(temporary_png);
    None
}

fn app_icon_cache_key(bundle: &Path, stem: &str) -> String {
    let resources = bundle.join("Contents/Resources");
    let icns = icns_in(bundle, &resources, stem);
    let mut hasher = DefaultHasher::new();
    bundle.hash(&mut hasher);
    for path in [
        Some(bundle.join("Contents/Info.plist")),
        Some(resources.join("Assets.car")),
        icns,
    ]
    .into_iter()
    .flatten()
    {
        path.hash(&mut hasher);
        if let Ok(metadata) = std::fs::metadata(&path) {
            metadata.len().hash(&mut hasher);
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

fn is_valid_png(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return false;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 0 || height == 0 {
        return false;
    }

    // The PNG header alone accepts truncated files. Decode pixels before the
    // launcher uses a cached icon so damaged cache entries get regenerated.
    let data = NSData::with_bytes(&bytes);
    NSBitmapImageRep::imageRepWithData(&data)
        .is_some_and(|rep| rep.pixelsWide() > 0 && rep.pixelsHigh() > 0)
}

fn commit_cached_icon(temporary_png: &Path, png: &Path) -> Option<PathBuf> {
    if !is_valid_png(temporary_png) {
        let _ = std::fs::remove_file(temporary_png);
        return None;
    }
    if std::fs::rename(temporary_png, png).is_err() {
        let _ = std::fs::remove_file(temporary_png);
    }
    is_valid_png(png).then(|| png.to_path_buf())
}

/// Picks the bundle's highest-resolution icon file: CFBundleIconFile in Info.plist first,
/// then conventional names, and finally the largest .icns in Resources.
fn icns_in(bundle: &Path, resources: &Path, stem: &str) -> Option<PathBuf> {
    let plist_path = bundle.join("Contents/Info.plist");
    if let Ok(plist_content) = std::fs::read_to_string(&plist_path) {
        if let Some(pos) = plist_content.find("<key>CFBundleIconFile</key>") {
            let rest = &plist_content[pos + 27..];
            if let Some(start_str) = rest.find("<string>") {
                let val_start = start_str + 8;
                if let Some(end_str) = rest[val_start..].find("</string>") {
                    let icon_name = rest[val_start..val_start + end_str].trim();
                    let with_ext = if icon_name.ends_with(".icns") {
                        icon_name.to_string()
                    } else {
                        format!("{icon_name}.icns")
                    };
                    let candidate = resources.join(&with_ext);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
    }

    for candidate in [
        "AppIcon.icns",
        "AppIconUpdated.icns",
        "appicon.icns",
        "icon.icns",
        &format!("{stem}.icns"),
    ] {
        let path = resources.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }

    let mut icns: Vec<PathBuf> = std::fs::read_dir(resources)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "icns"))
        .collect();
    icns.sort_by_key(|p| std::cmp::Reverse(p.metadata().map(|m| m.len()).unwrap_or(0)));
    icns.into_iter().next()
}

/// The pid of the currently frontmost app, unless it is this process.
/// The launcher reads it before opening its panel so it can hand
/// activation back right after.
pub fn frontmost_app_pid() -> Option<i32> {
    let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = frontmost.processIdentifier();
    (pid != std::process::id() as i32).then_some(pid)
}

/// The pid and display name of the currently frontmost app, unless it is this process.
pub fn frontmost_app_info() -> Option<(i32, String)> {
    let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = frontmost.processIdentifier();
    if pid == std::process::id() as i32 {
        return None;
    }
    let name = frontmost
        .localizedName()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Active App".into());
    Some((pid, name))
}

/// Returns the CGDirectDisplayID of the display where user interaction is focused
/// (the display containing the mouse cursor, or primary display as fallback).
pub fn active_display_id() -> Option<u32> {
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if !event.is_null() {
            let point = CGEventGetLocation(event);
            CFRelease(event);
            let mut display_id = 0u32;
            let mut count = 0u32;
            if CGGetDisplaysWithPoint(point, 1, &mut display_id, &mut count) == 0 && count > 0 {
                return Some(display_id);
            }
        }
        Some(CGMainDisplayID())
    }
}

/// Hands activation to the previously focused app by pid.
#[allow(deprecated)]
pub fn activate_app(pid: i32) {
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return;
    };
    let options = NSApplicationActivationOptions::ActivateIgnoringOtherApps
        | NSApplicationActivationOptions::ActivateAllWindows;
    let _ = app.activateWithOptions(options);
}

/// The process runs as an agent: no Dock icon, no Cmd+Tab entry. GPUI
/// sets the regular policy while launching, so this runs after the
/// platform is up. Raycast does the same through `LSUIElement`.
pub fn run_as_agent() {
    let Some(marker) = MainThreadMarker::new() else {
        eprintln!("corvo: agent policy skipped, not on the main thread");
        return;
    };
    let app = NSApplication::sharedApplication(marker);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

fn hide_traffic_lights(panel: &objc2_app_kit::NSWindow) {
    use objc2_app_kit::NSWindowButton;
    for btn in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = panel.standardWindowButton(btn) {
            button.setHidden(true);
            button.setAlphaValue(0.0);
            button.setFrame(objc2_foundation::NSRect {
                origin: objc2_foundation::NSPoint {
                    x: -1000.0,
                    y: -1000.0,
                },
                size: objc2_foundation::NSSize {
                    width: 0.0,
                    height: 0.0,
                },
            });
        }
    }
}

/// Makes the launcher panel appear with no fade. GPUI marks `PopUp`
/// panels as utility windows, which AppKit fades; this runs on the
/// `Floating` panel instead, sets the popup level, all-spaces behavior,
/// and turns off window animation.
pub fn make_panel_instant(width: f64, height: f64) {
    use objc2_app_kit::{NSColor, NSWindowCollectionBehavior, NSWindowTitleVisibility};
    let Some(panel) = find_new_panel(width, height) else {
        crate::diagnostics::record_error("window", "launcher_panel_missing");
        eprintln!("corvo: launcher panel not found before show");
        return;
    };
    LAUNCHER_PANEL.with(|current| *current.borrow_mut() = Some(panel.clone()));
    panel.setTitle(&objc2_foundation::NSString::from_str(
        "corvo_launcher_panel",
    ));
    panel.setLevel(101); // NSPopUpWindowLevel
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    panel.setTitlebarAppearsTransparent(true);
    hide_traffic_lights(&panel);
}

/// Removes the AppKit shadow from the temporary toast panel.
pub fn remove_action_toast_shadow(width: f64, height: f64) {
    use objc2_app_kit::NSWindowCollectionBehavior;

    let Some(panel) = find_new_panel(width, height) else {
        return;
    };
    panel.setTitle(&objc2_foundation::NSString::from_str("corvo_action_toast"));
    panel.setHasShadow(false);
    panel.invalidateShadow();
    panel.setLevel(101);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    panel.orderFrontRegardless();
}

/// Queries whether the process is trusted for macOS Accessibility.
/// If `prompt` is true, displays the system permission dialog when untrusted.
pub fn is_accessibility_trusted(prompt: bool) -> bool {
    if !prompt {
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        return unsafe { AXIsProcessTrusted() };
    }
    use objc2::runtime::ProtocolObject;
    let key = NSString::from_str("AXTrustedCheckOptionPrompt");
    let val = NSNumber::numberWithBool(prompt);
    unsafe {
        let dict = NSDictionary::<NSString, NSNumber>::dictionaryWithObject_forKey(
            &*val,
            ProtocolObject::from_ref(&*key),
        );
        AXIsProcessTrustedWithOptions(&*dict as *const _ as *const std::ffi::c_void)
    }
}

/// Queries whether the process is authorized to access macOS Calendars with full access.
pub fn is_calendar_access_granted() -> bool {
    calendar_authorization_status() == 3
}

/// Returns the raw EventKit authorization status for events:
/// 0 = NotDetermined, 1 = Restricted, 2 = Denied, 3 = FullAccess / Authorized, 4 = WriteOnly.
pub fn calendar_authorization_status() -> isize {
    unsafe {
        let handle = libc::dlopen(
            c"/System/Library/Frameworks/EventKit.framework/EventKit".as_ptr(),
            libc::RTLD_NOW,
        );
        if handle.is_null() {
            return 0;
        }

        extern "C" {
            fn objc_getClass(name: *const libc::c_char) -> *const std::ffi::c_void;
            fn sel_registerName(name: *const libc::c_char) -> *const std::ffi::c_void;
        }

        let cls = objc_getClass(c"EKEventStore".as_ptr());
        let sel = sel_registerName(c"authorizationStatusForEntityType:".as_ptr());
        if cls.is_null() || sel.is_null() {
            return 0;
        }

        type MsgSendFn =
            unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void, isize) -> isize;
        let msg_send: MsgSendFn =
            std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
        msg_send(cls, sel, 0) // 0 = EKEntityTypeEvent
    }
}

/// Requests full access to macOS Calendars using EventKit.
/// On macOS 14+ (Sonoma, Sequoia), calls `requestFullAccessToEventsWithCompletion:`.
/// On earlier macOS versions, calls `requestAccessToEntityType:completion:`.
pub fn request_calendar_access() {
    unsafe {
        let handle = libc::dlopen(
            c"/System/Library/Frameworks/EventKit.framework/EventKit".as_ptr(),
            libc::RTLD_NOW,
        );
        if handle.is_null() {
            return;
        }

        extern "C" {
            fn objc_getClass(name: *const libc::c_char) -> *const std::ffi::c_void;
            fn sel_registerName(name: *const libc::c_char) -> *const std::ffi::c_void;
        }

        let cls = objc_getClass(c"EKEventStore".as_ptr());
        if cls.is_null() {
            return;
        }

        let sel_alloc = sel_registerName(c"alloc".as_ptr());
        let sel_init = sel_registerName(c"init".as_ptr());
        let sel_responds = sel_registerName(c"respondsToSelector:".as_ptr());
        let sel_full = sel_registerName(c"requestFullAccessToEventsWithCompletion:".as_ptr());
        let sel_legacy = sel_registerName(c"requestAccessToEntityType:completion:".as_ptr());

        type MsgSend = unsafe extern "C" fn(
            *const std::ffi::c_void,
            *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
        type MsgSendBool = unsafe extern "C" fn(
            *const std::ffi::c_void,
            *const std::ffi::c_void,
            *const std::ffi::c_void,
        ) -> bool;

        let msg_send: MsgSend =
            std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
        let msg_send_bool: MsgSendBool =
            std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));

        let store_alloc = msg_send(cls, sel_alloc);
        if store_alloc.is_null() {
            return;
        }
        let store = msg_send(store_alloc, sel_init);
        if store.is_null() {
            return;
        }

        #[repr(C)]
        struct BlockDescriptor {
            reserved: libc::c_ulong,
            size: libc::c_ulong,
        }

        static BLOCK_DESCRIPTOR: BlockDescriptor = BlockDescriptor {
            reserved: 0,
            size: std::mem::size_of::<BlockLiteral>() as libc::c_ulong,
        };

        #[repr(C)]
        struct BlockLiteral {
            isa: *const std::ffi::c_void,
            flags: libc::c_int,
            reserved: libc::c_int,
            invoke: unsafe extern "C" fn(*mut BlockLiteral, bool, *mut std::ffi::c_void),
            descriptor: *const BlockDescriptor,
        }

        unsafe impl Send for BlockLiteral {}
        unsafe impl Sync for BlockLiteral {}

        unsafe extern "C" fn dummy_completion(
            _block: *mut BlockLiteral,
            _granted: bool,
            _err: *mut std::ffi::c_void,
        ) {
        }

        extern "C" {
            static _NSConcreteGlobalBlock: [u8; 0];
        }

        static BLOCK: std::sync::OnceLock<BlockLiteral> = std::sync::OnceLock::new();
        let block = BLOCK.get_or_init(|| BlockLiteral {
            isa: _NSConcreteGlobalBlock.as_ptr() as *const std::ffi::c_void,
            flags: 1 << 29, // BLOCK_IS_GLOBAL
            reserved: 0,
            invoke: dummy_completion,
            descriptor: &BLOCK_DESCRIPTOR,
        });
        let block_ptr: *const std::ffi::c_void = block as *const _ as *const std::ffi::c_void;

        let has_full = msg_send_bool(store, sel_responds, sel_full as *const _);
        if has_full {
            type MsgSendFull = unsafe extern "C" fn(
                *const std::ffi::c_void,
                *const std::ffi::c_void,
                *const std::ffi::c_void,
            );
            let msg_send_full: MsgSendFull =
                std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
            msg_send_full(store, sel_full, block_ptr);
        } else {
            type MsgSendLegacy = unsafe extern "C" fn(
                *const std::ffi::c_void,
                *const std::ffi::c_void,
                isize,
                *const std::ffi::c_void,
            );
            let msg_send_legacy: MsgSendLegacy =
                std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
            msg_send_legacy(store, sel_legacy, 0, block_ptr);
        }
    }
}

/// Queries whether the process is authorized for screen capture/recording without prompting.
pub fn is_screen_recording_granted() -> bool {
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Triggers the system screen recording permission dialog if untrusted.
pub fn request_screen_recording() -> bool {
    extern "C" {
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    unsafe { CGRequestScreenCaptureAccess() }
}

/// Synthesizes and posts a Cmd+V keystroke into the session event tap.
pub fn post_paste_keystroke() -> PlatformResult<()> {
    unsafe {
        let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE);
        if source.is_null() {
            return Err(PlatformError::Os("failed to create CGEventSource".into()));
        }

        let key_down = CGEventCreateKeyboardEvent(source, K_VK_ANSI_V, true);
        let key_up = CGEventCreateKeyboardEvent(source, K_VK_ANSI_V, false);

        if key_down.is_null() || key_up.is_null() {
            if !key_down.is_null() {
                CFRelease(key_down);
            }
            if !key_up.is_null() {
                CFRelease(key_up);
            }
            CFRelease(source);
            return Err(PlatformError::Os("failed to create keyboard event".into()));
        }

        CGEventSetFlags(key_down, K_CG_EVENT_FLAG_MASK_COMMAND);
        CGEventSetFlags(key_up, K_CG_EVENT_FLAG_MASK_COMMAND);

        CGEventPost(K_CG_SESSION_EVENT_TAP, key_down);
        CGEventPost(K_CG_SESSION_EVENT_TAP, key_up);

        CFRelease(key_down);
        CFRelease(key_up);
        CFRelease(source);
    }
    Ok(())
}

/// Automatically copies text to the pasteboard, reactivates the previous app,
/// yields for focus transfer, and posts synthetic Cmd+V.
pub async fn auto_paste(target_pid: i32, text: &str) -> PlatformResult<()> {
    copy_to_pasteboard(text);
    activate_app(target_pid);

    // Yield to allow macOS WindowServer to switch focus
    smol::Timer::after(std::time::Duration::from_millis(50)).await;

    if !is_accessibility_trusted(true) {
        return Err(PlatformError::Os(
            "Accessibility permission required for auto-paste. Please enable Corvo in System Settings -> Privacy & Security -> Accessibility.".into(),
        ));
    }

    post_paste_keystroke()
}

/// Automatically copies image bytes to the pasteboard, reactivates the previous app,
/// yields for focus transfer, and posts synthetic Cmd+V.
pub async fn auto_paste_image(target_pid: i32, png_bytes: &[u8]) -> PlatformResult<()> {
    copy_image_to_pasteboard(png_bytes)?;
    activate_app(target_pid);

    // Yield to allow macOS WindowServer to switch focus
    smol::Timer::after(std::time::Duration::from_millis(50)).await;

    if !is_accessibility_trusted(true) {
        return Err(PlatformError::Os(
            "Accessibility permission required for auto-paste. Please enable Corvo in System Settings -> Privacy & Security -> Accessibility.".into(),
        ));
    }

    post_paste_keystroke()
}

/// Orders the launcher panel front and makes it key, synchronously.
pub fn order_panel_front(_width: f64, _height: f64) {
    use objc2::runtime::AnyObject;
    let Some(panel) = find_launcher_panel() else {
        return;
    };
    panel.orderFront(None::<&AnyObject>);
    panel.makeKeyWindow();
}

/// Finds the launcher floating panel.
pub fn find_launcher_panel() -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    MainThreadMarker::new()?;
    LAUNCHER_PANEL.with(|current| current.borrow().clone())
}

pub fn forget_launcher_panel() {
    LAUNCHER_PANEL.with(|current| *current.borrow_mut() = None);
}

/// Find the new GPUI panel before it gets its launcher title.
fn find_new_panel(width: f64, height: f64) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    let marker = MainThreadMarker::new()?;
    let windows = NSApplication::sharedApplication(marker).windows();
    (0..windows.len())
        .map(|index| windows.objectAtIndex(index))
        .find(|window| {
            let content = window.contentRectForFrameRect(window.frame()).size;
            window.class().name().to_string_lossy() == "GPUIPanel"
                && window.title().to_string() != "corvo_launcher_panel"
                && window.title().to_string() != "corvo_action_toast"
                && (content.width - width).abs() <= 1.5
                && (content.height - height).abs() <= 1.5
        })
}

/// Resizes the launcher panel to the target dimensions while keeping its top edge anchored.
///
/// All Cocoa calls are wrapped in a CATransaction with `disableActions: YES` so Core Animation
/// never interpolates between the old and the new frame. The `display` flag is `false` so
/// AppKit does not composite the stale Metal texture during the resize; GPUI will present its
/// own frame once the render pass completes.
pub fn resize_launcher_panel(target_width: f64, target_height: f64) -> bool {
    let Some(panel) = find_launcher_panel() else {
        return false;
    };
    hide_traffic_lights(&panel);
    let current_frame = panel.frame();
    let current_content = panel.contentRectForFrameRect(current_frame).size;
    if (current_content.height - target_height).abs() < 1.0
        && (current_content.width - target_width).abs() < 1.0
    {
        return true;
    }
    // Anchor top edge: top_y in Cocoa coordinates is origin.y + frame.size.height.
    // The top edge is preserved so the search bar stays pixel-stable while the window grows downwards.
    let top_y = current_frame.origin.y + current_frame.size.height;
    let target_content = objc2_foundation::NSRect {
        origin: current_frame.origin,
        size: objc2_foundation::NSSize {
            width: target_width,
            height: target_height,
        },
    };
    let target_frame_size = panel.frameRectForContentRect(target_content).size;
    let new_origin = objc2_foundation::NSPoint {
        x: current_frame.origin.x,
        y: top_y - target_frame_size.height,
    };
    let new_frame = objc2_foundation::NSRect {
        origin: new_origin,
        size: target_frame_size,
    };

    // Disable implicit Core Animation animations for this resize so the compositor
    // never stretches the previous texture while transitioning to the new size.
    unsafe {
        extern "C" {
            fn objc_getClass(name: *const libc::c_char) -> *const std::ffi::c_void;
            fn sel_registerName(name: *const libc::c_char) -> *const std::ffi::c_void;
        }
        type MsgSend0 = unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void);
        type MsgSendBool =
            unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void, bool);
        let msg_send0: MsgSend0 =
            std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
        let msg_send_bool: MsgSendBool =
            std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
        let cls = objc_getClass(c"CATransaction".as_ptr());
        let sel_begin = sel_registerName(c"begin".as_ptr());
        let sel_disable = sel_registerName(c"setDisableActions:".as_ptr());
        let sel_commit = sel_registerName(c"commit".as_ptr());
        msg_send0(cls, sel_begin);
        msg_send_bool(cls, sel_disable, true);
        panel.setFrame_display(new_frame, false);
        msg_send0(cls, sel_commit);
    }

    let actual_content = panel.contentRectForFrameRect(panel.frame()).size;
    if (actual_content.height - target_height).abs() >= 1.0
        || (actual_content.width - target_width).abs() >= 1.0
    {
        unsafe {
            extern "C" {
                fn objc_getClass(name: *const libc::c_char) -> *const std::ffi::c_void;
                fn sel_registerName(name: *const libc::c_char) -> *const std::ffi::c_void;
            }
            type MsgSend0 = unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void);
            type MsgSendBool =
                unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void, bool);
            let msg_send0: MsgSend0 =
                std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
            let msg_send_bool: MsgSendBool =
                std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, c"objc_msgSend".as_ptr()));
            let cls = objc_getClass(c"CATransaction".as_ptr());
            let sel_begin = sel_registerName(c"begin".as_ptr());
            let sel_disable = sel_registerName(c"setDisableActions:".as_ptr());
            let sel_commit = sel_registerName(c"commit".as_ptr());
            msg_send0(cls, sel_begin);
            msg_send_bool(cls, sel_disable, true);
            panel.setContentSize(target_content.size);
            panel.setFrameTopLeftPoint(objc2_foundation::NSPoint {
                x: current_frame.origin.x,
                y: top_y,
            });
            msg_send0(cls, sel_commit);
        }
    }
    hide_traffic_lights(&panel);
    let actual_content = panel.contentRectForFrameRect(panel.frame()).size;
    (actual_content.height - target_height).abs() < 1.0
        && (actual_content.width - target_width).abs() < 1.0
}

/// Orders an application window matching the given dimensions front, regardless of app focus.
pub fn order_window_front(width: f64, height: f64) {
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(marker);
    let windows = app.windows();
    for index in 0..windows.len() {
        let window = windows.objectAtIndex(index);
        let frame = window.frame();
        if (frame.size.width - width).abs() <= 1.5 && (frame.size.height - height).abs() <= 1.5 {
            use objc2_app_kit::NSWindowCollectionBehavior;
            window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
            window.orderFrontRegardless();
            window.makeKeyWindow();
            break;
        }
    }
}

type DisplayServicesGetBrightnessFn = unsafe extern "C" fn(u32, *mut f32) -> i32;
type DisplayServicesSetBrightnessFn = unsafe extern "C" fn(u32, f32) -> i32;

/// Adjusts system display brightness by delta (-1.0 to 1.0).
pub fn adjust_brightness(delta: f32) -> PlatformResult<f32> {
    unsafe {
        let path = std::ffi::CString::new(
            "/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices",
        )
        .map_err(|e| PlatformError::Os(e.to_string()))?;
        let handle = libc::dlopen(path.as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            return Err(PlatformError::Os(
                "failed to load DisplayServices framework".into(),
            ));
        }
        let get_name = std::ffi::CString::new("DisplayServicesGetBrightness")
            .map_err(|e| PlatformError::Os(e.to_string()))?;
        let set_name = std::ffi::CString::new("DisplayServicesSetBrightness")
            .map_err(|e| PlatformError::Os(e.to_string()))?;
        let get_sym = libc::dlsym(handle, get_name.as_ptr());
        let set_sym = libc::dlsym(handle, set_name.as_ptr());
        if get_sym.is_null() || set_sym.is_null() {
            libc::dlclose(handle);
            return Err(PlatformError::Os(
                "failed to resolve DisplayServices symbols".into(),
            ));
        }
        let get_brightness: DisplayServicesGetBrightnessFn = std::mem::transmute(get_sym);
        let set_brightness: DisplayServicesSetBrightnessFn = std::mem::transmute(set_sym);

        let display = CGMainDisplayID();
        let mut cur: f32 = 0.5;
        let get_status = get_brightness(display, &mut cur);
        if get_status != 0 {
            libc::dlclose(handle);
            return Err(PlatformError::Os(format!(
                "could not read display brightness (status {get_status})"
            )));
        }
        let target = (cur + delta).clamp(0.0, 1.0);
        let set_status = set_brightness(display, target);
        libc::dlclose(handle);
        if set_status != 0 {
            return Err(PlatformError::Os(format!(
                "could not set display brightness (status {set_status})"
            )));
        }
        Ok(target * 100.0)
    }
}

pub fn adjust_audio_output_with_level(delta: f32) -> PlatformResult<f32> {
    if !delta.is_finite() {
        return Err(PlatformError::Os("volume change must be finite".into()));
    }
    let current = super::audio_output_level()
        .ok_or_else(|| PlatformError::Os("could not read the current output volume".into()))?;
    let target = (current + delta.clamp(-1.0, 1.0) * 100.0).clamp(0.0, 100.0);
    let script = format!("set volume output volume {:.0}", target);
    let output = std::process::Command::new("osascript")
        .args(["-e", script.as_str()])
        .output()
        .map_err(|error| PlatformError::Os(format!("could not change output volume: {error}")))?;
    if !output.status.success() {
        return Err(PlatformError::Os(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    super::audio_output_level()
        .ok_or_else(|| PlatformError::Os("could not read the resulting output volume".into()))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

static SCREENS_CACHE: std::sync::RwLock<Option<Vec<ScreenRect>>> = std::sync::RwLock::new(None);
static RESTORE_CACHE: std::sync::Mutex<Option<(i32, ScreenRect)>> = std::sync::Mutex::new(None);

/// Updates the cached screen dimensions from AppKit NSScreen on the main thread.
pub fn update_screens_cache() {
    if let Some(mtm) = MainThreadMarker::new() {
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        let primary_height = if screens.count() > 0 {
            screens.objectAtIndex(0).frame().size.height
        } else {
            0.0
        };
        let mut list = Vec::with_capacity(screens.count());
        for i in 0..screens.count() {
            let scr = screens.objectAtIndex(i);
            let visible = scr.visibleFrame();
            let ax_x = visible.origin.x;
            let ax_y = primary_height - (visible.origin.y + visible.size.height);
            let ax_w = visible.size.width;
            let ax_h = visible.size.height;
            list.push(ScreenRect {
                x: ax_x,
                y: ax_y,
                width: ax_w,
                height: ax_h,
            });
        }
        if let Ok(mut cache) = SCREENS_CACHE.write() {
            *cache = Some(list);
        }
    }
}

fn get_all_screens() -> Vec<ScreenRect> {
    if let Ok(cache) = SCREENS_CACHE.read() {
        if let Some(ref screens) = *cache {
            if !screens.is_empty() {
                return screens.clone();
            }
        }
    }
    if let Some(mtm) = MainThreadMarker::new() {
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        let primary_height = if screens.count() > 0 {
            screens.objectAtIndex(0).frame().size.height
        } else {
            0.0
        };
        let mut list = Vec::with_capacity(screens.count());
        for i in 0..screens.count() {
            let scr = screens.objectAtIndex(i);
            let visible = scr.visibleFrame();
            let ax_x = visible.origin.x;
            let ax_y = primary_height - (visible.origin.y + visible.size.height);
            list.push(ScreenRect {
                x: ax_x,
                y: ax_y,
                width: visible.size.width,
                height: visible.size.height,
            });
        }
        if !list.is_empty() {
            return list;
        }
    }
    get_cg_screens()
}

fn get_cg_screens() -> Vec<ScreenRect> {
    unsafe {
        let mut displays = [0u32; 16];
        let mut count = 0u32;
        let err = CGGetActiveDisplayList(16, displays.as_mut_ptr(), &mut count);
        if err != 0 || count == 0 {
            return vec![ScreenRect {
                x: 0.0,
                y: 25.0,
                width: 1440.0,
                height: 875.0,
            }];
        }
        let mut list = Vec::with_capacity(count as usize);
        for &d in &displays[..count as usize] {
            let bounds = CGDisplayBounds(d);
            let is_main = CGDisplayIsMain(d);
            let y_offset = if is_main { 25.0 } else { 0.0 };
            let h = (bounds.size.height - y_offset).max(100.0);
            list.push(ScreenRect {
                x: bounds.origin.x,
                y: bounds.origin.y + y_offset,
                width: bounds.size.width,
                height: h,
            });
        }
        list
    }
}

fn intersection_area(r1: ScreenRect, r2: ScreenRect) -> f64 {
    let left = r1.x.max(r2.x);
    let right = (r1.x + r1.width).min(r2.x + r2.width);
    let top = r1.y.max(r2.y);
    let bottom = (r1.y + r1.height).min(r2.y + r2.height);
    if right > left && bottom > top {
        (right - left) * (bottom - top)
    } else {
        0.0
    }
}

/// Tiles and arranges the target application's window using native AXUIElement.
pub fn tile_window(target_pid: Option<i32>, action: &str) -> PlatformResult<()> {
    if !is_accessibility_trusted(true) {
        let _ = super::open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        );
        return Err(PlatformError::Os(
            "Accessibility permission required for window management. Please enable Corvo in System Settings -> Privacy & Security -> Accessibility.".into(),
        ));
    }

    let pid = match target_pid {
        Some(p) => p,
        None => frontmost_app_pid()
            .ok_or_else(|| PlatformError::Os("no active application found to tile".into()))?,
    };

    activate_app(pid);

    unsafe {
        let app_ref = AXUIElementCreateApplication(pid as libc::pid_t);
        if app_ref.is_null() {
            return Err(PlatformError::Os(
                "failed to create AXUIElement for process".into(),
            ));
        }

        // Set 50ms messaging timeout to prevent hung apps from blocking the UI thread
        AXUIElementSetMessagingTimeout(app_ref, 0.05);

        // Suppress Chromium/Electron AXEnhancedUserInterface stall during resize
        let attr_enhanced = NSString::from_str("AXEnhancedUserInterface");
        let mut original_enhanced: *const std::ffi::c_void = std::ptr::null();
        let had_enhanced = AXUIElementCopyAttributeValue(
            app_ref,
            &*attr_enhanced as *const _ as *const std::ffi::c_void,
            &mut original_enhanced,
        ) == 0
            && !original_enhanced.is_null();

        if had_enhanced && CFEqual(original_enhanced, kCFBooleanTrue) {
            AXUIElementSetAttributeValue(
                app_ref,
                &*attr_enhanced as *const _ as *const std::ffi::c_void,
                kCFBooleanFalse,
            );
        }

        let attr_focused = NSString::from_str("AXFocusedWindow");
        let mut window_ref: *const std::ffi::c_void = std::ptr::null();
        let mut status = AXUIElementCopyAttributeValue(
            app_ref,
            &*attr_focused as *const _ as *const std::ffi::c_void,
            &mut window_ref,
        );

        if status != 0 || window_ref.is_null() {
            let attr_main = NSString::from_str("AXMainWindow");
            status = AXUIElementCopyAttributeValue(
                app_ref,
                &*attr_main as *const _ as *const std::ffi::c_void,
                &mut window_ref,
            );
        }

        if status != 0 || window_ref.is_null() {
            let attr_windows = NSString::from_str("AXWindows");
            let mut windows_ref: *const std::ffi::c_void = std::ptr::null();
            let arr_status = AXUIElementCopyAttributeValue(
                app_ref,
                &*attr_windows as *const _ as *const std::ffi::c_void,
                &mut windows_ref,
            );
            if arr_status == 0 && !windows_ref.is_null() {
                use objc2_foundation::NSArray;
                let arr = &*(windows_ref as *const NSArray<objc2::runtime::AnyObject>);
                if arr.count() > 0 {
                    let first = arr.objectAtIndex(0);
                    window_ref = &*first as *const _ as *const std::ffi::c_void;
                }
            }
        }

        if window_ref.is_null() {
            if had_enhanced {
                AXUIElementSetAttributeValue(
                    app_ref,
                    &*attr_enhanced as *const _ as *const std::ffi::c_void,
                    original_enhanced,
                );
                CFRelease(original_enhanced);
            }
            CFRelease(app_ref as *const std::ffi::c_void);
            return Err(PlatformError::Os(
                "no accessible window found for application".into(),
            ));
        }

        AXUIElementSetMessagingTimeout(window_ref as *mut _, 0.05);

        // Read current window position and size
        let attr_pos = NSString::from_str("AXPosition");
        let attr_size = NSString::from_str("AXSize");

        let mut cur_pos_val: *const std::ffi::c_void = std::ptr::null();
        let mut cur_size_val: *const std::ffi::c_void = std::ptr::null();
        let mut cur_pos = CGPoint { x: 0.0, y: 0.0 };
        let mut cur_size = CGSize {
            width: 0.0,
            height: 0.0,
        };

        if AXUIElementCopyAttributeValue(
            window_ref as *mut _,
            &*attr_pos as *const _ as *const _,
            &mut cur_pos_val,
        ) == 0
            && !cur_pos_val.is_null()
        {
            AXValueGetValue(
                cur_pos_val,
                K_AX_VALUE_CGPOINT_TYPE,
                &mut cur_pos as *mut _ as *mut _,
            );
            CFRelease(cur_pos_val);
        }
        if AXUIElementCopyAttributeValue(
            window_ref as *mut _,
            &*attr_size as *const _ as *const _,
            &mut cur_size_val,
        ) == 0
            && !cur_size_val.is_null()
        {
            AXValueGetValue(
                cur_size_val,
                K_AX_VALUE_CGSIZE_TYPE,
                &mut cur_size as *mut _ as *mut _,
            );
            CFRelease(cur_size_val);
        }

        let cur_window_rect = ScreenRect {
            x: cur_pos.x,
            y: cur_pos.y,
            width: cur_size.width,
            height: cur_size.height,
        };

        // Multi-display detection via maximum intersection area
        let screens = get_all_screens();
        let (current_screen_idx, current_screen) = screens
            .iter()
            .enumerate()
            .max_by(|(_, s1), (_, s2)| {
                let a1 = intersection_area(cur_window_rect, **s1);
                let a2 = intersection_area(cur_window_rect, **s2);
                a1.total_cmp(&a2)
            })
            .map(|(idx, &s)| (idx, s))
            .unwrap_or((
                0,
                ScreenRect {
                    x: 0.0,
                    y: 25.0,
                    width: 1440.0,
                    height: 875.0,
                },
            ));

        // Handle restore action or store current frame for future restore
        if action == "restore" {
            let restored = {
                let mut guard = RESTORE_CACHE.lock().unwrap();
                match *guard {
                    Some((saved_pid, rect)) if saved_pid == pid => {
                        *guard = None;
                        Some(rect)
                    }
                    _ => None,
                }
            };

            let Some(target) = restored else {
                if had_enhanced {
                    AXUIElementSetAttributeValue(
                        app_ref,
                        &*attr_enhanced as *const _ as *const std::ffi::c_void,
                        original_enhanced,
                    );
                    CFRelease(original_enhanced);
                }
                CFRelease(app_ref as *const std::ffi::c_void);
                CFRelease(window_ref);
                return Ok(());
            };

            apply_window_frame(window_ref, target);

            if had_enhanced {
                AXUIElementSetAttributeValue(
                    app_ref,
                    &*attr_enhanced as *const _ as *const std::ffi::c_void,
                    original_enhanced,
                );
                CFRelease(original_enhanced);
            }
            CFRelease(app_ref as *const std::ffi::c_void);
            CFRelease(window_ref);
            return Ok(());
        }

        // Cache the pre-tile frame if not already cached for this PID
        {
            let mut guard = RESTORE_CACHE.lock().unwrap();
            let should_cache = match *guard {
                Some((saved_pid, _)) => saved_pid != pid,
                None => true,
            };
            if should_cache && cur_window_rect.width > 50.0 && cur_window_rect.height > 50.0 {
                *guard = Some((pid, cur_window_rect));
            }
        }

        let sx = current_screen.x;
        let sy = current_screen.y;
        let sw = current_screen.width;
        let sh = current_screen.height;

        let gap = super::get_window_gap() as f64;

        let target_rect = if gap <= 0.0 {
            match action {
                // Halves
                "left-half" => ScreenRect {
                    x: sx,
                    y: sy,
                    width: sw / 2.0,
                    height: sh,
                },
                "right-half" => ScreenRect {
                    x: sx + sw / 2.0,
                    y: sy,
                    width: sw / 2.0,
                    height: sh,
                },
                "top-half" => ScreenRect {
                    x: sx,
                    y: sy,
                    width: sw,
                    height: sh / 2.0,
                },
                "bottom-half" => ScreenRect {
                    x: sx,
                    y: sy + sh / 2.0,
                    width: sw,
                    height: sh / 2.0,
                },

                // Thirds
                "first-third" => ScreenRect {
                    x: sx,
                    y: sy,
                    width: sw / 3.0,
                    height: sh,
                },
                "center-third" => ScreenRect {
                    x: sx + sw / 3.0,
                    y: sy,
                    width: sw / 3.0,
                    height: sh,
                },
                "last-third" => ScreenRect {
                    x: sx + 2.0 * sw / 3.0,
                    y: sy,
                    width: sw / 3.0,
                    height: sh,
                },
                "first-two-thirds" => ScreenRect {
                    x: sx,
                    y: sy,
                    width: 2.0 * sw / 3.0,
                    height: sh,
                },
                "last-two-thirds" => ScreenRect {
                    x: sx + sw / 3.0,
                    y: sy,
                    width: 2.0 * sw / 3.0,
                    height: sh,
                },

                // Quarters
                "top-left" => ScreenRect {
                    x: sx,
                    y: sy,
                    width: sw / 2.0,
                    height: sh / 2.0,
                },
                "top-right" => ScreenRect {
                    x: sx + sw / 2.0,
                    y: sy,
                    width: sw / 2.0,
                    height: sh / 2.0,
                },
                "bottom-left" => ScreenRect {
                    x: sx,
                    y: sy + sh / 2.0,
                    width: sw / 2.0,
                    height: sh / 2.0,
                },
                "bottom-right" => ScreenRect {
                    x: sx + sw / 2.0,
                    y: sy + sh / 2.0,
                    width: sw / 2.0,
                    height: sh / 2.0,
                },

                // Whole Screen / Centering
                "maximize" => ScreenRect {
                    x: sx,
                    y: sy,
                    width: sw,
                    height: sh,
                },
                "almost-maximize" => {
                    let w = sw * 0.9;
                    let h = sh * 0.9;
                    ScreenRect {
                        x: sx + (sw - w) / 2.0,
                        y: sy + (sh - h) / 2.0,
                        width: w,
                        height: h,
                    }
                }
                "center" => {
                    let w = sw * 0.7;
                    let h = sh * 0.8;
                    ScreenRect {
                        x: sx + (sw - sw * 0.7) / 2.0,
                        y: sy + (sh - sh * 0.8) / 2.0,
                        width: w,
                        height: h,
                    }
                }
                _ => ScreenRect {
                    x: sx,
                    y: sy,
                    width: sw,
                    height: sh,
                },
            }
        } else {
            let hw = (sw - 3.0 * gap).max(100.0) / 2.0;
            let hh = (sh - 3.0 * gap).max(100.0) / 2.0;
            let tw = (sw - 4.0 * gap).max(100.0) / 3.0;

            match action {
                // Halves with gap
                "left-half" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap,
                    width: hw,
                    height: sh - 2.0 * gap,
                },
                "right-half" => ScreenRect {
                    x: sx + gap + hw + gap,
                    y: sy + gap,
                    width: hw,
                    height: sh - 2.0 * gap,
                },
                "top-half" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap,
                    width: sw - 2.0 * gap,
                    height: hh,
                },
                "bottom-half" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap + hh + gap,
                    width: sw - 2.0 * gap,
                    height: hh,
                },

                // Thirds with gap
                "first-third" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap,
                    width: tw,
                    height: sh - 2.0 * gap,
                },
                "center-third" => ScreenRect {
                    x: sx + gap + tw + gap,
                    y: sy + gap,
                    width: tw,
                    height: sh - 2.0 * gap,
                },
                "last-third" => ScreenRect {
                    x: sx + gap + 2.0 * (tw + gap),
                    y: sy + gap,
                    width: tw,
                    height: sh - 2.0 * gap,
                },
                "first-two-thirds" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap,
                    width: 2.0 * tw + gap,
                    height: sh - 2.0 * gap,
                },
                "last-two-thirds" => ScreenRect {
                    x: sx + gap + tw + gap,
                    y: sy + gap,
                    width: 2.0 * tw + gap,
                    height: sh - 2.0 * gap,
                },

                // Quarters with gap
                "top-left" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap,
                    width: hw,
                    height: hh,
                },
                "top-right" => ScreenRect {
                    x: sx + gap + hw + gap,
                    y: sy + gap,
                    width: hw,
                    height: hh,
                },
                "bottom-left" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap + hh + gap,
                    width: hw,
                    height: hh,
                },
                "bottom-right" => ScreenRect {
                    x: sx + gap + hw + gap,
                    y: sy + gap + hh + gap,
                    width: hw,
                    height: hh,
                },

                // Whole Screen / Centering with gap
                "maximize" => ScreenRect {
                    x: sx + gap,
                    y: sy + gap,
                    width: sw - 2.0 * gap,
                    height: sh - 2.0 * gap,
                },
                "almost-maximize" => {
                    let w = (sw - 2.0 * gap) * 0.9;
                    let h = (sh - 2.0 * gap) * 0.9;
                    ScreenRect {
                        x: sx + gap + (sw - 2.0 * gap - w) / 2.0,
                        y: sy + gap + (sh - 2.0 * gap - h) / 2.0,
                        width: w,
                        height: h,
                    }
                }
                "center" => {
                    let w = (sw - 2.0 * gap) * 0.7;
                    let h = (sh - 2.0 * gap) * 0.8;
                    ScreenRect {
                        x: sx + gap + (sw - 2.0 * gap - w) / 2.0,
                        y: sy + gap + (sh - 2.0 * gap - h) / 2.0,
                        width: w,
                        height: h,
                    }
                }
                _ => ScreenRect {
                    x: sx + gap,
                    y: sy + gap,
                    width: sw - 2.0 * gap,
                    height: sh - 2.0 * gap,
                },
            }
        };

        let target_rect = match action {
            "next-display" | "prev-display" => {
                if screens.len() <= 1 {
                    cur_window_rect
                } else {
                    let next_idx = if action == "next-display" {
                        (current_screen_idx + 1) % screens.len()
                    } else {
                        (current_screen_idx + screens.len() - 1) % screens.len()
                    };
                    let next_s = screens[next_idx];
                    let rel_x = (cur_window_rect.x - sx) / sw.max(1.0);
                    let rel_y = (cur_window_rect.y - sy) / sh.max(1.0);
                    let rel_w = cur_window_rect.width / sw.max(1.0);
                    let rel_h = cur_window_rect.height / sh.max(1.0);
                    ScreenRect {
                        x: next_s.x + rel_x * next_s.width,
                        y: next_s.y + rel_y * next_s.height,
                        width: (rel_w * next_s.width).min(next_s.width),
                        height: (rel_h * next_s.height).min(next_s.height),
                    }
                }
            }
            "restore" => target_rect,
            _ if matches!(
                action,
                "left-half"
                    | "right-half"
                    | "top-half"
                    | "bottom-half"
                    | "first-third"
                    | "center-third"
                    | "last-third"
                    | "first-two-thirds"
                    | "last-two-thirds"
                    | "top-left"
                    | "top-right"
                    | "bottom-left"
                    | "bottom-right"
                    | "maximize"
                    | "almost-maximize"
                    | "center"
            ) =>
            {
                target_rect
            }
            _ => {
                if had_enhanced {
                    AXUIElementSetAttributeValue(
                        app_ref,
                        &*attr_enhanced as *const _ as *const std::ffi::c_void,
                        original_enhanced,
                    );
                    CFRelease(original_enhanced);
                }
                CFRelease(app_ref as *const std::ffi::c_void);
                CFRelease(window_ref);
                return Err(PlatformError::Os(format!(
                    "unknown window action: {action}"
                )));
            }
        };

        apply_window_frame(window_ref, target_rect);

        // Right-edge compensation if target was right-aligned and actual width exceeded target_rect.width
        let is_right_aligned = matches!(
            action,
            "right-half" | "last-third" | "last-two-thirds" | "top-right" | "bottom-right"
        );
        if is_right_aligned {
            let mut final_size_val: *const std::ffi::c_void = std::ptr::null();
            let mut final_size = CGSize {
                width: target_rect.width,
                height: target_rect.height,
            };
            if AXUIElementCopyAttributeValue(
                window_ref as *mut _,
                &*attr_size as *const _ as *const _,
                &mut final_size_val,
            ) == 0
                && !final_size_val.is_null()
            {
                AXValueGetValue(
                    final_size_val,
                    K_AX_VALUE_CGSIZE_TYPE,
                    &mut final_size as *mut _ as *mut _,
                );
                CFRelease(final_size_val);
            }
            if final_size.width > target_rect.width {
                let adjusted_x = (target_rect.x + target_rect.width) - final_size.width;
                let adj_pt = CGPoint {
                    x: adjusted_x,
                    y: target_rect.y,
                };
                let adj_val =
                    AXValueCreate(K_AX_VALUE_CGPOINT_TYPE, &adj_pt as *const _ as *const _);
                if !adj_val.is_null() {
                    AXUIElementSetAttributeValue(
                        window_ref as *mut _,
                        &*attr_pos as *const _ as *const _,
                        adj_val,
                    );
                    CFRelease(adj_val);
                }
            }
        }

        // Restore Chromium Enhanced UI if it was temporarily disabled
        if had_enhanced {
            AXUIElementSetAttributeValue(
                app_ref,
                &*attr_enhanced as *const _ as *const std::ffi::c_void,
                original_enhanced,
            );
            CFRelease(original_enhanced);
        }

        CFRelease(app_ref as *const std::ffi::c_void);
        CFRelease(window_ref);

        Ok(())
    }
}

/// Helper that sets window geometry using the robust size -> position -> size sequence.
unsafe fn apply_window_frame(window_ref: *const std::ffi::c_void, rect: ScreenRect) {
    let pt = CGPoint {
        x: rect.x,
        y: rect.y,
    };
    let sz = CGSize {
        width: rect.width,
        height: rect.height,
    };

    let pos_val = AXValueCreate(
        K_AX_VALUE_CGPOINT_TYPE,
        &pt as *const _ as *const std::ffi::c_void,
    );
    let size_val = AXValueCreate(
        K_AX_VALUE_CGSIZE_TYPE,
        &sz as *const _ as *const std::ffi::c_void,
    );

    let attr_pos = NSString::from_str("AXPosition");
    let attr_size = NSString::from_str("AXSize");

    // Sequence: size -> position -> size
    // macOS clamps size to current display bounds before moving across displays;
    // setting size first shrinks it, position moves it, second size applies target dimensions.
    AXUIElementSetAttributeValue(
        window_ref as *mut _,
        &*attr_size as *const _ as *const std::ffi::c_void,
        size_val as *const std::ffi::c_void,
    );
    AXUIElementSetAttributeValue(
        window_ref as *mut _,
        &*attr_pos as *const _ as *const std::ffi::c_void,
        pos_val as *const std::ffi::c_void,
    );
    AXUIElementSetAttributeValue(
        window_ref as *mut _,
        &*attr_size as *const _ as *const std::ffi::c_void,
        size_val as *const std::ffi::c_void,
    );

    if !pos_val.is_null() {
        CFRelease(pos_val as *const std::ffi::c_void);
    }
    if !size_val.is_null() {
        CFRelease(size_val as *const std::ffi::c_void);
    }
}

/// Opens an application by path or by name.
pub fn open_app(name_or_path: &str) -> PlatformResult<()> {
    let p = Path::new(name_or_path);
    if p.is_absolute() && p.exists() {
        let ns_path = NSString::from_str(&p.to_string_lossy());
        let url = NSURL::fileURLWithPath_isDirectory(&ns_path, true);
        if NSWorkspace::sharedWorkspace().openURL(&url) {
            return Ok(());
        }
    }

    let default_scopes = vec![
        "/Applications".to_string(),
        "/Applications/Utilities".to_string(),
        "/System/Applications".to_string(),
        "/System/Applications/Utilities".to_string(),
        "~/Applications".to_string(),
    ];
    if let Ok(apps) = list_apps_in_scopes(&default_scopes) {
        let clean_name = name_or_path.strip_suffix(".app").unwrap_or(name_or_path);
        if let Some(app) = apps.into_iter().find(|a| {
            a.name.eq_ignore_ascii_case(clean_name)
                || a.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(clean_name))
        }) {
            let ns_path = NSString::from_str(&app.path.to_string_lossy());
            let url = NSURL::fileURLWithPath_isDirectory(&ns_path, true);
            if NSWorkspace::sharedWorkspace().openURL(&url) {
                return Ok(());
            }
        }
    }

    let status = std::process::Command::new("open")
        .arg("-a")
        .arg(name_or_path)
        .spawn();
    if status.is_ok() {
        Ok(())
    } else {
        Err(PlatformError::Os(format!(
            "failed to open application: {name_or_path}"
        )))
    }
}

/// Captures visible window frames and determines standard tile placements for running apps.
pub fn capture_current_window_layout() -> Vec<(String, String)> {
    let mut results = Vec::new();
    let screens = get_all_screens();
    let primary_screen = screens.first().copied().unwrap_or(ScreenRect {
        x: 0.0,
        y: 25.0,
        width: 1440.0,
        height: 875.0,
    });

    let workspace = NSWorkspace::sharedWorkspace();
    let running = workspace.runningApplications();
    for i in 0..running.count() {
        let app = running.objectAtIndex(i);
        if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
            continue;
        }
        let pid = app.processIdentifier();
        if pid == std::process::id() as libc::pid_t {
            continue;
        }
        let app_name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if app_name.is_empty() {
            continue;
        }

        unsafe {
            let app_ref = AXUIElementCreateApplication(pid);
            if app_ref.is_null() {
                continue;
            }
            AXUIElementSetMessagingTimeout(app_ref, 0.04);
            let attr_window = NSString::from_str("AXFocusedWindow");
            let mut window_ref: *const std::ffi::c_void = std::ptr::null();
            let mut status = AXUIElementCopyAttributeValue(
                app_ref,
                &*attr_window as *const _ as *const std::ffi::c_void,
                &mut window_ref,
            );
            if status != 0 || window_ref.is_null() {
                let attr_main = NSString::from_str("AXMainWindow");
                status = AXUIElementCopyAttributeValue(
                    app_ref,
                    &*attr_main as *const _ as *const std::ffi::c_void,
                    &mut window_ref,
                );
            }
            if !window_ref.is_null() {
                let mut cur_pos_val: *const std::ffi::c_void = std::ptr::null();
                let mut cur_size_val: *const std::ffi::c_void = std::ptr::null();
                let mut cur_pos = CGPoint { x: 0.0, y: 0.0 };
                let mut cur_size = CGSize {
                    width: 0.0,
                    height: 0.0,
                };

                let attr_pos = NSString::from_str("AXPosition");
                let attr_size = NSString::from_str("AXSize");
                if AXUIElementCopyAttributeValue(
                    window_ref as *mut _,
                    &*attr_pos as *const _ as *const _,
                    &mut cur_pos_val,
                ) == 0
                    && !cur_pos_val.is_null()
                {
                    AXValueGetValue(
                        cur_pos_val,
                        K_AX_VALUE_CGPOINT_TYPE,
                        &mut cur_pos as *mut _ as *mut _,
                    );
                    CFRelease(cur_pos_val);
                }
                if AXUIElementCopyAttributeValue(
                    window_ref as *mut _,
                    &*attr_size as *const _ as *const _,
                    &mut cur_size_val,
                ) == 0
                    && !cur_size_val.is_null()
                {
                    AXValueGetValue(
                        cur_size_val,
                        K_AX_VALUE_CGSIZE_TYPE,
                        &mut cur_size as *mut _ as *mut _,
                    );
                    CFRelease(cur_size_val);
                }
                CFRelease(window_ref);

                if cur_size.width > 100.0 && cur_size.height > 100.0 {
                    let sw = primary_screen.width;
                    let sh = primary_screen.height;
                    let sx = primary_screen.x;
                    let sy = primary_screen.y;

                    let rel_x = cur_pos.x - sx;
                    let rel_y = cur_pos.y - sy;
                    let w = cur_size.width;
                    let h = cur_size.height;

                    let position = if (w - sw).abs() < 120.0 && (h - sh).abs() < 120.0 {
                        "maximize"
                    } else if (w - sw / 2.0).abs() < 100.0 {
                        if rel_x < sw / 4.0 {
                            "left-half"
                        } else {
                            "right-half"
                        }
                    } else if (h - sh / 2.0).abs() < 100.0 {
                        if rel_y < sh / 4.0 {
                            "top-half"
                        } else {
                            "bottom-half"
                        }
                    } else if (w - sw / 3.0).abs() < 100.0 {
                        if rel_x < sw / 3.0 {
                            "first-third"
                        } else if rel_x < 2.0 * sw / 3.0 {
                            "center-third"
                        } else {
                            "last-third"
                        }
                    } else {
                        "center"
                    };

                    results.push((app_name, position.to_string()));
                }
            }
            CFRelease(app_ref as *const std::ffi::c_void);
        }
    }

    results
}

/// Applies standard window placements for multiple applications.
pub fn apply_window_layout(placements: &[(String, String)]) -> PlatformResult<()> {
    for (app_name, position) in placements {
        let _ = open_app(app_name);
        std::thread::sleep(std::time::Duration::from_millis(60));

        let workspace = NSWorkspace::sharedWorkspace();
        let running = workspace.runningApplications();
        let mut target_pid = None;
        for i in 0..running.count() {
            let app = running.objectAtIndex(i);
            let name = app
                .localizedName()
                .map(|s| s.to_string())
                .unwrap_or_default();
            if name.eq_ignore_ascii_case(app_name) {
                target_pid = Some(app.processIdentifier());
                break;
            }
        }

        if let Some(pid) = target_pid {
            let _ = tile_window(Some(pid), position);
        }
    }
    Ok(())
}

/// Queries whether launch at login is currently enabled.
pub fn is_launch_at_login_enabled() -> bool {
    if let Some(home) = std::env::var_os("HOME") {
        let plist_path = Path::new(&home).join("Library/LaunchAgents/sh.corvo.corvo.plist");
        if plist_path.exists() {
            return true;
        }
    }
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of every login item",
        ])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let names = String::from_utf8_lossy(&output.stdout);
            return names
                .split(',')
                .any(|name| name.trim().eq_ignore_ascii_case("Corvo"));
        }
    }
    false
}

/// Enables or disables launch at login on macOS.
/// Uses the macOS Login Item API when running as a .app bundle, and a
/// LaunchAgent plist when running unbundled (development builds). Installing
/// both simultaneously would cause Corvo to launch twice on login.
pub fn set_launch_at_login(enabled: bool) -> PlatformResult<()> {
    let app_path = find_corvo_app_path();

    if let Some(app_path) = app_path {
        // Bundled: use Login Item only. Clean up any stale LaunchAgent.
        if enabled {
            let path_str = app_path.to_string_lossy();
            let script = format!(
                "tell application \"System Events\" to if not (exists login item \"Corvo\") then make login item at end with properties {{name:\"Corvo\", path:\"{path_str}\", hidden:false}}"
            );
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .output();
        } else {
            let _ = std::process::Command::new("osascript")
                .args(["-e", "tell application \"System Events\" to if exists login item \"Corvo\" then delete login item \"Corvo\""])
                .output();
        }
        // Always remove the LaunchAgent when bundled to prevent double-launch.
        if let Some(home) = std::env::var_os("HOME") {
            let plist_path =
                std::path::Path::new(&home).join("Library/LaunchAgents/sh.corvo.corvo.plist");
            if plist_path.exists() {
                let _ = std::process::Command::new("launchctl")
                    .args(["unload", "-w", &plist_path.to_string_lossy()])
                    .output();
                let _ = std::fs::remove_file(&plist_path);
            }
        }
    } else {
        // Unbundled (dev build): use LaunchAgent only. Clean up any Login Item.
        let _ = std::process::Command::new("osascript")
            .args(["-e", "tell application \"System Events\" to if exists login item \"Corvo\" then delete login item \"Corvo\""])
            .output();

        if let Some(home) = std::env::var_os("HOME") {
            let launch_agents_dir = std::path::Path::new(&home).join("Library/LaunchAgents");
            let plist_path = launch_agents_dir.join("sh.corvo.corvo.plist");

            if enabled {
                let _ = std::fs::create_dir_all(&launch_agents_dir);
                let target_exe = std::env::current_exe()
                    .unwrap_or_else(|_| PathBuf::from("/usr/local/bin/corvo"));
                let plist_content = format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>sh.corvo.corvo</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
                    target_exe.display()
                );
                if std::fs::write(&plist_path, plist_content).is_ok() {
                    let _ = std::process::Command::new("launchctl")
                        .args(["load", "-w", &plist_path.to_string_lossy()])
                        .output();
                }
            } else if plist_path.exists() {
                let _ = std::process::Command::new("launchctl")
                    .args(["unload", "-w", &plist_path.to_string_lossy()])
                    .output();
                let _ = std::fs::remove_file(&plist_path);
            }
        }
    }

    Ok(())
}

fn find_corvo_app_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let mut bundle_dir = exe;
        while let Some(parent) = bundle_dir.parent() {
            if bundle_dir.extension().and_then(|e| e.to_str()) == Some("app") {
                return Some(bundle_dir);
            }
            bundle_dir = parent.to_path_buf();
        }
    }
    let standard = PathBuf::from("/Applications/Corvo.app");
    if standard.exists() {
        return Some(standard);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_app = Path::new(&home).join("Applications/Corvo.app");
        if user_app.exists() {
            return Some(user_app);
        }
    }
    None
}

/// Marker that tells a running relaunch watchdog this process exited
/// cleanly. The watchdog relaunches only while the marker exists, so a
/// normal Quit inside the watch window is honored.
fn screen_recording_relaunch_marker() -> PathBuf {
    std::env::temp_dir().join("sh.corvo.corvo.relaunch")
}

/// Marks a clean exit. Call before quitting Corvo while a watchdog may run.
pub fn clear_screen_recording_relaunch_marker() {
    let _ = std::fs::remove_file(screen_recording_relaunch_marker());
}

/// Granting Screen Recording only takes effect after the process dies: macOS
/// kills the app ("Quit & Reopen" in System Settings). Arms a detached
/// watcher that relaunches Corvo the moment this process disappears, turning
/// the forced quit into a background restart. Gives up silently if the
/// process survives two minutes (grant cancelled) or exits cleanly.
pub fn arm_screen_recording_relaunch_watchdog() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let marker = screen_recording_relaunch_marker();
    if std::fs::write(&marker, b"").is_err() {
        return;
    }
    let pid = std::process::id();
    // The path travels as a positional argument, never as script text, so a
    // quote in the install path cannot break the script.
    let (relaunch, target) = match find_corvo_app_path() {
        Some(app) if exe.starts_with(&app) => ("exec open \"$1\"", app.display().to_string()),
        _ => ("nohup \"$1\" >/dev/null 2>&1 &", exe.display().to_string()),
    };
    let script = format!(
        "i=0\n\
         while kill -0 {pid} 2>/dev/null; do\n\
         \x20 i=$((i+1))\n\
         \x20 if [ $i -gt 240 ]; then rm -f \"$2\"; exit 0; fi\n\
         \x20 sleep 0.5\n\
         done\n\
         [ -e \"$2\" ] || exit 0\n\
         rm -f \"$2\"\n\
         sleep 1\n\
         {relaunch}"
    );
    let _ = std::process::Command::new("sh")
        .args(["-c", &script, "corvo-watchdog", &target])
        .arg(&marker)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn unimplemented_os(what: &str) -> PlatformError {
    PlatformError::Unsupported(format!("macos {what} arrives in phase 4"))
}

/// Shows a macOS notification. Runs on the timer thread, where waiting
/// the ~100ms for osascript is acceptable.
pub fn notify(title: &str, body: &str) -> PlatformResult<()> {
    let escape = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape(body),
        escape(title)
    );
    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(PlatformError::Os(format!("osascript exited with {status}"))),
        Err(error) => Err(PlatformError::Os(format!(
            "could not start osascript: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn copy_text_roundtrips_unicode() {
        // A uniquely named pasteboard keeps the roundtrip off the real
        // clipboard; the old version wrote "🚀 hello üòÄ" over whatever
        // the user had copied whenever the suite ran.
        let pboard = objc2_app_kit::NSPasteboard::pasteboardWithUniqueName();
        let text = "🚀 hello üòÄ";
        super::write_string_to_pasteboard(&pboard, text);
        let pasted = unsafe { pboard.stringForType(objc2_app_kit::NSPasteboardTypeString) };
        assert_eq!(
            pasted.as_ref().map(|s| s.to_string()),
            Some(text.to_string())
        );
    }

    #[test]
    fn parses_png_dimensions_correctly() {
        let mut sample = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x07, 0x80, // 1920
            0x00, 0x00, 0x04, 0x38, // 1080
        ];
        assert_eq!(super::parse_png_dimensions(&sample), Some((1920, 1080)));

        sample.truncate(20);
        assert_eq!(super::parse_png_dimensions(&sample), None);
    }

    #[test]
    fn extracts_high_res_app_icon() {
        let books = std::path::Path::new("/System/Applications/Books.app");
        if books.exists() {
            let icon_path = super::extract_app_icon(books).expect("extracts Books icon");
            assert!(icon_path.exists());
            let bytes = std::fs::read(&icon_path).expect("reads icon bytes");
            let dims = super::parse_png_dimensions(&bytes).expect("parses dimensions");
            assert_eq!(dims, (256, 256));
            // High-res icon should be richly detailed (> 15KB), not a 1.8KB blurry stub
            assert!(
                bytes.len() > 15_000,
                "expected >15KB high res icon, got {}",
                bytes.len()
            );
        }
    }

    #[test]
    fn app_icon_cache_key_separates_bundles_with_the_same_name() {
        let first = std::path::Path::new("/Applications/First/Foo.app");
        let second = std::path::Path::new("/Applications/Second/Foo.app");
        assert_ne!(
            super::app_icon_cache_key(first, "Foo"),
            super::app_icon_cache_key(second, "Foo")
        );
    }

    #[test]
    fn invalid_cached_png_is_rejected() {
        let path = std::env::temp_dir().join(format!(
            "corvo-invalid-icon-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::write(&path, b"not a png").expect("writes the fixture");
        assert!(!super::is_valid_png(&path));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn launch_at_login_toggles_cleanly() {
        let original = super::is_launch_at_login_enabled();
        super::set_launch_at_login(true).expect("enables launch at login");
        assert!(super::is_launch_at_login_enabled());
        super::set_launch_at_login(false).expect("disables launch at login");
        assert!(!super::is_launch_at_login_enabled());
        if original {
            let _ = super::set_launch_at_login(true);
        }
    }
}

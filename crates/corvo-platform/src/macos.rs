//! macOS implementation. Session control arrives in phase 4 (SPEC §11);
//! app-launcher inputs and the agent policy are live in phase 1.

use std::path::{Path, PathBuf};

use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy,
    NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypeString,
    NSRunningApplication, NSWindowAnimationBehavior, NSWorkspace,
};
use objc2_foundation::{NSDictionary, MainThreadMarker, NSData, NSNumber, NSString, NSURL};

use super::{AppEntry, PlatformError, PlatformOps, PlatformResult, WindowHandle};

// CoreGraphics FFI for synthetic paste keystrokes
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
    fn CGMainDisplayID() -> u32;
    fn CGEventCreate(source: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
    fn CGGetDisplaysWithPoint(
        point: CGPoint,
        max_displays: u32,
        displays: *mut u32,
        matching_display_count: *mut u32,
    ) -> i32;
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
    fn AXValueCreate(value_type: u32, value_ptr: *const std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGSize {
    width: f64,
    height: f64,
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
        // Search order is priority: a name found in an earlier root wins,
        // so /Applications beats the system copies.
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut roots: Vec<PathBuf> = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
        ];
        if let Some(home) = home {
            roots.push(home.join("Applications"));
        }

        let mut seen = std::collections::HashSet::new();
        let mut apps = Vec::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = bundle_app_name(&path) {
                    if seen.insert(name.clone()) {
                        let icon_png = extract_app_icon(&path);
                        apps.push(AppEntry { name, path, icon_png });
                    }
                } else if path.is_dir() {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !file_name.starts_with('.') && !file_name.ends_with(".app") {
                        if let Ok(sub_entries) = std::fs::read_dir(&path) {
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

    fn open_path(&self, path: &Path) -> PlatformResult<()> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
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
    let pboard = NSPasteboard::generalPasteboard();
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
        pboard.stringForType(NSPasteboardTypeString).map(|s| s.to_string())
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
                if let Some(png_data) = rep.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &empty_props,
                ) {
                    return Some(png_data.to_vec());
                }
            }
        }
    }

    None
}

/// Writes PNG bytes to NSPasteboard with dual PNG + TIFF types.
pub fn copy_image_to_pasteboard(png_bytes: &[u8]) {
    let pboard = NSPasteboard::generalPasteboard();
    pboard.clearContents();

    let ns_data = NSData::with_bytes(png_bytes);
    let png_type = NSString::from_str("public.png");
    let tiff_type = NSString::from_str("public.tiff");

    pboard.setData_forType(Some(&ns_data), &png_type);

    if let Some(rep) = NSBitmapImageRep::imageRepWithData(&ns_data) {
        if let Some(tiff_data) = rep.TIFFRepresentation() {
            pboard.setData_forType(Some(&tiff_data), &tiff_type);
        }
    }
}

/// Extracts (width, height) from a PNG byte slice without decoding pixels.
#[inline]
pub fn parse_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 {
        return None;
    }
    if &bytes[0..8] != PNG_SIGNATURE || &bytes[12..16] != b"IHDR" {
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

/// The display name of an `.app` bundle. The bundle directory name is a
/// stable stand-in for `CFBundleName`; localization lands with phase 3
/// config work if it is ever missed.
fn bundle_app_name(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let stem = file_name.strip_suffix(".app")?;
    if stem.is_empty() { None } else { Some(stem.to_string()) }
}

/// Renders the bundle's `.icns` icon to a cached PNG through `sips`.
/// Returns `None` when the bundle carries no icon or `sips` refuses.
/// The cache lives in the temp directory for phase 1; `corvo-config`
/// owns real paths from phase 3 (SPEC §7).
fn extract_app_icon(bundle: &Path) -> Option<PathBuf> {
    let stem = bundle_app_name(bundle)?;
    let cache_dir = std::env::temp_dir().join("corvo-icons");
    let png = cache_dir.join(format!("{stem}.png"));
    if png.exists() {
        return Some(png);
    }
    let resources = bundle.join("Contents/Resources");
    if let Some(icns) = icns_in(&resources, &stem) {
        let png_clone = png.clone();
        let cache_dir_clone = cache_dir.clone();
        std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(&cache_dir_clone);
            let _ = std::process::Command::new("/usr/bin/sips")
                .args(["-Z", "64", "-s", "format", "png"])
                .arg(&icns)
                .arg("--out")
                .arg(&png_clone)
                .output();
        });
    }
    None
}

/// Picks the bundle's icon file: the conventional names first, then the
/// alphabetically first `.icns` in Resources.
fn icns_in(resources: &Path, stem: &str) -> Option<PathBuf> {
    for candidate in ["AppIcon.icns", &format!("{stem}.icns")] {
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
    icns.sort();
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

/// Makes the launcher panel appear with no fade. GPUI marks `PopUp`
/// panels as utility windows, which AppKit fades; this runs on the
/// `Floating` panel instead, sets the popup level, all-spaces behavior,
/// and turns off window animation.
pub fn make_panel_instant(width: f64, height: f64) {
    use objc2_app_kit::{NSColor, NSWindowCollectionBehavior};
    let Some(panel) = find_panel(width, height) else { return };
    panel.setLevel(101); // NSPopUpWindowLevel
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
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
    copy_image_to_pasteboard(png_bytes);
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
pub fn order_panel_front(width: f64, height: f64) {
    use objc2::runtime::AnyObject;
    let Some(panel) = find_panel(width, height) else { return };
    panel.orderFront(None::<&AnyObject>);
    panel.makeKeyWindow();
}

/// Our `Floating` panel, matched by its point size.
fn find_panel(width: f64, height: f64) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    let marker = MainThreadMarker::new()?;
    let app = NSApplication::sharedApplication(marker);
    let windows = app.windows();
    (0..windows.len())
        .map(|index| windows.objectAtIndex(index))
        .find(|window| {
            if window.class().name().to_string_lossy() != "GPUIPanel" {
                return false;
            }
            let frame = window.frame();
            (frame.size.width - width).abs() <= 0.5 && (frame.size.height - height).abs() <= 0.5
        })
}

type DisplayServicesGetBrightnessFn = unsafe extern "C" fn(u32, *mut f32) -> i32;
type DisplayServicesSetBrightnessFn = unsafe extern "C" fn(u32, f32) -> i32;

/// Adjusts system display brightness by delta (-1.0 to 1.0).
pub fn adjust_brightness(delta: f32) -> PlatformResult<()> {
    unsafe {
        let path = std::ffi::CString::new(
            "/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices",
        )
        .map_err(|e| PlatformError::Os(e.to_string()))?;
        let handle = libc::dlopen(path.as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            return Err(PlatformError::Os("failed to load DisplayServices framework".into()));
        }
        let get_name = std::ffi::CString::new("DisplayServicesGetBrightness")
            .map_err(|e| PlatformError::Os(e.to_string()))?;
        let set_name = std::ffi::CString::new("DisplayServicesSetBrightness")
            .map_err(|e| PlatformError::Os(e.to_string()))?;
        let get_sym = libc::dlsym(handle, get_name.as_ptr());
        let set_sym = libc::dlsym(handle, set_name.as_ptr());
        if get_sym.is_null() || set_sym.is_null() {
            libc::dlclose(handle);
            return Err(PlatformError::Os("failed to resolve DisplayServices symbols".into()));
        }
        let get_brightness: DisplayServicesGetBrightnessFn = std::mem::transmute(get_sym);
        let set_brightness: DisplayServicesSetBrightnessFn = std::mem::transmute(set_sym);

        let display = CGMainDisplayID();
        let mut cur: f32 = 0.5;
        let _ = get_brightness(display, &mut cur);
        let target = (cur + delta).clamp(0.0, 1.0);
        let _ = set_brightness(display, target);
        libc::dlclose(handle);
        Ok(())
    }
}

/// Tiles and arranges the target application's window using native AXUIElement.
pub fn tile_window(target_pid: Option<i32>, action: &str) -> PlatformResult<()> {
    if !is_accessibility_trusted(true) {
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
            return Err(PlatformError::Os("failed to create AXUIElement for process".into()));
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

        CFRelease(app_ref as *const std::ffi::c_void);

        if window_ref.is_null() {
            return Err(PlatformError::Os("no accessible window found for application".into()));
        }

        let (screen_x, screen_y, screen_w, screen_h) = get_screen_visible_bounds();

        let (target_x, target_y, target_w, target_h) = match action {
            "left-half" => (screen_x, screen_y, screen_w / 2.0, screen_h),
            "right-half" => (screen_x + screen_w / 2.0, screen_y, screen_w / 2.0, screen_h),
            "top-half" => (screen_x, screen_y, screen_w, screen_h / 2.0),
            "bottom-half" => (screen_x, screen_y + screen_h / 2.0, screen_w, screen_h / 2.0),
            "maximize" => (screen_x, screen_y, screen_w, screen_h),
            "center" => {
                let w = screen_w * 0.7;
                let h = screen_h * 0.8;
                let x = screen_x + (screen_w - w) / 2.0;
                let y = screen_y + (screen_h - h) / 2.0;
                (x, y, w, h)
            }
            "almost-maximize" => {
                let w = screen_w * 0.9;
                let h = screen_h * 0.9;
                let x = screen_x + (screen_w - w) / 2.0;
                let y = screen_y + (screen_h - h) / 2.0;
                (x, y, w, h)
            }
            _ => {
                CFRelease(window_ref);
                return Err(PlatformError::Os(format!("unknown window action: {action}")));
            }
        };

        let pt = CGPoint { x: target_x, y: target_y };
        let sz = CGSize { width: target_w, height: target_h };

        let pos_val = AXValueCreate(K_AX_VALUE_CGPOINT_TYPE, &pt as *const _ as *const std::ffi::c_void);
        let size_val = AXValueCreate(K_AX_VALUE_CGSIZE_TYPE, &sz as *const _ as *const std::ffi::c_void);

        let attr_pos = NSString::from_str("AXPosition");
        let attr_size = NSString::from_str("AXSize");

        AXUIElementSetAttributeValue(window_ref as *mut _, &*attr_pos as *const _ as *const std::ffi::c_void, pos_val as *const std::ffi::c_void);
        AXUIElementSetAttributeValue(window_ref as *mut _, &*attr_size as *const _ as *const std::ffi::c_void, size_val as *const std::ffi::c_void);
        AXUIElementSetAttributeValue(window_ref as *mut _, &*attr_pos as *const _ as *const std::ffi::c_void, pos_val as *const std::ffi::c_void);

        if !pos_val.is_null() {
            CFRelease(pos_val as *const std::ffi::c_void);
        }
        if !size_val.is_null() {
            CFRelease(size_val as *const std::ffi::c_void);
        }
        CFRelease(window_ref);

        Ok(())
    }
}

fn get_screen_visible_bounds() -> (f64, f64, f64, f64) {
    if let Some(mtm) = MainThreadMarker::new() {
        if let Some(screen) = objc2_app_kit::NSScreen::mainScreen(mtm) {
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            let screens = objc2_app_kit::NSScreen::screens(mtm);
            let primary_height = if screens.count() > 0 {
                screens.objectAtIndex(0).frame().size.height
            } else {
                frame.size.height
            };
            let ax_x = visible.origin.x;
            let ax_y = primary_height - (visible.origin.y + visible.size.height);
            let ax_w = visible.size.width;
            let ax_h = visible.size.height;
            return (ax_x, ax_y, ax_w, ax_h);
        }
    }
    (0.0, 25.0, 1440.0, 900.0)
}

fn unimplemented_os(what: &str) -> PlatformError {
    PlatformError::Unsupported(format!("macos {what} arrives in phase 4"))
}

#[cfg(test)]
mod tests {
    use super::MacPlatform;
    use super::PlatformOps;

    #[test]
    fn copy_text_roundtrips_unicode() {
        let platform = MacPlatform;
        let text = "🚀 hello üòÄ";
        platform.copy_text(text).expect("copy_text writes");
        let pasted = std::process::Command::new("/usr/bin/pbpaste")
            .output()
            .expect("pbpaste runs");
        assert!(pasted.status.success());
        assert_eq!(String::from_utf8_lossy(&pasted.stdout).trim_end(), text);
    }

    #[test]
    fn parses_png_dimensions_correctly() {
        let mut sample = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D,
            b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x07, 0x80, // 1920
            0x00, 0x00, 0x04, 0x38, // 1080
        ];
        assert_eq!(super::parse_png_dimensions(&sample), Some((1920, 1080)));

        sample.truncate(20);
        assert_eq!(super::parse_png_dimensions(&sample), None);
    }
}

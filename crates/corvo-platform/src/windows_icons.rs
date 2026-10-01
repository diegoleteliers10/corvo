pub(super) const ICON_SIZE: usize = 32;

fn rgba_from_backgrounds(black: &[u8], white: &[u8]) -> Option<Vec<u8>> {
    let pixels = black
        .chunks_exact(4)
        .zip(white.chunks_exact(4))
        .flat_map(|(black, white)| {
            let background = (0..3)
                .map(|channel| white[channel].saturating_sub(black[channel]))
                .max()
                .unwrap_or_default();
            let alpha = 255 - background;
            let channel = |index: usize| {
                if alpha == 0 {
                    0
                } else {
                    ((u32::from(black[index]) * 255 + u32::from(alpha) / 2) / u32::from(alpha))
                        .min(255) as u8
                }
            };
            [channel(2), channel(1), channel(0), alpha]
        })
        .collect::<Vec<_>>();
    pixels
        .chunks_exact(4)
        .any(|pixel| pixel[3] != 0)
        .then_some(pixels)
}

#[cfg(target_os = "windows")]
pub(super) use native::extract_icon;

#[cfg(target_os = "windows")]
mod native {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use super::{rgba_from_backgrounds, ICON_SIZE};

    #[repr(C)]
    struct ShellFileInfo {
        icon: isize,
        index: i32,
        attributes: u32,
        display_name: [u16; 260],
        type_name: [u16; 80],
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        image_size: u32,
        x_pixels_per_meter: i32,
        y_pixels_per_meter: i32,
        colors_used: u32,
        colors_important: u32,
    }

    #[repr(C)]
    struct BitmapInfo {
        header: BitmapInfoHeader,
        colors: [u32; 1],
    }

    #[allow(non_snake_case)]
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
        fn CoUninitialize();
        fn CoTaskMemFree(memory: *mut c_void);
    }

    #[allow(non_snake_case)]
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHParseDisplayName(
            name: *const u16,
            binding: *mut c_void,
            pidl: *mut *mut c_void,
            attributes: u32,
            parsed_attributes: *mut u32,
        ) -> i32;
        fn SHGetFileInfoW(
            path: *const u16,
            attributes: u32,
            info: *mut ShellFileInfo,
            size: u32,
            flags: u32,
        ) -> usize;
    }

    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn DestroyIcon(icon: isize) -> i32;
        fn DrawIconEx(
            dc: isize,
            x: i32,
            y: i32,
            icon: isize,
            width: i32,
            height: i32,
            step: u32,
            brush: isize,
            flags: u32,
        ) -> i32;
    }

    #[allow(non_snake_case)]
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateCompatibleDC(dc: isize) -> isize;
        fn DeleteDC(dc: isize) -> i32;
        fn CreateDIBSection(
            dc: isize,
            info: *const BitmapInfo,
            usage: u32,
            pixels: *mut *mut c_void,
            section: isize,
            offset: u32,
        ) -> isize;
        fn DeleteObject(object: isize) -> i32;
        fn SelectObject(dc: isize, object: isize) -> isize;
        fn GdiFlush() -> i32;
    }

    enum ComApartment {
        Owned,
        Borrowed,
    }

    impl ComApartment {
        fn enter() -> Option<Self> {
            let status = unsafe { CoInitializeEx(ptr::null_mut(), 2) };
            match status {
                0 | 1 => Some(Self::Owned),
                -2147417850 => Some(Self::Borrowed),
                _ => None,
            }
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if matches!(self, Self::Owned) {
                unsafe { CoUninitialize() };
            }
        }
    }

    struct ItemId(*mut c_void);

    impl Drop for ItemId {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(self.0) };
        }
    }

    struct ShellIcon(isize);

    impl Drop for ShellIcon {
        fn drop(&mut self) {
            unsafe { DestroyIcon(self.0) };
        }
    }

    struct Canvas {
        dc: isize,
        bitmap: isize,
        previous: isize,
        pixels: *mut u8,
    }

    impl Canvas {
        fn new() -> Option<Self> {
            let dc = unsafe { CreateCompatibleDC(0) };
            if dc == 0 {
                return None;
            }
            let mut canvas = Self {
                dc,
                bitmap: 0,
                previous: 0,
                pixels: ptr::null_mut(),
            };
            let info = BitmapInfo {
                header: BitmapInfoHeader {
                    size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                    width: ICON_SIZE as i32,
                    height: -(ICON_SIZE as i32),
                    planes: 1,
                    bit_count: 32,
                    compression: 0,
                    image_size: 0,
                    x_pixels_per_meter: 0,
                    y_pixels_per_meter: 0,
                    colors_used: 0,
                    colors_important: 0,
                },
                colors: [0],
            };
            let mut pixels = ptr::null_mut();
            canvas.bitmap = unsafe { CreateDIBSection(dc, &info, 0, &mut pixels, 0, 0) };
            if canvas.bitmap == 0 || pixels.is_null() {
                return None;
            }
            canvas.pixels = pixels.cast();
            let previous = unsafe { SelectObject(dc, canvas.bitmap) };
            if previous == 0 || previous == -1 {
                return None;
            }
            canvas.previous = previous;
            Some(canvas)
        }

        fn draw(&mut self, icon: &ShellIcon, background: u8) -> Option<Vec<u8>> {
            let length = ICON_SIZE * ICON_SIZE * 4;
            unsafe { ptr::write_bytes(self.pixels, background, length) };
            let drawn = unsafe {
                DrawIconEx(
                    self.dc,
                    0,
                    0,
                    icon.0,
                    ICON_SIZE as i32,
                    ICON_SIZE as i32,
                    0,
                    0,
                    3,
                )
            };
            if drawn == 0 || unsafe { GdiFlush() } == 0 {
                return None;
            }
            Some(unsafe { std::slice::from_raw_parts(self.pixels, length) }.to_vec())
        }
    }

    impl Drop for Canvas {
        fn drop(&mut self) {
            unsafe {
                if self.previous != 0 {
                    SelectObject(self.dc, self.previous);
                }
                if self.bitmap != 0 {
                    DeleteObject(self.bitmap);
                }
                DeleteDC(self.dc);
            }
        }
    }

    pub(crate) fn extract_icon(path: &Path) -> Option<Vec<u8>> {
        let _apartment = ComApartment::enter()?;
        let name = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut info: ShellFileInfo = unsafe { std::mem::zeroed() };
        let mut pidl = ItemId(ptr::null_mut());
        let (source, flags) = if path.to_string_lossy().starts_with("shell:AppsFolder\\") {
            let mut attributes = 0;
            let status = unsafe {
                SHParseDisplayName(
                    name.as_ptr(),
                    ptr::null_mut(),
                    &mut pidl.0,
                    0,
                    &mut attributes,
                )
            };
            if status < 0 || pidl.0.is_null() {
                return None;
            }
            (pidl.0.cast::<u16>() as *const u16, 0x108)
        } else {
            (name.as_ptr(), 0x100)
        };
        let found = unsafe {
            SHGetFileInfoW(
                source,
                0,
                &mut info,
                std::mem::size_of::<ShellFileInfo>() as u32,
                flags,
            )
        };
        let icon = ShellIcon(info.icon);
        if found == 0 || icon.0 == 0 {
            return None;
        }
        let mut canvas = Canvas::new()?;
        let black = canvas.draw(&icon, 0)?;
        let white = canvas.draw(&icon, 255)?;
        rgba_from_backgrounds(&black, &white)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_opaque_icon_keeps_its_color_when_gdi_has_no_alpha() {
        assert_eq!(
            rgba_from_backgrounds(&[10, 20, 200, 0], &[10, 20, 200, 0]),
            Some(vec![200, 20, 10, 255])
        );
    }

    #[test]
    fn windows_masked_icon_background_stays_transparent() {
        assert_eq!(
            rgba_from_backgrounds(
                &[0, 0, 0, 0, 10, 20, 200, 0],
                &[255, 255, 255, 0, 10, 20, 200, 0]
            ),
            Some(vec![0, 0, 0, 0, 200, 20, 10, 255])
        );
    }

    #[test]
    fn windows_alpha_blended_icon_recovers_straight_color() {
        assert_eq!(
            rgba_from_backgrounds(&[0, 0, 64, 128], &[127, 127, 191, 255]),
            Some(vec![128, 0, 0, 128])
        );
    }

    #[test]
    fn windows_icon_output_has_one_rgba_pixel_per_input_pixel() {
        let black = vec![0; ICON_SIZE * ICON_SIZE * 4];
        let white = vec![0; black.len()];
        assert_eq!(
            rgba_from_backgrounds(&black, &white).unwrap().len(),
            black.len()
        );
    }

    #[test]
    fn windows_empty_shell_icon_is_rejected() {
        assert!(rgba_from_backgrounds(&[0, 0, 0, 0], &[255, 255, 255, 0]).is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_native_shell_extracts_an_icon_without_powershell() {
        let serial = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("corvo-ícono-{}-{serial}.ico", std::process::id()));
        let pixel_bytes = (ICON_SIZE * ICON_SIZE * 4) as u32;
        let mask_bytes = (ICON_SIZE * 4) as u32;
        let mut fixture = vec![0, 0, 1, 0, 1, 0, 32, 32, 0, 0, 1, 0, 32, 0];
        fixture.extend_from_slice(&(40 + pixel_bytes + mask_bytes).to_le_bytes());
        fixture.extend_from_slice(&22_u32.to_le_bytes());
        fixture.extend_from_slice(&40_u32.to_le_bytes());
        fixture.extend_from_slice(&(ICON_SIZE as i32).to_le_bytes());
        fixture.extend_from_slice(&((ICON_SIZE * 2) as i32).to_le_bytes());
        fixture.extend_from_slice(&1_u16.to_le_bytes());
        fixture.extend_from_slice(&32_u16.to_le_bytes());
        fixture.extend_from_slice(&0_u32.to_le_bytes());
        fixture.extend_from_slice(&pixel_bytes.to_le_bytes());
        fixture.extend_from_slice(&[0; 16]);
        fixture.extend(std::iter::repeat_n([0, 0, 255, 255], ICON_SIZE * ICON_SIZE).flatten());
        fixture.extend(std::iter::repeat_n(0, mask_bytes as usize));
        std::fs::write(&path, fixture).unwrap();
        let pixels = extract_icon(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(pixels.len(), ICON_SIZE * ICON_SIZE * 4);
        assert!(pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
    }
}

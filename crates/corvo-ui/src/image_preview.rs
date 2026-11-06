//! Decoding for clipboard image previews.
//!
//! GPUI caches every decoded image for the life of the process when it
//! loads a file path. A clipboard screenshot is several megapixels, so a
//! preview drawn straight from the file keeps tens of megabytes resident
//! after the window that showed it is gone. This module decodes the PNG
//! down to the size the preview card can draw, and the caller passes the
//! result as an in-memory image, which bypasses that cache.

use std::path::Path;
use std::sync::Arc;

use gpui::RenderImage;

/// The preview card is at most 420 x 240 logical points. Twice that
/// covers a Retina display without keeping the full screenshot.
const PREVIEW_MAX_WIDTH: u32 = 840;
const PREVIEW_MAX_HEIGHT: u32 = 480;

/// Decodes `path` and scales it down to the preview card bounds.
///
/// Returns `None` when the file is not a readable PNG, or when it has no
/// pixels. Never upscales.
pub fn load_clipboard_preview(path: &Path) -> Option<Arc<RenderImage>> {
    let source = image::open(path).ok()?.into_rgba8();
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    let scale = (PREVIEW_MAX_WIDTH as f32 / width as f32)
        .min(PREVIEW_MAX_HEIGHT as f32 / height as f32)
        .min(1.0);
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);

    let mut rgba = if target_width == width && target_height == height {
        source
    } else {
        image::imageops::resize(
            &source,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        )
    };

    // GPUI draws premultiplied BGRA.
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }

    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_elem(
        image::Frame::new(rgba),
        1,
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_missing_file() {
        assert!(load_clipboard_preview(Path::new("/no/such/corvo-preview.png")).is_none());
    }

    #[test]
    fn scales_a_screenshot_down_to_the_card_bounds() {
        let path = std::env::temp_dir().join(format!(
            "corvo-preview-test-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = image::RgbaImage::from_pixel(2940, 1912, image::Rgba([10, 20, 30, 255]));
        image::save_buffer(
            &path,
            source.as_raw(),
            source.width(),
            source.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();

        let preview = load_clipboard_preview(&path).expect("preview should decode");
        let size = preview.size(0);
        let _ = std::fs::remove_file(&path);

        assert!(size.width.0 <= PREVIEW_MAX_WIDTH as i32, "{size:?}");
        assert!(size.height.0 <= PREVIEW_MAX_HEIGHT as i32, "{size:?}");
        // 2940 x 1912 hits the height bound before the width bound.
        assert_eq!(size.width.0, 738);
        assert_eq!(size.height.0, PREVIEW_MAX_HEIGHT as i32);
    }
}

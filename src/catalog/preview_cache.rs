//! On-disk demosaic preview cache for Develop mode.
//!
//! Backed by [`cacache-sync`]: content-addressable, integrity-checked,
//! concurrent-safe key/value storage under `{catalog_dir}/Previews/`.
//!
//! Values are JPEG (quality 92) at the develop preview resolution
//! ([`crate::develop::PREVIEW_MAX_DIM`]).

use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::develop::{PreviewImage, PreviewSource};

const PREVIEWS_DIR: &str = "Previews";
const JPEG_QUALITY: u8 = 92;

/// Root directory of the demosaic preview cache.
pub fn cache_dir(catalog_dir: &Path) -> PathBuf {
    catalog_dir.join(PREVIEWS_DIR)
}

fn cache_key(photo_id: i64) -> String {
    format!("demosaic/{photo_id}")
}

/// Load a cached demosaic preview. Returns `None` on miss or corruption
/// (cacache verifies integrity; bad entries are treated as misses).
pub fn load_preview(catalog_dir: &Path, photo_id: i64) -> Option<PreviewImage> {
    let dir = cache_dir(catalog_dir);
    let key = cache_key(photo_id);
    let data = cacache_sync::read(&dir, &key).ok()?;
    decode_jpeg_preview(&data)
}

/// Encode `img` as JPEG and store it under `photo_id`.
///
/// Only call for final develop images (demosaic / decoder preview).
pub fn save_preview(
    catalog_dir: &Path,
    photo_id: i64,
    img: &PreviewImage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let jpeg = encode_jpeg_preview(img)?;
    let dir = cache_dir(catalog_dir);
    let key = cache_key(photo_id);
    cacache_sync::write(&dir, &key, &jpeg)?;
    Ok(())
}

/// Drop the index entry for `photo_id` (orphaned content is left for
/// cacache; it will not be returned by future key lookups).
pub fn remove_preview(catalog_dir: &Path, photo_id: i64) {
    let dir = cache_dir(catalog_dir);
    let key = cache_key(photo_id);
    let _ = cacache_sync::remove(&dir, &key);
}

fn encode_jpeg_preview(
    img: &PreviewImage,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let rgba = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone())
        .ok_or("invalid preview dimensions")?;
    let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
    let mut buf = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    encoder.encode(
        &rgb,
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(buf)
}

fn decode_jpeg_preview(data: &[u8]) -> Option<PreviewImage> {
    let reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    let img = reader.decode().ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(PreviewImage {
        width,
        height,
        rgba: rgba.into_raw(),
        source: PreviewSource::CachedPreview,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_image(w: u32, h: u32) -> PreviewImage {
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let x = (i % w) as u8;
                let y = (i / w) as u8;
                [x, y, 64, 255]
            })
            .collect();
        PreviewImage {
            width: w,
            height: h,
            rgba,
            source: PreviewSource::Demosaic,
        }
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let img = sample_image(64, 48);
        save_preview(dir.path(), 7, &img).expect("save");
        let loaded = load_preview(dir.path(), 7).expect("load");
        assert_eq!(loaded.width, 64);
        assert_eq!(loaded.height, 48);
        assert_eq!(loaded.source, PreviewSource::CachedPreview);
        assert_eq!(loaded.rgba.len(), 64 * 48 * 4);
    }

    #[test]
    fn miss_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_preview(dir.path(), 99).is_none());
    }

    #[test]
    fn remove_drops_entry() {
        let dir = tempfile::tempdir().unwrap();
        let img = sample_image(16, 16);
        save_preview(dir.path(), 3, &img).unwrap();
        assert!(load_preview(dir.path(), 3).is_some());
        remove_preview(dir.path(), 3);
        assert!(load_preview(dir.path(), 3).is_none());
    }

    #[test]
    fn overwrite_replaces() {
        let dir = tempfile::tempdir().unwrap();
        save_preview(dir.path(), 1, &sample_image(8, 8)).unwrap();
        save_preview(dir.path(), 1, &sample_image(32, 16)).unwrap();
        let loaded = load_preview(dir.path(), 1).unwrap();
        assert_eq!((loaded.width, loaded.height), (32, 16));
    }
}

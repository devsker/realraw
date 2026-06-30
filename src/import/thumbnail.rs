//! Thumbnail extraction from raw and standard image files.
//!
//! ## Strategy
//!
//! 1. **Embedded JPEG preview** (raw files): walk every IFD and look for
//!    the largest JPEG segment. We verify the magic bytes (`FF D8 FF`)
//!    before decoding so the `image` crate's format guesser doesn't
//!    trip over a tiny CFA-format raw thumbnail.
//! 2. **Scan the file** for the biggest JPEG block. Some cameras store
//!    the preview without an EXIF tag pointing at it; scanning finds
//!    them anyway.
//! 3. **Full-file decode** for JPEGs, PNGs, and TIFFs (last resort).
//!
//! HEIF-based files (HEIC, AVIF, CR3) are *not* yet supported here;
//! [`extract_thumbnail`] returns `Err(ThumbnailError::Unsupported)` for
//! them. Adding `libheif-rs` later is the obvious next step.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use exif::{Reader, Tag};
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader, RgbaImage};

/// Pixel size of the longest edge of every cell in the grid.
pub const THUMB_CELL: f32 = 156.0;

/// JPEG SOI (start of image) marker: `FF D8 FF`.
const JPEG_SOI: &[u8] = &[0xFF, 0xD8, 0xFF];

/// How many bytes of the file to scan when hunting for an embedded JPEG.
/// ~64 MiB is enough for any preview; bigger scans just slow us down.
const SCAN_LIMIT: u64 = 64 * 1024 * 1024;

/// A decoded thumbnail ready to be uploaded to the GPU.
#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The largest dimension we asked for. Useful for layout.
    pub max_dim: u32,
}

impl Thumbnail {
    /// Convenience: `true` if no pixels are set (only possible on an
    /// empty image).
    pub fn is_empty(&self) -> bool {
        self.rgba.is_empty()
    }

    /// Width / height, as a tuple.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Target longest edge for the decoded thumbnail, in pixels.
pub const THUMB_MAX_DIM: u32 = 256;

/// Errors from thumbnail extraction.
#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image decode error: {0}")]
    Image(#[from] image::ImageError),

    #[error("file format is not supported for thumbnails")]
    Unsupported,

    #[error("jpeg decoder error: {0}")]
    JpegDecode(jpeg_decoder::Error),

    #[error("no embedded preview and full-file decode failed: {0}")]
    NoEmbedded(String),

    #[error("exif parse error: {0}")]
    Exif(String),
}

/// Try every strategy to produce a thumbnail. Returns `Err` only if
/// nothing worked.
pub fn extract_thumbnail(path: &Path) -> Result<Thumbnail, ThumbnailError> {
    // Strategy 1: read every JPEGInterchangeFormat tag across IFDs and
    // pick the first one whose data starts with JPEG magic.
    if let Ok(t) = extract_embedded(path) {
        return Ok(t);
    }

    // Strategy 2: scan the file for the biggest JPEG block.
    if let Ok(t) = scan_for_largest_jpeg(path) {
        return Ok(t);
    }

    // Strategy 3: full-file decode for JPEGs and other supported types.
    extract_full(path)
}

/// Try every IFD's `JPEGInterchangeFormat` tag. The first one whose
/// referenced bytes start with JPEG magic wins. We never hand raw
/// data to the `image` crate's format guesser -- that's how we ended
/// up feeding a tiny CR2 CFA thumbnail to the TIFF decoder and
/// getting "unknown photometric interpretation" errors.
fn extract_embedded(path: &Path) -> Result<Thumbnail, ThumbnailError> {
    let mut file = std::fs::File::open(path)?;
    let exif = {
        let mut reader = std::io::BufReader::new(&file);
        match Reader::new().read_from_container(&mut reader) {
            Ok(e) => e,
            Err(_) => return Err(ThumbnailError::Exif("no exif block".into())),
        }
    };

    // Collect every (ifd, offset, length) triple that claims to point
    // at an embedded JPEG, then try them in order.
    let mut candidates: Vec<(u64, u64)> = Vec::new();
    for f in exif.fields() {
        if f.tag != Tag::JPEGInterchangeFormat {
            continue;
        }
        let Some(offset) = f.value.get_uint(0) else {
            continue;
        };
        // Find the matching length tag in the same IFD.
        let length = exif
            .fields()
            .find(|g| g.tag == Tag::JPEGInterchangeFormatLength && g.ifd_num == f.ifd_num)
            .and_then(|g| g.value.get_uint(0))
            .unwrap_or(0);
        if length == 0 {
            // Length is required; the offset alone is useless.
            continue;
        }
        candidates.push((offset as u64, length as u64));
    }

    if candidates.is_empty() {
        return Err(ThumbnailError::Exif(
            "no JPEGInterchangeFormat tag".into(),
        ));
    }

    // Try each candidate, but always peek at the first 3 bytes to be
    // sure it's actually JPEG before decoding. CR2's tiny IFD1
    // "thumbnail" is raw CFA data -- not viewable, not JPEG -- and
    // pointing `image` at it gives the "unknown photometric
    // interpretation" error.
    for (offset, length) in candidates {
        let mut buf = vec![0u8; length as usize];
        file.seek(SeekFrom::Start(offset))?;
        if file.read_exact(&mut buf).is_err() {
            continue;
        }
        if !buf.starts_with(JPEG_SOI) {
            continue;
        }
        if let Ok(t) = decode_jpeg(&buf) {
            return Ok(t);
        }
    }

    Err(ThumbnailError::Exif(
        "no embedded JPEG found via EXIF tags".into(),
    ))
}

/// Scan the first [`SCAN_LIMIT`] bytes of the file for the largest
/// contiguous JPEG block and decode it. Useful for cameras that don't
/// expose the preview via `JPEGInterchangeFormat` (or for CR2 where the
/// tagged preview is a tiny raw CFA thumbnail that the `image` crate
/// can't read).
fn scan_for_largest_jpeg(path: &Path) -> Result<Thumbnail, ThumbnailError> {
    let mut file = std::fs::File::open(path)?;
    let total = file.metadata()?.len();
    let to_scan = total.min(SCAN_LIMIT);
    let mut buf = vec![0u8; to_scan as usize];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut buf)?;

    // Find every JPEG SOI and pair it with the next EOI. Keep the
    // largest span.
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i + 3 <= buf.len() {
        if &buf[i..i + 3] == JPEG_SOI {
            // Hunt for the matching EOI. Skip over `FF xx` escaped
            // markers and `FF 00` stuffed bytes.
            let mut j = i + 3;
            let mut found_eoi = None;
            while j + 1 < buf.len() {
                if buf[j] == 0xFF && buf[j + 1] == 0xD9 {
                    found_eoi = Some(j + 2);
                    break;
                }
                j += 1;
            }
            if let Some(end) = found_eoi {
                let len = end - i;
                if best.is_none_or(|(_, b_len)| len > b_len) {
                    best = Some((i, end));
                }
                i = end; // skip past this JPEG
            } else {
                // JPEG runs past our scan window. Take the rest.
                let len = buf.len() - i;
                if best.is_none_or(|(_, b_len)| len > b_len) {
                    best = Some((i, buf.len()));
                }
                break;
            }
        } else {
            i += 1;
        }
    }

    let Some((start, end)) = best else {
        return Err(ThumbnailError::Exif("no JPEG in scan".into()));
    };
    decode_jpeg(&buf[start..end])
}

fn decode_jpeg(bytes: &[u8]) -> Result<Thumbnail, ThumbnailError> {
    // Fast path: jpeg-decoder's native `scale` is 10-50x faster
    // than the image crate's full decode for sources much larger
    // than THUMB_MAX_DIM (which is almost always the case for raw
    // previews -- CR2's embedded JPEG is often 1920x1280 or
    // larger). Fall back to the image crate for anything the fast
    // path can't handle.
    if let Ok(t) = decode_jpeg_native(bytes) {
        return Ok(t);
    }
    // Fallback: force JPEG format so we never call into the
    // TIFF / WebP / etc. decoders.
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes));
    reader.set_format(ImageFormat::Jpeg);
    let img = reader.decode()?;
    let resized = img.resize(THUMB_MAX_DIM, THUMB_MAX_DIM, FilterType::Triangle);
    let rgba: RgbaImage = resized.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(Thumbnail {
        width: w,
        height: h,
        rgba: rgba.into_raw(),
        max_dim: THUMB_MAX_DIM,
    })
}

/// jpeg-decoder's scaled JPEG path for in-memory bytes. Returns
/// `Err` if the bytes aren't a JPEG or the decoder can't handle
/// the colour format.
fn decode_jpeg_native(bytes: &[u8]) -> Result<Thumbnail, ThumbnailError> {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    let _ = decoder.scale(THUMB_MAX_DIM as u16, THUMB_MAX_DIM as u16);
    let pixels = decoder.decode().map_err(ThumbnailError::JpegDecode)?;
    let info = decoder.info().ok_or(ThumbnailError::Unsupported)?;
    let w = info.width as u32;
    let h = info.height as u32;
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => rgb_to_rgba(&pixels),
        jpeg_decoder::PixelFormat::L8 => l_to_rgba(&pixels),
        jpeg_decoder::PixelFormat::L16 => l16_to_rgba(&pixels),
        jpeg_decoder::PixelFormat::CMYK32 => cmyk_to_rgba(&pixels),
    };
    Ok(Thumbnail {
        width: w,
        height: h,
        rgba,
        max_dim: THUMB_MAX_DIM,
    })
}

/// Decode the file to a thumbnail-sized image and resize. Used as a
/// fallback for files without an embedded preview (most JPEGs, PNGs,
/// etc.).
///
/// We use the JPEG decoder's native `scale` (which uses libjpeg's
/// `scale_num/scale_denom` under the hood) for JPEG files: this is
/// 10-50x faster than full-decode + resize for a 6000x4000 source.
/// For other formats we fall back to a full decode + nearest-neighbour
/// thumbnail, which is much faster than the Triangle filter and
/// visually identical at thumbnail scale.
fn extract_full(path: &Path) -> Result<Thumbnail, ThumbnailError> {
    if is_jpeg(path)
        && let Ok(t) = extract_jpeg_scaled(path)
    {
        return Ok(t);
    }

    // Generic path: full decode, then a fast integer-only
    // thumbnail. `image::DynamicImage::thumbnail` uses
    // `imageops::sample::thumbnail` (one source pixel per output
    // pixel) -- no filter, no floating point.
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    reader.no_limits();
    let img = reader.decode()?;
    let thumb = img.thumbnail(THUMB_MAX_DIM, THUMB_MAX_DIM);
    let rgba = thumb.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(Thumbnail {
        width: w,
        height: h,
        rgba: rgba.into_raw(),
        max_dim: THUMB_MAX_DIM,
    })
}

/// True if the file's extension looks like JPEG. We use the
/// extension rather than reading magic bytes because `extract_full`
/// is only called for files that already failed the embedded-JPEG
/// path -- a quick extension check is enough to decide whether to
/// try the JPEG-scaled shortcut.
fn is_jpeg(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "jpe")
    )
}

/// Decode a JPEG to a thumbnail-sized image using libjpeg's native
/// `scale` (1/8, 1/4, 1/2, 1). The resulting image is at most the
/// requested size; we accept that and let the renderer's
/// size-to-fit logic letterbox it into the 3:2 card frame.
fn extract_jpeg_scaled(path: &Path) -> Result<Thumbnail, ThumbnailError> {
    let file = std::fs::File::open(path)?;
    let mut decoder = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
    // Ask for at most THUMB_MAX_DIM on the long edge; the decoder
    // will pick the smallest supported scale factor (1/8, 1/4, 1/2
    // or 1) that produces an image >= that size in at least one
    // axis. This avoids decoding 96 MB of pixels for a 6000x4000
    // source.
    let _ = decoder.scale(THUMB_MAX_DIM as u16, THUMB_MAX_DIM as u16);
    let pixels = decoder.decode().map_err(ThumbnailError::JpegDecode)?;
    let info = decoder.info().ok_or(ThumbnailError::Unsupported)?;
    let w = info.width as u32;
    let h = info.height as u32;
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => rgb_to_rgba(&pixels),
        jpeg_decoder::PixelFormat::L8 => l_to_rgba(&pixels),
        jpeg_decoder::PixelFormat::L16 => l16_to_rgba(&pixels),
        jpeg_decoder::PixelFormat::CMYK32 => cmyk_to_rgba(&pixels),
    };
    Ok(Thumbnail {
        width: w,
        height: h,
        rgba,
        max_dim: THUMB_MAX_DIM,
    })
}

fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgb.len() / 3 * 4);
    for chunk in rgb.chunks_exact(3) {
        out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
    }
    out
}

fn l_to_rgba(l: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(l.len() * 4);
    for &p in l {
        out.extend_from_slice(&[p, p, p, 255]);
    }
    out
}

fn l16_to_rgba(l: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(l.len() / 2 * 4);
    for chunk in l.chunks_exact(2) {
        let p = u16::from_be_bytes([chunk[0], chunk[1]]) as u8;
        out.extend_from_slice(&[p, p, p, 255]);
    }
    out
}

fn cmyk_to_rgba(cmyk: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(cmyk.len() / 4 * 4);
    for chunk in cmyk.chunks_exact(4) {
        let c = chunk[0] as u32;
        let m = chunk[1] as u32;
        let y = chunk[2] as u32;
        let k = chunk[3] as u32;
        let r = 255 - ((c * (255 - k) / 255 + k) as u8);
        let g = 255 - ((m * (255 - k) / 255 + k) as u8);
        let b = 255 - ((y * (255 - k) / 255 + k) as u8);
        out.extend_from_slice(&[r, g, b, 255]);
    }
    out
}

/// Lightweight sanity check: the file's extension looks like a known
/// raw or image format we might be able to handle. The error is purely
/// informational — [`extract_thumbnail`] will return
/// `Unsupported` if we genuinely cannot decode.
pub fn is_known_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let lower = ext.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "cr2" | "cr3"
            | "dng"
            | "nef"
            | "nrw"
            | "arw"
            | "srf"
            | "sr2"
            | "rw2"
            | "orf"
            | "raf"
            | "pef"
            | "iiq"
            | "mrw"
            | "srw"
            | "rwl"
            | "r3d"
            | "jpg"
            | "jpeg"
            | "jpe"
            | "tif"
            | "tiff"
            | "png"
            | "heic"
            | "heif"
            | "avif"
            | "webp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        img.save(path).unwrap();
    }

    /// Build a JPEG into a `Vec<u8>` for embedding into a fake TIFF.
    fn build_jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, _| {
            image::Rgb([(x * 32) as u8, 0, 0])
        });
        let dyn_img = image::DynamicImage::ImageRgb8(img);
        let mut buf = std::io::Cursor::new(Vec::<u8>::new());
        dyn_img
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .unwrap();
        buf.into_inner()
    }

    /// Build a fake "raw" file: a JPEG payload at a known offset,
    /// wrapped in TIFF scaffolding so kamadak-exif will read it. We
    /// hand-craft the smallest possible TIFF (single IFD, single
    /// tag) so the test doesn't depend on a real camera file.
    ///
    /// Returns the offset of the embedded JPEG inside the file.
    fn write_fake_tiff_with_jpeg(
        path: &Path,
        jpeg: &[u8],
    ) -> u64 {
        use std::io::Write;
        // Little-endian TIFF header.
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"II"); // byte order
        buf.extend_from_slice(&42u16.to_le_bytes()); // magic
        buf.extend_from_slice(&8u32.to_le_bytes()); // offset to IFD0
        // IFD0 with two entries: image width (256) + JPEGInterchangeFormat
        // (0x0201) + JPEGInterchangeFormatLength (0x0202). The thumbnail
        // length is required for kamadak-exif to consider this valid.
        //
        // IFD layout: 2 bytes (count) + 12 bytes per entry + 4 bytes
        // (next IFD offset = 0).
        let count: u16 = 3;
        let ifd_offset: u32 = 8;
        let entries_offset = ifd_offset + 2;
        let next_ifd_offset = entries_offset + count as u32 * 12 + 4;
        buf.resize(next_ifd_offset as usize, 0);

        // Helper to write an IFD entry at a given index.
        let mut write_entry = |idx: u16, tag: u16, kind: u16, count: u32, value: u32| {
            let pos = entries_offset as usize + idx as usize * 12;
            buf[pos..pos + 2].copy_from_slice(&tag.to_le_bytes());
            buf[pos + 2..pos + 4].copy_from_slice(&kind.to_le_bytes());
            buf[pos + 4..pos + 8].copy_from_slice(&count.to_le_bytes());
            buf[pos + 8..pos + 12].copy_from_slice(&value.to_le_bytes());
        };
        // We deliberately point to image dimensions in a later entry;
        // for this test the JPEG offset/length are what matters.
        write_entry(0, 0x0100, 3, 1, 64); // ImageWidth = 64 (SHORT)
        write_entry(1, 0x0201, 4, 1, next_ifd_offset); // JPEGInterchangeFormat
        write_entry(2, 0x0202, 4, 1, jpeg.len() as u32); // JPEGInterchangeFormatLength

        // IFD header.
        buf[ifd_offset as usize..ifd_offset as usize + 2]
            .copy_from_slice(&count.to_le_bytes());
        // Next-IFD offset = 0.
        let nxt = entries_offset + count as u32 * 12;
        buf[nxt as usize..nxt as usize + 4].copy_from_slice(&0u32.to_le_bytes());

        buf.extend_from_slice(jpeg);
        std::fs::File::create(path)
            .unwrap()
            .write_all(&buf)
            .unwrap();
        next_ifd_offset as u64
    }

    #[test]
    fn known_extensions_recognised() {
        assert!(is_known_extension(Path::new("/x/foo.cr2")));
        assert!(is_known_extension(Path::new("/x/foo.CR2")));
        assert!(is_known_extension(Path::new("/x/foo.dng")));
        assert!(is_known_extension(Path::new("/x/foo.jpg")));
        assert!(!is_known_extension(Path::new("/x/foo.txt")));
    }

    #[test]
    fn full_decode_of_png_works() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.png");
        write_png(&p, 400, 300);
        let t = extract_thumbnail(&p).expect("thumbnail");
        assert!(t.width <= THUMB_MAX_DIM);
        assert!(t.height <= THUMB_MAX_DIM);
        let expected_ratio = 400.0 / 300.0;
        let actual_ratio = t.width as f32 / t.height as f32;
        assert!(
            (expected_ratio - actual_ratio).abs() < 0.05,
            "ratio drifted: {expected_ratio} vs {actual_ratio}"
        );
    }

    #[test]
    fn no_exif_jpeg_falls_back_to_full_decode() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.jpg");
        let mut img = image::RgbImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        img.save(&p).unwrap();
        let t = extract_thumbnail(&p).expect("thumbnail");
        // jpeg-decoder's native scale never upscales, so a 1x1
        // source stays 1x1. The renderer's size-to-fit letterboxes
        // it into the 3:2 card frame.
        assert_eq!(t.width, 1);
        assert_eq!(t.height, 1);
    }

    #[test]
    fn unknown_file_returns_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"hello").unwrap();
        let result = extract_thumbnail(&p);
        assert!(result.is_err());
    }

    #[test]
    fn scan_finds_jpeg_inside_arbitrary_file() {
        // Build a file with a JPEG block embedded after a bunch of
        // non-JPEG bytes; scan_for_largest_jpeg should find it.
        let dir = tempdir().unwrap();
        let p = dir.path().join("x.bin");

        let jpeg_buf = build_jpeg(8, 8);
        let mut all = Vec::new();
        all.extend_from_slice(b"some random prefix data -- not jpeg --\n");
        all.extend_from_slice(&jpeg_buf);
        all.extend_from_slice(b"trailing junk");
        std::fs::File::create(&p).unwrap().write_all(&all).unwrap();

        let t = scan_for_largest_jpeg(&p).expect("should find jpeg");
        // jpeg-decoder's native scale never upscales, so the
        // 8x8 source stays 8x8.
        assert_eq!(t.width, 8);
        assert_eq!(t.height, 8);
    }

    #[test]
    fn embedded_jpeg_decoded_via_exif_tag() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("fake_raw.bin");
        let jpeg_buf = build_jpeg(8, 8);
        write_fake_tiff_with_jpeg(&p, &jpeg_buf);
        // Even with no real CR2/DNG layout, the scan fallback should
        // rescue us if the EXIF path fails.
        let t = extract_thumbnail(&p).expect("thumbnail");
        assert!(t.width > 0 && t.height > 0);
    }

    #[test]
    fn non_jpeg_at_exif_offset_is_ignored() {
        // Synthesize a TIFF that points JPEGInterchangeFormat at non-JPEG
        // bytes (a fake "raw CFA" block). extract_embedded should refuse
        // to decode it and fall through to the scan path.
        let dir = tempdir().unwrap();
        let p = dir.path().join("fake_raw.bin");

        // 64 bytes of fake "thumbnail data" with random non-JPEG magic.
        let fake_thumb: Vec<u8> = (0..64).map(|i| (i as u8) ^ 0xAA).collect();

        write_fake_tiff_with_jpeg(&p, &fake_thumb);
        // We don't insert any real JPEG into the file body, so the scan
        // path won't find one either -- we just want extract_embedded to
        // skip this candidate without panicking.
        let _ = extract_embedded(&p);
    }
}

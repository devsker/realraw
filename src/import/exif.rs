//! EXIF / IPTC / XMP / capture metadata extraction.
//!
//! Backed by [`kamadak_exif`], a pure-Rust parser that handles JPEG, TIFF,
//! HEIF (CR3), PNG, and WebP directly. Because every mainstream raw
//! format (CR2, NEF, ARW, DNG, RW2, ORF, PEF, RAF, ...) uses a TIFF
//! container, this crate also covers them: we hand it the raw file and it
//! walks the TIFF IFD chain.
//!
//! CRW (Canon CIFF) is *not* supported; the file is silently treated as
//! having no EXIF data.

use std::io::BufReader;
use std::path::Path;

use exif::{Exif, In, Rational, Reader, Tag, Value};

use crate::catalog::PhotoInsert;

/// Capture metadata read from a file. Every field is optional because
/// partial EXIF (e.g. an image stripped of maker notes) is the norm, not
/// the exception.
#[derive(Debug, Clone, Default)]
pub struct ExifData {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub date_taken: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<f64>,
    pub iso: Option<i64>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub gps_altitude: Option<f64>,
    pub copyright: Option<String>,
    pub file_format: Option<String>,
}

impl ExifData {
    /// Apply this data to a `PhotoInsert`, overwriting any existing
    /// values with `Some`.
    pub fn apply_to(&self, p: &mut PhotoInsert) {
        p.width = p.width.or(self.width);
        p.height = p.height.or(self.height);
        p.orientation = p.orientation.or(self.orientation);
        p.date_taken = p.date_taken.or(self.date_taken);
        p.camera_make = p.camera_make.clone().or_else(|| self.camera_make.clone());
        p.camera_model = p.camera_model.clone().or_else(|| self.camera_model.clone());
        p.lens = p.lens.clone().or_else(|| self.lens.clone());
        p.focal_length = p.focal_length.or(self.focal_length);
        p.aperture = p.aperture.or(self.aperture);
        p.shutter_speed = p.shutter_speed.or(self.shutter_speed);
        p.iso = p.iso.or(self.iso);
        p.gps_lat = p.gps_lat.or(self.gps_lat);
        p.gps_lon = p.gps_lon.or(self.gps_lon);
        p.gps_altitude = p.gps_altitude.or(self.gps_altitude);
        p.copyright = p.copyright.clone().or_else(|| self.copyright.clone());
        p.file_format = p.file_format.clone().or_else(|| self.file_format.clone());
    }
}

/// Extract EXIF data from `path`. Returns `Ok(ExifData::default())` if
/// the file has no EXIF block (we don't want import to fail on
/// metadata-less files).
pub fn extract_exif(path: &Path) -> Result<ExifData, ExifError> {
    let file = std::fs::File::open(path).map_err(ExifError::Io)?;
    let mut reader = BufReader::new(file);
    let exif = match Reader::new().read_from_container(&mut reader) {
        Ok(e) => e,
        Err(_) => return Ok(ExifData::default()),
    };
    let mut out = ExifData::default();
    copy_fields(&exif, &mut out);
    Ok(out)
}

fn copy_fields(exif: &Exif, out: &mut ExifData) {
    if let Some(f) = exif.get_field(Tag::ImageWidth, In::PRIMARY) {
        out.width = f.value.get_uint(0).map(|v| v as i64).or_else(|| {
            // Some files store ImageWidth as a Rational (e.g. 72/1 dpi).
            if let Value::Rational(r) = &f.value {
                r.first().map(|r: &Rational| r.to_f64() as i64)
            } else {
                None
            }
        });
    }
    if let Some(f) = exif.get_field(Tag::ImageLength, In::PRIMARY) {
        out.height = f.value.get_uint(0).map(|v| v as i64).or_else(|| {
            if let Value::Rational(r) = &f.value {
                r.first().map(|r: &Rational| r.to_f64() as i64)
            } else {
                None
            }
        });
    }
    if let Some(f) = exif.get_field(Tag::Orientation, In::PRIMARY) {
        out.orientation = f.value.get_uint(0).map(|v| v as i64);
    }
    if let Some(f) = exif.get_field(Tag::DateTimeOriginal, In::PRIMARY) {
        out.date_taken = parse_exif_datetime(f.display_value().to_string().as_str());
    } else if let Some(f) = exif.get_field(Tag::DateTime, In::PRIMARY) {
        out.date_taken = parse_exif_datetime(f.display_value().to_string().as_str());
    }
    if let Some(f) = exif.get_field(Tag::Make, In::PRIMARY) {
        out.camera_make = Some(trim_exif_string(f.display_value().to_string().as_str()));
    }
    if let Some(f) = exif.get_field(Tag::Model, In::PRIMARY) {
        out.camera_model = Some(trim_exif_string(f.display_value().to_string().as_str()));
    }
    if let Some(f) = exif.get_field(Tag::LensModel, In::PRIMARY) {
        out.lens = Some(trim_exif_string(f.display_value().to_string().as_str()));
    } else if let Some(f) = exif.get_field(Tag::LensMake, In::PRIMARY) {
        out.lens = Some(trim_exif_string(f.display_value().to_string().as_str()));
    }
    if let Some(f) = exif.get_field(Tag::FocalLength, In::PRIMARY) {
        out.focal_length = rational_first(&f.value).map(|r| r.to_f64()).filter(|v| v.is_finite());
    }
    if let Some(f) = exif.get_field(Tag::FNumber, In::PRIMARY) {
        out.aperture = rational_first(&f.value).map(|r| r.to_f64()).filter(|v| v.is_finite());
    }
    if let Some(f) = exif.get_field(Tag::ExposureTime, In::PRIMARY) {
        out.shutter_speed =
            rational_first(&f.value).map(|r| r.to_f64()).filter(|v| v.is_finite());
    }
    if let Some(f) = exif.get_field(Tag::PhotographicSensitivity, In::PRIMARY) {
        out.iso = f.value.get_uint(0).map(|v| v as i64);
    } else if let Some(f) = exif.get_field(Tag::ISOSpeed, In::PRIMARY) {
        out.iso = f.value.get_uint(0).map(|v| v as i64);
    }
    if let Some(f) = exif.get_field(Tag::Copyright, In::PRIMARY) {
        out.copyright = Some(trim_exif_string(f.display_value().to_string().as_str()));
    }

    // GPS is split across multiple tags. Combine lat/lon in a single place.
    out.gps_lat = gps_coordinate(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, 'N', 'S');
    out.gps_lon = gps_coordinate(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, 'E', 'W');
    if let Some(f) = exif.get_field(Tag::GPSAltitude, In::PRIMARY) {
        out.gps_altitude =
            rational_first(&f.value).map(|r| r.to_f64()).filter(|v| v.is_finite());
    }
}

fn rational_first(v: &Value) -> Option<Rational> {
    if let Value::Rational(r) = v {
        r.first().copied()
    } else if let Value::SRational(r) = v {
        // We treat SRational as a positive rational for our needs.
        let first = r.first()?;
        Some(Rational {
            num: first.num.unsigned_abs(),
            denom: first.denom.unsigned_abs(),
        })
    } else {
        None
    }
}

fn gps_coordinate(
    exif: &Exif,
    coord_tag: Tag,
    ref_tag: Tag,
    pos: char,
    neg: char,
) -> Option<f64> {
    let coord = exif.get_field(coord_tag, In::PRIMARY)?;
    let r = if let Value::Rational(v) = &coord.value { v } else { return None };
    if r.len() < 3 {
        return None;
    }
    let deg = r[0].to_f64();
    let min = r[1].to_f64();
    let sec = r[2].to_f64();
    let mut v = deg + min / 60.0 + sec / 3600.0;
    if let Some(hemi) = exif.get_field(ref_tag, In::PRIMARY) {
        let s = hemi.display_value().to_string();
        if s.trim().starts_with(neg) {
            v = -v;
        } else if !s.trim().starts_with(pos) {
            // Unknown hemisphere character: leave the value positive.
        }
    }
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

/// Parse an EXIF `YYYY:MM:DD HH:MM:SS` string into a Unix timestamp.
/// EXIF timestamps are stored in local time without a timezone; we
/// treat them as UTC for sorting purposes.
fn parse_exif_datetime(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let year: i32 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: u32 = s.get(11..13)?.parse().ok()?;
    let minute: u32 = s.get(14..16)?.parse().ok()?;
    let second: u32 = s.get(17..19)?.parse().ok()?;
    let (y, m, d) = civil_from_ymd(year, month, day)?;
    let days = days_from_civil(y, m, d);
    let secs = days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64;
    Some(secs)
}

/// Trim trailing null bytes and surrounding whitespace that EXIF strings
/// love to carry around.
fn trim_exif_string(s: &str) -> String {
    s.trim().trim_end_matches('\0').trim().to_string()
}

/// Howard Hinnant's date algorithm: days from Unix epoch (1970-01-01).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m = m as i32;
    let d = d as i32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u32;
    era as i64 * 146097 + doe as i64 - 719468
}

fn civil_from_ymd(y: i32, m: u32, d: u32) -> Option<(i32, u32, u32)> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// Errors from EXIF extraction. We collapse most failures into a
/// "no EXIF" return so partial success is the norm.
#[derive(Debug, thiserror::Error)]
pub enum ExifError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exif_datetime_basic() {
        // 2023-08-15 14:30:00
        let ts = parse_exif_datetime("2023:08:15 14:30:00").unwrap();
        // 2023-08-15 is day 19584 since 1970-01-01.
        // 14*3600 + 30*60 = 52200
        assert_eq!(ts, 19584 * 86_400 + 52_200);
    }

    #[test]
    fn trim_exif_string_strips_nulls() {
        assert_eq!(trim_exif_string("Canon\0"), "Canon");
        assert_eq!(trim_exif_string("  EOS R5  "), "EOS R5");
    }

    #[test]
    fn days_from_civil_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn days_from_civil_known_date() {
        // 2000-01-01 is day 10957 since epoch.
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
    }

    #[test]
    fn apply_to_preserves_existing_values() {
        let mut p = PhotoInsert {
            iso: Some(800),
            ..Default::default()
        };
        let d = ExifData {
            iso: Some(200),
            camera_make: Some("Canon".to_string()),
            ..Default::default()
        };
        d.apply_to(&mut p);
        assert_eq!(p.iso, Some(800)); // pre-existing kept
        assert_eq!(p.camera_make.as_deref(), Some("Canon"));
    }
}

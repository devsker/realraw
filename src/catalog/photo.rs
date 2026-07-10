//! Photo rows in the catalog.
//!
//! [`Photo`] is a read model (the data we get back from queries).
//! [`PhotoInsert`] is the data the import pipeline hands to the
//! catalog to be persisted.

use std::path::Path;

use rusqlite::{params_from_iter, Connection, Row};
use time::OffsetDateTime;

use crate::catalog::{Catalog, Result};

/// A row read from the `photos` table. Fields that may be unknown (EXIF,
/// path stats) are `Option`.
#[derive(Debug, Clone, Default)]
pub struct Photo {
    pub id: i64,
    pub folder_id: Option<i64>,
    pub path: String,
    pub file_size: Option<i64>,
    pub mtime: Option<i64>,
    pub sha1: Option<Vec<u8>>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub imported_at: i64,
    pub rating: i64,
    pub pick_flag: i64,
    pub color_label: i64,

    // V002
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
    pub file_extension: Option<String>,
    pub error: Option<String>,

    // V003
    pub thumbnail_status: i64,
}

impl Photo {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            folder_id: row.get("folder_id")?,
            path: row.get("path")?,
            file_size: row.get("file_size")?,
            mtime: row.get("mtime")?,
            sha1: row.get("sha1")?,
            width: row.get("width")?,
            height: row.get("height")?,
            imported_at: row.get("imported_at")?,
            rating: row.get("rating")?,
            pick_flag: row.get("pick_flag")?,
            color_label: row.get("color_label")?,
            orientation: row.get("orientation")?,
            date_taken: row.get("date_taken")?,
            camera_make: row.get("camera_make")?,
            camera_model: row.get("camera_model")?,
            lens: row.get("lens")?,
            focal_length: row.get("focal_length")?,
            aperture: row.get("aperture")?,
            shutter_speed: row.get("shutter_speed")?,
            iso: row.get("iso")?,
            gps_lat: row.get("gps_lat")?,
            gps_lon: row.get("gps_lon")?,
            gps_altitude: row.get("gps_altitude")?,
            copyright: row.get("copyright")?,
            file_format: row.get("file_format")?,
            file_extension: row.get("file_extension")?,
            error: row.get("error")?,
            thumbnail_status: row.get("thumbnail_status")?,
        })
    }
}

/// Data the import pipeline feeds to the catalog. Mirrors the columns of the
/// `photos` table; anything we don't have is `None`.
#[derive(Debug, Clone, Default)]
pub struct PhotoInsert {
    pub path: String,
    pub folder_id: Option<i64>,
    pub file_size: Option<i64>,
    pub mtime: Option<i64>,
    pub sha1: Option<Vec<u8>>,
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
    pub file_extension: Option<String>,
    pub error: Option<String>,
    /// Star rating from XMP (`None` → keep DB default 0).
    pub rating: Option<i64>,
    /// Color label id from XMP (`None` → keep DB default 0).
    pub color_label: Option<i64>,
    /// Keywords from XMP `dc:subject` (applied after upsert).
    pub keywords: Vec<String>,
}

impl Catalog {
    /// Build a `PhotoInsert` from an absolute path, filling in `path`,
    /// `file_size`, `mtime`, and `file_extension` from the filesystem.
    pub fn photo_insert_from_path(path: &Path) -> Result<PhotoInsert> {
        let path_str = path.to_string_lossy().into_owned();
        let meta = std::fs::metadata(path).ok();
        let file_size = meta.as_ref().map(|m| m.len() as i64);
        let mtime = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let file_extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_ascii_lowercase()));
        let file_format = file_extension
            .as_deref()
            .map(|e| e.trim_start_matches('.').to_ascii_uppercase());
        Ok(PhotoInsert {
            path: path_str,
            folder_id: None,
            file_size,
            mtime,
            file_extension,
            file_format,
            ..Default::default()
        })
    }

    /// `current_time()` used to stamp `imported_at`.
    pub fn now() -> i64 {
        OffsetDateTime::now_utc().unix_timestamp()
    }

    /// Insert or update a photo row, keyed by absolute path. Returns the
    /// photo id. When `p.keywords` is non-empty, replaces the photo's
    /// keyword links.
    pub fn upsert_photo(&self, p: &PhotoInsert) -> Result<i64> {
        let conn = self.pool.get()?;
        upsert_photo(&conn, p, Self::now())
    }

    /// Keywords attached to a photo (ordered by name).
    pub fn photo_keywords(&self, photo_id: i64) -> Result<Vec<String>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT k.name FROM keywords k \
             JOIN photo_keywords pk ON pk.keyword_id = k.id \
             WHERE pk.photo_id = ?1 \
             ORDER BY k.name",
        )?;
        let rows = stmt.query_map([photo_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Bulk variant: a single transaction, much faster for large imports.
    /// Returns the number of rows written.
    pub fn upsert_photos(&self, items: &[PhotoInsert]) -> Result<usize> {
        let mut conn = self.pool.get()?;
        upsert_photos_in_tx(&mut conn, items, Self::now())
    }

    /// Look up a photo by its absolute path.
    pub fn find_photo_by_path(&self, path: &str) -> Result<Option<Photo>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM photos WHERE path = ?1")?;
        let mut rows = stmt.query([path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Photo::from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// List photos in the catalog. `limit` caps the number of rows
    /// returned (use `None` for everything). Sorted by capture date
    /// (falling back to import date so newly-imported photos without
    /// EXIF dates appear near the top), then id as a stable tiebreak.
    pub fn list_photos(&self, limit: Option<i64>) -> Result<Vec<Photo>> {
        let conn = self.pool.get()?;
        let sql = if let Some(lim) = limit {
            format!("SELECT * FROM photos ORDER BY COALESCE(date_taken, imported_at) DESC, id DESC LIMIT {lim}")
        } else {
            "SELECT * FROM photos ORDER BY COALESCE(date_taken, imported_at) DESC, id DESC".to_string()
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Photo::from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Re-read a single photo by id (after a status change, etc.).
    pub fn find_photo_by_id(&self, id: i64) -> Result<Option<Photo>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM photos WHERE id = ?1")?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Photo::from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Delete a photo by its id. Returns `true` if a row was actually
    /// removed. Foreign key cascades (`ON DELETE CASCADE`) handle
    /// `collection_photos` and `photo_keywords` automatically.
    pub fn delete_photo(&self, id: i64) -> Result<bool> {
        let conn = self.pool.get()?;
        let affected = conn.execute("DELETE FROM photos WHERE id = ?1", [id])?;
        Ok(affected > 0)
    }

    /// Set of absolute paths already in the catalog.
    pub fn existing_paths(&self) -> Result<std::collections::HashSet<String>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT path FROM photos")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = std::collections::HashSet::new();
        for r in rows {
            out.insert(r?);
        }
        Ok(out)
    }
}

const PHOTO_COLUMNS: &str = "\
    path, folder_id, file_size, mtime, sha1, width, height, \
    orientation, date_taken, camera_make, camera_model, lens, \
    focal_length, aperture, shutter_speed, iso, \
    gps_lat, gps_lon, gps_altitude, copyright, \
    file_format, file_extension, error, rating, color_label";

fn upsert_photo(conn: &Connection, p: &PhotoInsert, now: i64) -> Result<i64> {
    let rating = p.rating.unwrap_or(0);
    let color_label = p.color_label.unwrap_or(0);
    let sql = format!(
        "INSERT INTO photos (imported_at, {cols}) \
         VALUES (?1, {placeholders}) \
         ON CONFLICT(path) DO UPDATE SET \
            folder_id     = excluded.folder_id, \
            file_size     = excluded.file_size, \
            mtime         = excluded.mtime, \
            sha1          = excluded.sha1, \
            width         = excluded.width, \
            height        = excluded.height, \
            orientation   = excluded.orientation, \
            date_taken    = excluded.date_taken, \
            camera_make   = excluded.camera_make, \
            camera_model  = excluded.camera_model, \
            lens          = excluded.lens, \
            focal_length  = excluded.focal_length, \
            aperture      = excluded.aperture, \
            shutter_speed = excluded.shutter_speed, \
            iso           = excluded.iso, \
            gps_lat       = excluded.gps_lat, \
            gps_lon       = excluded.gps_lon, \
            gps_altitude  = excluded.gps_altitude, \
            copyright     = excluded.copyright, \
            file_format   = excluded.file_format, \
            file_extension= excluded.file_extension, \
            error         = excluded.error, \
            rating        = excluded.rating, \
            color_label   = excluded.color_label \
         RETURNING id",
        cols = PHOTO_COLUMNS,
        placeholders = repeat_placeholders(PHOTO_COLUMNS.split(',').count())
    );
    let params: Vec<&dyn rusqlite::ToSql> = vec![
        &now,
        &p.path,
        &p.folder_id,
        &p.file_size,
        &p.mtime,
        &p.sha1,
        &p.width,
        &p.height,
        &p.orientation,
        &p.date_taken,
        &p.camera_make,
        &p.camera_model,
        &p.lens,
        &p.focal_length,
        &p.aperture,
        &p.shutter_speed,
        &p.iso,
        &p.gps_lat,
        &p.gps_lon,
        &p.gps_altitude,
        &p.copyright,
        &p.file_format,
        &p.file_extension,
        &p.error,
        &rating,
        &color_label,
    ];
    let id: i64 = conn.query_row(&sql, params_from_iter(params), |r| r.get(0))?;
    if !p.keywords.is_empty() {
        set_photo_keywords(conn, id, &p.keywords)?;
    }
    Ok(id)
}

/// Replace the keyword set for `photo_id` with `keywords` (creates missing
/// keyword rows). Empty `keywords` clears all links.
fn set_photo_keywords(conn: &Connection, photo_id: i64, keywords: &[String]) -> Result<()> {
    conn.execute(
        "DELETE FROM photo_keywords WHERE photo_id = ?1",
        [photo_id],
    )?;
    for raw in keywords {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        conn.execute(
            "INSERT INTO keywords (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            [name],
        )?;
        let kid: i64 = conn.query_row(
            "SELECT id FROM keywords WHERE name = ?1",
            [name],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO photo_keywords (photo_id, keyword_id) VALUES (?1, ?2)",
            [photo_id, kid],
        )?;
    }
    Ok(())
}

fn upsert_photos_in_tx(conn: &mut Connection, items: &[PhotoInsert], now: i64) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut written = 0;
    for p in items {
        let _id = upsert_photo(&tx, p, now)?;
        written += 1;
    }
    tx.commit()?;
    Ok(written)
}

fn repeat_placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n * 2);
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push('?');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn photo_insert_from_path_populates_extension() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("foo.CR2");
        std::fs::write(&p, b"x").unwrap();
        let ins = Catalog::photo_insert_from_path(&p).unwrap();
        assert_eq!(ins.file_extension.as_deref(), Some(".cr2"));
        assert_eq!(ins.file_format.as_deref(), Some("CR2"));
        assert!(ins.path.ends_with("foo.CR2"));
        assert!(ins.file_size.is_some());
    }

    #[test]
    fn upsert_inserts_and_updates() {
        let dir = tempdir().unwrap();
        let cat = Catalog::create(&dir.path().join("cat.sqlite")).unwrap();
        let mut ins = PhotoInsert {
            path: "/photos/a.jpg".into(),
            ..Default::default()
        };
        ins.camera_make = Some("Canon".into());
        let id1 = cat.upsert_photo(&ins).unwrap();
        let count = cat.counts().unwrap().photos;
        assert_eq!(count, 1);
        ins.camera_make = Some("Nikon".into());
        let id2 = cat.upsert_photo(&ins).unwrap();
        assert_eq!(id1, id2, "upsert must return the same id");
        let row = cat.find_photo_by_path("/photos/a.jpg").unwrap().unwrap();
        assert_eq!(row.camera_make.as_deref(), Some("Nikon"));
    }

    #[test]
    fn upsert_stores_rating_and_keywords() {
        let dir = tempdir().unwrap();
        let cat = Catalog::create(&dir.path().join("cat.sqlite")).unwrap();
        let id = cat
            .upsert_photo(&PhotoInsert {
                path: "/photos/b.jpg".into(),
                rating: Some(3),
                color_label: Some(2),
                keywords: vec!["alpha".into(), "beta".into()],
                ..Default::default()
            })
            .unwrap();
        let row = cat.find_photo_by_id(id).unwrap().unwrap();
        assert_eq!(row.rating, 3);
        assert_eq!(row.color_label, 2);
        assert_eq!(cat.photo_keywords(id).unwrap(), vec!["alpha", "beta"]);
    }

    #[test]
    fn existing_paths_round_trip() {
        let dir = tempdir().unwrap();
        let cat = Catalog::create(&dir.path().join("cat.sqlite")).unwrap();
        cat.upsert_photo(&PhotoInsert {
            path: "/x/a.jpg".into(),
            ..Default::default()
        })
        .unwrap();
        cat.upsert_photo(&PhotoInsert {
            path: "/x/b.jpg".into(),
            ..Default::default()
        })
        .unwrap();
        let paths = cat.existing_paths().unwrap();
        assert!(paths.contains("/x/a.jpg"));
        assert!(paths.contains("/x/b.jpg"));
    }

    /// Stress test: many concurrent writers hitting the catalog
    /// should not produce "database is locked" errors, thanks to
    /// the per-catalog write mutex that serialises
    /// check-then-write sequences.
    #[test]
    fn concurrent_writers_serialize_via_write_lock() {
        use std::sync::Arc;
        use std::thread;

        let dir = tempdir().unwrap();
        let cat = Arc::new(Catalog::create(&dir.path().join("cat.sqlite")).unwrap());

        let threads = 8;
        let per_thread = 16;
        let mut handles = Vec::new();
        for t in 0..threads {
            let cat = cat.clone();
            handles.push(thread::spawn(move || {
                for i in 0..per_thread {
                    let _guard = cat.write_lock();
                    let path = format!("/stress/t{t:02}/img_{i:03}.jpg");
                    cat.upsert_photo(&PhotoInsert {
                        path,
                        ..Default::default()
                    })
                    .unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let count = cat.counts().unwrap().photos;
        assert_eq!(count, (threads * per_thread) as i64);
    }
}

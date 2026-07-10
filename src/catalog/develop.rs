//! Per-photo develop settings in the catalog.

use rusqlite::params;
use time::OffsetDateTime;

use crate::catalog::{Catalog, Result};
use crate::develop::DevelopSettings;

impl Catalog {
    /// Load develop settings for a photo. Returns defaults if none stored.
    pub fn get_develop(&self, photo_id: i64) -> Result<DevelopSettings> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT exposure, contrast, highlights, shadows, whites, blacks, \
                    clarity, vibrance, saturation, temp, tint \
             FROM photo_develop WHERE photo_id = ?1",
        )?;
        let mut rows = stmt.query([photo_id])?;
        if let Some(row) = rows.next()? {
            Ok(DevelopSettings {
                exposure: row.get(0)?,
                contrast: row.get(1)?,
                highlights: row.get(2)?,
                shadows: row.get(3)?,
                whites: row.get(4)?,
                blacks: row.get(5)?,
                clarity: row.get(6)?,
                vibrance: row.get(7)?,
                saturation: row.get(8)?,
                temp: row.get(9)?,
                tint: row.get(10)?,
            })
        } else {
            Ok(DevelopSettings::default())
        }
    }

    /// Insert or replace develop settings for a photo.
    pub fn set_develop(&self, photo_id: i64, s: &DevelopSettings) -> Result<()> {
        let conn = self.pool.get()?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        conn.execute(
            "INSERT INTO photo_develop (
                photo_id, exposure, contrast, highlights, shadows, whites, blacks,
                clarity, vibrance, saturation, temp, tint, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(photo_id) DO UPDATE SET
                exposure = excluded.exposure,
                contrast = excluded.contrast,
                highlights = excluded.highlights,
                shadows = excluded.shadows,
                whites = excluded.whites,
                blacks = excluded.blacks,
                clarity = excluded.clarity,
                vibrance = excluded.vibrance,
                saturation = excluded.saturation,
                temp = excluded.temp,
                tint = excluded.tint,
                updated_at = excluded.updated_at",
            params![
                photo_id,
                s.exposure as f64,
                s.contrast as f64,
                s.highlights as f64,
                s.shadows as f64,
                s.whites as f64,
                s.blacks as f64,
                s.clarity as f64,
                s.vibrance as f64,
                s.saturation as f64,
                s.temp as f64,
                s.tint as f64,
                now,
            ],
        )?;
        Ok(())
    }

    /// Delete develop settings for a photo (back to defaults).
    pub fn clear_develop(&self, photo_id: i64) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute("DELETE FROM photo_develop WHERE photo_id = ?1", [photo_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use crate::catalog::PhotoInsert;

    #[test]
    fn develop_round_trip() {
        let dir = tempdir().unwrap();
        let cat = Catalog::create(&dir.path().join("cat.sqlite")).unwrap();
        let id = cat
            .upsert_photo(&PhotoInsert {
                path: "/a.jpg".into(),
                ..Default::default()
            })
            .unwrap();

        assert!(cat.get_develop(id).unwrap().is_identity());

        let s = DevelopSettings {
            exposure: 1.25,
            contrast: 20.0,
            highlights: -30.0,
            shadows: 15.0,
            whites: 5.0,
            blacks: -10.0,
            clarity: 12.0,
            vibrance: 8.0,
            saturation: -5.0,
            temp: 10.0,
            tint: -3.0,
        };
        cat.set_develop(id, &s).unwrap();
        assert_eq!(cat.get_develop(id).unwrap(), s);

        cat.clear_develop(id).unwrap();
        assert!(cat.get_develop(id).unwrap().is_identity());
    }

    #[test]
    fn delete_photo_cascades_develop() {
        let dir = tempdir().unwrap();
        let cat = Catalog::create(&dir.path().join("cat.sqlite")).unwrap();
        let id = cat
            .upsert_photo(&PhotoInsert {
                path: "/b.jpg".into(),
                ..Default::default()
            })
            .unwrap();
        cat.set_develop(
            id,
            &DevelopSettings {
                exposure: 2.0,
                ..Default::default()
            },
        )
        .unwrap();
        cat.delete_photo(id).unwrap();
        // Row gone; get_develop on missing photo returns defaults (no row).
        assert!(cat.get_develop(id).unwrap().is_identity());
    }
}

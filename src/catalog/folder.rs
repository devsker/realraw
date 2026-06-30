//! Folder rows in the catalog.
//!
//! Each row represents a directory that contains (or has contained) one or
//! more photos. Folders are looked up by absolute path; ancestor folders
//! are created on demand so the import pipeline can always set
//! `photos.folder_id`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, Row};

use crate::catalog::{Catalog, Result};

/// A row read from the `folders` table.
#[derive(Debug, Clone)]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub parent_id: Option<i64>,
}

impl Folder {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            path: row.get("path")?,
            parent_id: row.get("parent_id")?,
        })
    }
}

impl Catalog {
    /// Look up a folder row by absolute path.
    pub fn find_folder_by_path(&self, path: &str) -> Result<Option<Folder>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM folders WHERE path = ?1")?;
        let mut rows = stmt.query([path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Folder::from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Find or create the folder row for `path`. All ancestors are
    /// inserted too so the folder tree stays consistent.
    pub fn ensure_folder(&self, path: &Path) -> Result<i64> {
        let mut conn = self.pool.get()?;
        ensure_folder(&mut conn, path)
    }

    /// Bulk variant: upsert a folder row for each unique path. Faster than
    /// calling [`ensure_folder`](Self::ensure_folder) in a loop because
    /// the ancestor walks happen inside one transaction.
    pub fn ensure_folders(&self, paths: &[PathBuf]) -> Result<()> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        for p in paths {
            ensure_folder_in_tx(&tx, p)?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn ensure_folder(conn: &mut Connection, path: &Path) -> Result<i64> {
    let tx = conn.transaction()?;
    let id = ensure_folder_in_tx(&tx, path)?;
    tx.commit()?;
    Ok(id)
}

fn ensure_folder_in_tx(conn: &Connection, path: &Path) -> Result<i64> {
    let path_str = path.to_string_lossy().into_owned();

    // Fast path: already there.
    if let Some(id) = lookup_folder(conn, &path_str)? {
        return Ok(id);
    }

    // Recursively insert ancestors first so parent_id resolves.
    let parent_id = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() && p != path => {
            Some(ensure_folder_in_tx(conn, p)?)
        }
        _ => None,
    };

    conn.execute(
        "INSERT INTO folders (path, parent_id) VALUES (?1, ?2) \
         ON CONFLICT(path) DO UPDATE SET parent_id = excluded.parent_id",
        rusqlite::params![&path_str, parent_id],
    )?;
    let id = lookup_folder(conn, &path_str)?
        .ok_or_else(|| crate::catalog::CatalogError::NotFound(path.to_path_buf()))?;
    Ok(id)
}

fn lookup_folder(conn: &Connection, path: &str) -> Result<Option<i64>> {
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM folders WHERE path = ?1",
            [path],
            |r| r.get(0),
        )
        .ok();
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ensure_folder_creates_ancestors() {
        let dir = tempdir().unwrap();
        let cat = Catalog::create(&dir.path().join("cat.sqlite")).unwrap();
        let deep = std::path::PathBuf::from("/photos/2024/jan/holiday");
        let id = cat.ensure_folder(&deep).unwrap();

        // Ancestors must exist too.
        for ancestor in ["/photos", "/photos/2024", "/photos/2024/jan"] {
            let f = cat.find_folder_by_path(ancestor).unwrap();
            assert!(f.is_some(), "ancestor {ancestor} missing");
        }

        // The leaf id must match the row we just inserted.
        let leaf = cat.find_folder_by_path("/photos/2024/jan/holiday").unwrap();
        assert_eq!(leaf.unwrap().id, id);
    }

    #[test]
    fn ensure_folder_is_idempotent() {
        let dir = tempdir().unwrap();
        let cat = Catalog::create(&dir.path().join("cat.sqlite")).unwrap();
        let p = std::path::PathBuf::from("/dup/path");
        let id1 = cat.ensure_folder(&p).unwrap();
        let id2 = cat.ensure_folder(&p).unwrap();
        assert_eq!(id1, id2);
    }
}

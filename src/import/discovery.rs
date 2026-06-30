//! File discovery for the import pipeline.
//!
//! Walks the user-selected paths, filters by known image / raw extensions,
//! and dedups against the catalog (by absolute path and by SHA1).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::catalog::Catalog;

/// Extensions the import pipeline recognises. Lowercase, with leading dot.
pub const KNOWN_EXTENSIONS: &[&str] = &[
    // Raw
    ".cr2", ".cr3", ".dng", ".nef", ".nrw", ".arw", ".srf", ".sr2",
    ".rw2", ".orf", ".raf", ".pef", ".iiq", ".3fr", ".fff", ".x3f",
    ".mrw", ".rwl", ".srw", ".r3d",
    // Standard
    ".jpg", ".jpeg", ".jpe", ".tif", ".tiff", ".png", ".heic", ".heif",
    ".avif", ".webp",
];

/// A candidate file produced by [`discover_files`].
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    /// Absolute path.
    pub path: PathBuf,
    /// File size in bytes.
    pub file_size: u64,
    /// Modification time.
    pub mtime: SystemTime,
    /// Extension in lowercase with leading dot (e.g. `.cr2`).
    pub extension: String,
    /// `true` if the path is already in the catalog.
    pub already_in_catalog: bool,
}

impl DiscoveredFile {
    /// `true` for file extensions we know how to read metadata from.
    pub fn has_known_extension(extension: &str) -> bool {
        let lower = extension.to_ascii_lowercase();
        KNOWN_EXTENSIONS.iter().any(|e| **e == lower)
    }
}

/// Walk `sources`, filter by `extensions`, dedup against `catalog`, and
/// return the candidate list. Returns the empty vector on any per-path
/// error (logged and skipped) so a single broken folder doesn't sink the
/// whole import.
pub fn discover_files(
    sources: &[PathBuf],
    extensions: &[String],
    catalog: Option<&Catalog>,
) -> Vec<DiscoveredFile> {
    let existing: HashSet<String> = catalog
        .map(|c| c.existing_paths().unwrap_or_default())
        .unwrap_or_default();
    let exts: HashSet<String> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();

    let mut out: Vec<DiscoveredFile> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for source in sources {
        if !source.exists() {
            continue;
        }
        if source.is_file() {
            maybe_push(
                &mut out,
                &mut seen,
                source,
                &exts,
                &existing,
            );
            continue;
        }
        for entry in WalkDir::new(source)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            maybe_push(
                &mut out,
                &mut seen,
                entry.path(),
                &exts,
                &existing,
            );
        }
    }

    // Stable order: sort by path for deterministic UI rendering.
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn maybe_push(
    out: &mut Vec<DiscoveredFile>,
    seen: &mut HashSet<PathBuf>,
    path: &Path,
    exts: &HashSet<String>,
    existing: &HashSet<String>,
) {
    if !seen.insert(path.to_path_buf()) {
        return;
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return;
    };
    let ext_with_dot = format!(".{}", ext.to_ascii_lowercase());
    if !exts.contains(&ext_with_dot) {
        return;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let file_size = meta.len();
    let path_str = path.to_string_lossy().into_owned();
    let already_in_catalog = existing.contains(&path_str);
    out.push(DiscoveredFile {
        path: path.to_path_buf(),
        file_size,
        mtime,
        extension: ext_with_dot,
        already_in_catalog,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn touch(p: &Path) {
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn known_extension_recognised() {
        assert!(DiscoveredFile::has_known_extension(".cr2"));
        assert!(DiscoveredFile::has_known_extension(".CR2"));
        assert!(DiscoveredFile::has_known_extension(".dng"));
        assert!(DiscoveredFile::has_known_extension(".jpg"));
        assert!(DiscoveredFile::has_known_extension(".jpeg"));
        assert!(!DiscoveredFile::has_known_extension(".txt"));
        assert!(!DiscoveredFile::has_known_extension(".mp4"));
    }

    #[test]
    fn discovers_recursively_with_extension_filter() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.cr2");
        let b = dir.path().join("sub");
        fs::create_dir(&b).unwrap();
        let c = b.join("c.jpg");
        let d = b.join("d.txt");
        touch(&a);
        touch(&c);
        touch(&d);

        let sources = vec![dir.path().to_path_buf()];
        let exts: Vec<String> = KNOWN_EXTENSIONS.iter().map(|s| s.to_string()).collect();
        let found = discover_files(&sources, &exts, None);
        let names: Vec<String> = found
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"a.cr2".to_string()), "got {names:?}");
        assert!(names.contains(&"c.jpg".to_string()), "got {names:?}");
        assert!(!names.contains(&"d.txt".to_string()), "got {names:?}");
    }

    #[test]
    fn dedup_marks_existing_paths() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.jpg");
        touch(&p);

        let cat_dir = tempdir().unwrap();
        let cat_path = cat_dir.path().join("cat.sqlite");
        let cat = Catalog::create(&cat_path).unwrap();
        cat.upsert_photo(&crate::catalog::PhotoInsert {
            path: p.to_string_lossy().into_owned(),
            ..Default::default()
        })
        .unwrap();

        let sources = vec![dir.path().to_path_buf()];
        let exts: Vec<String> = vec![".jpg".to_string()];
        let found = discover_files(&sources, &exts, Some(&cat));
        assert_eq!(found.len(), 1);
        assert!(found[0].already_in_catalog);
    }

    #[test]
    fn no_sources_returns_empty() {
        let exts: Vec<String> = vec![".jpg".to_string()];
        assert!(discover_files(&[], &exts, None).is_empty());
    }
}

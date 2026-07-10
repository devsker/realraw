//! Import pipeline: discovery → EXIF + XMP + thumbnail → DB upsert.
//!
//! The flow is split into stages that each expose a focused API:
//!
//! 1. [`discovery`] — walk a list of user-selected paths, filter by known
//!    extensions, dedup against the catalog, and return a
//!    [`DiscoveredFile`] for each candidate.
//! 2. [`exif`] — extract EXIF / capture metadata from one file into an
//!    [`ExifData`].
//! 3. [`xmp`] — read/write XMP sidecars (rating, labels, keywords, …)
//!    and pair them with image files.
//! 4. [`thumbnail`] — load an embedded JPEG preview (or fall back to a
//!    full-file decode) and return RGBA bytes ready for an egui texture.
//! 5. [`worker`] — the [`import_batch`] function: given a catalog and
//!    a list of paths, walk + dedup + hash + exif + xmp + upsert in
//!    background tasks, reporting progress through the
//!    [`TaskManager`](crate::task::TaskManager).
//!
//! The [`dialog`] module hosts the in-window import UI (thumbnail grid +
//! checkboxes + commit) that drives the pipeline.

pub mod dialog;
pub mod discovery;
pub mod exif;
pub mod thumbnail;
pub mod worker;
pub mod xmp;

pub use dialog::ImportDialog;
pub use discovery::{discover_files, DiscoveredFile, KNOWN_EXTENSIONS};
pub use exif::{extract_exif, ExifData};
pub use thumbnail::{extract_thumbnail, Thumbnail};
pub use worker::{import_batch, ImportSummary};
pub use xmp::{
    find_sidecar, parse_xmp_file, update_sidecar_develop, write_sidecar_for_image, write_xmp_file,
    XmpData,
};

/// Whether imported files should be copied or moved into the collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAction {
    Copy,
    Move,
}

/// What the user picked in the import dialog. Held by the dialog and the
/// app shell; the worker reads it when the user presses "Import".
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Files / folders the user chose in the file dialog.
    pub sources: Vec<std::path::PathBuf>,
    /// Restrict to these extensions (lowercase, with leading dot). Defaults
    /// to [`KNOWN_EXTENSIONS`].
    pub extensions: Vec<String>,
    /// Recurse into directories. Default: true.
    pub recursive: bool,
    /// Skip files that are already in the catalog (by path or by SHA1).
    /// Default: true.
    pub dedup: bool,
    /// Copy or move files into the collection directory.
    pub action: ImportAction,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            extensions: KNOWN_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            recursive: true,
            dedup: true,
            action: ImportAction::Copy,
        }
    }
}

use std::path::PathBuf;

/// All errors that can come out of the catalog layer.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("migration error: {0}")]
    Migration(#[from] refinery::Error),

    #[error("catalog file already exists: {0}")]
    AlreadyExists(PathBuf),

    #[error("catalog file does not exist: {0}")]
    NotFound(PathBuf),

    #[error("could not determine default catalog directory")]
    NoDefaultDir,
}

pub type Result<T> = std::result::Result<T, CatalogError>;

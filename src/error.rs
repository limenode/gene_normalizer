use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by the gene normalizer library.
#[derive(Debug, Error)]
pub enum Error {
    /// The species filter matched neither a known taxon id nor a species name.
    #[error("unknown species (not a known taxon or species name): {0}")]
    UnknownSpecies(String),

    /// The source TSV header is missing a column the cache build requires.
    #[error("column '{0}' not found in TSV header")]
    MissingColumn(String),

    /// No per-platform cache directory could be determined.
    #[error("could not determine a cache directory for this platform")]
    NoCacheDir,

    /// The cache directory could not be created.
    #[error("failed to create cache directory {path}: {source}")]
    CreateCacheDir {
        path: PathBuf,
        source: std::io::Error,
    },

    /// A filesystem operation on the cache failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// An underlying SQLite operation failed.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// A `Result` whose error is this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

use std::path::Path;

use rusqlite::Connection;

use crate::cache::{self, cache_db_path, gene_columns, load_cache};
use crate::error::Result;
use crate::types::GeneRecord;

/// An opened gene cache, ready for lookups.
///
/// This is the primary entry point for embedding the normalizer in another
/// program: it owns the SQLite connection and exposes normalization without
/// leaking `rusqlite` into the public API. Open it once and reuse it for many
/// lookups.
pub struct GeneNormalizer {
    conn: Connection,
}

impl GeneNormalizer {
    /// Open the cache at the default platform cache path (see
    /// [`cache_db_path`]), building it from the Alliance Genome data if absent.
    pub fn open_default() -> Result<Self> {
        let path = cache_db_path()?;
        Self::open(&path)
    }

    /// Open the cache at an explicit path, building it if absent.
    pub fn open(db_path: &Path) -> Result<Self> {
        Ok(Self {
            conn: load_cache(db_path)?,
        })
    }

    /// Normalize any number of aliases to gene records, in input order.
    ///
    /// Each alias maps to a `Vec<GeneRecord>`: empty for a miss, one entry for a
    /// unique hit, and more than one when the alias is ambiguous — across species
    /// (only possible when `species` is `None`) and/or across case-variants when
    /// `ignore_case` is set. See [`cache::lookup`] for the full contract.
    pub fn lookup(
        &self,
        aliases: &[&str],
        species: Option<&str>,
        ignore_case: bool,
    ) -> Result<Vec<(String, Vec<GeneRecord>)>> {
        cache::lookup(&self.conn, aliases, species, ignore_case)
    }

    /// The `genes` table columns, in order — i.e. the cached TSV header.
    pub fn columns(&self) -> Result<Vec<String>> {
        gene_columns(&self.conn)
    }

    /// Borrow the underlying connection, for callers that still need raw access
    /// (e.g. benchmarks). Prefer the methods above where possible.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

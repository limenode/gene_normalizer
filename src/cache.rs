use flate2::read::GzDecoder;
use reqwest::blocking::get;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use directories::ProjectDirs;

use crate::types::{GeneRecord, TsvStream};

/// Canonical on-disk location for the gene cache DB, following each platform's
/// conventions. Creates the parent directory if it doesn't yet exist.
pub fn cache_db_path() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("org", "AllianceGenome", "gene_normalizer").ok_or_else(|| {
        anyhow::anyhow!("could not determine a cache directory for this platform")
    })?;
    let dir = dirs.cache_dir();
    fs::create_dir_all(dir)
        .map_err(|e| anyhow::anyhow!("failed to create cache directory {}: {e}", dir.display()))?;
    Ok(dir.join("gene_cache.db"))
}

const ALLIANCE_GENOME_GENE_TSV_URL: &str =
    "https://download.alliancegenome.org/9.0.0/downloads/GENE_TSV_COMBINED.tsv.gz";

// Returns the header lines and an iterator over the data lines of the TSV file
pub fn stream_gene_tsv_lines() -> TsvStream {
    let response = get(ALLIANCE_GENOME_GENE_TSV_URL)
        .expect("Failed to download GENE_TSV_COMBINED.tsv.gz via HTTP");

    let gz_decoder = GzDecoder::new(response);
    let mut lines = BufReader::new(gz_decoder)
        .lines()
        .map(|line| line.expect("Failed to read line from GENE_TSV_COMBINED.tsv.gz"))
        .peekable();

    let metadata: Vec<String> =
        std::iter::from_fn(|| lines.next_if(|line| line.starts_with('#'))).collect();

    let header: Vec<String> = lines
        .next()
        .expect("Failed to read header line from GENE_TSV_COMBINED.tsv.gz")
        .split('\t')
        .map(str::to_owned)
        .collect();

    let rows: Box<dyn Iterator<Item = String>> = Box::new(lines);

    TsvStream {
        metadata,
        header,
        rows,
    }
}

pub fn build_cache(db_path: &Path) -> anyhow::Result<()> {
    let tsv = stream_gene_tsv_lines();

    let col = |name: &str| -> anyhow::Result<usize> {
        tsv.header
            .iter()
            .position(|col_name| col_name == name)
            .ok_or_else(|| anyhow::anyhow!("Column '{}' not found in TSV header", name))
    };

    let col_defs = tsv
        .header
        .iter()
        .map(|col_name| format!("{} TEXT", col_name))
        .collect::<Vec<String>>()
        .join(",\n");

    let id_idx = col("GeneId")?;
    let symbol_idx = col("GeneSymbol")?;
    let secondary_idx = col("GeneSecondaryIds")?;
    let taxon_idx = col("Taxon")?;
    let species_name_idx = col("SpeciesName")?;

    let mut conn = Connection::open(db_path)?;

    conn.execute_batch(&format!(
        r#"
        CREATE TABLE IF NOT EXISTS genes (
        {col_defs},
            primary key ("GeneId")
        ) WITHOUT ROWID;

        -- (alias, taxon) is the key so the same symbol/secondary id can resolve to a
        -- different gene per species. `alias` leads the PK so alias-only lookups still
        -- use the index (prefix scan); `alias = ? AND taxon = ?` is a point lookup.
        CREATE TABLE IF NOT EXISTS gene_aliases (
            alias TEXT NOT NULL,
            taxon TEXT NOT NULL,
            gene_id TEXT NOT NULL,
            PRIMARY KEY (alias, taxon)
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS species (
            taxon TEXT PRIMARY KEY,
            species_name TEXT NOT NULL
        ) WITHOUT ROWID;
        "#
    ))?;

    let quoted_cols = tsv
        .header
        .iter()
        .map(|col_name| format!("\"{col_name}\""))
        .collect::<Vec<String>>()
        .join(", ");

    let placeholders = (1..=tsv.header.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<String>>()
        .join(", ");

    let tx = conn.transaction()?;
    {
        let mut gene_stmt = tx.prepare(&format!(
            "INSERT OR IGNORE INTO genes ({quoted_cols}) VALUES ({placeholders})"
        ))?;

        let mut alias_stmt = tx.prepare(
            "INSERT OR IGNORE INTO gene_aliases (alias, taxon, gene_id) VALUES (?1, ?2, ?3)",
        )?;

        let mut species_stmt =
            tx.prepare("INSERT OR IGNORE INTO species (taxon, species_name) VALUES (?1, ?2)")?;

        for line in tsv.rows {
            let fields: Vec<&str> = line.split('\t').collect();
            let gene_id = fields[id_idx];
            let symbol = fields[symbol_idx];
            let secondary_ids = fields[secondary_idx];
            let taxon = fields[taxon_idx];
            let species_name = fields[species_name_idx];

            gene_stmt.execute(rusqlite::params_from_iter(fields.iter()))?;

            species_stmt.execute(params![taxon, species_name])?;

            alias_stmt.execute(params![symbol, taxon, gene_id])?; // symbol -> gene_id
            alias_stmt.execute(params![gene_id, taxon, gene_id])?; // gene_id -> gene_id

            // secondaries -> gene_id
            if !secondary_ids.is_empty() && secondary_ids != "None" {
                for sec_id in secondary_ids
                    .split('|')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    alias_stmt.execute(params![sec_id, taxon, gene_id])?;
                }
            }
        }
    }
    tx.commit()?;

    // Built after the bulk insert (cheaper than maintaining it per-row). Powers
    // case-insensitive (`COLLATE NOCASE`) alias lookups; the case-sensitive PK still
    // keeps case-distinct aliases (e.g. mouse `Gs` vs `gs`) as separate rows.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_gene_aliases_nocase
             ON gene_aliases (alias COLLATE NOCASE, taxon);",
    )?;

    Ok(())
}

/// Load the cache from the given path, building it if it doesn't exist.
///
/// The cache is a SQLite database built from the Alliance Genome GENE_TSV_COMBINED.tsv.gz
/// file, structured to support efficient lookups of gene records by symbol or ID,
/// optionally filtered by species.
pub fn load_cache(db_path: &Path) -> anyhow::Result<Connection> {
    if !db_path.exists() {
        let mut tmp_os = db_path.as_os_str().to_owned();
        tmp_os.push(".tmp");
        let tmp_path = PathBuf::from(tmp_os);
        let mut journal_os = tmp_path.as_os_str().to_owned();
        journal_os.push("-journal");

        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(PathBuf::from(journal_os));

        eprint!("Building cache...");
        build_cache(&tmp_path)?;
        fs::rename(&tmp_path, db_path)?;
        eprintln!(" done.");
    }
    Ok(Connection::open(db_path)?)
}

/// The column names of the `genes` table, in order — i.e. the cached TSV header.
/// Used to validate user-requested output fields against the actual schema.
pub fn gene_columns(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let stmt = conn.prepare("SELECT * FROM genes LIMIT 0")?;
    Ok(stmt.column_names().into_iter().map(String::from).collect())
}

/// Resolve a species filter (either a taxon id like `NCBITaxon:9606` or a species
/// name like `Homo sapiens`) to its canonical taxon id. Errors if it matches neither.
pub fn resolve_taxon(conn: &Connection, species: &str) -> anyhow::Result<String> {
    conn.query_row(
        "SELECT taxon FROM species WHERE taxon = ?1 OR species_name = ?1 LIMIT 1",
        params![species],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown species (not a known taxon or species name): {}",
            species
        )
    })
}

/// Look up any number of aliases, optionally restricted to a single species (given as a
/// taxon id or a species name).
///
/// Results echo the input order. Each alias maps to a `Vec<GeneRecord>`: empty for a
/// miss, one entry for a unique hit, and more than one when the alias is ambiguous —
/// across species (only possible when `species` is `None`), and/or across case-variants
/// when `ignore_case` is set (e.g. `gs` matching both `Gs` and `gs`).
///
/// With `ignore_case`, matching is ASCII-case-insensitive (`COLLATE NOCASE`).
///
/// The species is resolved to a taxon exactly once here, then the aliases are queried in
/// chunks (see `lookup_chunk`) to stay under SQLite's bound-parameter limit.
pub fn lookup(
    conn: &Connection,
    aliases: &[&str],
    species: Option<&str>,
    ignore_case: bool,
) -> anyhow::Result<Vec<(String, Vec<GeneRecord>)>> {
    if aliases.is_empty() {
        return Ok(Vec::new());
    }

    let taxon: Option<String> = match species {
        Some(s) => Some(resolve_taxon(conn, s)?),
        None => None,
    };

    aliases
        .chunks(999)
        .map(|chunk| lookup_chunk(conn, chunk, taxon.as_deref(), ignore_case))
        .try_fold(Vec::new(), |mut acc, chunk_result| {
            acc.extend(chunk_result?);
            Ok::<_, anyhow::Error>(acc)
        })
}

fn lookup_chunk(
    conn: &Connection,
    aliases: &[&str],
    taxon: Option<&str>,
    ignore_case: bool,
) -> rusqlite::Result<Vec<(String, Vec<GeneRecord>)>> {
    let placeholders = (1..=aliases.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<String>>()
        .join(", ");

    let alias_expr = if ignore_case {
        "a.alias COLLATE NOCASE"
    } else {
        "a.alias"
    };

    let mut sql = format!(
        "SELECT a.alias, g.* FROM genes g \
         JOIN gene_aliases a ON g.GeneId = a.gene_id \
         WHERE {alias_expr} IN ({placeholders})"
    );
    if taxon.is_some() {
        // Filter on the alias index's trailing column — a point lookup, not a post-join scan.
        sql.push_str(&format!(" AND a.taxon = ?{}", aliases.len() + 1));
    }

    let mut stmt = conn.prepare(&sql)?;

    let columns: Arc<[String]> = stmt
        .column_names()
        .into_iter()
        .skip(1)
        .map(String::from)
        .collect();

    // Bind the aliases, then the taxon filter (if any).
    let mut binds: Vec<&str> = aliases.to_vec();
    if let Some(t) = taxon {
        binds.push(t);
    }

    // Group results by the same key the SQL matched on: the exact alias, or its
    // ASCII-lowercased form to mirror SQLite's NOCASE folding.
    let key = |s: &str| {
        if ignore_case {
            s.to_ascii_lowercase()
        } else {
            s.to_string()
        }
    };

    let mut found: HashMap<String, Vec<GeneRecord>> = HashMap::new();
    let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), |row| {
        let alias: String = row.get(0)?;
        let count = row.as_ref().column_count();
        let values: rusqlite::Result<Vec<String>> = (1..count).map(|i| row.get(i)).collect();
        Ok((alias, values?))
    })?;

    for row in rows {
        let (alias, values) = row?;
        found.entry(key(&alias)).or_default().push(GeneRecord {
            columns: columns.clone(),
            values,
        });
    }

    Ok(aliases
        .iter()
        .map(|&alias| {
            (
                alias.to_string(),
                found.get(&key(alias)).cloned().unwrap_or_default(),
            )
        })
        .collect())
}

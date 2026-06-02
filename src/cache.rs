use flate2::read::GzDecoder;
use reqwest::blocking::get;
use rusqlite::{Connection, OptionalExtension, params};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::collections::HashMap;

use crate::types::TsvStream;

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

pub fn build_cache(db_path: &str) -> anyhow::Result<()> {
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

    let mut conn = Connection::open(db_path)?;

    conn.execute_batch(&format!(
        r#"
        CREATE TABLE IF NOT EXISTS genes (
        {col_defs},
            primary key ("GeneId")
        ) WITHOUT ROWID;


        CREATE TABLE IF NOT EXISTS gene_aliases (
            alias TEXT PRIMARY KEY,
            gene_id TEXT NOT NULL
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

        let mut alias_stmt =
            tx.prepare("INSERT OR IGNORE INTO gene_aliases (alias, gene_id) VALUES (?1, ?2)")?;

        for line in tsv.rows {
            let fields: Vec<&str> = line.split('\t').collect();
            let gene_id = fields[id_idx];
            let symbol = fields[symbol_idx];
            let secondary_ids = fields[secondary_idx];

            gene_stmt.execute(rusqlite::params_from_iter(fields.iter()))?;

            alias_stmt.execute(params![symbol, gene_id])?; // symbol -> gene_id
            alias_stmt.execute(params![gene_id, gene_id])?; // gene_id -> gene_id

            // secondaries -> gene_id
            if !secondary_ids.is_empty() && secondary_ids != "None" {
                for sec_id in secondary_ids
                    .split('|')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    alias_stmt.execute(params![sec_id, gene_id])?;
                }
            }
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn load_cache(db_path: &str) -> anyhow::Result<Connection> {
    if !Path::new(db_path).exists() {
        let tmp_path = format!("{}.tmp", db_path);
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(format!("{}-journal", tmp_path));
        build_cache(&tmp_path)?;
        fs::rename(&tmp_path, db_path)?;
    }
    Ok(Connection::open(db_path)?)
}

pub fn lookup(conn: &Connection, alias: &str) -> rusqlite::Result<Option<Vec<String>>> {
    conn.query_row(
        r#"
            SELECT g.* FROM genes g
            JOIN gene_aliases a ON g.GeneId = a.gene_id
            WHERE a.alias = ?1
            "#,
        params![alias],
        |row| {
            let count = row.as_ref().column_count();
            (0..count).map(|i| row.get(i)).collect()
        },
    )
    .optional()
}

pub fn lookup_many(
    conn: &Connection,
    aliases: &[&str],
) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    if aliases.is_empty() {
        return Ok(HashMap::new());
    }
    
    let placeholders = (1..=aliases.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<String>>()
        .join(", ");

    let sql = format!(
        r#"
        SELECT a.alias, g.* FROM genes g
        JOIN gene_aliases a ON g.GeneId = a.gene_id
        WHERE a.alias IN ({placeholders})
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut results = HashMap::new();

    let rows = stmt.query_map(rusqlite::params_from_iter(aliases.iter()), |row| {
        let alias: String = row.get(0)?;
        let count = row.as_ref().column_count();
        let gene: rusqlite::Result<Vec<String>> = (1..count).map(|i| row.get(i)).collect();
        Ok((alias, gene?))
    })?;

    for row in rows {
        let (alias, gene) = row?;
        results.insert(alias, gene);
    }

    return Ok(results);
}

pub fn lookup_many_chunked(
    conn: &Connection,
    aliases: &[&str],
) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    aliases.chunks(999)
        .map(|chunk| lookup_many(conn, chunk))
        .try_fold(HashMap::new(), |mut acc, chunk_result| {
            acc.extend(chunk_result?);
            Ok(acc)
        })
}
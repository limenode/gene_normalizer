# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Rust CLI that normalizes gene symbols/IDs to canonical gene records. It downloads
the Alliance of Genome Resources combined gene TSV (covering ~10 species), builds a
local SQLite cache, and resolves any alias (gene symbol, primary `GeneId`, or secondary
ID) to the full gene row. Lookups can be scoped to a single species, because the same
symbol/secondary ID can refer to different genes in different species.

## Commands

```bash
cargo build --release                          # build
cargo run                                      # interactive REPL (reads aliases from stdin)
cargo run -- --input genes.txt                 # batch lookup, one alias per line
cargo run -- --input genes.txt --species "Homo sapiens"   # scope to a species (name or taxon)
cargo run -- --species NCBITaxon:9606          # REPL scoped to a species
cargo run -- --input genes.txt --ignore-case   # case-insensitive matching (tp53 finds TP53)
cargo run --example inspect_tsv                # dev-only: dump the source TSV's header + first rows
cargo test                                     # run tests
cargo bench                                    # run criterion benchmarks (bench: lookup)
RUST_LOG=debug cargo run                       # enable logging (env_logger; internal logs are debug-level)
```

The `--species` flag accepts either a taxon id (`NCBITaxon:9606`) or a species name
(`Homo sapiens`); see `resolve_taxon`. Internal logging is off unless `RUST_LOG` is set.

## Architecture

The substance lives in `src/cache.rs`; `main.rs` is the CLI/REPL shell, `src/types.rs`
holds the shared data types (`TsvStream`, `GeneRecord`).

- **`stream_gene_tsv_lines()`** downloads the gzipped TSV over HTTP (blocking reqwest)
  and returns a `TsvStream`: `metadata` (lines starting with `#`), `header` (tab-split
  column names), and `rows` (a lazy `Box<dyn Iterator>`). The column schema is driven
  entirely by this header — table columns are derived from it at runtime, not hardcoded.
  This call downloads + gunzips the whole file, so it is used **only** by `build_cache`
  (and the `inspect_tsv` example), never on a normal cached run.

- **`build_cache(db_path)`** creates three SQLite tables (all `WITHOUT ROWID`):
  - `genes` — one column per TSV header field, primary key `GeneId` (globally unique).
  - `gene_aliases` — `(alias, taxon, gene_id)` with composite primary key
    `(alias, taxon)`. `alias` leads the key so alias-only lookups still use the index
    (prefix scan) while `alias = ? AND taxon = ?` is a point lookup. Each gene row
    inserts: `GeneSymbol -> GeneId`, `GeneId -> GeneId` (self), and each `|`-separated
    `GeneSecondaryIds` value -> `GeneId`, all tagged with the row's `Taxon`. The
    composite key is what lets the *same* symbol coexist across species; `INSERT OR
    IGNORE` only guards genuine within-species duplicates. A secondary index
    `idx_gene_aliases_nocase (alias COLLATE NOCASE, taxon)` is built *after* the bulk
    insert to serve case-insensitive lookups. The PK stays case-sensitive so case-distinct
    aliases (e.g. mouse `Gs` MGI:95839 vs `gs` MGI:95840 — both real, different genes)
    survive; a case-insensitive PK would silently drop one.
  - `species` — `(taxon PK, species_name)`, the ~10-row map used to resolve a species
    name to its taxon.

- **`load_cache(db_path)`** is the entry point: if the DB file is absent it builds into
  a `.tmp` file and atomically renames into place (so a partial/interrupted build never
  leaves a usable-looking DB). If the file exists it's opened as-is — **there is no
  staleness check or rebuild**; delete `gene_cache.db` to force a refresh. A DB built by
  an older schema version will NOT be migrated — delete and rebuild after schema changes.

- **Lookups** join `gene_aliases` to `genes` on `gene_id`, filtering by `alias` (and
  `taxon` when a species is given). Column names come from `stmt.column_names()` — the
  header is read from SQLite, never re-streamed from the TSV. Results are wrapped in
  `GeneRecord { columns: Arc<[String]>, values: Vec<String> }`: a positional `Vec` (cheap,
  like the raw row) paired with a *shared* header (`Arc` bump per record, not a copy), so
  each record is self-describing via `.get("Col")` / `.iter()` over `(col, value)` pairs.
  - `lookup(conn, aliases: &[&str], species, ignore_case: bool)` is **the** public lookup
    entry point (for one alias or many — single-alias callers pass a 1-element slice). It
    returns `Vec<(String, Vec<GeneRecord>)>` in **input order**; the inner `Vec` is empty
    for a miss, one entry for a unique hit, and >1 when an alias is ambiguous — across
    species (when `species` is `None`) and/or across case-variants (when `ignore_case`).
    It resolves the species to a taxon **once**, then delegates to the private
    `lookup_chunk`.
  - `lookup_chunk` (private) runs one query for up to 999 aliases with an already-resolved
    taxon — the 999 cap stays under SQLite's bound-parameter limit. Resolution lives in
    `lookup`, not here, so it happens once regardless of how many chunks the input spans.
    With `ignore_case` it matches via `alias COLLATE NOCASE IN (...)` (uses
    `idx_gene_aliases_nocase`) and groups results by the ASCII-lowercased alias — mirroring
    SQLite's NOCASE folding — so the input reunites with hits stored in different casing.
  - `resolve_taxon` (public utility) maps a taxon-or-name to a canonical taxon, erroring
    if unknown.
  - `lookup` / `resolve_taxon` return `anyhow::Result` (to surface unknown-species
    errors); `build_cache` / `load_cache` also use `anyhow`.

## Important constraints

- `gene_cache.db` is ~500 MB and is gitignored (`*.db`). It is rebuilt on first run if
  missing, which requires network access to download.alliancegenome.org.
- The cache path is hardcoded to `"gene_cache.db"` (relative to CWD) in `main.rs` and
  `benches/lookup.rs`.
- Dev-only diagnostics live in `examples/` (e.g. `inspect_tsv`), not in the library, so
  they never ship in the production binary.
- Edition 2024 — uses recent Rust; ensure a current toolchain.

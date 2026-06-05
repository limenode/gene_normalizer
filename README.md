# gene_normalizer

A fast, standalone command-line tool for normalizing gene symbols and IDs to
canonical gene records across multiple species.

`gene_normalizer` downloads the [Alliance of Genome Resources](https://www.alliancegenome.org/)
combined gene dataset, builds a local SQLite cache, and resolves any alias — a gene
symbol, a primary gene ID, or a secondary ID — to its full gene record. Because the same
symbol can refer to different genes in different organisms, lookups can be scoped to a
single species. It is designed to drop cleanly into a shell pipeline as a normalization
pre-processing step, or to be used interactively for one-off lookups.

## Features

* **Multi-species** — covers nine model organisms in a single dataset (see [Data Source](#data-source)).
* **Flexible input** — resolves gene symbols, primary `GeneId`s, and secondary IDs to the same canonical record.
* **Species scoping** — restrict lookups to one organism by taxon id (`NCBITaxon:9606`) or name (`Homo sapiens`) to disambiguate symbols shared across species.
* **Case-insensitive matching** — optional `--ignore-case` so `tp53` finds `TP53`.
* **Pipeline-friendly** — reads piped input automatically and writes clean tab-separated output, so it composes with `cut`, `sort`, `awk`, and friends.
* **Interactive mode** — a prompt for quick manual lookups when run in a terminal.
* **Local caching** — builds a SQLite cache on first run; subsequent lookups are fast and fully offline.

## Installation

**Build from source** (requires a recent [Rust toolchain](https://rustup.rs/), edition 2024):

```bash
git clone https://github.com/limenode/gene_normalizer.git
cd gene_normalizer
cargo build --release
```

The binary will be located at:

```text
./target/release/gene_normalizer
```

> **First run:** the tool downloads the Alliance gene dataset and builds a local cache
> (`gene_cache.db`, ~600 MB) in the current directory. This requires network access and
> may take a few minutes. Every run after that is offline and fast.

## Usage

`gene_normalizer` adapts to how you run it.

### Interactive lookups

Run it in a terminal with no input file to get an interactive prompt:

```bash
./gene_normalizer
```

```text
Enter a gene symbol/ID to lookup (or 'exit' to quit): BRCA1
BRCA1
  Taxon - NCBITaxon:9606
  SpeciesName - Homo sapiens
  GeneId - HGNC:1100
  GeneSymbol - BRCA1
  ...
```

### Batch lookups (files and pipelines)

Pass a file with one alias per line, or pipe aliases in on standard input:

```bash
# from a file
./gene_normalizer --input genes.txt

# from a pipeline (stdin is read automatically)
cut -f1 my_data.tsv | ./gene_normalizer
```

When the output is piped or redirected, results are written as **tab-separated values** —
one line per input — making them easy to feed into other tools:

```text
TP53      HGNC:11998   TP53
BRCA1     HGNC:1100    BRCA1
NOTAGENE
```

The first column echoes your input, so unmatched entries (like `NOTAGENE` above) still
produce a row and your output stays aligned with your input, line for line.

### Scoping to a species

The same symbol can mean different genes in different organisms. Restrict a lookup with
`--species`, using either a name or an NCBI taxon id:

```bash
./gene_normalizer --input genes.txt --species "Homo sapiens"
./gene_normalizer --input genes.txt --species NCBITaxon:10090   # Mus musculus
```

### Choosing what to output

By default the tab-separated output contains the input alias, the canonical `GeneId`, and
the `GeneSymbol`. You can change which columns are emitted:

```bash
# only the canonical ID
cat genes.txt | ./gene_normalizer --id-only

# only the official symbol
cat genes.txt | ./gene_normalizer --symbol-only

# pick specific columns, with a header row
cat genes.txt | ./gene_normalizer --fields GeneId,GeneSymbol,SpeciesName --header
```

### Common options

| Option | Description |
| --- | --- |
| `-i`, `--input <FILE>` | Read aliases from a file (one per line). Use `-` to force reading from stdin. |
| `-s`, `--species <NAME\|TAXON>` | Restrict lookups to one species, e.g. `"Homo sapiens"` or `NCBITaxon:9606`. |
| `--ignore-case` | Match aliases case-insensitively (`tp53` finds `TP53`). |
| `-o`, `--format <pretty\|tsv>` | Output format. Defaults to `pretty` in a terminal, `tsv` when piped. |
| `--fields <COLS>` | Comma-separated columns for `tsv` output (default `GeneId,GeneSymbol`). |
| `--id-only` / `--symbol-only` | Shorthand for `--fields GeneId` / `--fields GeneSymbol`. |
| `--no-echo` | Omit the leading column that echoes the input alias. |
| `--header` | Print a header row before tab-separated output. |
| `--rebuild-cache` | Clear and rebuild the local cache from the latest dataset before running. |

> **Tip — ambiguous matches:** if an alias maps to more than one gene (for example, the
> same symbol in several species, or case variants under `--ignore-case`), all matches are
> printed and a warning is written to standard error. Adding `--species` removes ambiguity
> that comes from a symbol being shared across organisms. Note that it does not collapse
> case-insensitive matches within a single species (e.g. mouse `Gs` and `gs`), so a lookup can
> still return several genes when `--ignore-case` is on.

## Output columns

Each record carries every field from the source dataset. The most useful columns include:

| Column | Description |
| --- | --- |
| `GeneId` | Canonical, globally unique gene identifier (e.g. `HGNC:1100`, `MGI:95839`). |
| `GeneSymbol` | Official gene symbol. |
| `SpeciesName` / `Taxon` | Source organism and its NCBI taxon id. |
| `GeneSynonyms` | Alternative names and symbols. |
| `GeneSecondaryIds` | Cross-referenced secondary identifiers (also resolvable as input). |
| `Chromosome`, `StartPosition`, `EndPosition`, `Strand` | Genomic location. |

Run a lookup in `pretty` format to see the full set of fields for any gene.

## Data Source

This tool uses the combined gene dataset published by the
[Alliance of Genome Resources](https://www.alliancegenome.org/), which consolidates gene
nomenclature from its member model-organism databases.

- Alliance website: https://www.alliancegenome.org/

The following URL is used by this program to retrieve the dataset:

* https://download.alliancegenome.org/9.0.0/downloads/GENE_TSV_COMBINED.tsv.gz

The dataset covers the following model organisms:

| Species | Taxon |
| --- | --- |
| *Homo sapiens* | `NCBITaxon:9606` |
| *Mus musculus* | `NCBITaxon:10090` |
| *Rattus norvegicus* | `NCBITaxon:10116` |
| *Danio rerio* | `NCBITaxon:7955` |
| *Drosophila melanogaster* | `NCBITaxon:7227` |
| *Caenorhabditis elegans* | `NCBITaxon:6239` |
| *Saccharomyces cerevisiae* S288C | `NCBITaxon:559292` |
| *Xenopus laevis* | `NCBITaxon:8355` |
| *Xenopus tropicalis* | `NCBITaxon:8364` |

### Updating the cache

The local cache (`gene_cache.db`) is built once and reused; there is no automatic staleness
check. To refresh it to the latest dataset, run the tool with `--rebuild-cache`, which
clears the existing cache and rebuilds it on startup:

```bash
./gene_normalizer --rebuild-cache --input genes.txt
```

Equivalently, you can delete the cache file manually and run the tool again — it is rebuilt
whenever it is missing:

```bash
rm gene_cache.db
./gene_normalizer --input genes.txt
```

## Citation / Attribution

If you use data retrieved with this tool in published work, please cite the Alliance of
Genome Resources (and the member databases being used when applicable).

See their website for current citation guidelines:

https://www.alliancegenome.org/cite-us

## Acknowledgements

This project is built using several excellent Rust crates, including:

- [`rusqlite`](https://github.com/rusqlite/rusqlite) for the bundled SQLite cache
- [`clap`](https://github.com/clap-rs/clap) for command-line parsing
- [`reqwest`](https://github.com/seanmonstar/reqwest) for HTTP requests
- [`flate2`](https://github.com/rust-lang/flate2-rs) for gzip decompression
- [`anyhow`](https://github.com/dtolnay/anyhow) for ergonomic error handling

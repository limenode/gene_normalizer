use std::io::{BufRead, IsTerminal, Read, Write};

use anyhow::{Context, bail};
use clap::{Parser, ValueEnum};
use gene_normalizer::cache::{cache_db_path, gene_columns, load_cache, lookup};
use gene_normalizer::types::GeneRecord;

#[derive(Parser, Debug)]
#[command(
    name = "gene_normalizer",
    about = "Gene symbol/ID normalization tool",
    long_about = None
)]
struct Cli {
    /// Input file with one alias per line; use "-" to read aliases from stdin.
    /// If omitted, aliases are read from stdin when it is piped, otherwise an
    /// interactive prompt is shown.
    #[arg(short, long)]
    input: Option<String>,

    /// Restrict lookups to one species, given as a taxon id
    /// (e.g. "NCBITaxon:9606") or species name (e.g. "Homo sapiens")
    #[arg(short, long)]
    species: Option<String>,

    /// Match aliases case-insensitively (e.g. "tp53" finds "TP53")
    #[arg(long)]
    ignore_case: bool,

    /// Output format. Defaults to "pretty" when stdout is a terminal and "tsv"
    /// when stdout is piped/redirected.
    #[arg(short = 'o', long, value_enum)]
    format: Option<Format>,

    /// Comma-separated columns to emit in tsv format (e.g. "GeneId,GeneSymbol").
    /// Defaults to "GeneId,GeneSymbol". Pretty format always shows the full record.
    #[arg(long, conflicts_with_all = ["id_only", "symbol_only"])]
    fields: Option<String>,

    /// Shorthand for `--fields GeneId`.
    #[arg(long, conflicts_with = "symbol_only")]
    id_only: bool,

    /// Shorthand for `--fields GeneSymbol`.
    #[arg(long)]
    symbol_only: bool,

    /// Drop the leading column that echoes the input alias (tsv format).
    #[arg(long)]
    no_echo: bool,

    /// Print a header row before tsv output.
    #[arg(long)]
    header: bool,

    /// Rebuild the cache from the latest Alliance of Genome Resources (AGR) data source
    #[arg(long)]
    rebuild_cache: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    /// Human-readable `column - value` blocks (full record).
    Pretty,
    /// Tab-separated, one line per match, selected fields only.
    Tsv,
}

/// Resolved output configuration shared by the batch and interactive paths.
struct OutputCfg {
    format: Format,
    fields: Vec<String>,
    echo: bool,
    header: bool,
}

fn main() {
    if let Err(e) = run() {
        // A downstream reader closing the pipe (e.g. `... | head`) is normal for a
        // pipeline tool: exit quietly like grep/cut rather than printing an error.
        if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
            if io_err.kind() == std::io::ErrorKind::BrokenPipe {
                std::process::exit(0);
            }
        }
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    env_logger::init();

    let args = Cli::parse();

    let db_path = cache_db_path()?;

    let rebuild_cache = args.rebuild_cache;
    if rebuild_cache {
        std::fs::remove_file(&db_path).ok();
    }

    let conn = load_cache(&db_path).context("Failed to load cache")?;
    let species = args.species.as_deref();
    let ignore_case = args.ignore_case;

    // stdout TTY decides the *default* output format; stdin TTY decides whether
    // we prompt interactively or read a piped batch.
    let format = args.format.unwrap_or_else(|| {
        if std::io::stdout().is_terminal() {
            Format::Pretty
        } else {
            Format::Tsv
        }
    });

    // Resolve which gene columns the tsv format should emit, then validate them
    // against the actual cached schema
    let fields: Vec<String> = if args.id_only {
        vec!["GeneId".to_string()]
    } else if args.symbol_only {
        vec!["GeneSymbol".to_string()]
    } else if let Some(spec) = &args.fields {
        spec.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    } else {
        vec!["GeneId".to_string(), "GeneSymbol".to_string()]
    };

    if format == Format::Tsv {
        let valid = gene_columns(&conn)?;
        for f in &fields {
            if !valid.contains(f) {
                bail!(
                    "Unknown field '{}'. Available columns: {}",
                    f,
                    valid.join(", ")
                );
            }
        }
    }

    let cfg = OutputCfg {
        format,
        fields,
        echo: !args.no_echo,
        header: args.header,
    };

    // Decide the input source. Explicit --input wins; "-" and a piped stdin both
    // mean "batch-read stdin"; otherwise fall back to the interactive prompt.
    let stdin_is_tty = std::io::stdin().is_terminal();
    match args.input.as_deref() {
        Some("-") => run_batch(&conn, read_stdin_aliases()?, species, ignore_case, &cfg),
        Some(path) => {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read input file '{}'", path))?;
            run_batch(&conn, parse_aliases(&content), species, ignore_case, &cfg)
        }
        None if !stdin_is_tty => {
            run_batch(&conn, read_stdin_aliases()?, species, ignore_case, &cfg)
        }
        None => run_repl(&conn, species, ignore_case, &cfg),
    }
}

fn parse_aliases(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

fn read_stdin_aliases() -> anyhow::Result<Vec<String>> {
    let mut buf = String::new();
    std::io::stdin()
        .lock()
        .read_to_string(&mut buf)
        .context("Failed to read aliases from stdin")?;
    Ok(parse_aliases(&buf))
}

/// Run a non-interactive batch lookup and emit results to stdout.
fn run_batch(
    conn: &rusqlite::Connection,
    aliases: Vec<String>,
    species: Option<&str>,
    ignore_case: bool,
    cfg: &OutputCfg,
) -> anyhow::Result<()> {
    let refs: Vec<&str> = aliases.iter().map(String::as_str).collect();
    let results = lookup(conn, &refs, species, ignore_case)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if cfg.format == Format::Tsv && cfg.header {
        emit_tsv_header(&mut out, cfg)?;
    }
    for (alias, records) in &results {
        warn_if_ambiguous(alias, records);
        emit(&mut out, alias, records, cfg)?;
    }
    Ok(())
}

/// Interactive REPL: read one alias per line from the terminal until EOF or "exit".
fn run_repl(
    conn: &rusqlite::Connection,
    species: Option<&str>,
    ignore_case: bool,
    cfg: &OutputCfg,
) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if cfg.format == Format::Tsv && cfg.header {
        emit_tsv_header(&mut out, cfg)?;
    }
    loop {
        // Prompt on stderr so it never contaminates redirected stdout.
        eprint!("\nEnter a gene symbol/ID to lookup (or 'exit' to quit): ");
        std::io::stderr().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break; // EOF (Ctrl-D)
        }
        let alias = line.trim();
        if alias.is_empty() {
            continue;
        }
        if alias.eq_ignore_ascii_case("exit") {
            break;
        }

        match lookup(conn, &[alias], species, ignore_case) {
            Ok(results) => {
                for (a, records) in &results {
                    warn_if_ambiguous(a, records);
                    emit(&mut out, a, records, cfg)?;
                }
            }
            Err(e) => eprintln!("Error looking up '{}': {}", alias, e),
        }
    }
    Ok(())
}

/// Warn (on stderr) when an alias resolves to more than one gene, so scripts can
/// detect ambiguity even though every match is still emitted.
fn warn_if_ambiguous(alias: &str, records: &[GeneRecord]) {
    if records.len() > 1 {
        eprintln!(
            "warning: '{}' is ambiguous ({} matches); scope with --species to disambiguate",
            alias,
            records.len()
        );
    }
}

fn emit(
    out: &mut impl Write,
    alias: &str,
    records: &[GeneRecord],
    cfg: &OutputCfg,
) -> anyhow::Result<()> {
    match cfg.format {
        Format::Pretty => emit_pretty(out, alias, records),
        Format::Tsv => emit_tsv(out, alias, records, cfg),
    }
}

fn emit_pretty(out: &mut impl Write, alias: &str, records: &[GeneRecord]) -> anyhow::Result<()> {
    if records.is_empty() {
        writeln!(out, "{}: (no match)", alias)?;
        return Ok(());
    }
    for record in records {
        writeln!(out, "{}", alias)?;
        for (col, val) in record.iter() {
            writeln!(out, "  {} - {}", col, val)?;
        }
        // Blank line between multiple cross-species/case matches for one alias.
        if records.len() > 1 {
            writeln!(out)?;
        }
    }
    Ok(())
}

fn emit_tsv_header(out: &mut impl Write, cfg: &OutputCfg) -> anyhow::Result<()> {
    let mut cols: Vec<&str> = Vec::with_capacity(cfg.fields.len() + 1);
    if cfg.echo {
        cols.push("input");
    }
    cols.extend(cfg.fields.iter().map(String::as_str));
    writeln!(out, "{}", cols.join("\t"))?;
    Ok(())
}

fn emit_tsv(
    out: &mut impl Write,
    alias: &str,
    records: &[GeneRecord],
    cfg: &OutputCfg,
) -> anyhow::Result<()> {
    // A miss still emits a row (echo + blanks) so output stays line-aligned with input.
    if records.is_empty() {
        let mut row: Vec<&str> = Vec::with_capacity(cfg.fields.len() + 1);
        if cfg.echo {
            row.push(alias);
        }
        row.extend(cfg.fields.iter().map(|_| ""));
        writeln!(out, "{}", row.join("\t"))?;
        return Ok(());
    }
    for record in records {
        let mut row: Vec<&str> = Vec::with_capacity(cfg.fields.len() + 1);
        if cfg.echo {
            row.push(alias);
        }
        for f in &cfg.fields {
            row.push(record.get(f).unwrap_or(""));
        }
        writeln!(out, "{}", row.join("\t"))?;
    }
    Ok(())
}

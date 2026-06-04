use std::path::Path;

use clap::Parser;
use gene_normalizer::cache::{load_cache, lookup};

#[derive(Parser, Debug)]
#[command(
    name = "gene_normalizer",
    about = "Gene symbol/ID normalization tool",
    long_about = None
)]
struct Cli {
    /// Input file containing gene symbols/IDs
    #[arg(short, long, default_value = "")]
    input: String,

    /// Restrict lookups to one species, given as a taxon id
    /// (e.g. "NCBITaxon:9606") or species name (e.g. "Homo sapiens")
    #[arg(short, long)]
    species: Option<String>,

    /// Match aliases case-insensitively (e.g. "tp53" finds "TP53")
    #[arg(long)]
    ignore_case: bool,
}

/// Print a record as `column - value` pairs, one per line.
fn print_record(record: &gene_normalizer::types::GeneRecord) {
    for (col, val) in record.iter() {
        println!("{} - {}", col, val);
    }
}

fn main() {
    env_logger::init();

    let args = Cli::parse();
    log::debug!("Input file: {}", args.input);

    let conn = load_cache("gene_cache.db").expect("Failed to load cache");
    let species = args.species.as_deref();
    let ignore_case = args.ignore_case;

    // Non-interactive / Input file
    if Path::new(&args.input).exists() {
        let file_content = std::fs::read_to_string(&args.input).expect("Failed to read input file");
        let input_data = file_content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>();

        match lookup(&conn, &input_data, species, ignore_case) {
            Ok(results) => {
                for (alias, records) in results {
                    println!("{}", alias);
                    if records.is_empty() {
                        println!("  (no match)");
                    }
                    for record in &records {
                        print_record(record);
                        // Separate multiple cross-species matches for the same alias.
                        if records.len() > 1 {
                            println!();
                        }
                    }
                }
            }
            Err(e) => eprintln!("Error during batch lookup: {}", e),
        }
        return;
    }

    loop {
        println!("\nEnter a gene symbol/ID to lookup (or 'exit' to quit):");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");
        let input = input.trim();
        if input.eq_ignore_ascii_case("exit") {
            break;
        }
        match lookup(&conn, &[input], species, ignore_case) {
            Ok(results) => {
                let records = results
                    .into_iter()
                    .next()
                    .map(|(_, r)| r)
                    .unwrap_or_default();
                if records.is_empty() {
                    println!("No match for '{}'", input);
                }
                for record in &records {
                    print_record(record);
                    if records.len() > 1 {
                        println!();
                    }
                }
            }
            Err(e) => println!("Error looking up '{}': {}", input, e),
        }
    }
}

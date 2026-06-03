use clap::Parser;
use gene_normalizer::cache::{load_cache, lookup, lookup_many, stream_gene_tsv_lines};

#[derive(Parser, Debug)]
#[command(
    name = "gene_normalizer",
    about = "Gene symbol/ID normalization tool",
    long_about = None
)]
struct Cli {
    /// Input file containing gene symbols/IDs
    #[arg(short, long)]
    input: String,
}

fn main() {
    env_logger::init();

    let args = Cli::parse();

    log::debug!("Input file: {}", args.input);

    let mut counter = 0;

    // Handle caching
    let tsv_stream = stream_gene_tsv_lines();

    log::debug!("Metadata lines:");
    for line in tsv_stream.metadata {
        log::debug!("{}", line);
    }

    log::debug!("\nHeader line:");
    log::debug!("{}", tsv_stream.header.join("\t"));

    log::debug!("\nData lines:");
    for line in tsv_stream.rows {
        log::debug!("{}", line);
        counter += 1;
        if counter >= 10 {
            break;
        }
    }

    let conn = load_cache("gene_cache.db").expect("Failed to load cache");

    log::debug!("\nPerforming lookup for 'BRCA1'...");
    let result = lookup(&conn, "BRCA1").expect("Failed to lookup gene");
    log::debug!("Lookup result: {:?}", result);

    log::debug!("\nPerforming batch lookup for ['BRCA1', 'TP53', 'EGFR']...");
    let batch_result = lookup_many(&conn, &["BRCA1", "TP53", "EGFR"]).expect("Failed to perform batch lookup");
    log::debug!("Batch lookup result: {:?}", batch_result);

    log::debug!("\nPerforming lookup for 'this does not exist'...");
    let result = lookup(&conn, "this does not exist").expect("Failed to lookup gene");
    log::debug!("Lookup result: {:?}", result);

    log::debug!("\nPerforming batch lookup for ['BRCA1', 'this does not exist']...");
    let batch_result = lookup_many(&conn, &["BRCA1", "this does not exist"]).expect("Failed to perform batch lookup");
    log::debug!("Batch lookup result: {:?}", batch_result);
}

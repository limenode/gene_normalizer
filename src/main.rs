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
    let args = Cli::parse();

    println!("Input file: {}", args.input);

    let mut counter = 0;

    // Handle caching
    let tsv_stream = stream_gene_tsv_lines();

    println!("Metadata lines:");
    for line in tsv_stream.metadata {
        println!("{}", line);
    }

    println!("\nHeader line:");
    println!("{}", tsv_stream.header.join("\t"));

    println!("\nData lines:");
    for line in tsv_stream.rows {
        println!("{}", line);
        counter += 1;
        if counter >= 10 {
            break;
        }
    }

    println!("\nLoading cache...");
    let conn = load_cache("gene_cache.db").expect("Failed to load cache");
    println!("Cache loaded successfully.");

    println!("\nPerforming lookup for 'BRCA1'...");
    let result = lookup(&conn, "BRCA1").expect("Failed to lookup gene");
    println!("Lookup result: {:?}", result);

    println!("\nPerforming batch lookup for ['BRCA1', 'TP53', 'EGFR']...");
    let batch_result = lookup_many(&conn, &["BRCA1", "TP53", "EGFR"]).expect("Failed to perform batch lookup");
    println!("Batch lookup result: {:?}", batch_result);

    println!("\nPerforming lookup for 'this does not exist'...");
    let result = lookup(&conn, "this does not exist").expect("Failed to lookup gene");
    println!("Lookup result: {:?}", result);

    println!("\nPerforming batch lookup for ['BRCA1', 'this does not exist']...");
    let batch_result = lookup_many(&conn, &["BRCA1", "this does not exist"]).expect("Failed to perform batch lookup");
    println!("Batch lookup result: {:?}", batch_result);
}

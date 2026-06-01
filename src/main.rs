mod cache;
mod types;
use clap::{Parser};

use crate::cache::stream_gene_tsv_lines;

#[derive(Parser, Debug)]
#[command(
    name = "gene_normalizer",
    about = "Gene symbol/ID normalization tool",
    long_about = None
)]
struct Cli {
    /// Input file containing gene symbols/IDs
    #[arg(short, long)]
    input: String
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
}

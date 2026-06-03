use std::path::Path;

use clap::Parser;
use gene_normalizer::cache::{
    load_cache, lookup, lookup_many_chunked, print_tsv_info, stream_gene_tsv_lines,
};

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
}

fn main() {
    env_logger::init();

    let args = Cli::parse();
    log::debug!("Input file: {}", args.input);

    let tsv_stream = stream_gene_tsv_lines();

    print_tsv_info(10);

    let conn = load_cache("gene_cache.db").expect("Failed to load cache");

    // Non-interactive / Input file
    if Path::new(&args.input).exists() {
        let file_content = std::fs::read_to_string(&args.input)
            .expect("Failed to read input file");
        let input_data = file_content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>();

        match lookup_many_chunked(&conn, &input_data) {
            Ok(results) => {
                for (alias, gene_record) in results {
                    println!("{}", alias);
                    for i in 0..gene_record.len() {
                        println!("{} - {}", tsv_stream.header[i], gene_record[i]);
                    }
                }
            }
            Err(e) => eprintln!("Error during batch lookup: {}", e)
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
        match lookup(&conn, input) {
            Ok(result) => println!("Lookup result for '{}': {:?}", input, result),
            Err(e) => println!("Error looking up '{}': {}", input, e),
        }
    }
}

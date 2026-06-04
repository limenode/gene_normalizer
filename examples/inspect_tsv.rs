//! Development-only diagnostic for inspecting the source TSV.
//!
//! Downloads the Alliance of Genome Resources combined gene TSV and prints its
//! metadata lines, the header, and the first few data rows. This is a manual tool
//! for figuring out the input schema — it is never built into the production binary.
//!
//! Run with: `cargo run --example inspect_tsv`

use gene_normalizer::cache::stream_gene_tsv_lines;

fn main() {
    let n_data_lines = 10;
    let tsv = stream_gene_tsv_lines();

    println!("Metadata lines:");
    for line in tsv.metadata {
        println!("{}", line);
    }

    println!("\nHeader line:");
    println!("{}", tsv.header.join("\t"));

    println!("\nData lines: (showing first {} lines)", n_data_lines);
    for line in tsv.rows.take(n_data_lines) {
        println!("{}", line);
    }
}

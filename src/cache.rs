use flate2::read::GzDecoder;
use reqwest::blocking::get;
use std::io::{BufRead, BufReader};

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

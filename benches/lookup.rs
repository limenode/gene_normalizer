use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use gene_normalizer::cache::{load_cache, lookup};
use std::hint::black_box;
use std::time::Duration;

fn load_aliases(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn.prepare("SELECT DISTINCT alias FROM gene_aliases").unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn make_batch(real_aliases: &[String], batch_size: usize, hit_rate: f64) -> Vec<String> {
    let hit_count = (batch_size as f64 * hit_rate).round() as usize;
    let miss_count = batch_size - hit_count;

    real_aliases.iter()
        .cycle()
        .take(hit_count)
        .cloned()
        .chain((0..miss_count).map(|i| format!("NONEXISTENT_{i}")))
        .collect()
}

fn bench_lookup(c: &mut Criterion) {
    let conn = load_cache("gene_cache.db").unwrap();
    let aliases = load_aliases(&conn);
    let mut cycle = aliases.iter().cycle();

    c.bench_function("lookup", |b| {
        b.iter(|| black_box(lookup(&conn, &[cycle.next().unwrap().as_str()], None, false)))
    });
}

fn bench_lookup_many(c: &mut Criterion) {
    let conn = load_cache("gene_cache.db").unwrap();
    let aliases = load_aliases(&conn);

    let mut group = c.benchmark_group("lookup_many");
    group.measurement_time(Duration::from_secs(15));

    for batch_size in [10, 100, 500] {
        for hit_rate in [1.0, 0.75, 0.5] {
            let batch_strings = make_batch(&aliases, batch_size, hit_rate);
            let batch: Vec<&str> = batch_strings.iter().map(String::as_str).collect();

            group.bench_with_input(
                BenchmarkId::new(format!("{batch_size}"), format!("{:.0}pct", hit_rate * 100.0)),
                &batch,
                |b, batch| b.iter(|| black_box(lookup(&conn, batch, None, false))),
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_lookup, bench_lookup_many);
criterion_main!(benches);

//! Benchmarks for `StandardScaler::transform` and `transform_into`.
//!
//! Measures the hot path across feature dimensions used by the audit
//! (`d = 8, 32, 128, 1024`) and compares the allocating `transform` path
//! against the buffer-reusing `transform_into` path.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rill_ml::preprocessing::StandardScaler;
use rill_ml::traits::Transformer;

/// Build a scaler that has seen `samples` rows so its means/variances are
/// non-trivial (avoids the degenerate zero-state fast path).
fn warmed_scaler(d: usize, samples: usize) -> StandardScaler {
    let mut scaler = StandardScaler::new(d).unwrap();
    for row in 0..samples {
        let features: Vec<f64> = (0..d).map(|i| (row + i) as f64 * 0.1 + 1.0).collect();
        scaler.update(&features).unwrap();
    }
    scaler
}

fn bench_transform(c: &mut Criterion) {
    let mut group = c.benchmark_group("standard_scaler_transform");
    for &d in &[8usize, 32, 128, 1024] {
        let scaler = warmed_scaler(d, 64);
        let features: Vec<f64> = (0..d).map(|i| i as f64 * 0.3 + 0.5).collect();
        group.bench_with_input(BenchmarkId::from_parameter(d), &d, |b, _| {
            b.iter(|| {
                black_box(scaler.transform(&features).unwrap());
            })
        });
    }
    group.finish();
}

fn bench_transform_into(c: &mut Criterion) {
    let mut group = c.benchmark_group("standard_scaler_transform_into");
    for &d in &[8usize, 32, 128, 1024] {
        let scaler = warmed_scaler(d, 64);
        let features: Vec<f64> = (0..d).map(|i| i as f64 * 0.3 + 0.5).collect();
        let mut buffer: Vec<f64> = Vec::with_capacity(d);
        group.bench_with_input(BenchmarkId::from_parameter(d), &d, |b, _| {
            b.iter(|| {
                scaler.transform_into(&features, &mut buffer).unwrap();
                black_box(&buffer);
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_transform, bench_transform_into);
criterion_main!(benches);

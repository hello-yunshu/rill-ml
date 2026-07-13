//! Benchmarks for online statistics.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rill_ml::stats::{ExponentiallyWeightedMean, Mean, RollingMean, Variance, VarianceKind};
use rill_ml::traits::OnlineStatistic;

fn bench_mean(c: &mut Criterion) {
    c.bench_function("mean_update", |b| {
        b.iter(|| {
            let mut m = Mean::new();
            for i in 0..1000 {
                m.update(i as f64).unwrap();
            }
            black_box(m.value());
        })
    });
}

fn bench_variance(c: &mut Criterion) {
    c.bench_function("variance_update", |b| {
        b.iter(|| {
            let mut v = Variance::new(VarianceKind::Population);
            for i in 0..1000 {
                v.update(i as f64).unwrap();
            }
            black_box(v.value());
        })
    });
}

fn bench_ew_mean(c: &mut Criterion) {
    c.bench_function("ewmean_update", |b| {
        b.iter(|| {
            let mut ew = ExponentiallyWeightedMean::new(0.1).unwrap();
            for i in 0..1000 {
                ew.update(i as f64).unwrap();
            }
            black_box(ew.value());
        })
    });
}

fn bench_rolling_mean(c: &mut Criterion) {
    c.bench_function("rolling_mean_update/100", |b| {
        b.iter(|| {
            let mut rm = RollingMean::new(100).unwrap();
            for i in 0..1000 {
                rm.update(i as f64).unwrap();
            }
            black_box(rm.value());
        })
    });
}

criterion_group!(
    benches,
    bench_mean,
    bench_variance,
    bench_ew_mean,
    bench_rolling_mean
);
criterion_main!(benches);

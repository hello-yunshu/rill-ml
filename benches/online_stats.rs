//! Benchmarks for online statistics.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rill_ml::stats::{
    ExponentiallyWeightedMean, Mean, RollingMean, RollingMedianMad, Variance, VarianceKind,
};
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

fn bench_rolling_median_mad(c: &mut Criterion) {
    let mut group = c.benchmark_group("rolling_median_mad");
    for window_size in [128usize, 1_024, 4_096] {
        let mut statistic = RollingMedianMad::new(window_size, window_size).unwrap();
        for index in 0..window_size {
            statistic.update((index % 97) as f64).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("update", window_size),
            &window_size,
            |b, _| {
                let mut value = 0.0;
                b.iter(|| {
                    statistic.update(black_box(value)).unwrap();
                    value = (value + 1.0) % 97.0;
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("summary", window_size),
            &window_size,
            |b, _| b.iter(|| black_box(statistic.summary().unwrap())),
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_mean,
    bench_variance,
    bench_ew_mean,
    bench_rolling_mean,
    bench_rolling_median_mad
);
criterion_main!(benches);

//! Stable-vs-Preview LinUCB benchmarks.
//!
//! Run with `cargo bench --bench linucb`. Selection cases cover small,
//! medium and large feature dimensions plus multiple arm counts. Update cases
//! use identical finite contexts and rewards; both implementations retain
//! learned state across iterations, matching an online serving path.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rand::{SeedableRng, rngs::StdRng};
use rill_ml::bandit::{ContextualBandit, LinUcb, LinUcbConfig, LinUcbFast};

fn configured(arms: usize, features: usize) -> (LinUcb, LinUcbFast, Vec<f64>) {
    let mut config = LinUcbConfig::default();
    config.alpha = 0.5;
    config.arm_count = arms;
    config.feature_count = features;
    let mut stable = LinUcb::new(config.clone()).unwrap();
    let mut fast = LinUcbFast::new(config).unwrap();
    let context: Vec<f64> = (0..features)
        .map(|index| ((index + 1) as f64 / features as f64).sin())
        .collect();
    for arm in 0..arms {
        stable
            .update(arm, &context, arm as f64 / arms as f64)
            .unwrap();
        fast.update(arm, &context, arm as f64 / arms as f64)
            .unwrap();
    }
    (stable, fast, context)
}

fn bench_select(c: &mut Criterion) {
    let mut group = c.benchmark_group("linucb_select");
    for &(arms, features) in &[(2, 8), (8, 32), (8, 128)] {
        let (stable, fast, context) = configured(arms, features);
        group.bench_with_input(
            BenchmarkId::new("stable", format!("a{arms}_d{features}")),
            &(arms, features),
            |bench, _| {
                let mut rng = StdRng::seed_from_u64(7);
                bench.iter(|| black_box(stable.select(&context, &mut rng).unwrap()));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("fast", format!("a{arms}_d{features}")),
            &(arms, features),
            |bench, _| {
                let mut rng = StdRng::seed_from_u64(7);
                bench.iter(|| black_box(fast.select(&context, &mut rng).unwrap()));
            },
        );
    }
    group.finish();
}

fn bench_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("linucb_update");
    for &features in &[8, 32, 128] {
        let (mut stable, mut fast, context) = configured(4, features);
        group.bench_with_input(
            BenchmarkId::new("stable", features),
            &features,
            |bench, _| bench.iter(|| stable.update(0, &context, black_box(0.25)).unwrap()),
        );
        group.bench_with_input(BenchmarkId::new("fast", features), &features, |bench, _| {
            bench.iter(|| fast.update(0, &context, black_box(0.25)).unwrap())
        });
    }
    group.finish();
}

criterion_group!(benches, bench_select, bench_update);
criterion_main!(benches);

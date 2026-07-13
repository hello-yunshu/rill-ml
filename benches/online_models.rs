//! Benchmarks for online models.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rill_ml::{
    loss::RegressionLoss,
    models::{
        LinearRegression, LinearRegressionConfig, LogisticRegression, LogisticRegressionConfig,
    },
    optim::{Optimizer, SgdConfig},
    pipeline::RegressionPipeline,
    preprocessing::StandardScaler,
    traits::{OnlineBinaryClassifier, OnlineRegressor},
};

fn bench_linear_predict(c: &mut Criterion) {
    let mut group = c.benchmark_group("linear_predict");
    for &d in &[8usize, 32, 128] {
        let model = LinearRegression::new(
            d,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        let features: Vec<f64> = (0..d).map(|i| i as f64 * 0.1).collect();
        group.bench_with_input(BenchmarkId::from_parameter(d), &d, |b, _| {
            b.iter(|| {
                black_box(model.predict(&features).unwrap());
            })
        });
    }
    group.finish();
}

fn bench_linear_learn(c: &mut Criterion) {
    let mut group = c.benchmark_group("linear_learn");
    for &d in &[8usize, 32, 128] {
        let mut model = LinearRegression::new(
            d,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        let features: Vec<f64> = (0..d).map(|i| i as f64 * 0.1).collect();
        group.bench_with_input(BenchmarkId::from_parameter(d), &d, |b, _| {
            b.iter(|| {
                model.learn(&features, 1.0).unwrap();
            })
        });
    }
    group.finish();
}

fn bench_pipeline(c: &mut Criterion) {
    let d = 32;
    let scaler = StandardScaler::new(d).unwrap();
    let model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    let mut pipeline = RegressionPipeline::new(scaler, model).unwrap();
    let features: Vec<f64> = (0..d).map(|i| i as f64 * 0.1).collect();

    c.bench_function("pipeline_predict_learn", |b| {
        b.iter(|| {
            let p = black_box(pipeline.predict(&features).unwrap());
            pipeline.learn(&features, p).unwrap();
        })
    });
}

fn bench_logistic(c: &mut Criterion) {
    let d = 32;
    let mut model = LogisticRegression::new(
        d,
        LogisticRegressionConfig {
            optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
            loss: Default::default(),
        },
    )
    .unwrap();
    let features: Vec<f64> = (0..d).map(|i| i as f64 * 0.1).collect();
    c.bench_function("logistic_predict_learn", |b| {
        b.iter(|| {
            let p = black_box(model.predict_proba(&features).unwrap());
            model.learn(&features, p > 0.5).unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_linear_predict,
    bench_linear_learn,
    bench_pipeline,
    bench_logistic
);
criterion_main!(benches);

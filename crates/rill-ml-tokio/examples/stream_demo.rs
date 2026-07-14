//! Stream-based progressive evaluation demo.
//!
//! Generates a synthetic regression stream of 1,000 samples, then compares
//! `LinearRegression` against the `MeanRegressor` baseline using `Mae` over a
//! `tokio_stream::iter` stream.

use rill_ml::evaluate::RegressionSample;
use rill_ml::metrics::Mae;
use rill_ml::models::{BaselineConfig, LinearRegression, LinearRegressionConfig, MeanRegressor};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml_tokio::progressive_regress_stream;
use tokio_stream::iter;

const SAMPLES: usize = 1_000;

fn make_samples() -> Vec<RegressionSample> {
    (0..SAMPLES)
        .map(|i| {
            let x = i as f64 * 0.01;
            // y = 3x + 2 + small deterministic "noise"
            let target = 3.0 * x + 2.0 + ((i as f64).sin() * 0.5);
            RegressionSample {
                features: vec![x],
                target,
            }
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let samples = make_samples();

    // Baseline
    let mut baseline = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut baseline_mae = Mae::default();
    let baseline_final =
        progressive_regress_stream(&mut baseline, &mut baseline_mae, iter(samples.clone()))
            .await
            .unwrap();

    // Linear regression
    let optimizer = Optimizer::sgd(
        1,
        SgdConfig {
            learning_rate: 0.005,
            l2: 0.0,
        },
    )
    .unwrap();
    let regression = LinearRegression::new(
        1,
        LinearRegressionConfig {
            optimizer,
            loss: Default::default(),
        },
    )
    .unwrap();
    let mut model = regression;
    let mut model_mae = Mae::default();
    let model_final = progressive_regress_stream(&mut model, &mut model_mae, iter(samples))
        .await
        .unwrap();

    println!(
        "Streamed {} samples through progressive evaluation",
        SAMPLES
    );
    println!(
        "Baseline (MeanRegressor) MAE: {:.4}",
        baseline_final.unwrap_or(0.0)
    );
    println!(
        "LinearRegression        MAE: {:.4}",
        model_final.unwrap_or(0.0)
    );
    if model_final.unwrap_or(0.0) < baseline_final.unwrap_or(0.0) {
        println!("LinearRegression wins on this stream.");
    } else {
        println!("Baseline wins on this stream (linear model still warming up).");
    }
}

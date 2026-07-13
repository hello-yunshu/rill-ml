//! Diagnostics demo: prediction reports, baseline comparison, and model selection.
//!
//! This example demonstrates the v0.2 diagnostics module on a synthetic
//! regression stream. It shows how to:
//!
//! - Track training statistics with [`TrainingSummary`].
//! - Generate confidence-aware [`PredictionReport`]s with prediction intervals.
//! - Compare multiple models with [`BaselineComparator`].
//! - Automatically select the best model with [`OnlineModelSelector`].
//! - Detect unhealthy parameters with [`ModelHealthReport`].
//!
//! Run with:
//!
//! ```sh
//! cargo run --example diagnostics_demo
//! ```

use rand::SeedableRng;
use rill_ml::{
    Metric, OnlineRegressor,
    diagnostics::{
        ModelHealthReport, OnlineModelSelector, PredictionReporter, SelectorConfig, TrainingSummary,
    },
    metrics::Mae,
    models::{BaselineConfig, LinearRegression, LinearRegressionConfig, MeanRegressor},
    optim::{Optimizer, SgdConfig},
    pipeline::RegressionPipeline,
    preprocessing::StandardScaler,
};

fn main() {
    println!("=== RillML v0.2 Diagnostics Demo ===\n");

    // --- Setup: two competing models on a synthetic linear stream ---
    let feature_count = 2;
    let scaler = StandardScaler::new(feature_count).unwrap();
    let optimizer = Optimizer::sgd(
        feature_count,
        SgdConfig {
            learning_rate: 0.05,
            l2: 0.0,
        },
    )
    .unwrap();
    let regression = LinearRegression::new(
        feature_count,
        LinearRegressionConfig {
            optimizer,
            loss: Default::default(),
        },
    )
    .unwrap();
    let mut linear_pipeline: RegressionPipeline<StandardScaler, LinearRegression> =
        RegressionPipeline::new(scaler, regression).unwrap();
    let mut mean_baseline = MeanRegressor::new(BaselineConfig::default()).unwrap();

    // --- Diagnostics ---
    let mut selector = OnlineModelSelector::new(
        &["MeanBaseline", "LinearRegression"],
        SelectorConfig::default(),
    )
    .unwrap();
    let mut reporter = PredictionReporter::default();
    let mut summary = TrainingSummary::default();
    let mut mae = Mae::default();

    // A reasonable baseline error: if the linear model's recent error falls
    // below this, it is considered better than a naive baseline.
    reporter.set_baseline(0.5).unwrap();
    summary.set_baseline_error(0.5).unwrap();

    // --- Stream: y = 3*x1 + 2*x2 + noise ---
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    let n = 200;

    for i in 0..n {
        let x1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let x2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let noise = rand::Rng::gen_range(&mut rng, -0.05..0.05);
        let y = 3.0 * x1 + 2.0 * x2 + noise;

        // Progressive evaluation: predict before learning.
        let mean_pred = mean_baseline.predict(&[x1, x2]).unwrap();
        let linear_pred = linear_pipeline.predict(&[x1, x2]).unwrap();

        let linear_abs_err = (y - linear_pred).abs();
        mae.update(y, linear_pred).unwrap();
        summary.record_sample();
        summary.record_error(linear_abs_err).unwrap();

        // Record both models' errors for the selector.
        selector
            .record(0, y, mean_pred)
            .expect("record mean baseline");
        selector
            .record(1, y, linear_pred)
            .expect("record linear model");

        // Reporter observes the linear model's predictions.
        reporter.observe(linear_pred, y).unwrap();

        // Both models learn from this sample.
        mean_baseline.learn(&[x1, x2], y).unwrap();
        linear_pipeline.learn(&[x1, x2], y).unwrap();

        // Periodic report.
        if (i + 1) % 50 == 0 {
            selector.select();
            let report = reporter.report(linear_pred).unwrap();
            println!("--- step {} ---", i + 1);
            println!(
                "  best model: {} (switches: {})",
                selector.current_best_name().unwrap_or("(none yet)"),
                selector.switch_count()
            );
            println!(
                "  warmup state: {:?}, confidence: {:?}",
                report.warmup_state(),
                report.confidence()
            );
            if let (Some(lo), Some(hi)) = (report.lower_bound(), report.upper_bound()) {
                println!(
                    "  prediction: {:.4}, interval: [{:.4}, {:.4}]",
                    report.prediction(),
                    lo,
                    hi
                );
            } else {
                println!(
                    "  prediction: {:.4}, interval: (insufficient data)",
                    report.prediction()
                );
            }
            println!(
                "  recent error: {:?}, beats baseline: {:?}",
                report.recent_error(),
                report.beats_baseline()
            );
        }
    }

    // --- Final summary ---
    println!("\n=== Final Summary ===");
    println!("Total samples: {}", summary.total_samples());
    println!("Final MAE: {:.6}", mae.value().unwrap());
    println!(
        "Best model: {} (switches: {})",
        selector.current_best_name().unwrap_or("(none)"),
        selector.switch_count()
    );
    println!(
        "Recent error: {:?}, best error: {:?}",
        summary.recent_error(),
        summary.best_error()
    );

    // --- Model health check ---
    let linear = linear_pipeline.model();
    let health = ModelHealthReport::from_parameters(linear.weights(), Some(linear.intercept()));
    println!("\n=== Linear Model Health ===");
    println!("Parameter count: {}", health.parameter_count());
    println!(
        "Weight range: [{:?}, {:?}]",
        health.weight_min(),
        health.weight_max()
    );
    println!(
        "Has NaN: {}, Has Infinity: {}",
        health.has_nan(),
        health.has_infinity()
    );
    println!("Healthy: {}", health.is_healthy());
    println!("State size: {} bytes", health.state_size_bytes());

    // Expected: weights ≈ [3.0, 2.0], intercept ≈ 0.0
    println!(
        "\nLearned weights: [{:.4}, {:.4}], intercept: {:.4}",
        linear.weights()[0],
        linear.weights()[1],
        linear.intercept()
    );
}

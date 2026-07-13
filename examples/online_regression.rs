//! Generic online regression example.
//!
//! Simulates a stream of feature vectors with a known linear relationship and
//! concept drift, then predicts the target using online linear regression
//! compared against simple baselines.
//!
//! **Note:** This is a *generic* online learning example. Real-world
//! regression tasks require application-layer data cleaning: outlier removal,
//! handling missing values, feature engineering, and discarding unreliable
//! samples. Those concerns belong to the application layer, not to RillML.
//!
//! This example requires the `serde` feature to demonstrate state persistence.

use rand::SeedableRng;
use rill_ml::{
    Metric, OnlineRegressor, RegressionSample,
    evaluate::evaluate_regression,
    metrics::{Mae, RollingMae},
    models::{
        BaselineConfig, ExponentiallyWeightedMeanRegressor, LinearRegression,
        LinearRegressionConfig, MeanRegressor,
    },
    optim::{Optimizer, SgdConfig},
    persistence::Snapshot,
    pipeline::RegressionPipeline,
    preprocessing::StandardScaler,
};

/// A synthetic sample with four generic features and a real-valued target.
struct Sample {
    x1: f64,
    x2: f64,
    x3: f64,
    x4: f64,
    y: f64,
}

fn features(s: &Sample) -> Vec<f64> {
    vec![s.x1, s.x2, s.x3, s.x4]
}

fn generate_samples(n: usize) -> Vec<Sample> {
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(2024);
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        // Concept drift: the underlying relationship shifts halfway through.
        let phase = if i < n / 2 { 0.0 } else { 0.15 };
        let x1 = rand::Rng::gen_range(&mut rng, 0.2..1.0);
        let x2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let x3 = rand::Rng::gen_range(&mut rng, 0.0..1.0) + phase;
        let x4 = rand::Rng::gen_range(&mut rng, 0.1..1.0);

        // Underlying linear relationship with noise.
        let y = 2.0 * x1 + 1.5 * x2 + 0.8 * x3 - 0.3 * x4
            + phase * 2.0
            + rand::Rng::gen_range(&mut rng, -0.3..0.3);

        samples.push(Sample {
            x1,
            x2,
            x3: x3.min(1.0),
            x4,
            y: y.max(0.0),
        });
    }
    samples
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = 4;
    let samples_raw = generate_samples(800);
    let samples: Vec<RegressionSample> = samples_raw
        .iter()
        .map(|s| RegressionSample {
            features: features(s),
            target: s.y,
        })
        .collect();

    // --- Baseline 1: MeanRegressor ---
    let mut mean_model = MeanRegressor::new(BaselineConfig {
        initial_prediction: 2.0,
    })?;
    let mut mean_mae = Mae::default();
    let mean_final = evaluate_regression(&mut mean_model, &mut mean_mae, samples.clone())?;

    // --- Baseline 2: ExponentiallyWeightedMeanRegressor ---
    let mut ew_model = ExponentiallyWeightedMeanRegressor::new(
        0.3,
        BaselineConfig {
            initial_prediction: 2.0,
        },
    )?;
    let mut ew_mae = Mae::default();
    let ew_final = evaluate_regression(&mut ew_model, &mut ew_mae, samples.clone())?;

    // --- Linear regression pipeline ---
    let scaler = StandardScaler::new(d)?;
    let model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.02,
                    l2: 0.001,
                },
            )?,
            loss: Default::default(),
        },
    )?;
    let mut pipeline = RegressionPipeline::new(scaler, model)?;
    let mut lr_mae = Mae::default();
    let mut lr_rolling = RollingMae::new(50)?;
    let lr_final = evaluate_regression(&mut pipeline, &mut lr_mae, samples.clone())?;

    // Also track rolling MAE separately (requires another pass).
    let scaler2 = StandardScaler::new(d)?;
    let model2 = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.02,
                    l2: 0.001,
                },
            )?,
            loss: Default::default(),
        },
    )?;
    let mut pipeline2 = RegressionPipeline::new(scaler2, model2)?;
    // Manually run to track rolling MAE.
    for s in &samples {
        let pred = pipeline2.predict(&s.features)?;
        lr_rolling.update(s.target, pred)?;
        pipeline2.learn(&s.features, s.target)?;
    }

    println!("=== Online regression (simulated stream with drift) ===");
    println!("Samples: {}", samples.len());
    println!();
    println!("Model                              Final MAE");
    println!("-------------------------------------------");
    println!("MeanRegressor                      {:?}", mean_final);
    println!("EWMeanRegressor (alpha=0.3)        {:?}", ew_final);
    println!("LinearRegression + StandardScaler  {:?}", lr_final);
    println!();
    println!(
        "Recent window MAE (last 50):       {:?}",
        lr_rolling.value()
    );

    let better = match (lr_final, mean_final) {
        (Some(lr), Some(mean)) => lr < mean,
        _ => false,
    };
    println!();
    if better {
        println!("-> Linear regression outperformed the mean baseline.");
    } else {
        println!("-> Linear regression did NOT outperform the mean baseline.");
    }
    println!("   (Only trust the model if it consistently beats baselines.)");

    // --- Demonstrate serialization ---
    println!();
    println!("=== Serialization demo ===");
    let snapshot = Snapshot::new(pipeline);
    let json = serde_json::to_string_pretty(&snapshot)?;
    println!("Snapshot JSON length: {} bytes", json.len());

    let restored: Snapshot<RegressionPipeline<StandardScaler, LinearRegression>> =
        serde_json::from_str(&json)?;
    let restored_pipeline = restored.into_model()?;

    // Verify restored model produces the same predictions.
    let test_features = &samples[0].features;
    let pred_original = pipeline2.predict(test_features)?;
    let pred_restored = restored_pipeline.predict(test_features)?;
    assert!(
        (pred_original - pred_restored).abs() < 1e-9,
        "restored model should produce the same prediction"
    );
    println!("Round-trip serialization verified: prediction = {pred_restored:.6}");

    Ok(())
}

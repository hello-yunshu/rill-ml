//! Progressive evaluation example.
//!
//! Demonstrates the fixed evaluation order:
//! ```text
//! predict → metric.update → learn
//! ```
//! This order is essential: the prediction for the current sample must be
//! made *before* the model learns from it, otherwise we would be evaluating
//! with hindsight and overestimate performance.

use rand::SeedableRng;
use rill_ml::{
    Metric, OnlineRegressor, RegressionSample,
    evaluate::evaluate_regression_with_steps,
    metrics::{Mae, Mse, Rmse},
    models::{LinearRegression, LinearRegressionConfig},
    optim::{Optimizer, SgdConfig},
    pipeline::RegressionPipeline,
    preprocessing::StandardScaler,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = 2;
    let scaler = StandardScaler::new(d)?;
    let model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.05,
                    l2: 0.0,
                },
            )?,
            loss: Default::default(),
        },
    )?;
    let mut pipeline = RegressionPipeline::new(scaler, model)?;

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    let samples: Vec<RegressionSample> = (0..500)
        .map(|_| {
            let x1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
            let x2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
            let y = 3.0 * x1 - 1.0 * x2 + 0.5;
            RegressionSample {
                features: vec![x1, x2],
                target: y,
            }
        })
        .collect();

    let mut mae = Mae::default();
    let mut mse = Mse::default();
    let mut rmse = Rmse::default();

    // Run progressive evaluation, which internally follows
    // predict → metric.update → learn.
    let (final_mae, steps) =
        evaluate_regression_with_steps(&mut pipeline, &mut mae, samples.clone())?;

    // Re-run for MSE and RMSE (each metric needs its own pass because
    // the model state advances during evaluation).
    let mut pipeline2 = {
        let scaler = StandardScaler::new(d)?;
        let model = LinearRegression::new(
            d,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(
                    d,
                    SgdConfig {
                        learning_rate: 0.05,
                        l2: 0.0,
                    },
                )?,
                loss: Default::default(),
            },
        )?;
        RegressionPipeline::new(scaler, model)?
    };
    evaluate_regression_with_steps(&mut pipeline2, &mut mse, samples.clone())?;

    let mut pipeline3 = {
        let scaler = StandardScaler::new(d)?;
        let model = LinearRegression::new(
            d,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(
                    d,
                    SgdConfig {
                        learning_rate: 0.05,
                        l2: 0.0,
                    },
                )?,
                loss: Default::default(),
            },
        )?;
        RegressionPipeline::new(scaler, model)?
    };
    evaluate_regression_with_steps(&mut pipeline3, &mut rmse, samples)?;

    println!("Progressive evaluation complete ({} samples)", steps.len());
    println!("  Final MAE:  {:?}", final_mae);
    println!("  Final MSE:  {:?}", mse.value());
    println!("  Final RMSE: {:?}", rmse.value());

    // Show the first few steps to illustrate the order.
    for step in steps.iter().take(5) {
        println!("  step {}: metric = {:?}", step.index, step.metric_value);
    }
    println!("  ...");
    for step in steps.iter().rev().take(3) {
        println!("  step {}: metric = {:?}", step.index, step.metric_value);
    }

    // Verify the model learned something: final MAE should be lower than initial.
    if let (Some(first), Some(last)) = (
        steps.first().and_then(|s| s.metric_value),
        steps.last().and_then(|s| s.metric_value),
    ) {
        println!("\n  Initial MAE: {:.6}", first);
        println!("  Final MAE:   {:.6}", last);
        if last < first {
            println!("  -> Model improved over the stream.");
        }
    }

    // Demonstrate that predict does not mutate state.
    let features = [0.5, 0.5];
    let p1 = pipeline.predict(&features)?;
    let p2 = pipeline.predict(&features)?;
    assert!((p1 - p2).abs() < 1e-12, "predict must be side-effect free");
    println!("\nPredict side-effect check passed: {p1:.6} == {p2:.6}");

    Ok(())
}

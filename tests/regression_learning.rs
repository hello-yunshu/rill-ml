//! Integration tests: regression model learning behavior.
//!
//! Verifies that `LinearRegression` and baseline regressors converge to
//! correct solutions on known data-generating processes.

use rand::SeedableRng;
use rill_ml::OnlineRegressor;
use rill_ml::loss::RegressionLoss;
use rill_ml::models::{
    BaselineConfig, ExponentiallyWeightedMeanRegressor, LastValueRegressor, LinearRegression,
    LinearRegressionConfig, MeanRegressor,
};
use rill_ml::optim::{Optimizer, SgdConfig};

#[test]
fn linear_regression_converges_to_true_weights() {
    let d = 3;
    let true_weights = [2.0, -1.0, 0.5];
    let true_intercept = 3.0;
    let mut model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.05,
                    l2: 0.0,
                },
            )
            .unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    for _ in 0..2000 {
        let x: Vec<f64> = (0..d)
            .map(|_| rand::Rng::gen_range(&mut rng, -2.0..2.0))
            .collect();
        let y = true_weights
            .iter()
            .zip(&x)
            .map(|(w, xi)| w * xi)
            .sum::<f64>()
            + true_intercept;
        model.learn(&x, y).unwrap();
    }

    let test_x = vec![1.5, -0.5, 2.0];
    let expected_y: f64 = true_weights
        .iter()
        .zip(&test_x)
        .map(|(w, xi)| w * xi)
        .sum::<f64>()
        + true_intercept;
    let predicted = model.predict(&test_x).unwrap();
    assert!(
        (predicted - expected_y).abs() < 0.5,
        "predicted = {predicted}, expected = {expected_y}"
    );
}

#[test]
fn mean_regressor_converges_to_mean() {
    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let data = [10.0, 20.0, 30.0, 40.0, 50.0];
    for &y in &data {
        model.learn(&[], y).unwrap();
    }
    let predicted = model.predict(&[]).unwrap();
    assert!((predicted - 30.0).abs() < 1e-9);
}

#[test]
fn ew_mean_regressor_weights_recent() {
    let mut model =
        ExponentiallyWeightedMeanRegressor::new(0.5, BaselineConfig::default()).unwrap();
    model.learn(&[], 10.0).unwrap();
    model.learn(&[], 20.0).unwrap();
    model.learn(&[], 30.0).unwrap();
    let predicted = model.predict(&[]).unwrap();
    // EWMean: 10 -> 15 -> 22.5
    assert!((predicted - 22.5).abs() < 1e-9);
}

#[test]
fn last_value_regressor_tracks_last() {
    let mut model = LastValueRegressor::new(BaselineConfig::default()).unwrap();
    model.learn(&[], 7.0).unwrap();
    model.learn(&[], 42.0).unwrap();
    assert_eq!(model.predict(&[]).unwrap(), 42.0);
}

#[test]
fn linear_regression_with_adagrad_converges() {
    use rill_ml::optim::AdaGradConfig;
    let d = 2;
    let mut model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::adagrad(
                d,
                AdaGradConfig {
                    learning_rate: 1.0,
                    l2: 0.0,
                    epsilon: 1e-8,
                },
            )
            .unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    for _ in 0..1000 {
        let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
        let x2 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
        let y = 1.0 * x1 + (-2.0) * x2;
        model.learn(&[x1, x2], y).unwrap();
    }

    let pred = model.predict(&[1.0, -1.0]).unwrap();
    // y = 1*1 + (-2)*(-1) = 3
    assert!((pred - 3.0).abs() < 0.5, "pred = {pred}");
}

#[test]
fn predict_is_side_effect_free() {
    let d = 2;
    let model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    let samples_before = model.samples_seen();
    let _ = model.predict(&[1.0, 2.0]).unwrap();
    let _ = model.predict(&[3.0, 4.0]).unwrap();
    assert_eq!(model.samples_seen(), samples_before);
}

#[test]
fn reset_clears_model_state() {
    let d = 1;
    let mut model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    model.learn(&[1.0], 5.0).unwrap();
    model.learn(&[2.0], 10.0).unwrap();
    assert_eq!(model.samples_seen(), 2);
    model.reset();
    assert_eq!(model.samples_seen(), 0);
    assert_eq!(model.predict(&[1.0]).unwrap(), 0.0);
}

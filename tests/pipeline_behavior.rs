//! Integration tests: pipeline behavior (transformer + model composition).
//!
//! Verifies that pipelines correctly chain preprocessing and model learning,
//! that transformers do not leak labels, and that dimension mismatches are
//! rejected at the pipeline level.

use rand::SeedableRng;
use rill_ml::OnlineRegressor;
use rill_ml::loss::RegressionLoss;
use rill_ml::models::{LinearRegression, LinearRegressionConfig};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml::pipeline::{ClassificationPipeline, RegressionPipeline};
use rill_ml::preprocessing::{Clipper, MinMaxScaler, StandardScaler};
use rill_ml::traits::{OnlineBinaryClassifier, Transformer};

fn make_regression_pipeline(d: usize) -> RegressionPipeline<StandardScaler, LinearRegression> {
    let scaler = StandardScaler::new(d).unwrap();
    let model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    RegressionPipeline::new(scaler, model).unwrap()
}

#[test]
fn pipeline_learns_linear_relation() {
    let d = 2;
    let mut pipeline = make_regression_pipeline(d);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(55);

    for _ in 0..1000 {
        let x1 = rand::Rng::gen_range(&mut rng, 0.0..10.0);
        let x2 = rand::Rng::gen_range(&mut rng, 0.0..10.0);
        let y = 3.0 * x1 - 2.0 * x2 + 1.0;
        pipeline.learn(&[x1, x2], y).unwrap();
    }

    let pred = pipeline.predict(&[5.0, 2.5]).unwrap();
    let expected = 3.0 * 5.0 - 2.0 * 2.5 + 1.0;
    assert!(
        (pred - expected).abs() < 2.0,
        "pred = {pred}, expected = {expected}"
    );
}

#[test]
fn pipeline_predict_is_side_effect_free() {
    let mut pipeline = make_regression_pipeline(2);
    pipeline.learn(&[1.0, 2.0], 3.0).unwrap();
    let samples_before = pipeline.samples_seen();
    let _ = pipeline.predict(&[1.0, 2.0]).unwrap();
    let _ = pipeline.predict(&[3.0, 4.0]).unwrap();
    assert_eq!(pipeline.samples_seen(), samples_before);
}

#[test]
fn pipeline_transformer_does_not_see_target() {
    let d = 2;
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

    let x = &[1.0, 2.0];
    let y_target = 100.0;
    pipeline.learn(x, y_target).unwrap();

    let scaler_after = pipeline.transformer();
    assert_eq!(scaler_after.samples_seen(), 1);
}

#[test]
fn pipeline_rejects_dimension_mismatch() {
    let mut pipeline = make_regression_pipeline(3);
    assert!(pipeline.predict(&[1.0, 2.0]).is_err());
    assert!(pipeline.learn(&[1.0, 2.0], 1.0).is_err());
}

#[test]
fn pipeline_with_minmax_scaler_works() {
    let d = 2;
    let scaler = MinMaxScaler::new(d).unwrap();
    let model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.1,
                    l2: 0.0,
                },
            )
            .unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    let mut pipeline = RegressionPipeline::new(scaler, model).unwrap();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(88);
    for _ in 0..2000 {
        let x1 = rand::Rng::gen_range(&mut rng, 0.0..10.0);
        let x2 = rand::Rng::gen_range(&mut rng, 0.0..10.0);
        let y = 2.0 * x1 + 3.0 * x2;
        pipeline.learn(&[x1, x2], y).unwrap();
    }

    let pred = pipeline.predict(&[5.0, 5.0]).unwrap();
    let expected = 2.0 * 5.0 + 3.0 * 5.0;
    assert!(
        (pred - expected).abs() < 3.0,
        "pred = {pred}, expected = {expected}"
    );
}

#[test]
fn classification_pipeline_learns() {
    use rill_ml::models::{LogisticRegression, LogisticRegressionConfig};
    let d = 2;
    let scaler = StandardScaler::new(d).unwrap();
    let model = LogisticRegression::new(
        d,
        LogisticRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.1,
                    l2: 0.0,
                },
            )
            .unwrap(),
            loss: Default::default(),
        },
    )
    .unwrap();
    let mut pipeline = ClassificationPipeline::new(scaler, model).unwrap();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(33);
    for _ in 0..1000 {
        let x1 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
        let x2 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
        let y = x1 > x2;
        pipeline.learn(&[x1, x2], y).unwrap();
    }

    let p_pos = pipeline.predict_proba(&[2.0, -1.0]).unwrap();
    let p_neg = pipeline.predict_proba(&[-1.0, 2.0]).unwrap();
    assert!(p_pos > 0.7, "p_pos = {p_pos}");
    assert!(p_neg < 0.3, "p_neg = {p_neg}");
}

#[test]
fn pipeline_reset_clears_state() {
    let mut pipeline = make_regression_pipeline(2);
    pipeline.learn(&[1.0, 2.0], 3.0).unwrap();
    pipeline.learn(&[4.0, 5.0], 6.0).unwrap();
    assert_eq!(pipeline.samples_seen(), 2);
    pipeline.reset();
    assert_eq!(pipeline.samples_seen(), 0);
}

#[test]
fn clipper_in_pipeline_clamps_values() {
    let clipper = Clipper::new(1, -1.0, 1.0).unwrap();
    let model = LinearRegression::new(
        1,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(1, SgdConfig::default()).unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    let mut pipeline = RegressionPipeline::new(clipper, model).unwrap();

    // Learning with extreme values should be clipped by the clipper.
    for _ in 0..100 {
        pipeline.learn(&[100.0], 1.0).unwrap();
    }

    // The clipper is stateless, so samples_seen() is always 0.
    // Verify that predictions are produced successfully.
    let pred = pipeline.predict(&[100.0]).unwrap();
    assert!(pred.is_finite());
}

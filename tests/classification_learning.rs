//! Integration tests: classification model learning behavior.
//!
//! Verifies that `LogisticRegression` converges to a correct decision
//! boundary on linearly separable data.

use rand::SeedableRng;
use rill_ml::OnlineBinaryClassifier;
use rill_ml::models::{LogisticRegression, LogisticRegressionConfig};
use rill_ml::optim::{Optimizer, SgdConfig};

fn make_model(d: usize, lr: f64) -> LogisticRegression {
    LogisticRegression::new(
        d,
        LogisticRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: lr,
                    l2: 0.0,
                },
            )
            .unwrap(),
            loss: Default::default(),
        },
    )
    .unwrap()
}

#[test]
fn logistic_regression_separates_linearly() {
    let mut model = make_model(2, 0.5);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(123);

    // y = 1 when x1 > 0, y = 0 when x1 <= 0
    for _ in 0..2000 {
        let x1 = rand::Rng::gen_range(&mut rng, -3.0..3.0);
        let x2 = rand::Rng::gen_range(&mut rng, -3.0..3.0);
        let y = x1 > 0.0;
        model.learn(&[x1, x2], y).unwrap();
    }

    let p_pos = model.predict_proba(&[2.0, 0.0]).unwrap();
    let p_neg = model.predict_proba(&[-2.0, 0.0]).unwrap();
    assert!(p_pos > 0.9, "p_pos = {p_pos}");
    assert!(p_neg < 0.1, "p_neg = {p_neg}");

    assert!(model.predict(&[2.0, 0.0]).unwrap());
    assert!(!model.predict(&[-2.0, 0.0]).unwrap());
}

#[test]
fn logistic_regression_probabilities_bounded() {
    let mut model = make_model(3, 0.1);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);
    for _ in 0..500 {
        let x: Vec<f64> = (0..3)
            .map(|_| rand::Rng::gen_range(&mut rng, -5.0..5.0))
            .collect();
        let y = x[0] + x[1] > x[2];
        model.learn(&x, y).unwrap();
    }

    for _ in 0..100 {
        let x: Vec<f64> = (0..3)
            .map(|_| rand::Rng::gen_range(&mut rng, -10.0..10.0))
            .collect();
        let p = model.predict_proba(&x).unwrap();
        assert!((0.0..=1.0).contains(&p), "p = {p} should be in [0, 1]");
    }
}

#[test]
fn predict_does_not_update_state() {
    let model = make_model(2, 0.1);
    let before = model.samples_seen();
    let _ = model.predict(&[1.0, 2.0]).unwrap();
    let _ = model.predict_proba(&[3.0, 4.0]).unwrap();
    assert_eq!(model.samples_seen(), before);
}

#[test]
fn reset_clears_classifier_state() {
    let mut model = make_model(2, 0.1);
    model.learn(&[1.0, 1.0], true).unwrap();
    model.learn(&[-1.0, -1.0], false).unwrap();
    assert_eq!(model.samples_seen(), 2);
    model.reset();
    assert_eq!(model.samples_seen(), 0);
    let p = model.predict_proba(&[1.0, 1.0]).unwrap();
    assert!(
        (p - 0.5).abs() < 1e-12,
        "after reset p should be 0.5, got {p}"
    );
}

#[test]
fn dimension_mismatch_rejected() {
    let mut model = make_model(3, 0.1);
    assert!(model.predict_proba(&[1.0, 2.0]).is_err());
    assert!(model.learn(&[1.0, 2.0], true).is_err());
}

//! Integration tests for FTRL-Proximal learning.
//!
//! These tests verify that [`FtrlRegressor`] and [`FtrlClassifier`] converge
//! on known data-generating processes, that L1 regularization produces sparse
//! weights, that the models support dynamic feature growth, and that the
//! feature hasher integrates cleanly with the sparse classifier API.

use rand::SeedableRng;
use rill_ml::feature_hasher::FeatureHasher;
use rill_ml::loss::RegressionLoss;
use rill_ml::metrics::{F1Score, Mae};
use rill_ml::models::{
    FtrlClassifier, FtrlConfig, FtrlRegressor, LinearRegression, LinearRegressionConfig,
};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml::sparse::SparseFeatures;
use rill_ml::{Metric, OnlineRegressor, SparseClassifier, SparseRegressor};

/// Generate `n` regression samples following `y = 3*x1 + 2*x2`.
fn make_regression_data(n: usize) -> Vec<(SparseFeatures, f64)> {
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    let mut data = Vec::with_capacity(n);
    for _ in 0..n {
        let x1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let x2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let y = 3.0 * x1 + 2.0 * x2;
        let sf = SparseFeatures::from_sorted(vec![(0, x1), (1, x2)]).unwrap();
        data.push((sf, y));
    }
    data
}

/// Generate `n` binary classification samples with `label = (x1 + x2 > 1.0)`.
fn make_classification_data(n: usize) -> Vec<(SparseFeatures, bool)> {
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    let mut data = Vec::with_capacity(n);
    for _ in 0..n {
        let x1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let x2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let label = x1 + x2 > 1.0;
        let sf = SparseFeatures::from_sorted(vec![(0, x1), (1, x2)]).unwrap();
        data.push((sf, label));
    }
    data
}

// ---------------------------------------------------------------------------
// FtrlRegressor
// ---------------------------------------------------------------------------

#[test]
fn ftrl_regressor_converges_on_linear_data() {
    // On `y = 3*x1 + 2*x2`, the average MAE over the last 50 training
    // samples must be strictly lower than over the first 50, proving that
    // the model is learning the linear relationship.
    let mut model = FtrlRegressor::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 0.0,
        l2: 0.0,
    })
    .unwrap();

    let data = make_regression_data(500);
    let mut first_mae = Mae::new();
    let mut last_mae = Mae::new();

    for (i, (sf, y)) in data.iter().enumerate() {
        let pred = model.predict(sf).unwrap();
        if i < 50 {
            first_mae.update(*y, pred).unwrap();
        }
        if i >= 450 {
            last_mae.update(*y, pred).unwrap();
        }
        model.learn(sf, *y).unwrap();
    }

    let first = first_mae
        .value()
        .expect("first 50 samples should produce an MAE");
    let last = last_mae
        .value()
        .expect("last 50 samples should produce an MAE");
    assert!(
        last < first,
        "MAE should decrease over training: first={first}, last={last}"
    );
    assert!(
        last < 0.5,
        "final MAE should be small for a linear DGP, got {last}"
    );
}

#[test]
fn ftrl_regressor_comparable_to_linear_regression() {
    // On the same low-dimensional linear DGP, both FtrlRegressor and
    // LinearRegression should achieve an MAE below 1.0 after 500 steps.
    let data = make_regression_data(500);

    let mut ftrl = FtrlRegressor::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 0.0,
        l2: 0.0,
    })
    .unwrap();

    let d = 2;
    let mut linreg = LinearRegression::new(
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

    let mut ftrl_mae = Mae::new();
    let mut linreg_mae = Mae::new();

    for (sf, y) in &data {
        let dense = vec![sf.get(0).unwrap(), sf.get(1).unwrap()];

        let p_ftrl = ftrl.predict(sf).unwrap();
        ftrl_mae.update(*y, p_ftrl).unwrap();
        ftrl.learn(sf, *y).unwrap();

        let p_lin = linreg.predict(&dense).unwrap();
        linreg_mae.update(*y, p_lin).unwrap();
        linreg.learn(&dense, *y).unwrap();
    }

    let ftrl_val = ftrl_mae.value().unwrap();
    let lin_val = linreg_mae.value().unwrap();
    assert!(ftrl_val < 1.0, "FTRL MAE should be < 1.0, got {ftrl_val}");
    assert!(
        lin_val < 1.0,
        "LinearRegression MAE should be < 1.0, got {lin_val}"
    );
}

#[test]
fn ftrl_regressor_l1_sparsity() {
    // With a very high L1 coefficient, the FTRL soft-thresholding rule
    // (|z| <= lambda1 -> w = 0) should drive all feature weights to zero,
    // producing an empty weights() vector.
    let mut model = FtrlRegressor::new(FtrlConfig {
        alpha: 0.1,
        beta: 1.0,
        l1: 100.0,
        l2: 0.0,
    })
    .unwrap();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    for _ in 0..300 {
        let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
        let x2 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
        let y = 0.5 * x1 + 0.5 * x2;
        let sf = SparseFeatures::from_sorted(vec![(0, x1), (1, x2)]).unwrap();
        model.learn(&sf, y).unwrap();
    }

    let weights = model.weights();
    assert!(
        weights.is_empty(),
        "high L1 should zero all weights, got {weights:?}"
    );
}

#[test]
fn ftrl_regressor_dynamic_features() {
    // The model should accept new FeatureIds that were never seen in
    // earlier samples. feature_count() should grow as new ids appear,
    // and earlier features should remain tracked.
    let mut model = FtrlRegressor::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 0.0,
        l2: 0.0,
    })
    .unwrap();

    assert_eq!(model.feature_count(), 0);

    // Train with features 0 and 1.
    for _ in 0..50 {
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, 3.0).unwrap();
    }
    assert_eq!(model.feature_count(), 2);

    // Introduce a new feature id mid-training.
    for _ in 0..50 {
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0), (5, 4.0)]).unwrap();
        model.learn(&sf, 7.0).unwrap();
    }
    assert_eq!(model.feature_count(), 3);

    // Predictions should still succeed and not panic on the new id.
    let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0), (5, 4.0)]).unwrap();
    let pred = model.predict(&sf).unwrap();
    assert!(
        pred.is_finite(),
        "prediction must be finite after dynamic growth"
    );
}

// ---------------------------------------------------------------------------
// FtrlClassifier
// ---------------------------------------------------------------------------

#[test]
fn ftrl_classifier_converges() {
    // Train on separable data; the F1 score on the training samples should
    // exceed 0.5 after training, indicating the model has learned a useful
    // decision boundary.
    let mut model = FtrlClassifier::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 0.0,
        l2: 0.0,
    })
    .unwrap();

    let data = make_classification_data(500);

    // Train the model.
    for (sf, y) in &data {
        model.learn(sf, *y).unwrap();
    }

    // Evaluate F1 on the same data (training F1 should be high).
    let mut f1 = F1Score::default();
    for (sf, y) in &data {
        let pred = model.predict(sf).unwrap();
        f1.update(*y, pred).unwrap();
    }

    let score = f1.value().expect("F1 should be available after evaluation");
    assert!(
        score > 0.5,
        "F1 should be > 0.5 after training on separable data, got {score}"
    );
}

#[test]
fn ftrl_classifier_predict_proba_in_range() {
    // The sigmoid output must always lie strictly in (0, 1), regardless
    // of the magnitude of the input features.
    let mut model = FtrlClassifier::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 0.0,
        l2: 0.0,
    })
    .unwrap();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(13);
    for _ in 0..200 {
        let x1 = rand::Rng::gen_range(&mut rng, -5.0..5.0);
        let x2 = rand::Rng::gen_range(&mut rng, -5.0..5.0);
        let y = x1 + x2 > 0.0;
        let sf = SparseFeatures::from_sorted(vec![(0, x1), (1, x2)]).unwrap();
        model.learn(&sf, y).unwrap();
    }

    for _ in 0..100 {
        let x1 = rand::Rng::gen_range(&mut rng, -10.0..10.0);
        let x2 = rand::Rng::gen_range(&mut rng, -10.0..10.0);
        let sf = SparseFeatures::from_sorted(vec![(0, x1), (1, x2)]).unwrap();
        let p = model.predict_proba(&sf).unwrap();
        assert!(
            p > 0.0 && p < 1.0,
            "probability must be strictly in (0, 1), got {p}"
        );
    }
}

#[test]
fn ftrl_with_feature_hasher() {
    // End-to-end: hash string features into SparseFeatures, then train
    // an FtrlClassifier. The model should learn to discriminate between
    // two distinct sets of string features.
    let hasher = FeatureHasher::new(64, 42).unwrap();
    let mut model = FtrlClassifier::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 0.0,
        l2: 0.0,
    })
    .unwrap();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(31);
    for _ in 0..500 {
        // Class true: features from the "alpha" family.
        let pairs_true: Vec<(&str, f64)> = vec![
            ("alpha_a", 1.0),
            ("alpha_b", rand::Rng::gen_range(&mut rng, 0.5..1.5)),
        ];
        let sf_true = hasher.hash_strings(&pairs_true).unwrap();
        model.learn(&sf_true, true).unwrap();

        // Class false: features from the "beta" family.
        let pairs_false: Vec<(&str, f64)> = vec![
            ("beta_a", 1.0),
            ("beta_b", rand::Rng::gen_range(&mut rng, 0.5..1.5)),
        ];
        let sf_false = hasher.hash_strings(&pairs_false).unwrap();
        model.learn(&sf_false, false).unwrap();
    }

    // The model should now confidently distinguish the two families.
    let sf_true = hasher
        .hash_strings(&[("alpha_a", 1.0), ("alpha_b", 1.0)])
        .unwrap();
    let sf_false = hasher
        .hash_strings(&[("beta_a", 1.0), ("beta_b", 1.0)])
        .unwrap();

    let p_true = model.predict_proba(&sf_true).unwrap();
    let p_false = model.predict_proba(&sf_false).unwrap();
    assert!(
        p_true > 0.5,
        "alpha-family should predict > 0.5, got {p_true}"
    );
    assert!(
        p_false < 0.5,
        "beta-family should predict < 0.5, got {p_false}"
    );
    assert!(
        p_true > p_false,
        "alpha-family probability must exceed beta-family: {p_true} vs {p_false}"
    );
}

#[test]
fn ftrl_classifier_logloss_decreases() {
    // Compare the average LogLoss on the first 50 samples vs the last 50
    // samples. The latter should be substantially lower, demonstrating
    // that the classifier is becoming better calibrated over time.
    use rill_ml::metrics::LogLoss;

    let mut model = FtrlClassifier::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 0.0,
        l2: 0.0,
    })
    .unwrap();

    let data = make_classification_data(500);
    let mut first_loss = LogLoss::default();
    let mut last_loss = LogLoss::default();

    for (i, (sf, y)) in data.iter().enumerate() {
        let p = model.predict_proba(sf).unwrap();
        if i < 50 {
            first_loss.update(*y, p).unwrap();
        }
        if i >= 450 {
            last_loss.update(*y, p).unwrap();
        }
        model.learn(sf, *y).unwrap();
    }

    let first = first_loss
        .value()
        .expect("first 50 should produce a LogLoss");
    let last = last_loss.value().expect("last 50 should produce a LogLoss");
    assert!(
        last < first,
        "LogLoss should decrease over training: first={first}, last={last}"
    );
}

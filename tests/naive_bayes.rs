//! Integration tests for Naive Bayes classifiers.
//!
//! These tests cover [`GaussianNaiveBayes`], [`BernoulliNaiveBayes`], and
//! [`MultinomialNaiveBayes`] on synthetic data, including accuracy on
//! separable distributions, validation of input constraints, and a
//! comparison against [`LogisticRegression`] on shared data.

use rand::SeedableRng;
use rill_ml::loss::BinaryLogLoss;
use rill_ml::metrics::{Accuracy, F1Score};
use rill_ml::models::{
    BernoulliNaiveBayes, GaussianNaiveBayes, LogisticRegression, LogisticRegressionConfig,
    MultinomialNaiveBayes,
};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml::{Metric, OnlineBinaryClassifier};

/// Generate `n` samples from two well-separated Gaussian clusters:
/// class `true` is centered around (+3, +3), class `false` around (-3, -3),
/// each with uniform noise in [-1, 1] per dimension.
fn make_gaussian_data(n: usize) -> Vec<(Vec<f64>, bool)> {
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    let mut data = Vec::with_capacity(n);
    for _ in 0..n {
        let class = rand::Rng::gen_range(&mut rng, 0..2) == 1;
        let mean = if class { 3.0 } else { -3.0 };
        let x1 = mean + rand::Rng::gen_range(&mut rng, -1.0..1.0);
        let x2 = mean + rand::Rng::gen_range(&mut rng, -1.0..1.0);
        data.push((vec![x1, x2], class));
    }
    data
}

// ---------------------------------------------------------------------------
// GaussianNaiveBayes
// ---------------------------------------------------------------------------

#[test]
fn gaussian_nb_separable_data() {
    // On two well-separated Gaussian clusters, GaussianNB should achieve
    // high accuracy (> 0.8) after online training.
    let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
    let data = make_gaussian_data(400);

    for (x, y) in &data {
        model.learn(x, *y).unwrap();
    }

    let mut acc = Accuracy::default();
    for (x, y) in &data {
        let pred = model.predict(x).unwrap();
        acc.update(*y, pred).unwrap();
    }

    let value = acc.value().expect("accuracy should be available");
    assert!(
        value > 0.8,
        "GaussianNB should achieve accuracy > 0.8 on separable data, got {value}"
    );
}

#[test]
fn gaussian_nb_predict_proba_in_range() {
    // The sigmoid of the log-odds must always lie strictly in (0, 1).
    // We use overlapping classes (means at ±1 with large noise) so that
    // the log-odds stay bounded and sigmoid does not saturate to exactly
    // 0.0 or 1.0.
    let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);

    // Train on overlapping classes: means at ±1, noise in [-2, 2].
    for _ in 0..300 {
        let x1 = 1.0 + rand::Rng::gen_range(&mut rng, -2.0..2.0);
        let x2 = 1.0 + rand::Rng::gen_range(&mut rng, -2.0..2.0);
        model.learn(&[x1, x2], true).unwrap();
        let x1 = -1.0 + rand::Rng::gen_range(&mut rng, -2.0..2.0);
        let x2 = -1.0 + rand::Rng::gen_range(&mut rng, -2.0..2.0);
        model.learn(&[x1, x2], false).unwrap();
    }

    for _ in 0..100 {
        let x = vec![
            rand::Rng::gen_range(&mut rng, -2.0..2.0),
            rand::Rng::gen_range(&mut rng, -2.0..2.0),
        ];
        let p = model.predict_proba(&x).unwrap();
        assert!(
            p > 0.0 && p < 1.0,
            "probability must be strictly in (0, 1), got {p}"
        );
    }
}

// ---------------------------------------------------------------------------
// BernoulliNaiveBayes
// ---------------------------------------------------------------------------

#[test]
fn bernoulli_nb_binary_features() {
    // Train on binary features where class `true` tends to have feature 0
    // set and class `false` tends to have feature 1 set. Accuracy should
    // exceed 0.7.
    let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);

    for _ in 0..400 {
        // Class true: feature 0 is 1 with prob 0.9, feature 1 with prob 0.1.
        let f0 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.9 {
            1.0
        } else {
            0.0
        };
        let f1 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.1 {
            1.0
        } else {
            0.0
        };
        let f2 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.5 {
            1.0
        } else {
            0.0
        };
        model.learn(&[f0, f1, f2], true).unwrap();

        // Class false: feature 0 with prob 0.1, feature 1 with prob 0.9.
        let f0 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.1 {
            1.0
        } else {
            0.0
        };
        let f1 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.9 {
            1.0
        } else {
            0.0
        };
        let f2 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.5 {
            1.0
        } else {
            0.0
        };
        model.learn(&[f0, f1, f2], false).unwrap();
    }

    let mut acc = Accuracy::default();
    for _ in 0..200 {
        let f0 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.9 {
            1.0
        } else {
            0.0
        };
        let f1 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.1 {
            1.0
        } else {
            0.0
        };
        let f2 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.5 {
            1.0
        } else {
            0.0
        };
        let pred = model.predict(&[f0, f1, f2]).unwrap();
        acc.update(true, pred).unwrap();

        let f0 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.1 {
            1.0
        } else {
            0.0
        };
        let f1 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.9 {
            1.0
        } else {
            0.0
        };
        let f2 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.5 {
            1.0
        } else {
            0.0
        };
        let pred = model.predict(&[f0, f1, f2]).unwrap();
        acc.update(false, pred).unwrap();
    }

    let value = acc.value().expect("accuracy should be available");
    assert!(
        value > 0.7,
        "BernoulliNB should achieve accuracy > 0.7 on separable binary data, got {value}"
    );
}

#[test]
fn bernoulli_nb_rejects_negative() {
    // BernoulliNB requires non-negative inputs (binary features). Negative
    // values must be rejected in both learn() and predict_proba().
    let mut model = BernoulliNaiveBayes::new(2, Default::default()).unwrap();
    assert!(
        model.learn(&[-1.0, 0.0], true).is_err(),
        "learn must reject negative features"
    );
    assert!(
        model.predict_proba(&[-0.5, 0.0]).is_err(),
        "predict_proba must reject negative features"
    );

    // State should remain unchanged after the failed learn.
    assert_eq!(model.samples_seen(), 0);
}

// ---------------------------------------------------------------------------
// MultinomialNaiveBayes
// ---------------------------------------------------------------------------

#[test]
fn multinomial_nb_count_features() {
    // Train on count features: class `true` has high counts on features 0/1
    // and low counts on feature 2; class `false` is the opposite. Accuracy
    // should exceed 0.7.
    let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(11);

    for _ in 0..400 {
        // Class true: large counts on features 0 and 1.
        let f0 = rand::Rng::gen_range(&mut rng, 3.0..6.0);
        let f1 = rand::Rng::gen_range(&mut rng, 2.0..5.0);
        let f2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        model.learn(&[f0, f1, f2], true).unwrap();

        // Class false: large counts on feature 2.
        let f0 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let f1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let f2 = rand::Rng::gen_range(&mut rng, 3.0..6.0);
        model.learn(&[f0, f1, f2], false).unwrap();
    }

    let mut acc = Accuracy::default();
    for _ in 0..200 {
        let f0 = rand::Rng::gen_range(&mut rng, 3.0..6.0);
        let f1 = rand::Rng::gen_range(&mut rng, 2.0..5.0);
        let f2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let pred = model.predict(&[f0, f1, f2]).unwrap();
        acc.update(true, pred).unwrap();

        let f0 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let f1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let f2 = rand::Rng::gen_range(&mut rng, 3.0..6.0);
        let pred = model.predict(&[f0, f1, f2]).unwrap();
        acc.update(false, pred).unwrap();
    }

    let value = acc.value().expect("accuracy should be available");
    assert!(
        value > 0.7,
        "MultinomialNB should achieve accuracy > 0.7 on separable count data, got {value}"
    );
}

#[test]
fn multinomial_nb_rejects_negative() {
    // MultinomialNB requires non-negative inputs (counts). Negative values
    // must be rejected in both learn() and predict_proba().
    let mut model = MultinomialNaiveBayes::new(2, Default::default()).unwrap();
    assert!(
        model.learn(&[-1.0, 0.0], true).is_err(),
        "learn must reject negative features"
    );
    assert!(
        model.predict_proba(&[-0.5, 0.0]).is_err(),
        "predict_proba must reject negative features"
    );

    // State should remain unchanged after the failed learn.
    assert_eq!(model.samples_seen(), 0);
}

// ---------------------------------------------------------------------------
// GaussianNB vs LogisticRegression
// ---------------------------------------------------------------------------

#[test]
fn gaussian_nb_comparable_to_logistic() {
    // On the same well-separated Gaussian data, both GaussianNB and
    // LogisticRegression should achieve accuracy > 0.6.
    let data = make_gaussian_data(400);

    // Train GaussianNB.
    let mut gnb = GaussianNaiveBayes::new(2, Default::default()).unwrap();
    for (x, y) in &data {
        gnb.learn(x, *y).unwrap();
    }

    // Train LogisticRegression.
    let d = 2;
    let mut logreg = LogisticRegression::new(
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
            loss: BinaryLogLoss::new(),
        },
    )
    .unwrap();
    for (x, y) in &data {
        logreg.learn(x, *y).unwrap();
    }

    // Evaluate both on the training data.
    let mut gnb_acc = Accuracy::default();
    let mut logreg_acc = Accuracy::default();
    for (x, y) in &data {
        let p_gnb = gnb.predict(x).unwrap();
        gnb_acc.update(*y, p_gnb).unwrap();
        let p_log = logreg.predict(x).unwrap();
        logreg_acc.update(*y, p_log).unwrap();
    }

    let gnb_value = gnb_acc.value().unwrap();
    let log_value = logreg_acc.value().unwrap();
    assert!(
        gnb_value > 0.6,
        "GaussianNB accuracy should be > 0.6, got {gnb_value}"
    );
    assert!(
        log_value > 0.6,
        "LogisticRegression accuracy should be > 0.6, got {log_value}"
    );
}

// ---------------------------------------------------------------------------
// Predict does not update state
// ---------------------------------------------------------------------------

#[test]
fn naive_bayes_predict_does_not_update() {
    // Calling predict_proba() must not change samples_seen, regardless of
    // how many times it is invoked. This is part of the
    // OnlineBinaryClassifier contract.
    let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
    model.learn(&[1.0, 2.0], true).unwrap();
    model.learn(&[-1.0, -2.0], false).unwrap();
    let before = model.samples_seen();

    // Multiple predictions must not change state.
    for _ in 0..10 {
        let _ = model.predict_proba(&[0.5, 0.5]).unwrap();
    }
    assert_eq!(
        model.samples_seen(),
        before,
        "predict_proba must not increment samples_seen"
    );

    // Also verify F1 is computable and used (exercises the predict path).
    let mut f1 = F1Score::default();
    let pred = model.predict(&[0.5, 0.5]).unwrap();
    f1.update(true, pred).unwrap();
    assert!(f1.value().is_some());
}

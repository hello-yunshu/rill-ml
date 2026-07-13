//! Sparse feature classification example: click prediction.
//!
//! Demonstrates the v0.3 sparse feature pipeline:
//! 1. String feature names are hashed to FeatureIds via FeatureHasher.
//! 2. FtrlClassifier learns directly on sparse features (dynamic, L1-sparse).
//! 3. FeatureHasher + LogisticRegression learns on the hashed dense vector.
//! 4. BernoulliNaiveBayes learns on the hashed dense vector.
//!
//! Run with: `cargo run --example sparse_classification`

use rill_ml::feature_hasher::FeatureHasher;
use rill_ml::loss::BinaryLogLoss;
use rill_ml::metrics::{F1Score, LogLoss};
use rill_ml::models::{
    FtrlClassifier, FtrlConfig, GaussianNaiveBayes, LogisticRegression, LogisticRegressionConfig,
};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml::{Metric, OnlineBinaryClassifier, SparseClassifier};

/// Simulated ad click event.
struct ClickEvent {
    /// Feature name/value pairs (e.g. "user=alice", "device=mobile").
    features: Vec<(&'static str, f64)>,
    /// Whether the user clicked.
    clicked: bool,
}

fn generate_data() -> Vec<ClickEvent> {
    // Simplified click prediction: users "alice" and "bob" tend to click,
    // "charlie" rarely clicks. Mobile device correlates with clicks.
    // Time of day affects click rate.
    vec![
        ClickEvent {
            features: vec![
                ("user=alice", 1.0),
                ("device=mobile", 1.0),
                ("hour=morning", 1.0),
            ],
            clicked: true,
        },
        ClickEvent {
            features: vec![
                ("user=alice", 1.0),
                ("device=desktop", 1.0),
                ("hour=evening", 1.0),
            ],
            clicked: true,
        },
        ClickEvent {
            features: vec![
                ("user=bob", 1.0),
                ("device=mobile", 1.0),
                ("hour=afternoon", 1.0),
            ],
            clicked: true,
        },
        ClickEvent {
            features: vec![
                ("user=bob", 1.0),
                ("device=desktop", 1.0),
                ("hour=morning", 1.0),
            ],
            clicked: false,
        },
        ClickEvent {
            features: vec![
                ("user=charlie", 1.0),
                ("device=mobile", 1.0),
                ("hour=evening", 1.0),
            ],
            clicked: false,
        },
        ClickEvent {
            features: vec![
                ("user=charlie", 1.0),
                ("device=desktop", 1.0),
                ("hour=afternoon", 1.0),
            ],
            clicked: false,
        },
        ClickEvent {
            features: vec![
                ("user=alice", 1.0),
                ("device=mobile", 1.0),
                ("hour=afternoon", 1.0),
            ],
            clicked: true,
        },
        ClickEvent {
            features: vec![
                ("user=bob", 1.0),
                ("device=mobile", 1.0),
                ("hour=evening", 1.0),
            ],
            clicked: true,
        },
        ClickEvent {
            features: vec![
                ("user=charlie", 1.0),
                ("device=mobile", 1.0),
                ("hour=morning", 1.0),
            ],
            clicked: false,
        },
        ClickEvent {
            features: vec![
                ("user=alice", 1.0),
                ("device=desktop", 1.0),
                ("hour=afternoon", 1.0),
            ],
            clicked: true,
        },
        ClickEvent {
            features: vec![
                ("user=bob", 1.0),
                ("device=desktop", 1.0),
                ("hour=evening", 1.0),
            ],
            clicked: false,
        },
        ClickEvent {
            features: vec![
                ("user=charlie", 1.0),
                ("device=desktop", 1.0),
                ("hour=morning", 1.0),
            ],
            clicked: false,
        },
    ]
}

fn main() {
    let data = generate_data();
    let hasher = FeatureHasher::new(64, 42).unwrap();

    // FTRL classifier (sparse, direct on SparseFeatures)
    let mut ftrl = FtrlClassifier::new(FtrlConfig {
        alpha: 0.5,
        beta: 1.0,
        l1: 2.0,
        l2: 0.5,
    })
    .unwrap();

    // Logistic regression (dense, on hashed features)
    let d = 64;
    let log_reg = LogisticRegression::new(
        d,
        LogisticRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.1,
                    l2: 0.01,
                },
            )
            .unwrap(),
            loss: BinaryLogLoss::default(),
        },
    )
    .unwrap();
    let mut log_reg = log_reg;

    // Gaussian Naive Bayes (dense, on hashed features)
    let mut nb = GaussianNaiveBayes::new(d, Default::default()).unwrap();

    // Metrics
    let mut ftrl_f1 = F1Score::default();
    let mut logreg_f1 = F1Score::default();
    let mut nb_f1 = F1Score::default();
    let mut ftrl_logloss = LogLoss::default();
    let mut logreg_logloss = LogLoss::default();
    let mut nb_logloss = LogLoss::default();

    for event in &data {
        // Create sparse features from string pairs
        let sparse = hasher.hash_strings(&event.features).unwrap();
        let dense = hasher.transform(&sparse).unwrap();

        // FTRL prediction (sparse)
        let ftrl_proba = ftrl.predict_proba(&sparse).unwrap();
        let ftrl_pred = ftrl_proba >= 0.5;
        ftrl_f1.update(event.clicked, ftrl_pred).unwrap();
        ftrl_logloss.update(event.clicked, ftrl_proba).unwrap();

        // Logistic regression prediction (dense)
        let logreg_proba = log_reg.predict_proba(&dense).unwrap();
        let logreg_pred = logreg_proba >= 0.5;
        logreg_f1.update(event.clicked, logreg_pred).unwrap();
        logreg_logloss.update(event.clicked, logreg_proba).unwrap();

        // Naive Bayes prediction (dense)
        let nb_proba = nb.predict_proba(&dense).unwrap();
        let nb_pred = nb_proba >= 0.5;
        nb_f1.update(event.clicked, nb_pred).unwrap();
        nb_logloss.update(event.clicked, nb_proba).unwrap();

        // Learn from this sample
        ftrl.learn(&sparse, event.clicked).unwrap();
        log_reg.learn(&dense, event.clicked).unwrap();
        nb.learn(&dense, event.clicked).unwrap();
    }

    // Results
    println!("=== Click Prediction Results ({} samples) ===", data.len());
    println!();
    println!("{:<25} {:>10} {:>10}", "Model", "F1", "LogLoss");
    println!("{:-<47}", "");

    let f1_val = ftrl_f1.value().unwrap_or(0.0);
    let ll_val = ftrl_logloss.value().unwrap_or(0.0);
    println!("{:<25} {:>10.4} {:>10.4}", "FTRL (sparse)", f1_val, ll_val);

    let f1_val = logreg_f1.value().unwrap_or(0.0);
    let ll_val = logreg_logloss.value().unwrap_or(0.0);
    println!(
        "{:<25} {:>10.4} {:>10.4}",
        "Logistic (hashed)", f1_val, ll_val
    );

    let f1_val = nb_f1.value().unwrap_or(0.0);
    let ll_val = nb_logloss.value().unwrap_or(0.0);
    println!(
        "{:<25} {:>10.4} {:>10.4}",
        "GaussianNB (hashed)", f1_val, ll_val
    );

    // FTRL sparse weight analysis
    let weights = ftrl.weights();
    let total_features = ftrl.feature_count();
    let nonzero = weights.len();
    println!();
    println!("=== FTRL Sparsity ===");
    println!("Total features seen:  {}", total_features);
    println!("Non-zero weights:     {}", nonzero);
    if total_features > 0 {
        let sparsity = 1.0 - nonzero as f64 / total_features as f64;
        println!("Sparsity ratio:       {:.1}%", sparsity * 100.0);
    }

    println!();
    println!("=== Non-zero FTRL Weights ===");
    for (id, w) in &weights {
        println!("  Feature {:<6}: {:>+.6}", id, w);
    }
}

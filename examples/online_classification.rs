//! Online binary classification example.
//!
//! Demonstrates `LogisticRegression` with a `StandardScaler` on a streaming
//! binary classification task. The data distribution shifts slightly halfway
//! through to simulate real-world concept drift.

use rand::SeedableRng;
use rill_ml::{
    Metric, OnlineBinaryClassifier,
    metrics::{Accuracy, F1Score, LogLoss, Precision, Recall},
    models::{LogisticRegression, LogisticRegressionConfig},
    optim::{Optimizer, SgdConfig},
    pipeline::ClassificationPipeline,
    preprocessing::StandardScaler,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = 3;
    let scaler = StandardScaler::new(d)?;
    let model = LogisticRegression::new(
        d,
        LogisticRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.1,
                    l2: 0.0,
                },
            )?,
            loss: Default::default(),
        },
    )?;
    let mut pipeline = ClassificationPipeline::new(scaler, model)?;

    let mut accuracy = Accuracy::default();
    let mut precision = Precision::default();
    let mut recall = Recall::default();
    let mut f1 = F1Score::default();
    let mut log_loss = LogLoss::default();

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);
    let n = 1000;

    for i in 0..n {
        // Features in [0, 1).
        let x1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let x2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let x3 = rand::Rng::gen_range(&mut rng, 0.0..1.0);

        // Decision boundary shifts at t=500.
        let threshold = if i < 500 { 0.5 } else { 0.6 };
        let y = (x1 + x2 * 0.5 + x3 * 0.3) > threshold;

        // Progressive evaluation: predict, update metrics, learn.
        let pred = pipeline.predict(&[x1, x2, x3])?;
        let proba = pipeline.predict_proba(&[x1, x2, x3])?;

        accuracy.update(y, pred)?;
        precision.update(y, pred)?;
        recall.update(y, pred)?;
        f1.update(y, pred)?;
        log_loss.update(y, proba)?;

        pipeline.learn(&[x1, x2, x3], y)?;
    }

    println!("=== Online binary classification ===");
    println!("Samples: {n}\n");
    println!("Metric       Value");
    println!("----------------------");
    println!("Accuracy:    {:?}", accuracy.value());
    println!("Precision:   {:?}", precision.value());
    println!("Recall:      {:?}", recall.value());
    println!("F1:          {:?}", f1.value());
    println!("LogLoss:     {:?}", log_loss.value());

    Ok(())
}

//! Integration tests for rill-ml-tokio stream adapters.

use rill_ml::OnlineBinaryClassifier;
use rill_ml::OnlineRegressor;
use rill_ml::evaluate::{BinaryClassificationSample, RegressionSample};
use rill_ml::loss::BinaryLogLoss;
use rill_ml::metrics::{Accuracy, Mae};
use rill_ml::models::{
    BaselineConfig, LogisticRegression, LogisticRegressionConfig, MeanRegressor,
};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml_tokio::{progressive_classify_stream, progressive_regress_stream};
use tokio_stream::iter;

#[tokio::test]
async fn long_stream_increments_samples_seen() {
    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut mae = Mae::default();
    let samples: Vec<RegressionSample> = (0..500)
        .map(|i| RegressionSample {
            features: vec![],
            target: i as f64,
        })
        .collect();
    let n = samples.len();
    let stream = iter(samples);
    let final_value = progressive_regress_stream(&mut model, &mut mae, stream)
        .await
        .unwrap();
    assert!(final_value.is_some());
    assert_eq!(model.samples_seen(), n as u64);
}

#[tokio::test]
async fn stream_handles_finite_value_correctly() {
    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut mae = Mae::default();
    let samples = vec![
        RegressionSample {
            features: vec![],
            target: 5.0,
        },
        RegressionSample {
            features: vec![],
            target: 5.0,
        },
        RegressionSample {
            features: vec![],
            target: 5.0,
        },
    ];
    let stream = iter(samples);
    let final_value = progressive_regress_stream(&mut model, &mut mae, stream)
        .await
        .unwrap();
    // After 3 identical samples, mean is 5.0; errors: 5 (pred 0), 0, 0 → MAE = 5/3
    assert!((final_value.unwrap() - 5.0 / 3.0).abs() < 1e-9);
}

#[tokio::test]
async fn classify_stream_long_run_increments_samples() {
    let optimizer = Optimizer::sgd(
        1,
        SgdConfig {
            learning_rate: 0.1,
            l2: 0.0,
        },
    )
    .unwrap();
    let mut model = LogisticRegression::new(
        1,
        LogisticRegressionConfig {
            optimizer,
            loss: BinaryLogLoss::new(),
        },
    )
    .unwrap();
    let mut acc = Accuracy::default();
    let samples: Vec<BinaryClassificationSample> = (0..500)
        .map(|i| {
            let label = i % 2 == 0;
            let x = if label { 1.0 } else { -1.0 };
            BinaryClassificationSample {
                features: vec![x],
                target: label,
            }
        })
        .collect();
    let n = samples.len();
    let stream = iter(samples);
    let final_value = progressive_classify_stream(&mut model, &mut acc, stream)
        .await
        .unwrap();
    assert!(final_value.is_some());
    assert_eq!(model.samples_seen(), n as u64);
}

#[tokio::test]
async fn classify_stream_empty_returns_none() {
    let optimizer = Optimizer::sgd(
        1,
        SgdConfig {
            learning_rate: 0.1,
            l2: 0.0,
        },
    )
    .unwrap();
    let mut model = LogisticRegression::new(
        1,
        LogisticRegressionConfig {
            optimizer,
            loss: BinaryLogLoss::new(),
        },
    )
    .unwrap();
    let mut acc = Accuracy::default();
    let stream = iter::<Vec<BinaryClassificationSample>>(vec![]);
    let final_value = progressive_classify_stream(&mut model, &mut acc, stream)
        .await
        .unwrap();
    assert!(final_value.is_none());
    assert_eq!(model.samples_seen(), 0);
}

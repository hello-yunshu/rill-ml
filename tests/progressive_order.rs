//! Integration tests: progressive evaluation order (predict → metric.update → learn).
//!
//! These tests verify the core contract of progressive evaluation: the model
//! is evaluated on its prediction *before* learning from each sample, ensuring
//! an honest measure of generalization on streaming data.

use rill_ml::evaluate::{
    BinaryClassificationSample, RegressionSample, evaluate_binary_classification,
    evaluate_regression, evaluate_regression_with_steps,
};
use rill_ml::metrics::{Accuracy, Mae, Mse};
use rill_ml::models::{BaselineConfig, MeanRegressor};

#[test]
fn progressive_regression_predicts_before_learning() {
    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut mae = Mae::default();
    let samples = vec![
        RegressionSample {
            features: vec![],
            target: 10.0,
        },
        RegressionSample {
            features: vec![],
            target: 20.0,
        },
        RegressionSample {
            features: vec![],
            target: 30.0,
        },
    ];

    let (final_mae, steps) = evaluate_regression_with_steps(&mut model, &mut mae, samples).unwrap();

    // Step 0: predict 0.0 (initial), truth 10.0 → err 10
    // Step 1: predict 10.0 (mean of [10]), truth 20.0 → err 10
    // Step 2: predict 15.0 (mean of [10,20]), truth 30.0 → err 15
    // MAE = (10+10+15)/3 = 35/3
    assert!((final_mae.unwrap() - 35.0 / 3.0).abs() < 1e-9);
    assert_eq!(steps.len(), 3);

    let errors: Vec<f64> = steps.iter().map(|s| s.metric_value.unwrap()).collect();
    assert!((errors[0] - 10.0).abs() < 1e-9);
    assert!((errors[1] - 10.0).abs() < 1e-9);
    assert!((errors[2] - 35.0 / 3.0).abs() < 1e-9);
}

#[test]
fn progressive_regression_with_mse() {
    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut mse = Mse::default();
    let samples = vec![
        RegressionSample {
            features: vec![],
            target: 2.0,
        },
        RegressionSample {
            features: vec![],
            target: 4.0,
        },
    ];

    let final_mse = evaluate_regression(&mut model, &mut mse, samples).unwrap();
    // Step 0: predict 0.0, truth 2.0 → err^2 = 4
    // Step 1: predict 2.0 (mean of [2]), truth 4.0 → err^2 = 4
    // MSE = (4+4)/2 = 4
    assert!((final_mse.unwrap() - 4.0).abs() < 1e-9);
}

#[test]
fn progressive_classification_tracks_accuracy() {
    use rill_ml::models::{LogisticRegression, LogisticRegressionConfig};
    use rill_ml::optim::{Optimizer, SgdConfig};
    let d = 1;
    let mut model = LogisticRegression::new(
        d,
        LogisticRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.5,
                    l2: 0.0,
                },
            )
            .unwrap(),
            loss: Default::default(),
        },
    )
    .unwrap();

    let mut accuracy = Accuracy::default();
    let samples: Vec<BinaryClassificationSample> = (0..200)
        .map(|i| BinaryClassificationSample {
            features: vec![if i % 2 == 0 { 1.0 } else { -1.0 }],
            target: i % 2 == 0,
        })
        .collect();

    let final_acc = evaluate_binary_classification(&mut model, &mut accuracy, samples).unwrap();
    assert!(
        final_acc.unwrap() > 0.8,
        "accuracy should be high after learning, got {:?}",
        final_acc
    );
}

#[test]
fn progressive_steps_record_index_sequence() {
    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut mae = Mae::default();
    let samples: Vec<RegressionSample> = (0..10)
        .map(|i| RegressionSample {
            features: vec![],
            target: i as f64,
        })
        .collect();

    let (_, steps) = evaluate_regression_with_steps(&mut model, &mut mae, samples).unwrap();

    for (i, step) in steps.iter().enumerate() {
        assert_eq!(step.index, i);
    }
}

#[test]
fn progressive_empty_stream_returns_none() {
    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut mae = Mae::default();
    let samples: Vec<RegressionSample> = vec![];
    let result = evaluate_regression(&mut model, &mut mae, samples).unwrap();
    assert!(result.is_none());
}

//! Integration tests for rill-ml-polars.

use polars::prelude::*;
use rill_ml::OnlineRegressor;
use rill_ml::evaluate::{RegressionSample, evaluate_regression};
use rill_ml::metrics::Mae;
use rill_ml::models::{BaselineConfig, MeanRegressor};
use rill_ml_polars::{append_predictions_column, frame_to_samples};

#[test]
fn polars_frame_drives_rillml_model() {
    let df = df! {
        "x" => &[1.0_f64, 2.0, 3.0, 4.0],
        "y" => &[2.0_f64, 4.0, 6.0, 8.0],
    }
    .unwrap();

    let pairs = frame_to_samples(&df, &["x"], "y").unwrap();
    let samples: Vec<RegressionSample> = pairs
        .into_iter()
        .map(|(features, target)| RegressionSample { features, target })
        .collect();

    let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let mut mae = Mae::default();
    let _ = evaluate_regression(&mut model, &mut mae, samples).unwrap();
    assert_eq!(model.samples_seen(), 4);

    let predictions: Vec<f64> = (0..4)
        .map(|_| model.predict(&[1.0]).unwrap_or(0.0))
        .collect();
    let extended = append_predictions_column(&df, &predictions, "pred").unwrap();
    assert_eq!(extended.width(), 3);
    assert!(extended.column("pred").is_ok());
}

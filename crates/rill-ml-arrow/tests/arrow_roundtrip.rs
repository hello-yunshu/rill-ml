//! Integration tests for rill-ml-arrow.

use arrow::array::{Float64Array, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use rill_ml::OnlineRegressor;
use rill_ml::evaluate::RegressionSample;
use rill_ml::evaluate::evaluate_regression;
use rill_ml::metrics::Mae;
use rill_ml::models::{BaselineConfig, MeanRegressor};
use rill_ml_arrow::{append_predictions_column, floats_to_arrow_array, record_batch_to_features};
use std::sync::Arc;

#[test]
fn arrow_batch_drives_rillml_model() {
    let schema = Arc::new(Schema::new(vec![
        Field::new("x", DataType::Float64, false),
        Field::new("y", DataType::Float64, false),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0, 4.0])),
            Arc::new(Float64Array::from(vec![2.0, 4.0, 6.0, 8.0])),
        ],
    )
    .unwrap();

    let pairs = record_batch_to_features(&batch, &["x"], "y").unwrap();
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
    let extended = append_predictions_column(&batch, &predictions, "pred").unwrap();
    assert_eq!(extended.num_columns(), 3);
    assert!(extended.column_by_name("pred").is_some());
}

#[test]
fn floats_to_arrow_roundtrip() {
    let values = vec![0.1, 0.2, 0.3];
    let arr = floats_to_arrow_array(&values);
    for (i, v) in values.iter().enumerate() {
        assert!((arr.value(i) - v).abs() < 1e-12);
    }
}

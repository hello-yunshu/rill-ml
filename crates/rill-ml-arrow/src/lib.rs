//! Apache Arrow conversion helpers for RillML.
//!
//! RillML's core crate works with dense `&[f64]` slices. This adapter crate
//! bridges Apache Arrow's columnar `RecordBatch` / `Float64Array` to that
//! representation without bringing Arrow into the core crate.
//!
//! # Example
//!
//! ```
//! use arrow::array::{Float64Array, RecordBatch};
//! use arrow::datatypes::{DataType, Field, Schema};
//! use std::sync::Arc;
//! use rill_ml_arrow::record_batch_to_features;
//!
//! let schema = Arc::new(Schema::new(vec![
//!     Field::new("x1", DataType::Float64, false),
//!     Field::new("x2", DataType::Float64, false),
//!     Field::new("y", DataType::Float64, false),
//! ]));
//! let batch = RecordBatch::try_new(
//!     schema,
//!     vec![
//!         Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0])),
//!         Arc::new(Float64Array::from(vec![0.5, 1.5, 2.5])),
//!         Arc::new(Float64Array::from(vec![10.0, 20.0, 30.0])),
//!     ],
//! ).unwrap();
//! let samples = record_batch_to_features(&batch, &["x1", "x2"], "y").unwrap();
//! assert_eq!(samples.len(), 3);
//! assert_eq!(samples[0].0, vec![1.0, 0.5]);
//! assert!((samples[0].1 - 10.0).abs() < 1e-12);
//! ```

use arrow::array::{Array, Float64Array, RecordBatch};
use thiserror::Error;

/// Errors returned by Arrow conversion helpers.
#[derive(Debug, Error)]
pub enum ArrowConversionError {
    #[error("column `{0}` not found in record batch")]
    ColumnNotFound(String),
    #[error("column `{0}` is not Float64")]
    NotFloat64(String),
    #[error("row count mismatch: features={features}, target={target}")]
    RowCountMismatch { features: usize, target: usize },
    #[error("feature column count mismatch: expected {expected}, got {actual}")]
    FeatureCountMismatch { expected: usize, actual: usize },
    #[error("at least one feature column is required")]
    EmptyFeatureColumns,
    #[error("null value encountered at row {row} in column `{column}`")]
    NullValue { row: usize, column: String },
    #[error("arrow schema build failed: {0}")]
    SchemaBuild(String),
}

/// Extract `(features, target)` pairs from a `RecordBatch`.
///
/// `feature_columns` lists the column names to concatenate (in order) into the
/// feature vector. `target_column` is the regression target.
///
/// # Errors
///
/// Returns `ArrowConversionError` if a column is missing, not Float64, or
/// contains null values.
pub fn record_batch_to_features(
    batch: &RecordBatch,
    feature_columns: &[&str],
    target_column: &str,
) -> Result<Vec<(Vec<f64>, f64)>, ArrowConversionError> {
    let num_rows = batch.num_rows();
    if feature_columns.is_empty() {
        return Err(ArrowConversionError::EmptyFeatureColumns);
    }

    let mut feature_arrays: Vec<&Float64Array> = Vec::with_capacity(feature_columns.len());
    for name in feature_columns {
        let arr = batch
            .column_by_name(name)
            .ok_or_else(|| ArrowConversionError::ColumnNotFound((*name).to_string()))?;
        let f64_arr = arr
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or_else(|| ArrowConversionError::NotFloat64((*name).to_string()))?;
        if f64_arr.null_count() != 0 {
            for row in 0..num_rows {
                if f64_arr.is_null(row) {
                    return Err(ArrowConversionError::NullValue {
                        row,
                        column: (*name).to_string(),
                    });
                }
            }
        }
        feature_arrays.push(f64_arr);
    }

    let target_arr = batch
        .column_by_name(target_column)
        .ok_or_else(|| ArrowConversionError::ColumnNotFound(target_column.to_string()))?;
    let target_f64 = target_arr
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| ArrowConversionError::NotFloat64(target_column.to_string()))?;
    if target_f64.null_count() != 0 {
        for row in 0..num_rows {
            if target_f64.is_null(row) {
                return Err(ArrowConversionError::NullValue {
                    row,
                    column: target_column.to_string(),
                });
            }
        }
    }

    let mut out = Vec::with_capacity(num_rows);
    for row in 0..num_rows {
        let mut features = Vec::with_capacity(feature_arrays.len());
        for arr in &feature_arrays {
            features.push(arr.value(row));
        }
        let target = target_f64.value(row);
        out.push((features, target));
    }
    Ok(out)
}

/// Convert a `&[f64]` slice into an Arrow `Float64Array`.
pub fn floats_to_arrow_array(values: &[f64]) -> Float64Array {
    Float64Array::from(values.to_vec())
}

/// Append a `predictions` column to an existing `RecordBatch`.
///
/// The new column is added with the given `column_name`. All existing columns
/// are preserved.
///
/// # Errors
///
/// Returns `ArrowConversionError` if `predictions.len()` does not match the
/// batch row count.
pub fn append_predictions_column(
    batch: &RecordBatch,
    predictions: &[f64],
    column_name: &str,
) -> Result<RecordBatch, ArrowConversionError> {
    if predictions.len() != batch.num_rows() {
        return Err(ArrowConversionError::RowCountMismatch {
            features: batch.num_rows(),
            target: predictions.len(),
        });
    }

    let mut fields: Vec<arrow::datatypes::Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect();
    fields.push(arrow::datatypes::Field::new(
        column_name,
        arrow::datatypes::DataType::Float64,
        false,
    ));
    let new_schema = std::sync::Arc::new(arrow::datatypes::Schema::new(fields));

    let mut columns: Vec<std::sync::Arc<dyn Array>> = batch.columns().to_vec();
    columns.push(std::sync::Arc::new(Float64Array::from(
        predictions.to_vec(),
    )));
    RecordBatch::try_new(new_schema, columns)
        .map_err(|e| ArrowConversionError::SchemaBuild(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Float64Array, Int32Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    fn make_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("x1", DataType::Float64, false),
            Field::new("x2", DataType::Float64, false),
            Field::new("y", DataType::Float64, false),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0])),
                Arc::new(Float64Array::from(vec![0.5, 1.5, 2.5])),
                Arc::new(Float64Array::from(vec![10.0, 20.0, 30.0])),
            ],
        )
        .unwrap()
    }

    #[test]
    fn extracts_features_and_target() {
        let batch = make_batch();
        let samples = record_batch_to_features(&batch, &["x1", "x2"], "y").unwrap();
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].0, vec![1.0, 0.5]);
        assert!((samples[0].1 - 10.0).abs() < 1e-12);
        assert_eq!(samples[2].0, vec![3.0, 2.5]);
        assert!((samples[2].1 - 30.0).abs() < 1e-12);
    }

    #[test]
    fn missing_column_errors() {
        let batch = make_batch();
        let err = record_batch_to_features(&batch, &["x1", "missing"], "y").unwrap_err();
        assert!(matches!(err, ArrowConversionError::ColumnNotFound(_)));
    }

    #[test]
    fn non_float_column_errors() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("x1", DataType::Int32, false),
            Field::new("y", DataType::Float64, false),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int32Array::from(vec![1, 2, 3])),
                Arc::new(Float64Array::from(vec![10.0, 20.0, 30.0])),
            ],
        )
        .unwrap();
        let err = record_batch_to_features(&batch, &["x1"], "y").unwrap_err();
        assert!(matches!(err, ArrowConversionError::NotFloat64(_)));
    }

    #[test]
    fn floats_to_arrow_roundtrip() {
        let values = vec![1.5, 2.5, 3.5];
        let arr = floats_to_arrow_array(&values);
        assert_eq!(arr.len(), 3);
        assert!((arr.value(0) - 1.5).abs() < 1e-12);
        assert!((arr.value(2) - 3.5).abs() < 1e-12);
    }

    #[test]
    fn append_predictions_column_adds_column() {
        let batch = make_batch();
        let predictions = vec![11.0, 22.0, 33.0];
        let new_batch = append_predictions_column(&batch, &predictions, "pred").unwrap();
        assert_eq!(new_batch.num_columns(), 4);
        assert!(new_batch.column_by_name("pred").is_some());
        let pred_col = new_batch
            .column_by_name("pred")
            .unwrap()
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        assert!((pred_col.value(0) - 11.0).abs() < 1e-12);
    }

    #[test]
    fn append_predictions_mismatched_length_errors() {
        let batch = make_batch();
        let predictions = vec![1.0, 2.0]; // batch has 3 rows
        let err = append_predictions_column(&batch, &predictions, "pred").unwrap_err();
        assert!(matches!(err, ArrowConversionError::RowCountMismatch { .. }));
    }
}

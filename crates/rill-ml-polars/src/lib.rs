//! Polars DataFrame conversion helpers for RillML.
//!
//! Bridges Polars `DataFrame` and `(features, target)` sample pairs without
//! bringing a DataFrame engine into the core crate.
//!
//! # Example
//!
//! ```
//! use polars::prelude::*;
//! use rill_ml_polars::frame_to_samples;
//!
//! let df = df! {
//!     "x1" => &[1.0_f64, 2.0, 3.0],
//!     "x2" => &[0.5_f64, 1.5, 2.5],
//!     "y"  => &[10.0_f64, 20.0, 30.0],
//! }.unwrap();
//! let samples = frame_to_samples(&df, &["x1", "x2"], "y").unwrap();
//! assert_eq!(samples.len(), 3);
//! assert_eq!(samples[0].0, vec![1.0, 0.5]);
//! assert!((samples[0].1 - 10.0).abs() < 1e-12);
//! ```

use polars::prelude::*;
use thiserror::Error;

/// Errors returned by Polars conversion helpers.
#[derive(Debug, Error)]
pub enum PolarsConversionError {
    #[error("polars error: {0}")]
    Polars(#[from] polars::error::PolarsError),
    #[error("column `{0}` not found")]
    ColumnNotFound(String),
    #[error("column `{0}` is not Float64 or Float32")]
    NotFloat(String),
    #[error("feature column count mismatch: expected {expected}, got {actual}")]
    FeatureCountMismatch { expected: usize, actual: usize },
    #[error("at least one feature column is required")]
    EmptyFeatureColumns,
    #[error("row count mismatch: features={features}, predictions={predictions}")]
    RowCountMismatch { features: usize, predictions: usize },
}

fn column_as_f64_vec(column: &Column, name: &str) -> Result<Vec<f64>, PolarsConversionError> {
    let series = column.as_materialized_series();
    if series.dtype() == &DataType::Float64 {
        let chunked = series.f64()?;
        chunked
            .iter()
            .map(|v| v.ok_or_else(|| PolarsConversionError::NotFloat(name.to_string())))
            .collect()
    } else if series.dtype() == &DataType::Float32 {
        let chunked = series.f32()?;
        chunked
            .iter()
            .map(|v| v.ok_or_else(|| PolarsConversionError::NotFloat(name.to_string())))
            .map(|v| v.map(|x| x as f64))
            .collect()
    } else {
        Err(PolarsConversionError::NotFloat(name.to_string()))
    }
}

/// Extract `(features, target)` pairs from a Polars `DataFrame`.
///
/// `feature_cols` lists the column names to concatenate into the feature
/// vector (in order). `target_col` is the regression target.
///
/// # Errors
///
/// Returns `PolarsConversionError` if a column is missing, not a float type,
/// or contains nulls.
pub fn frame_to_samples(
    df: &DataFrame,
    feature_cols: &[&str],
    target_col: &str,
) -> Result<Vec<(Vec<f64>, f64)>, PolarsConversionError> {
    if feature_cols.is_empty() {
        return Err(PolarsConversionError::EmptyFeatureColumns);
    }
    let num_rows = df.height();

    let mut feature_columns: Vec<Vec<f64>> = Vec::with_capacity(feature_cols.len());
    for name in feature_cols {
        let col = df
            .column(name)
            .map_err(|_| PolarsConversionError::ColumnNotFound((*name).to_string()))?;
        feature_columns.push(column_as_f64_vec(col, name)?);
    }
    let target_col_ref = df
        .column(target_col)
        .map_err(|_| PolarsConversionError::ColumnNotFound(target_col.to_string()))?;
    let target_values = column_as_f64_vec(target_col_ref, target_col)?;

    let mut out = Vec::with_capacity(num_rows);
    for row in 0..num_rows {
        let mut features = Vec::with_capacity(feature_columns.len());
        for col in &feature_columns {
            features.push(col[row]);
        }
        out.push((features, target_values[row]));
    }
    Ok(out)
}

/// Append a column of predictions to an existing `DataFrame`.
///
/// # Errors
///
/// Returns `PolarsConversionError` if `predictions.len()` does not match the
/// frame height.
pub fn append_predictions_column(
    df: &DataFrame,
    predictions: &[f64],
    column_name: &str,
) -> Result<DataFrame, PolarsConversionError> {
    if predictions.len() != df.height() {
        return Err(PolarsConversionError::RowCountMismatch {
            features: df.height(),
            predictions: predictions.len(),
        });
    }
    let series = Series::new(column_name.into(), predictions.to_vec());
    let mut out = df.clone();
    out.with_column(series)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame() -> DataFrame {
        df! {
            "x1" => &[1.0_f64, 2.0, 3.0],
            "x2" => &[0.5_f64, 1.5, 2.5],
            "y"  => &[10.0_f64, 20.0, 30.0],
        }
        .unwrap()
    }

    #[test]
    fn extracts_features_and_target() {
        let df = make_frame();
        let samples = frame_to_samples(&df, &["x1", "x2"], "y").unwrap();
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].0, vec![1.0, 0.5]);
        assert!((samples[0].1 - 10.0).abs() < 1e-12);
        assert_eq!(samples[2].0, vec![3.0, 2.5]);
        assert!((samples[2].1 - 30.0).abs() < 1e-12);
    }

    #[test]
    fn missing_column_errors() {
        let df = make_frame();
        let err = frame_to_samples(&df, &["x1", "missing"], "y").unwrap_err();
        assert!(matches!(err, PolarsConversionError::ColumnNotFound(_)));
    }

    #[test]
    fn append_predictions_adds_column() {
        let df = make_frame();
        let predictions = vec![11.0, 22.0, 33.0];
        let new_df = append_predictions_column(&df, &predictions, "pred").unwrap();
        assert_eq!(new_df.width(), 4);
        assert!(new_df.column("pred").is_ok());
        let pred_col = new_df.column("pred").unwrap();
        let pred_values = column_as_f64_vec(pred_col, "pred").unwrap();
        assert!((pred_values[0] - 11.0).abs() < 1e-12);
        assert!((pred_values[2] - 33.0).abs() < 1e-12);
    }

    #[test]
    fn append_predictions_mismatched_length_errors() {
        let df = make_frame();
        let predictions = vec![1.0, 2.0]; // df has 3 rows
        let err = append_predictions_column(&df, &predictions, "pred").unwrap_err();
        assert!(matches!(
            err,
            PolarsConversionError::RowCountMismatch { .. }
        ));
    }
}

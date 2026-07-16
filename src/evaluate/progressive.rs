//! Progressive evaluation implementation.

use crate::error::{RillError, ensure_finite, ensure_finite_target};
use crate::traits::{Metric, OnlineBinaryClassifier, OnlineRegressor};

/// A single regression sample.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RegressionSample {
    /// Feature vector.
    pub features: Vec<f64>,
    /// Target value.
    pub target: f64,
}

/// A single binary classification sample.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BinaryClassificationSample {
    /// Feature vector.
    pub features: Vec<f64>,
    /// Boolean label.
    pub target: bool,
}

/// A single step of progressive evaluation, recording the prediction made
/// *before* learning from this sample.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgressiveStep {
    /// Zero-based index of the sample in the stream.
    pub index: usize,
    /// The metric value after incorporating this prediction.
    pub metric_value: Option<f64>,
}

/// Run progressive evaluation on a regression stream.
///
/// The model is updated in place. Returns the final metric value.
///
/// The evaluation order is strictly `predict → metric.update → learn`.
pub fn evaluate_regression<M, Met>(
    model: &mut M,
    metric: &mut Met,
    samples: impl IntoIterator<Item = RegressionSample>,
) -> Result<Option<f64>, RillError>
where
    M: OnlineRegressor,
    Met: Metric<Truth = f64, Prediction = f64>,
{
    evaluate_regression_with_steps(model, metric, samples).map(|(v, _)| v)
}

/// Like [`evaluate_regression`] but also collects per-step records.
pub fn evaluate_regression_with_steps<M, Met>(
    model: &mut M,
    metric: &mut Met,
    samples: impl IntoIterator<Item = RegressionSample>,
) -> Result<(Option<f64>, Vec<ProgressiveStep>), RillError>
where
    M: OnlineRegressor,
    Met: Metric<Truth = f64, Prediction = f64>,
{
    let mut steps = Vec::new();
    for (i, sample) in samples.into_iter().enumerate() {
        // 0. validate inputs (defense-in-depth before predict)
        ensure_finite_target(sample.target)?;
        // 1. predict (no state change)
        let prediction = model.predict(&sample.features)?;
        // 1a. validate prediction before passing to metric
        ensure_finite("prediction", prediction)?;
        // 2. update metric with truth and prediction
        metric.update(sample.target, prediction)?;
        // 3. learn from this sample
        model.learn(&sample.features, sample.target)?;
        steps.push(ProgressiveStep {
            index: i,
            metric_value: metric.value(),
        });
    }
    Ok((metric.value(), steps))
}

/// Run progressive evaluation on a binary classification stream.
pub fn evaluate_binary_classification<M, Met>(
    model: &mut M,
    metric: &mut Met,
    samples: impl IntoIterator<Item = BinaryClassificationSample>,
) -> Result<Option<f64>, RillError>
where
    M: OnlineBinaryClassifier,
    Met: Metric<Truth = bool, Prediction = bool>,
{
    for sample in samples.into_iter() {
        // prediction is bool for classifiers — no ensure_finite needed.
        let prediction = model.predict(&sample.features)?;
        metric.update(sample.target, prediction)?;
        model.learn(&sample.features, sample.target)?;
    }
    Ok(metric.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Mae;
    use crate::models::MeanRegressor;

    #[test]
    fn progressive_evaluates_before_learning() {
        // MeanRegressor: first prediction is initial_prediction (0.0),
        // then mean of targets seen so far.
        let mut model = MeanRegressor::default();
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
        let (final_mae, steps) =
            evaluate_regression_with_steps(&mut model, &mut mae, samples).unwrap();

        // Step 0: predict 0.0, truth 10.0 -> err 10
        // Step 1: predict 10.0 (mean of [10]), truth 20.0 -> err 10
        // Step 2: predict 15.0 (mean of [10,20]), truth 30.0 -> err 15
        // MAE = (10+10+15)/3 = 11.666...
        assert!((final_mae.unwrap() - 35.0 / 3.0).abs() < 1e-9);
        assert_eq!(steps.len(), 3);
    }
}

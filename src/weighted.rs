//! Additive weighted-learning APIs and Preview weighted accumulators.
//!
//! The frozen unweighted traits and state layouts are unchanged. A zero
//! weight validates inputs but performs no state update and does not increment
//! `samples_seen`.

use crate::error::{RillError, checked_finite_add, checked_increment, ensure_finite};
#[cfg(feature = "serde")]
use crate::persistence::ValidateState;
use crate::traits::{OnlineBinaryClassifier, OnlineRegressor};

/// Online statistic accepting finite, non-negative sample weights.
pub trait WeightedStatistic {
    /// Incorporate a value with a caller-defined weight.
    fn update_weighted(&mut self, value: f64, weight: f64) -> Result<(), RillError>;
    /// Sum of accepted positive weights.
    fn total_weight(&self) -> f64;
    /// Number of accepted positive-weight observations.
    fn samples_seen(&self) -> u64;
    /// Reset values, total weight, and sample count.
    fn reset(&mut self);
}

/// Additive weighted update contract for online regressors.
pub trait WeightedOnlineRegressor: OnlineRegressor {
    /// Learn from a finite non-negative weighted sample.
    fn learn_weighted(
        &mut self,
        features: &[f64],
        target: f64,
        weight: f64,
    ) -> Result<(), RillError>;
}

/// Additive weighted update contract for online binary classifiers.
pub trait WeightedOnlineBinaryClassifier: OnlineBinaryClassifier {
    /// Learn from a finite non-negative weighted sample.
    fn learn_weighted(
        &mut self,
        features: &[f64],
        target: bool,
        weight: f64,
    ) -> Result<(), RillError>;
}

/// Validate the shared weight contract.
pub fn validate_weight(weight: f64) -> Result<(), RillError> {
    ensure_finite("weight", weight)?;
    if weight < 0.0 {
        return Err(RillError::InvalidParameter {
            name: "weight",
            value: weight,
        });
    }
    Ok(())
}

/// Preview O(1)-space weighted arithmetic mean.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct WeightedMean {
    total_weight: f64,
    mean: f64,
    samples_seen: u64,
}

impl WeightedMean {
    /// Create an empty weighted mean.
    pub const fn new() -> Self {
        Self {
            total_weight: 0.0,
            mean: 0.0,
            samples_seen: 0,
        }
    }

    /// Current weighted mean, or `None` before a positive weight is seen.
    pub fn value(&self) -> Option<f64> {
        (self.total_weight > 0.0).then_some(self.mean)
    }
}

impl WeightedStatistic for WeightedMean {
    fn update_weighted(&mut self, value: f64, weight: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        validate_weight(weight)?;
        if weight == 0.0 {
            return Ok(());
        }
        let next_weight = checked_finite_add(self.total_weight, weight, "total weight")?;
        let delta = value - self.mean;
        ensure_finite("weighted mean delta", delta)?;
        let next_mean = self.mean + delta * (weight / next_weight);
        ensure_finite("weighted mean", next_mean)?;
        let next_samples = checked_increment(self.samples_seen, "weighted mean sample")?;
        self.total_weight = next_weight;
        self.mean = next_mean;
        self.samples_seen = next_samples;
        Ok(())
    }

    fn total_weight(&self) -> f64 {
        self.total_weight
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Preview O(1)-space weighted population variance (West's update).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct WeightedVariance {
    total_weight: f64,
    mean: f64,
    m2: f64,
    samples_seen: u64,
}

impl WeightedVariance {
    /// Create an empty weighted variance.
    pub const fn new() -> Self {
        Self {
            total_weight: 0.0,
            mean: 0.0,
            m2: 0.0,
            samples_seen: 0,
        }
    }

    /// Weighted mean, if any positive weight has been observed.
    pub fn mean(&self) -> Option<f64> {
        (self.total_weight > 0.0).then_some(self.mean)
    }

    /// Population variance `Σw(x-μ)² / Σw`.
    pub fn value(&self) -> Option<f64> {
        (self.total_weight > 0.0).then_some(self.m2 / self.total_weight)
    }
}

impl WeightedStatistic for WeightedVariance {
    fn update_weighted(&mut self, value: f64, weight: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        validate_weight(weight)?;
        if weight == 0.0 {
            return Ok(());
        }
        let next_weight = checked_finite_add(self.total_weight, weight, "total weight")?;
        let delta = value - self.mean;
        ensure_finite("weighted variance delta", delta)?;
        let ratio = weight / next_weight;
        let next_mean = self.mean + ratio * delta;
        ensure_finite("weighted variance mean", next_mean)?;
        let m2_delta = weight * delta * (value - next_mean);
        ensure_finite("weighted variance m2 delta", m2_delta)?;
        let next_m2 = checked_finite_add(self.m2, m2_delta, "weighted variance m2")?;
        if next_m2 < -f64::EPSILON * next_weight.max(1.0) {
            return Err(RillError::InvalidState(
                "weighted variance became negative".to_owned(),
            ));
        }
        let next_samples = checked_increment(self.samples_seen, "weighted variance sample")?;
        self.total_weight = next_weight;
        self.mean = next_mean;
        self.m2 = next_m2.max(0.0);
        self.samples_seen = next_samples;
        Ok(())
    }

    fn total_weight(&self) -> f64 {
        self.total_weight
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Preview weighted exponentially decayed mean.
///
/// A sample weight `w` uses effective smoothing
/// `1 - (1 - alpha)^w`, matching repeated integer-weight updates while also
/// supporting fractional weights.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct WeightedExponentiallyWeightedMean {
    alpha: f64,
    total_weight: f64,
    mean: f64,
    samples_seen: u64,
}

impl WeightedExponentiallyWeightedMean {
    /// Create an accumulator with `alpha` in `(0, 1]`.
    pub fn new(alpha: f64) -> Result<Self, RillError> {
        ensure_finite("alpha", alpha)?;
        if !(0.0 < alpha && alpha <= 1.0) {
            return Err(RillError::InvalidParameter {
                name: "alpha",
                value: alpha,
            });
        }
        Ok(Self {
            alpha,
            total_weight: 0.0,
            mean: 0.0,
            samples_seen: 0,
        })
    }

    /// Current value, or `None` before a positive weight is observed.
    pub fn value(&self) -> Option<f64> {
        (self.total_weight > 0.0).then_some(self.mean)
    }

    /// Configured base smoothing factor.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }
}

impl WeightedStatistic for WeightedExponentiallyWeightedMean {
    fn update_weighted(&mut self, value: f64, weight: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        validate_weight(weight)?;
        if weight == 0.0 {
            return Ok(());
        }
        let next_weight = checked_finite_add(self.total_weight, weight, "total weight")?;
        let next_samples = checked_increment(self.samples_seen, "weighted EW mean sample")?;
        let next_mean = if self.total_weight == 0.0 {
            value
        } else {
            let effective_alpha = 1.0 - (1.0 - self.alpha).powf(weight);
            ensure_finite("effective alpha", effective_alpha)?;
            self.mean + effective_alpha * (value - self.mean)
        };
        ensure_finite("weighted EW mean", next_mean)?;
        self.total_weight = next_weight;
        self.samples_seen = next_samples;
        self.mean = next_mean;
        Ok(())
    }

    fn total_weight(&self) -> f64 {
        self.total_weight
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        self.total_weight = 0.0;
        self.mean = 0.0;
        self.samples_seen = 0;
    }
}

/// Preview weighted mean absolute error.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct WeightedMae {
    errors: WeightedMean,
}

impl WeightedMae {
    /// Update with a weighted truth/prediction pair.
    pub fn update(&mut self, truth: f64, prediction: f64, weight: f64) -> Result<(), RillError> {
        ensure_finite("truth", truth)?;
        ensure_finite("prediction", prediction)?;
        let error = (truth - prediction).abs();
        ensure_finite("absolute error", error)?;
        self.errors.update_weighted(error, weight)
    }

    /// Current weighted MAE.
    pub fn value(&self) -> Option<f64> {
        self.errors.value()
    }

    /// Sum of weights.
    pub fn total_weight(&self) -> f64 {
        self.errors.total_weight()
    }

    /// Number of positive-weight observations.
    pub fn samples_seen(&self) -> u64 {
        self.errors.samples_seen()
    }

    /// Reset all metric state.
    pub fn reset(&mut self) {
        self.errors.reset();
    }
}

/// Preview weighted mean squared error.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct WeightedMse {
    errors: WeightedMean,
}

impl WeightedMse {
    /// Update with a weighted truth/prediction pair.
    pub fn update(&mut self, truth: f64, prediction: f64, weight: f64) -> Result<(), RillError> {
        ensure_finite("truth", truth)?;
        ensure_finite("prediction", prediction)?;
        let error = truth - prediction;
        let squared = error * error;
        ensure_finite("squared error", squared)?;
        self.errors.update_weighted(squared, weight)
    }

    /// Current weighted MSE.
    pub fn value(&self) -> Option<f64> {
        self.errors.value()
    }

    /// Sum of weights.
    pub fn total_weight(&self) -> f64 {
        self.errors.total_weight()
    }

    /// Number of positive-weight observations.
    pub fn samples_seen(&self) -> u64 {
        self.errors.samples_seen()
    }

    /// Reset all metric state.
    pub fn reset(&mut self) {
        self.errors.reset();
    }
}

#[cfg(feature = "serde")]
fn validate_weighted_moments(
    total_weight: f64,
    samples_seen: u64,
    values: &[(&'static str, f64)],
) -> Result<(), RillError> {
    ensure_finite("total weight", total_weight)?;
    if total_weight < 0.0 || (samples_seen == 0) != (total_weight == 0.0) {
        return Err(RillError::InvalidState(
            "weighted state has inconsistent count and total weight".to_owned(),
        ));
    }
    for &(name, value) in values {
        ensure_finite(name, value)?;
    }
    Ok(())
}

#[cfg(feature = "serde")]
impl ValidateState for WeightedMean {
    fn validate_state(&self) -> Result<(), RillError> {
        validate_weighted_moments(self.total_weight, self.samples_seen, &[("mean", self.mean)])
    }
}

#[cfg(feature = "serde")]
impl ValidateState for WeightedVariance {
    fn validate_state(&self) -> Result<(), RillError> {
        validate_weighted_moments(
            self.total_weight,
            self.samples_seen,
            &[("mean", self.mean), ("m2", self.m2)],
        )?;
        if self.m2 < 0.0 {
            return Err(RillError::InvalidState(
                "weighted variance m2 must be non-negative".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl ValidateState for WeightedExponentiallyWeightedMean {
    fn validate_state(&self) -> Result<(), RillError> {
        if !(self.alpha.is_finite() && 0.0 < self.alpha && self.alpha <= 1.0) {
            return Err(RillError::InvalidState(
                "weighted EW mean alpha must be in (0, 1]".to_owned(),
            ));
        }
        validate_weighted_moments(self.total_weight, self.samples_seen, &[("mean", self.mean)])
    }
}

#[cfg(feature = "serde")]
impl ValidateState for WeightedMae {
    fn validate_state(&self) -> Result<(), RillError> {
        self.errors.validate_state()
    }
}

#[cfg(feature = "serde")]
impl ValidateState for WeightedMse {
    fn validate_state(&self) -> Result<(), RillError> {
        self.errors.validate_state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn weighted_mean_and_variance_match_offline() {
        let samples = [(1.0, 0.5), (3.0, 2.0), (10.0, 1.5)];
        let total = samples.iter().map(|(_, weight)| weight).sum::<f64>();
        let offline_mean = samples.iter().map(|(x, w)| x * w).sum::<f64>() / total;
        let offline_variance = samples
            .iter()
            .map(|(x, w)| w * (x - offline_mean).powi(2))
            .sum::<f64>()
            / total;
        let mut mean = WeightedMean::new();
        let mut variance = WeightedVariance::new();
        for (x, weight) in samples {
            mean.update_weighted(x, weight).unwrap();
            variance.update_weighted(x, weight).unwrap();
        }
        assert_abs_diff_eq!(mean.value().unwrap(), offline_mean, epsilon = 1e-12);
        assert_abs_diff_eq!(variance.value().unwrap(), offline_variance, epsilon = 1e-12);
    }

    #[test]
    fn integer_weight_ew_mean_matches_repeated_samples() {
        let mut weighted = WeightedExponentiallyWeightedMean::new(0.2).unwrap();
        let mut repeated = WeightedExponentiallyWeightedMean::new(0.2).unwrap();
        weighted.update_weighted(1.0, 1.0).unwrap();
        repeated.update_weighted(1.0, 1.0).unwrap();
        weighted.update_weighted(5.0, 3.0).unwrap();
        for _ in 0..3 {
            repeated.update_weighted(5.0, 1.0).unwrap();
        }
        assert_abs_diff_eq!(
            weighted.value().unwrap(),
            repeated.value().unwrap(),
            epsilon = 1e-12
        );
    }

    #[test]
    fn zero_weight_is_validated_noop_and_bad_weights_are_atomic() {
        let mut mean = WeightedMean::new();
        mean.update_weighted(3.0, 0.0).unwrap();
        assert_eq!(mean, WeightedMean::new());
        let before = mean.clone();
        assert!(mean.update_weighted(1.0, -1.0).is_err());
        assert!(mean.update_weighted(1.0, f64::NAN).is_err());
        assert_eq!(mean, before);
    }

    #[test]
    fn weighted_errors_match_offline_calculation_and_reset() {
        let samples: [(f64, f64, f64); 3] = [(2.0, 1.0, 0.5), (5.0, 2.0, 2.0), (-1.0, 1.0, 1.5)];
        let total_weight = samples.iter().map(|sample| sample.2).sum::<f64>();
        let expected_mae = samples
            .iter()
            .map(|(truth, prediction, weight)| weight * (truth - prediction).abs())
            .sum::<f64>()
            / total_weight;
        let expected_mse = samples
            .iter()
            .map(|(truth, prediction, weight)| weight * (truth - prediction).powi(2))
            .sum::<f64>()
            / total_weight;
        let mut mae = WeightedMae::default();
        let mut mse = WeightedMse::default();
        for (truth, prediction, weight) in samples {
            mae.update(truth, prediction, weight).unwrap();
            mse.update(truth, prediction, weight).unwrap();
        }
        assert_abs_diff_eq!(mae.value().unwrap(), expected_mae, epsilon = 1e-12);
        assert_abs_diff_eq!(mse.value().unwrap(), expected_mse, epsilon = 1e-12);
        assert_eq!(mae.samples_seen(), 3);
        assert_eq!(mse.total_weight(), total_weight);
        mae.reset();
        mse.reset();
        assert_eq!(mae.value(), None);
        assert_eq!(mse.samples_seen(), 0);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn weighted_state_roundtrip_and_corruption_rejected() {
        let mut variance = WeightedVariance::new();
        variance.update_weighted(2.0, 0.5).unwrap();
        let json = serde_json::to_string(&variance).unwrap();
        let restored: WeightedVariance = serde_json::from_str(&json).unwrap();
        restored.validate_state().unwrap();
        let corrupt = json.replace("\"total_weight\":0.5", "\"total_weight\":-1.0");
        let restored: WeightedVariance = serde_json::from_str(&corrupt).unwrap();
        assert!(restored.validate_state().is_err());
    }
}

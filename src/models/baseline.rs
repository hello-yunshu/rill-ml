//! Baseline regressors.
//!
//! These simple models serve as comparison baselines. A complex online model
//! should be compared against at least [`MeanRegressor`] and
//! [`ExponentiallyWeightedMeanRegressor`] before being considered useful.

use crate::error::{RillError, checked_increment, ensure_finite, ensure_finite_target};
use crate::stats::{ExponentiallyWeightedMean, Mean};
use crate::traits::{OnlineRegressor, OnlineStatistic};

/// Configuration shared by baseline regressors.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BaselineConfig {
    /// Prediction returned before any target has been observed.
    pub initial_prediction: f64,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self {
            initial_prediction: 0.0,
        }
    }
}

fn validate_baseline_config(config: &BaselineConfig) -> Result<(), RillError> {
    ensure_finite("initial_prediction", config.initial_prediction)
}

/// A regressor that always predicts the running mean of observed targets.
///
/// This is the simplest meaningful online regression baseline.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MeanRegressor {
    config: BaselineConfig,
    mean: Mean,
}

impl MeanRegressor {
    /// Create a new mean regressor with the given configuration.
    pub fn new(config: BaselineConfig) -> Result<Self, RillError> {
        validate_baseline_config(&config)?;
        Ok(Self {
            config,
            mean: Mean::new(),
        })
    }

    /// The current running mean of targets.
    pub const fn mean(&self) -> f64 {
        self.mean.value()
    }
}

impl OnlineRegressor for MeanRegressor {
    fn feature_count(&self) -> usize {
        0
    }

    fn samples_seen(&self) -> u64 {
        self.mean.samples_seen()
    }

    fn predict(&self, _features: &[f64]) -> Result<f64, RillError> {
        if self.mean.count() == 0 {
            Ok(self.config.initial_prediction)
        } else {
            Ok(self.mean.value())
        }
    }

    fn learn(&mut self, _features: &[f64], target: f64) -> Result<(), RillError> {
        ensure_finite_target(target)?;
        self.mean.update(target)
    }

    fn reset(&mut self) {
        self.mean.reset();
    }
}

impl Default for MeanRegressor {
    fn default() -> Self {
        Self::new(BaselineConfig::default()).expect("default config is valid")
    }
}

/// A regressor that always predicts the last observed target.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LastValueRegressor {
    config: BaselineConfig,
    last_value: Option<f64>,
    count: u64,
}

impl LastValueRegressor {
    /// Create a new last-value regressor.
    pub fn new(config: BaselineConfig) -> Result<Self, RillError> {
        validate_baseline_config(&config)?;
        Ok(Self {
            config,
            last_value: None,
            count: 0,
        })
    }

    /// The last observed target, if any.
    pub const fn last_value(&self) -> Option<f64> {
        self.last_value
    }
}

impl OnlineRegressor for LastValueRegressor {
    fn feature_count(&self) -> usize {
        0
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn predict(&self, _features: &[f64]) -> Result<f64, RillError> {
        Ok(self.last_value.unwrap_or(self.config.initial_prediction))
    }

    fn learn(&mut self, _features: &[f64], target: f64) -> Result<(), RillError> {
        ensure_finite_target(target)?;
        let next_count = checked_increment(self.count, "last-value sample")?;
        self.last_value = Some(target);
        self.count = next_count;
        Ok(())
    }

    fn reset(&mut self) {
        self.last_value = None;
        self.count = 0;
    }
}

impl Default for LastValueRegressor {
    fn default() -> Self {
        Self::new(BaselineConfig::default()).expect("default config is valid")
    }
}

/// A regressor that predicts an exponentially weighted mean of targets.
///
/// Suitable when recent observations are more relevant than older ones.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExponentiallyWeightedMeanRegressor {
    config: BaselineConfig,
    ew: ExponentiallyWeightedMean,
}

impl ExponentiallyWeightedMeanRegressor {
    /// Create a new EW mean regressor.
    ///
    /// `alpha` must be in `(0, 1]`.
    pub fn new(alpha: f64, config: BaselineConfig) -> Result<Self, RillError> {
        validate_baseline_config(&config)?;
        Ok(Self {
            config,
            ew: ExponentiallyWeightedMean::new(alpha)?,
        })
    }

    /// The configured alpha.
    pub const fn alpha(&self) -> f64 {
        self.ew.alpha()
    }

    /// The current weighted mean.
    pub const fn value(&self) -> f64 {
        self.ew.value()
    }
}

impl OnlineRegressor for ExponentiallyWeightedMeanRegressor {
    fn feature_count(&self) -> usize {
        0
    }

    fn samples_seen(&self) -> u64 {
        self.ew.samples_seen()
    }

    fn predict(&self, _features: &[f64]) -> Result<f64, RillError> {
        if self.ew.count() == 0 {
            Ok(self.config.initial_prediction)
        } else {
            Ok(self.ew.value())
        }
    }

    fn learn(&mut self, _features: &[f64], target: f64) -> Result<(), RillError> {
        ensure_finite_target(target)?;
        self.ew.update(target)
    }

    fn reset(&mut self) {
        self.ew.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_regressor_cold_start() {
        let r = MeanRegressor::default();
        assert_eq!(r.predict(&[]).unwrap(), 0.0);
    }

    #[test]
    fn mean_regressor_predicts_running_mean() {
        let mut r = MeanRegressor::default();
        r.learn(&[], 10.0).unwrap();
        r.learn(&[], 20.0).unwrap();
        assert_eq!(r.predict(&[]).unwrap(), 15.0);
    }

    #[test]
    fn last_value_regressor_cold_start() {
        let r = LastValueRegressor::default();
        assert_eq!(r.predict(&[]).unwrap(), 0.0);
    }

    #[test]
    fn last_value_regressor_tracks_last() {
        let mut r = LastValueRegressor::default();
        r.learn(&[], 10.0).unwrap();
        r.learn(&[], 20.0).unwrap();
        assert_eq!(r.predict(&[]).unwrap(), 20.0);
    }

    #[test]
    fn ew_mean_regressor_cold_start() {
        let r = ExponentiallyWeightedMeanRegressor::new(0.5, BaselineConfig::default()).unwrap();
        assert_eq!(r.predict(&[]).unwrap(), 0.0);
    }

    #[test]
    fn ew_mean_regressor_weights_recent() {
        let mut r =
            ExponentiallyWeightedMeanRegressor::new(0.5, BaselineConfig::default()).unwrap();
        r.learn(&[], 10.0).unwrap();
        r.learn(&[], 20.0).unwrap();
        assert!((r.predict(&[]).unwrap() - 15.0).abs() < 1e-12);
    }

    #[test]
    fn initial_prediction_custom() {
        let r = MeanRegressor::new(BaselineConfig {
            initial_prediction: 42.0,
        })
        .unwrap();
        assert_eq!(r.predict(&[]).unwrap(), 42.0);
    }

    #[test]
    fn non_finite_target_rejected() {
        let mut r = MeanRegressor::default();
        assert!(r.learn(&[], f64::NAN).is_err());
    }
}

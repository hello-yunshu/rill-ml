//! Online standard scaler.
//!
//! Maintains per-feature Welford variance and mean. Time complexity per
//! update/transform: `O(d)`. Space complexity: `O(d)`.

use crate::error::{RillError, checked_increment, ensure_finite, validate_features};
use crate::traits::Transformer;

/// Configuration for [`StandardScaler`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StandardScalerConfig {
    /// Whether to subtract the running mean. Default: `true`.
    pub with_mean: bool,
    /// Whether to divide by the running standard deviation. Default: `true`.
    pub with_std: bool,
    /// Variance threshold below which the scale is treated as `1.0` to avoid
    /// division by zero. Default: `1e-12`.
    pub epsilon: f64,
}

impl Default for StandardScalerConfig {
    fn default() -> Self {
        Self {
            with_mean: true,
            with_std: true,
            epsilon: 1e-12,
        }
    }
}

/// Online standard scaler that standardizes features to approximately zero
/// mean and unit variance.
///
/// - When `with_mean = false`, the mean subtraction is skipped.
/// - When `with_std = false`, the scaling is skipped.
/// - When a feature has seen zero samples, its mean is `0` and scale is `1`,
///   so the original value is returned unchanged.
/// - When a feature's variance is below `epsilon`, the scale is `1` to avoid
///   NaN or Infinity.
///
/// `transform` does not update state; only `update` does.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct StandardScaler {
    feature_count: usize,
    config: StandardScalerConfig,
    counts: Vec<u64>,
    means: Vec<f64>,
    m2s: Vec<f64>,
}

impl StandardScaler {
    /// Create a new scaler for `feature_count` features with default config.
    pub fn new(feature_count: usize) -> Result<Self, RillError> {
        Self::with_config(feature_count, StandardScalerConfig::default())
    }

    /// Create a new scaler with a custom configuration.
    pub fn with_config(
        feature_count: usize,
        config: StandardScalerConfig,
    ) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("epsilon", config.epsilon)?;
        if config.epsilon < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "epsilon",
                value: config.epsilon,
            });
        }
        Ok(Self {
            feature_count,
            config,
            counts: vec![0; feature_count],
            means: vec![0.0; feature_count],
            m2s: vec![0.0; feature_count],
        })
    }

    /// The per-feature means.
    pub fn means(&self) -> &[f64] {
        &self.means
    }

    /// The per-feature variances (population).
    pub fn variances(&self) -> Vec<f64> {
        self.m2s
            .iter()
            .zip(&self.counts)
            .map(|(&m2, &n)| if n == 0 { 0.0 } else { m2 / n as f64 })
            .collect()
    }

    /// The per-feature standard deviations.
    pub fn std_devs(&self) -> Vec<f64> {
        self.variances().iter().map(|v| v.sqrt()).collect()
    }

    /// The per-feature scales used during transformation.
    pub fn scales(&self) -> Vec<f64> {
        self.variances()
            .iter()
            .map(|&var| {
                if var < self.config.epsilon {
                    1.0
                } else {
                    var.sqrt()
                }
            })
            .collect()
    }

    /// Validate all configuration and persisted-state invariants.
    ///
    /// This is run automatically during deserialization and before operations
    /// that index per-feature state.
    pub fn validate(&self) -> Result<(), RillError> {
        if self.feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("epsilon", self.config.epsilon)?;
        if self.config.epsilon < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "epsilon",
                value: self.config.epsilon,
            });
        }
        if self.counts.len() != self.feature_count
            || self.means.len() != self.feature_count
            || self.m2s.len() != self.feature_count
        {
            return Err(RillError::InvalidState(
                "standard scaler feature_count does not match state lengths".to_owned(),
            ));
        }
        if self.means.iter().any(|value| !value.is_finite())
            || self.m2s.iter().any(|value| !value.is_finite())
        {
            return Err(RillError::InvalidState(
                "standard scaler state must contain only finite values".to_owned(),
            ));
        }
        if self.counts.windows(2).any(|pair| pair[0] != pair[1]) {
            return Err(RillError::InvalidState(
                "standard scaler feature counts must stay synchronized".to_owned(),
            ));
        }
        Ok(())
    }
}

impl Transformer for StandardScaler {
    fn input_dim(&self) -> usize {
        self.feature_count
    }

    fn output_dim(&self) -> usize {
        self.feature_count
    }

    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError> {
        self.validate()?;
        validate_features(self.feature_count, features)?;
        let scales = self.scales();
        features
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                let mean = if self.config.with_mean {
                    self.means[i]
                } else {
                    0.0
                };
                let scale = if self.config.with_std { scales[i] } else { 1.0 };
                let transformed = (x - mean) / scale;
                ensure_finite("transformed feature", transformed)?;
                Ok(transformed)
            })
            .collect()
    }

    fn update(&mut self, features: &[f64]) -> Result<(), RillError> {
        self.validate()?;
        validate_features(self.feature_count, features)?;
        let mut next_counts = self.counts.clone();
        let mut next_means = self.means.clone();
        let mut next_m2s = self.m2s.clone();
        for (i, &x) in features.iter().enumerate() {
            let count = checked_increment(self.counts[i], "standard scaler sample")?;
            let delta = x - self.means[i];
            ensure_finite("standard scaler delta", delta)?;
            let mean = self.means[i] + delta / count as f64;
            ensure_finite("standard scaler mean", mean)?;
            let delta2 = x - mean;
            ensure_finite("standard scaler delta", delta2)?;
            let m2 = self.m2s[i] + delta * delta2;
            ensure_finite("standard scaler M2", m2)?;
            next_counts[i] = count;
            next_means[i] = mean;
            next_m2s[i] = m2;
        }
        self.counts = next_counts;
        self.means = next_means;
        self.m2s = next_m2s;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.counts.iter().copied().max().unwrap_or(0)
    }

    fn reset(&mut self) {
        for c in &mut self.counts {
            *c = 0;
        }
        for m in &mut self.means {
            *m = 0.0;
        }
        for m2 in &mut self.m2s {
            *m2 = 0.0;
        }
    }
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct StandardScalerState {
    feature_count: usize,
    config: StandardScalerConfig,
    counts: Vec<u64>,
    means: Vec<f64>,
    m2s: Vec<f64>,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for StandardScaler {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let state = StandardScalerState::deserialize(deserializer)?;
        let scaler = Self {
            feature_count: state.feature_count,
            config: state.config,
            counts: state.counts,
            means: state.means,
            m2s: state.m2s,
        };
        scaler.validate().map_err(serde::de::Error::custom)?;
        Ok(scaler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaler_zero_state_returns_original() {
        let s = StandardScaler::new(3).unwrap();
        let out = s.transform(&[1.0, 2.0, 3.0]).unwrap();
        // count == 0 -> mean=0, scale=1 -> original
        assert!((out[0] - 1.0).abs() < 1e-12);
        assert!((out[1] - 2.0).abs() < 1e-12);
        assert!((out[2] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn scaler_standardizes_after_updates() {
        let mut s = StandardScaler::new(2).unwrap();
        // feature 0: values [1, 3] -> mean 2, var 1, std 1
        // feature 1: values [10, 20] -> mean 15, var 25, std 5
        s.update(&[1.0, 10.0]).unwrap();
        s.update(&[3.0, 20.0]).unwrap();
        let out = s.transform(&[3.0, 20.0]).unwrap();
        // (3-2)/1 = 1, (20-15)/5 = 1
        assert!((out[0] - 1.0).abs() < 1e-9);
        assert!((out[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn transform_does_not_update_state() {
        let mut s = StandardScaler::new(1).unwrap();
        s.update(&[10.0]).unwrap();
        let mean_before = s.means()[0];
        let _ = s.transform(&[5.0]).unwrap();
        assert_eq!(s.means()[0], mean_before);
        assert_eq!(s.counts[0], 1);
    }

    #[test]
    fn update_rejects_overflow_without_mutating_state() {
        let mut scaler = StandardScaler::new(1).unwrap();
        scaler.update(&[f64::MAX]).unwrap();
        let before = scaler.clone();
        assert!(scaler.update(&[-f64::MAX]).is_err());
        assert_eq!(scaler.counts, before.counts);
        assert_eq!(scaler.means, before.means);
        assert_eq!(scaler.m2s, before.m2s);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_malformed_state() {
        let malformed = r#"{
            "feature_count":2,
            "config":{"with_mean":true,"with_std":true,"epsilon":1e-12},
            "counts":[1],
            "means":[0.0],
            "m2s":[0.0]
        }"#;
        assert!(serde_json::from_str::<StandardScaler>(malformed).is_err());
    }

    #[test]
    fn constant_feature_uses_scale_one() {
        let mut s = StandardScaler::new(1).unwrap();
        for _ in 0..10 {
            s.update(&[5.0]).unwrap();
        }
        // var = 0 < epsilon -> scale = 1, mean = 5 -> (5-5)/1 = 0
        let out = s.transform(&[5.0]).unwrap();
        assert!(out[0].abs() < 1e-12);
        assert!(!out[0].is_nan());
    }

    #[test]
    fn with_mean_false_keeps_offset() {
        let mut s = StandardScaler::with_config(
            1,
            StandardScalerConfig {
                with_mean: false,
                with_std: true,
                epsilon: 1e-12,
            },
        )
        .unwrap();
        s.update(&[1.0]).unwrap();
        s.update(&[3.0]).unwrap();
        // mean=2, var=1, std=1, but with_mean=false so x/1 = x
        let out = s.transform(&[3.0]).unwrap();
        assert!((out[0] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let mut s = StandardScaler::new(3).unwrap();
        assert!(s.transform(&[1.0, 2.0]).is_err());
        assert!(s.update(&[1.0, 2.0]).is_err());
    }

    #[test]
    fn zero_features_rejected() {
        assert!(matches!(
            StandardScaler::new(0),
            Err(RillError::EmptyFeatures)
        ));
    }

    #[test]
    fn non_finite_rejected() {
        let mut s = StandardScaler::new(2).unwrap();
        assert!(s.update(&[1.0, f64::NAN]).is_err());
    }

    #[test]
    fn reset_clears_state() {
        let mut s = StandardScaler::new(1).unwrap();
        s.update(&[1.0]).unwrap();
        s.update(&[2.0]).unwrap();
        s.reset();
        assert_eq!(s.counts[0], 0);
        assert_eq!(s.means()[0], 0.0);
    }
}

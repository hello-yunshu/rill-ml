//! Constant value imputer for missing data.
//!
//! Replaces `NaN` values with a fixed fill value. Non-NaN values are
//! passed through unchanged. This imputer has no learnable state.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::traits::Transformer;

/// Configuration for [`ConstantImputer`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConstantImputerConfig {
    /// The value to replace NaN with.
    pub fill_value: f64,
}

impl Default for ConstantImputerConfig {
    fn default() -> Self {
        Self { fill_value: 0.0 }
    }
}

/// Replaces `NaN` values with a constant.
///
/// This transformer accepts `NaN` in its input, unlike most other
/// transformers. The fill value must be finite.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConstantImputer {
    feature_count: usize,
    config: ConstantImputerConfig,
    samples_seen: u64,
}

impl ConstantImputer {
    /// Create a new imputer for `feature_count` features with default config.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `feature_count` is `0`.
    pub fn new(feature_count: usize) -> Result<Self, RillError> {
        Self::with_config(feature_count, ConstantImputerConfig::default())
    }

    /// Create a new imputer with a custom configuration.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `feature_count` is `0`.
    /// Returns [`RillError::NonFiniteValue`] if `fill_value` is not finite.
    pub fn with_config(
        feature_count: usize,
        config: ConstantImputerConfig,
    ) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("fill_value", config.fill_value)?;
        Ok(Self {
            feature_count,
            config,
            samples_seen: 0,
        })
    }

    /// The fill value used to replace NaN.
    pub fn fill_value(&self) -> f64 {
        self.config.fill_value
    }

    /// Validate only the dimension, allowing NaN values.
    fn check_dimension(&self, features: &[f64]) -> Result<(), RillError> {
        if features.is_empty() {
            return Err(RillError::EmptyFeatures);
        }
        if features.len() != self.feature_count {
            return Err(RillError::DimensionMismatch {
                expected: self.feature_count,
                actual: features.len(),
            });
        }
        Ok(())
    }
}

impl Transformer for ConstantImputer {
    fn input_dim(&self) -> usize {
        self.feature_count
    }

    fn output_dim(&self) -> usize {
        self.feature_count
    }

    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError> {
        self.check_dimension(features)?;
        let mut out = Vec::with_capacity(features.len());
        for &x in features {
            if x.is_nan() {
                out.push(self.config.fill_value);
            } else {
                ensure_finite("feature", x)?;
                out.push(x);
            }
        }
        Ok(out)
    }

    fn update(&mut self, features: &[f64]) -> Result<(), RillError> {
        self.check_dimension(features)?;
        for &x in features {
            if !x.is_nan() {
                ensure_finite("feature", x)?;
            }
        }
        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_replaced_with_fill_value() {
        let imp = ConstantImputer::new(3).unwrap();
        let out = imp.transform(&[1.0, f64::NAN, 3.0]).unwrap();
        assert_eq!(out, vec![1.0, 0.0, 3.0]);
    }

    #[test]
    fn non_nan_passed_through() {
        let imp = ConstantImputer::new(3).unwrap();
        let out = imp.transform(&[1.5, -2.0, 3.0]).unwrap();
        assert_eq!(out, vec![1.5, -2.0, 3.0]);
    }

    #[test]
    fn custom_fill_value() {
        let imp =
            ConstantImputer::with_config(2, ConstantImputerConfig { fill_value: -1.0 }).unwrap();
        let out = imp.transform(&[f64::NAN, 5.0]).unwrap();
        assert_eq!(out, vec![-1.0, 5.0]);
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let imp = ConstantImputer::new(3).unwrap();
        assert!(matches!(
            imp.transform(&[1.0, 2.0]),
            Err(RillError::DimensionMismatch { .. })
        ));
        let mut imp = imp;
        assert!(matches!(
            imp.update(&[1.0, 2.0, 3.0, 4.0]),
            Err(RillError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn reset_clears_state() {
        let mut imp = ConstantImputer::new(2).unwrap();
        imp.update(&[1.0, f64::NAN]).unwrap();
        imp.update(&[f64::NAN, 2.0]).unwrap();
        assert_eq!(imp.samples_seen(), 2);
        imp.reset();
        assert_eq!(imp.samples_seen(), 0);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut imp =
            ConstantImputer::with_config(3, ConstantImputerConfig { fill_value: 7.0 }).unwrap();
        imp.update(&[1.0, f64::NAN, 3.0]).unwrap();
        let json = serde_json::to_string(&imp).unwrap();
        let restored: ConstantImputer = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.input_dim(), imp.input_dim());
        assert_eq!(restored.output_dim(), imp.output_dim());
        assert_eq!(restored.samples_seen(), imp.samples_seen());
        assert_eq!(restored.fill_value(), imp.fill_value());
    }
}

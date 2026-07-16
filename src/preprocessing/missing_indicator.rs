//! Missing value indicator transformer.
//!
//! For each input feature, appends a binary indicator (1.0 if NaN, 0.0
//! otherwise). Output dimension = 2 * input dimension.
//!
//! This transformer accepts NaN values in its input, unlike most other
//! transformers.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::traits::Transformer;

/// Adds a missing-value indicator for each feature.
///
/// `transform([x1, x2, ...])` produces `[x1, is_nan(x1) as f64, x2, is_nan(x2) as f64, ...]`.
/// The original values are preserved (including NaN), with the indicator
/// appended after each value.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MissingIndicator {
    feature_count: usize,
    samples_seen: u64,
}

impl MissingIndicator {
    /// Create a new indicator for `feature_count` features.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `feature_count` is `0`.
    pub fn new(feature_count: usize) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        Ok(Self {
            feature_count,
            samples_seen: 0,
        })
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

impl Transformer for MissingIndicator {
    fn input_dim(&self) -> usize {
        self.feature_count
    }

    fn output_dim(&self) -> usize {
        self.feature_count * 2
    }

    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError> {
        self.check_dimension(features)?;
        let mut out = Vec::with_capacity(features.len() * 2);
        for &v in features {
            if !v.is_nan() {
                ensure_finite("feature", v)?;
            }
            out.push(v);
            out.push(if v.is_nan() { 1.0 } else { 0.0 });
        }
        Ok(out)
    }

    fn update(&mut self, features: &[f64]) -> Result<(), RillError> {
        self.check_dimension(features)?;
        for &v in features {
            if !v.is_nan() {
                ensure_finite("feature", v)?;
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
    fn no_missing_values() {
        let mi = MissingIndicator::new(3).unwrap();
        let out = mi.transform(&[1.0, 2.0, 3.0]).unwrap();
        assert_eq!(out, vec![1.0, 0.0, 2.0, 0.0, 3.0, 0.0]);
    }

    #[test]
    fn all_missing_values() {
        let mi = MissingIndicator::new(2).unwrap();
        let out = mi.transform(&[f64::NAN, f64::NAN]).unwrap();
        assert!(out[0].is_nan());
        assert_eq!(out[1], 1.0);
        assert!(out[2].is_nan());
        assert_eq!(out[3], 1.0);
    }

    #[test]
    fn mixed_missing_and_present() {
        let mi = MissingIndicator::new(3).unwrap();
        let out = mi.transform(&[1.0, f64::NAN, 3.0]).unwrap();
        assert_eq!(out[0], 1.0);
        assert_eq!(out[1], 0.0);
        assert!(out[2].is_nan());
        assert_eq!(out[3], 1.0);
        assert_eq!(out[4], 3.0);
        assert_eq!(out[5], 0.0);
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let mi = MissingIndicator::new(3).unwrap();
        assert!(matches!(
            mi.transform(&[1.0, 2.0]),
            Err(RillError::DimensionMismatch { .. })
        ));
        let mut mi = mi;
        assert!(matches!(
            mi.update(&[1.0, 2.0, 3.0, 4.0]),
            Err(RillError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn reset_clears_state() {
        let mut mi = MissingIndicator::new(2).unwrap();
        mi.update(&[1.0, 2.0]).unwrap();
        mi.update(&[3.0, 4.0]).unwrap();
        assert_eq!(mi.samples_seen(), 2);
        mi.reset();
        assert_eq!(mi.samples_seen(), 0);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut mi = MissingIndicator::new(3).unwrap();
        mi.update(&[1.0, 2.0, 3.0]).unwrap();
        let json = serde_json::to_string(&mi).unwrap();
        let restored: MissingIndicator = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.input_dim(), mi.input_dim());
        assert_eq!(restored.output_dim(), mi.output_dim());
        assert_eq!(restored.samples_seen(), mi.samples_seen());
    }
}

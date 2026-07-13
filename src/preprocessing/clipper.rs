//! Feature clipper.
//!
//! Clips each feature to `[min, max]`. Time complexity per transform:
//! `O(d)`. Space complexity: `O(1)`.

use crate::error::{RillError, ensure_finite};
use crate::traits::Transformer;

/// A simple element-wise clipper that bounds feature values to a fixed range.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Clipper {
    feature_count: usize,
    min: f64,
    max: f64,
}

impl Clipper {
    /// Create a new clipper for `feature_count` features, clamping to `[min, max]`.
    pub fn new(feature_count: usize, min: f64, max: f64) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("min", min)?;
        ensure_finite("max", max)?;
        if min > max {
            return Err(RillError::InvalidParameter {
                name: "min",
                value: min,
            });
        }
        Ok(Self {
            feature_count,
            min,
            max,
        })
    }

    /// The lower bound.
    pub const fn min(&self) -> f64 {
        self.min
    }

    /// The upper bound.
    pub const fn max(&self) -> f64 {
        self.max
    }
}

impl Transformer for Clipper {
    fn input_dim(&self) -> usize {
        self.feature_count
    }

    fn output_dim(&self) -> usize {
        self.feature_count
    }

    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError> {
        crate::error::validate_features(self.feature_count, features)?;
        Ok(features
            .iter()
            .map(|&x| x.clamp(self.min, self.max))
            .collect())
    }

    fn update(&mut self, _features: &[f64]) -> Result<(), RillError> {
        // Clipper is stateless; validate for consistency.
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        0
    }

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipper_clamps_values() {
        let c = Clipper::new(3, -1.0, 1.0).unwrap();
        let out = c.transform(&[-5.0, 0.5, 5.0]).unwrap();
        assert_eq!(out, vec![-1.0, 0.5, 1.0]);
    }

    #[test]
    fn invalid_bounds_rejected() {
        assert!(Clipper::new(1, 5.0, 1.0).is_err());
    }

    #[test]
    fn clipper_is_stateless() {
        let mut c = Clipper::new(1, 0.0, 1.0).unwrap();
        c.update(&[0.5]).unwrap();
        assert_eq!(c.samples_seen(), 0);
    }
}

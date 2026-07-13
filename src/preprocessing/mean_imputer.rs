//! Mean imputer for missing data.
//!
//! Replaces `NaN` values with the running mean of observed non-NaN
//! values for each feature. Uses Welford's algorithm for numerical
//! stability.

use crate::error::{RillError, ensure_finite};
use crate::traits::Transformer;

/// Replaces `NaN` values with the per-feature running mean.
///
/// When a feature has seen zero non-NaN values, `NaN` is replaced
/// with `0.0`. This transformer accepts `NaN` in its input.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MeanImputer {
    feature_count: usize,
    counts: Vec<u64>,
    means: Vec<f64>,
    samples_seen: u64,
}

impl MeanImputer {
    /// Create a new imputer for `feature_count` features.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `feature_count` is `0`.
    pub fn new(feature_count: usize) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        Ok(Self {
            feature_count,
            counts: vec![0; feature_count],
            means: vec![0.0; feature_count],
            samples_seen: 0,
        })
    }

    /// The per-feature running means of observed non-NaN values.
    pub fn means(&self) -> &[f64] {
        &self.means
    }

    /// The per-feature counts of observed non-NaN values.
    pub fn counts(&self) -> &[u64] {
        &self.counts
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

    /// Update the running mean for feature `idx` using Welford's algorithm.
    fn update_mean(&mut self, idx: usize, value: f64) {
        let n = self.counts[idx] + 1;
        self.counts[idx] = n;
        let delta = value - self.means[idx];
        self.means[idx] += delta / n as f64;
    }
}

impl Transformer for MeanImputer {
    fn input_dim(&self) -> usize {
        self.feature_count
    }

    fn output_dim(&self) -> usize {
        self.feature_count
    }

    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError> {
        self.check_dimension(features)?;
        Ok(features
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                if x.is_nan() {
                    if self.counts[i] == 0 {
                        0.0
                    } else {
                        self.means[i]
                    }
                } else {
                    x
                }
            })
            .collect())
    }

    fn update(&mut self, features: &[f64]) -> Result<(), RillError> {
        self.check_dimension(features)?;
        for (i, &x) in features.iter().enumerate() {
            if x.is_nan() {
                continue;
            }
            ensure_finite("feature", x)?;
            self.update_mean(i, x);
        }
        self.samples_seen += 1;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        for c in &mut self.counts {
            *c = 0;
        }
        for m in &mut self.means {
            *m = 0.0;
        }
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_replaced_with_mean_after_update() {
        let mut imp = MeanImputer::new(2).unwrap();
        // feature 0: observed [2.0, 4.0] -> mean 3.0
        // feature 1: observed [10.0] -> mean 10.0
        imp.update(&[2.0, 10.0]).unwrap();
        imp.update(&[4.0, f64::NAN]).unwrap();
        let out = imp.transform(&[f64::NAN, f64::NAN]).unwrap();
        assert!((out[0] - 3.0).abs() < 1e-12);
        assert!((out[1] - 10.0).abs() < 1e-12);
    }

    #[test]
    fn nan_replaced_with_zero_when_no_data() {
        let imp = MeanImputer::new(2).unwrap();
        let out = imp.transform(&[f64::NAN, f64::NAN]).unwrap();
        assert_eq!(out, vec![0.0, 0.0]);
    }

    #[test]
    fn non_nan_passed_through() {
        let mut imp = MeanImputer::new(2).unwrap();
        imp.update(&[5.0, 6.0]).unwrap();
        let out = imp.transform(&[1.5, -2.0]).unwrap();
        assert_eq!(out, vec![1.5, -2.0]);
    }

    #[test]
    fn mean_updates_correctly() {
        let mut imp = MeanImputer::new(1).unwrap();
        imp.update(&[1.0]).unwrap();
        imp.update(&[2.0]).unwrap();
        imp.update(&[3.0]).unwrap();
        assert!((imp.means()[0] - 2.0).abs() < 1e-12);
        assert_eq!(imp.counts()[0], 3);
    }

    #[test]
    fn nan_skipped_in_update() {
        let mut imp = MeanImputer::new(2).unwrap();
        // feature 0: [1.0, NaN, 3.0] -> mean 2.0 (NaN skipped)
        // feature 1: [NaN, NaN, NaN] -> count 0, mean 0.0
        imp.update(&[1.0, f64::NAN]).unwrap();
        imp.update(&[f64::NAN, f64::NAN]).unwrap();
        imp.update(&[3.0, f64::NAN]).unwrap();
        assert!((imp.means()[0] - 2.0).abs() < 1e-12);
        assert_eq!(imp.counts()[0], 2);
        assert_eq!(imp.counts()[1], 0);
        assert!((imp.means()[1] - 0.0).abs() < 1e-12);
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let imp = MeanImputer::new(3).unwrap();
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
        let mut imp = MeanImputer::new(2).unwrap();
        imp.update(&[1.0, 2.0]).unwrap();
        imp.update(&[3.0, 4.0]).unwrap();
        assert_eq!(imp.samples_seen(), 2);
        assert_eq!(imp.counts()[0], 2);
        imp.reset();
        assert_eq!(imp.samples_seen(), 0);
        assert_eq!(imp.counts()[0], 0);
        assert!((imp.means()[0] - 0.0).abs() < 1e-12);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut imp = MeanImputer::new(2).unwrap();
        imp.update(&[1.0, f64::NAN]).unwrap();
        imp.update(&[3.0, 5.0]).unwrap();
        let json = serde_json::to_string(&imp).unwrap();
        let restored: MeanImputer = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.input_dim(), imp.input_dim());
        assert_eq!(restored.output_dim(), imp.output_dim());
        assert_eq!(restored.samples_seen(), imp.samples_seen());
        assert_eq!(restored.counts(), imp.counts());
        assert!((restored.means()[0] - imp.means()[0]).abs() < 1e-12);
    }
}

//! Online min-max scaler.
//!
//! Tracks per-feature minimum and maximum. Time complexity per update:
//! `O(d)`. Space complexity: `O(d)`.

use crate::error::{RillError, ensure_finite, validate_features};
use crate::traits::Transformer;

/// Online min-max scaler that scales features to a configurable range.
///
/// When a feature has not been observed, `transform` returns the original
/// value. When a feature is constant (min == max), the output is the
/// midpoint of the target range to avoid NaN.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MinMaxScaler {
    feature_count: usize,
    mins: Vec<Option<f64>>,
    maxs: Vec<Option<f64>>,
    target_min: f64,
    target_max: f64,
}

impl MinMaxScaler {
    /// Create a new min-max scaler scaling to `[0, 1]`.
    pub fn new(feature_count: usize) -> Result<Self, RillError> {
        Self::with_range(feature_count, 0.0, 1.0)
    }

    /// Create a new min-max scaler scaling to `[target_min, target_max]`.
    pub fn with_range(
        feature_count: usize,
        target_min: f64,
        target_max: f64,
    ) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("target_min", target_min)?;
        ensure_finite("target_max", target_max)?;
        if target_min >= target_max {
            return Err(RillError::InvalidParameter {
                name: "target_min",
                value: target_min,
            });
        }
        Ok(Self {
            feature_count,
            mins: vec![None; feature_count],
            maxs: vec![None; feature_count],
            target_min,
            target_max,
        })
    }

    /// The per-feature minima (`None` if not yet observed).
    pub fn mins(&self) -> Vec<Option<f64>> {
        self.mins.clone()
    }

    /// The per-feature maxima (`None` if not yet observed).
    pub fn maxs(&self) -> Vec<Option<f64>> {
        self.maxs.clone()
    }
}

impl Transformer for MinMaxScaler {
    fn input_dim(&self) -> usize {
        self.feature_count
    }

    fn output_dim(&self) -> usize {
        self.feature_count
    }

    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError> {
        validate_features(self.feature_count, features)?;
        let range = self.target_max - self.target_min;
        Ok(features
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                match (self.mins[i], self.maxs[i]) {
                    (Some(min), Some(max)) => {
                        if (max - min).abs() < f64::EPSILON {
                            // constant feature -> return midpoint of target range
                            self.target_min + range / 2.0
                        } else {
                            self.target_min + (x - min) / (max - min) * range
                        }
                    }
                    _ => x,
                }
            })
            .collect())
    }

    fn update(&mut self, features: &[f64]) -> Result<(), RillError> {
        validate_features(self.feature_count, features)?;
        for (i, &x) in features.iter().enumerate() {
            ensure_finite("feature", x)?;
            self.mins[i] = Some(match self.mins[i] {
                None => x,
                Some(m) => m.min(x),
            });
            self.maxs[i] = Some(match self.maxs[i] {
                None => x,
                Some(m) => m.max(x),
            });
        }
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.mins.iter().filter(|m| m.is_some()).count() as u64
    }

    fn reset(&mut self) {
        for m in &mut self.mins {
            *m = None;
        }
        for m in &mut self.maxs {
            *m = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minmax_scales_to_zero_one() {
        let mut s = MinMaxScaler::new(1).unwrap();
        s.update(&[0.0]).unwrap();
        s.update(&[10.0]).unwrap();
        let out = s.transform(&[5.0]).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn constant_feature_returns_midpoint() {
        let mut s = MinMaxScaler::new(1).unwrap();
        s.update(&[5.0]).unwrap();
        s.update(&[5.0]).unwrap();
        let out = s.transform(&[5.0]).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn custom_range() {
        let mut s = MinMaxScaler::with_range(1, -1.0, 1.0).unwrap();
        s.update(&[0.0]).unwrap();
        s.update(&[10.0]).unwrap();
        let out = s.transform(&[5.0]).unwrap();
        assert!((out[0] - 0.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_range_rejected() {
        assert!(MinMaxScaler::with_range(1, 1.0, 1.0).is_err());
        assert!(MinMaxScaler::with_range(1, 2.0, 1.0).is_err());
    }

    #[test]
    fn unobserved_feature_returns_original() {
        let s = MinMaxScaler::new(1).unwrap();
        let out = s.transform(&[7.0]).unwrap();
        assert!((out[0] - 7.0).abs() < 1e-12);
    }
}

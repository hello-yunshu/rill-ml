//! Forward-fill imputer for missing data.
//!
//! Replaces `NaN` values with the last observed non-NaN value for
//! each feature. If no value has been seen yet, `NaN` is replaced
//! with `0.0`.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::traits::Transformer;

/// Replaces `NaN` values with the last observed non-NaN value.
///
/// This transformer accepts `NaN` in its input. When no value has
/// been seen for a feature, `0.0` is used as the fill value.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ForwardFill {
    feature_count: usize,
    last_values: Vec<Option<f64>>,
    samples_seen: u64,
}

impl ForwardFill {
    /// Create a new forward-fill imputer for `feature_count` features.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `feature_count` is `0`.
    pub fn new(feature_count: usize) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        Ok(Self {
            feature_count,
            last_values: vec![None; feature_count],
            samples_seen: 0,
        })
    }

    /// The per-feature last observed non-NaN values (`None` if unseen).
    pub fn last_values(&self) -> &[Option<f64>] {
        &self.last_values
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

impl Transformer for ForwardFill {
    fn input_dim(&self) -> usize {
        self.feature_count
    }

    fn output_dim(&self) -> usize {
        self.feature_count
    }

    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError> {
        self.check_dimension(features)?;
        let mut out = Vec::with_capacity(features.len());
        for (i, &x) in features.iter().enumerate() {
            if x.is_nan() {
                out.push(self.last_values[i].unwrap_or(0.0));
            } else {
                ensure_finite("feature", x)?;
                out.push(x);
            }
        }
        Ok(out)
    }

    fn update(&mut self, features: &[f64]) -> Result<(), RillError> {
        self.check_dimension(features)?;
        for (i, &x) in features.iter().enumerate() {
            if x.is_nan() {
                continue;
            }
            ensure_finite("feature", x)?;
            self.last_values[i] = Some(x);
        }
        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        for v in &mut self.last_values {
            *v = None;
        }
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_replaced_with_last_value() {
        let mut ff = ForwardFill::new(2).unwrap();
        ff.update(&[1.0, 2.0]).unwrap();
        ff.update(&[3.0, 4.0]).unwrap();
        let out = ff.transform(&[f64::NAN, f64::NAN]).unwrap();
        assert_eq!(out, vec![3.0, 4.0]);
    }

    #[test]
    fn nan_replaced_with_zero_when_no_data() {
        let ff = ForwardFill::new(2).unwrap();
        let out = ff.transform(&[f64::NAN, f64::NAN]).unwrap();
        assert_eq!(out, vec![0.0, 0.0]);
    }

    #[test]
    fn non_nan_passed_through() {
        let mut ff = ForwardFill::new(2).unwrap();
        ff.update(&[5.0, 6.0]).unwrap();
        let out = ff.transform(&[1.5, -2.0]).unwrap();
        assert_eq!(out, vec![1.5, -2.0]);
    }

    #[test]
    fn last_value_updates_correctly() {
        let mut ff = ForwardFill::new(2).unwrap();
        ff.update(&[1.0, 10.0]).unwrap();
        assert_eq!(ff.last_values()[0], Some(1.0));
        assert_eq!(ff.last_values()[1], Some(10.0));
        ff.update(&[2.0, f64::NAN]).unwrap();
        assert_eq!(ff.last_values()[0], Some(2.0));
        assert_eq!(ff.last_values()[1], Some(10.0));
    }

    #[test]
    fn nan_skipped_in_update() {
        let mut ff = ForwardFill::new(2).unwrap();
        ff.update(&[f64::NAN, 5.0]).unwrap();
        assert_eq!(ff.last_values()[0], None);
        assert_eq!(ff.last_values()[1], Some(5.0));
        ff.update(&[7.0, f64::NAN]).unwrap();
        assert_eq!(ff.last_values()[0], Some(7.0));
        assert_eq!(ff.last_values()[1], Some(5.0));
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let ff = ForwardFill::new(3).unwrap();
        assert!(matches!(
            ff.transform(&[1.0, 2.0]),
            Err(RillError::DimensionMismatch { .. })
        ));
        let mut ff = ff;
        assert!(matches!(
            ff.update(&[1.0, 2.0, 3.0, 4.0]),
            Err(RillError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn reset_clears_state() {
        let mut ff = ForwardFill::new(2).unwrap();
        ff.update(&[1.0, 2.0]).unwrap();
        ff.update(&[3.0, 4.0]).unwrap();
        assert_eq!(ff.samples_seen(), 2);
        ff.reset();
        assert_eq!(ff.samples_seen(), 0);
        assert_eq!(ff.last_values()[0], None);
        assert_eq!(ff.last_values()[1], None);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut ff = ForwardFill::new(2).unwrap();
        ff.update(&[1.0, f64::NAN]).unwrap();
        ff.update(&[3.0, 5.0]).unwrap();
        let json = serde_json::to_string(&ff).unwrap();
        let restored: ForwardFill = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.input_dim(), ff.input_dim());
        assert_eq!(restored.output_dim(), ff.output_dim());
        assert_eq!(restored.samples_seen(), ff.samples_seen());
        assert_eq!(restored.last_values(), ff.last_values());
    }
}

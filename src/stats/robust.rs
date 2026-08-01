//! Bounded robust streaming summaries.
//!
//! [`ClippedMean`] is exact for a caller-selected fixed clipping interval. The
//! bounds must come from domain knowledge or an independently validated
//! calibration path; estimating them from this statistic would hide tail
//! behaviour and invalidate the stated semantics.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::persistence::ValidateState;
use crate::traits::OnlineStatistic;

/// Exact online mean after clipping each observation to fixed bounds.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClippedMean {
    lower: f64,
    upper: f64,
    count: u64,
    mean: f64,
}

impl ClippedMean {
    /// Create a clipped mean with finite bounds satisfying `lower <= upper`.
    pub fn new(lower: f64, upper: f64) -> Result<Self, RillError> {
        ensure_finite("clipped mean lower", lower)?;
        ensure_finite("clipped mean upper", upper)?;
        if lower > upper {
            return Err(RillError::InvalidState(
                "clipped mean lower bound exceeds upper bound".to_owned(),
            ));
        }
        Ok(Self {
            lower,
            upper,
            count: 0,
            mean: 0.0,
        })
    }

    /// Fixed lower clipping bound.
    pub const fn lower(&self) -> f64 {
        self.lower
    }

    /// Fixed upper clipping bound.
    pub const fn upper(&self) -> f64 {
        self.upper
    }

    /// Current exact mean of clipped observations, or `0.0` when empty.
    pub const fn value(&self) -> f64 {
        self.mean
    }
}

impl OnlineStatistic for ClippedMean {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let clipped = value.clamp(self.lower, self.upper);
        let next_count = checked_increment(self.count, "clipped mean count")?;
        let delta = clipped - self.mean;
        ensure_finite("clipped mean delta", delta)?;
        let next_mean = self.mean + delta / next_count as f64;
        ensure_finite("clipped mean", next_mean)?;
        self.count = next_count;
        self.mean = next_mean;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.count = 0;
        self.mean = 0.0;
    }
}

impl ValidateState for ClippedMean {
    fn validate_state(&self) -> Result<(), RillError> {
        ClippedMean::new(self.lower, self.upper)?;
        ensure_finite("clipped mean", self.mean)?;
        if self.count == 0 && self.mean != 0.0 {
            return Err(RillError::InvalidState(
                "empty clipped mean must be zero".to_owned(),
            ));
        }
        if self.count > 0 && (self.mean < self.lower || self.mean > self.upper) {
            return Err(RillError::InvalidState(
                "clipped mean lies outside its clipping interval".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_mean_matches_offline_clipped_calculation() {
        let values = [-100.0, -1.0, 1.0, 2.0, 100.0];
        let mut statistic = ClippedMean::new(-2.0, 3.0).unwrap();
        for value in values {
            statistic.update(value).unwrap();
        }
        let expected = values
            .iter()
            .map(|value| value.clamp(-2.0, 3.0))
            .sum::<f64>()
            / values.len() as f64;
        assert!((statistic.value() - expected).abs() < 1e-12);
        statistic.validate_state().unwrap();
    }

    #[test]
    fn failure_is_atomic_and_reset_clears_state() {
        let mut statistic = ClippedMean::new(-10.0, 10.0).unwrap();
        statistic.update(1.0).unwrap();
        let before = statistic.clone();
        assert!(statistic.update(f64::INFINITY).is_err());
        assert_eq!(statistic, before);
        statistic.reset();
        assert_eq!(statistic.samples_seen(), 0);
        assert_eq!(statistic.value(), 0.0);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_preserves_future_updates() {
        let mut original = ClippedMean::new(-20.0, 20.0).unwrap();
        for value in 0..100 {
            original.update(value as f64).unwrap();
        }
        let json = serde_json::to_string(&original).unwrap();
        let mut restored: ClippedMean = serde_json::from_str(&json).unwrap();
        restored.validate_state().unwrap();
        for value in 100..200 {
            original.update(value as f64).unwrap();
            restored.update(value as f64).unwrap();
            assert_eq!(original, restored);
        }
    }
}

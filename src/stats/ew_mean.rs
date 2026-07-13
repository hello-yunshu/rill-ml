//! Exponentially weighted mean.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(1)`.
//!
//! The update rule is `mean = alpha * x + (1 - alpha) * mean`. The first
//! observation seeds the mean directly.

use crate::error::{RillError, ensure_finite};
use crate::traits::OnlineStatistic;

/// Exponentially weighted moving average.
///
/// `alpha` must satisfy `0 < alpha <= 1`. Smaller values give more weight to
/// the past; `alpha = 1` reduces to a [`LastValue`](crate::stats::extrema)-like
/// tracker.
///
/// # Examples
///
/// ```
/// use rill_ml::stats::ExponentiallyWeightedMean;
/// use rill_ml::OnlineStatistic;
///
/// let mut ew = ExponentiallyWeightedMean::new(0.5).unwrap();
/// ew.update(10.0).unwrap();
/// ew.update(20.0).unwrap();
/// // 10.0, then 0.5*20 + 0.5*10 = 15.0
/// assert!((ew.value() - 15.0).abs() < 1e-12);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExponentiallyWeightedMean {
    alpha: f64,
    mean: f64,
    count: u64,
}

impl ExponentiallyWeightedMean {
    /// Create a new exponentially weighted mean accumulator.
    ///
    /// Returns an error if `alpha` is not in `(0, 1]`.
    pub fn new(alpha: f64) -> Result<Self, RillError> {
        ensure_finite("alpha", alpha)?;
        if alpha <= 0.0 || alpha > 1.0 {
            return Err(RillError::InvalidParameter {
                name: "alpha",
                value: alpha,
            });
        }
        Ok(Self {
            alpha,
            mean: 0.0,
            count: 0,
        })
    }

    /// The configured alpha.
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Current weighted mean, or `0.0` if no observations have been seen.
    pub const fn value(&self) -> f64 {
        self.mean
    }

    /// Number of observations seen so far.
    pub const fn count(&self) -> u64 {
        self.count
    }
}

impl OnlineStatistic for ExponentiallyWeightedMean {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        if self.count == 0 {
            self.mean = value;
        } else {
            self.mean = self.alpha * value + (1.0 - self.alpha) * self.mean;
        }
        self.count += 1;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.mean = 0.0;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_seeds_mean() {
        let mut ew = ExponentiallyWeightedMean::new(0.3).unwrap();
        ew.update(10.0).unwrap();
        assert!((ew.value() - 10.0).abs() < 1e-12);
    }

    #[test]
    fn weighted_update_matches_formula() {
        let mut ew = ExponentiallyWeightedMean::new(0.5).unwrap();
        ew.update(10.0).unwrap();
        ew.update(20.0).unwrap();
        assert!((ew.value() - 15.0).abs() < 1e-12);
    }

    #[test]
    fn alpha_one_tracks_last_value() {
        let mut ew = ExponentiallyWeightedMean::new(1.0).unwrap();
        ew.update(3.0).unwrap();
        ew.update(7.0).unwrap();
        assert!((ew.value() - 7.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_alpha_rejected() {
        assert!(ExponentiallyWeightedMean::new(0.0).is_err());
        assert!(ExponentiallyWeightedMean::new(-0.1).is_err());
        assert!(ExponentiallyWeightedMean::new(1.5).is_err());
        assert!(ExponentiallyWeightedMean::new(f64::NAN).is_err());
    }

    #[test]
    fn reset_clears_state() {
        let mut ew = ExponentiallyWeightedMean::new(0.5).unwrap();
        ew.update(10.0).unwrap();
        ew.reset();
        assert_eq!(ew.count(), 0);
        assert_eq!(ew.value(), 0.0);
    }
}

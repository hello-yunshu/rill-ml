//! Online mean using a numerically stable incremental update.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(1)`.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::traits::OnlineStatistic;

/// Incremental mean computed with the delta method to minimize floating-point
/// accumulation error.
///
/// # Examples
///
/// ```
/// use rill_ml::stats::Mean;
/// use rill_ml::OnlineStatistic;
///
/// let mut m = Mean::new();
/// m.update(1.0).unwrap();
/// m.update(2.0).unwrap();
/// m.update(3.0).unwrap();
/// assert_eq!(m.value(), 2.0);
/// assert_eq!(m.samples_seen(), 3);
/// ```
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mean {
    count: u64,
    mean: f64,
}

impl Mean {
    /// Create a new empty mean accumulator.
    pub const fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
        }
    }

    /// Current mean, or `0.0` if no observations have been seen.
    pub const fn value(&self) -> f64 {
        self.mean
    }

    /// Number of observations seen so far.
    pub const fn count(&self) -> u64 {
        self.count
    }
}

impl OnlineStatistic for Mean {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let next_count = checked_increment(self.count, "mean sample")?;
        let delta = value - self.mean;
        ensure_finite("mean delta", delta)?;
        let next_mean = self.mean + delta / next_count as f64;
        ensure_finite("mean", next_mean)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn mean_of_simple_sequence() {
        let mut m = Mean::new();
        for x in [1.0, 2.0, 3.0, 4.0, 5.0] {
            m.update(x).unwrap();
        }
        assert_eq!(m.value(), 3.0);
        assert_eq!(m.count(), 5);
    }

    #[test]
    fn mean_empty_is_zero() {
        let m = Mean::new();
        assert_eq!(m.value(), 0.0);
        assert_eq!(m.count(), 0);
    }

    #[test]
    fn mean_rejects_non_finite() {
        let mut m = Mean::new();
        assert!(m.update(f64::NAN).is_err());
        assert!(m.update(f64::INFINITY).is_err());
        assert_eq!(m.count(), 0);
    }

    #[test]
    fn mean_rejects_overflow_without_mutating_state() {
        let mut m = Mean::new();
        m.update(f64::MAX).unwrap();
        let before = m.clone();
        assert!(m.update(-f64::MAX).is_err());
        assert_eq!(m.count(), before.count());
        assert_eq!(m.value(), before.value());
    }

    #[test]
    fn mean_reset() {
        let mut m = Mean::new();
        m.update(10.0).unwrap();
        m.update(20.0).unwrap();
        m.reset();
        assert_eq!(m.count(), 0);
        assert_eq!(m.value(), 0.0);
    }

    #[test]
    fn mean_matches_batch_formula() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let mut m = Mean::new();
        let mut data = Vec::new();
        for _ in 0..1000 {
            let x = rand::Rng::gen_range(&mut rng, -100.0..100.0);
            m.update(x).unwrap();
            data.push(x);
        }
        let batch: f64 = data.iter().sum::<f64>() / data.len() as f64;
        assert!((m.value() - batch).abs() < 1e-9);
    }
}

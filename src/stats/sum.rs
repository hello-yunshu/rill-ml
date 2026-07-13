//! Online sum of observations.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(1)`.

use crate::error::{RillError, checked_finite_add, checked_increment, ensure_finite};
use crate::traits::OnlineStatistic;

/// A running sum.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sum {
    sum: f64,
    count: u64,
}

impl Sum {
    /// Create a new empty sum accumulator.
    pub const fn new() -> Self {
        Self { sum: 0.0, count: 0 }
    }

    /// Current sum.
    pub const fn value(&self) -> f64 {
        self.sum
    }

    /// Number of observations.
    pub const fn count(&self) -> u64 {
        self.count
    }
}

impl OnlineStatistic for Sum {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let next_sum = checked_finite_add(self.sum, value, "sum")?;
        let next_count = checked_increment(self.count, "sum sample")?;
        self.sum = next_sum;
        self.count = next_count;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.sum = 0.0;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_accumulates() {
        let mut s = Sum::new();
        s.update(1.5).unwrap();
        s.update(2.5).unwrap();
        assert_eq!(s.value(), 4.0);
    }

    #[test]
    fn sum_rejects_overflow_without_mutating_state() {
        let mut s = Sum::new();
        s.update(f64::MAX).unwrap();
        assert!(s.update(f64::MAX).is_err());
        assert_eq!(s.value(), f64::MAX);
        assert_eq!(s.count(), 1);
    }
}

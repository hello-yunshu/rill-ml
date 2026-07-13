//! Online sum of observations.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(1)`.

use crate::error::{RillError, ensure_finite};
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
        self.sum += value;
        self.count += 1;
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
}

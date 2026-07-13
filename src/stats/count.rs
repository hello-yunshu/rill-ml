//! Online count of observations.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(1)`.

use crate::error::{RillError, checked_increment};
use crate::traits::OnlineStatistic;

/// A simple observation counter.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Count {
    count: u64,
}

impl Count {
    /// Create a new counter.
    pub const fn new() -> Self {
        Self { count: 0 }
    }

    /// Current count.
    pub const fn value(&self) -> u64 {
        self.count
    }
}

impl OnlineStatistic for Count {
    fn update(&mut self, _value: f64) -> Result<(), RillError> {
        self.count = checked_increment(self.count, "count sample")?;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_increments() {
        let mut c = Count::new();
        c.update(1.0).unwrap();
        c.update(2.0).unwrap();
        assert_eq!(c.value(), 2);
    }
}

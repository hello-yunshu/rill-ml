//! Online min and max trackers.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(1)`.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::traits::OnlineStatistic;

/// Running minimum of observed values.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Min {
    current: Option<f64>,
    count: u64,
}

impl Min {
    /// Create a new empty minimum tracker.
    pub const fn new() -> Self {
        Self {
            current: None,
            count: 0,
        }
    }

    /// Current minimum, or `None` if no observations have been seen.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl OnlineStatistic for Min {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let next_count = checked_increment(self.count, "minimum sample")?;
        self.current = Some(match self.current {
            None => value,
            Some(c) => c.min(value),
        });
        self.count = next_count;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.current = None;
        self.count = 0;
    }
}

/// Running maximum of observed values.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Max {
    current: Option<f64>,
    count: u64,
}

impl Max {
    /// Create a new empty maximum tracker.
    pub const fn new() -> Self {
        Self {
            current: None,
            count: 0,
        }
    }

    /// Current maximum, or `None` if no observations have been seen.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl OnlineStatistic for Max {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let next_count = checked_increment(self.count, "maximum sample")?;
        self.current = Some(match self.current {
            None => value,
            Some(c) => c.max(value),
        });
        self.count = next_count;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.current = None;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_tracks_minimum() {
        let mut m = Min::new();
        assert!(m.value().is_none());
        for x in [3.0, 1.0, 4.0, 1.0, 5.0, -2.0] {
            m.update(x).unwrap();
        }
        assert_eq!(m.value(), Some(-2.0));
    }

    #[test]
    fn max_tracks_maximum() {
        let mut m = Max::new();
        for x in [3.0, 1.0, 4.0, 1.0, 5.0, -2.0] {
            m.update(x).unwrap();
        }
        assert_eq!(m.value(), Some(5.0));
    }
}

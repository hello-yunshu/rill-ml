//! Rolling (windowed) statistics with fixed capacity.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(window_size)`.
//!
//! The implementation stores the window contents in a `VecDeque` and recomputes
//! the statistic on each update. For the window sizes typically used in online
//! learning this is simple and correct. A more sophisticated incremental update
//! could be added later, but only if accompanied by thorough removal tests.

use std::collections::VecDeque;

use crate::error::{RillError, ensure_finite};
use crate::stats::variance::VarianceKind;
use crate::traits::OnlineStatistic;

/// Rolling mean over a fixed-size window.
///
/// The window is a FIFO queue: when full, the oldest observation is removed
/// before the newest is inserted.
///
/// # Examples
///
/// ```
/// use rill_ml::stats::RollingMean;
/// use rill_ml::OnlineStatistic;
///
/// let mut rm = RollingMean::new(3).unwrap();
/// rm.update(1.0).unwrap();
/// rm.update(2.0).unwrap();
/// rm.update(3.0).unwrap();
/// assert!((rm.value().unwrap() - 2.0).abs() < 1e-12);
/// rm.update(6.0).unwrap(); // window becomes [2, 3, 6]
/// assert!((rm.value().unwrap() - 3.6666666666666665).abs() < 1e-12);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RollingMean {
    window: VecDeque<f64>,
    capacity: usize,
}

impl RollingMean {
    /// Create a new rolling mean with the given window capacity.
    ///
    /// Returns an error if `capacity` is zero.
    pub fn new(capacity: usize) -> Result<Self, RillError> {
        if capacity == 0 {
            return Err(RillError::InvalidWindowSize);
        }
        Ok(Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
        })
    }

    /// The configured window capacity.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of observations currently in the window.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// Whether the window is currently empty.
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Current rolling mean, or `None` if the window is empty.
    pub fn value(&self) -> Option<f64> {
        if self.window.is_empty() {
            None
        } else {
            let sum: f64 = self.window.iter().sum();
            if !sum.is_finite() {
                return None;
            }
            Some(sum / self.window.len() as f64)
        }
    }
}

impl OnlineStatistic for RollingMean {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        if self.window.len() == self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(value);
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.window.len() as u64
    }

    fn reset(&mut self) {
        self.window.clear();
    }
}

/// Rolling variance over a fixed-size window.
///
/// Uses [`VarianceKind`] to select population or sample variance. The window
/// contents are stored and the variance recomputed on each query.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RollingVariance {
    window: VecDeque<f64>,
    capacity: usize,
    kind: VarianceKind,
}

impl RollingVariance {
    /// Create a new rolling variance accumulator.
    ///
    /// Returns an error if `capacity` is zero.
    pub fn new(capacity: usize, kind: VarianceKind) -> Result<Self, RillError> {
        if capacity == 0 {
            return Err(RillError::InvalidWindowSize);
        }
        Ok(Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
            kind,
        })
    }

    /// The configured window capacity.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// The configured variance kind.
    pub const fn kind(&self) -> VarianceKind {
        self.kind
    }

    /// Number of observations currently in the window.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// Whether the window is currently empty.
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Current rolling mean, or `None` if the window is empty.
    pub fn mean(&self) -> Option<f64> {
        if self.window.is_empty() {
            None
        } else {
            let sum: f64 = self.window.iter().sum();
            if !sum.is_finite() {
                return None;
            }
            Some(sum / self.window.len() as f64)
        }
    }

    /// Current rolling variance, or `None` when not enough data is in the window.
    pub fn value(&self) -> Option<f64> {
        let n = self.window.len();
        if n == 0 {
            return None;
        }
        let denom = match self.kind {
            VarianceKind::Population => n,
            VarianceKind::Sample => {
                if n < 2 {
                    return None;
                }
                n - 1
            }
        };
        let mean = self.mean()?;
        let ss = self.window.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
        if !ss.is_finite() {
            return None;
        }
        Some(ss / denom as f64)
    }

    /// Current rolling standard deviation, or `None` when not enough data.
    pub fn std_dev(&self) -> Option<f64> {
        self.value().map(|v| v.sqrt())
    }
}

impl OnlineStatistic for RollingVariance {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        if self.window.len() == self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(value);
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.window.len() as u64
    }

    fn reset(&mut self) {
        self.window.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_mean_basic() {
        let mut rm = RollingMean::new(3).unwrap();
        for x in [1.0, 2.0, 3.0] {
            rm.update(x).unwrap();
        }
        assert!((rm.value().unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn rolling_mean_evicts_oldest() {
        let mut rm = RollingMean::new(2).unwrap();
        rm.update(10.0).unwrap();
        rm.update(20.0).unwrap();
        rm.update(30.0).unwrap();
        assert!((rm.value().unwrap() - 25.0).abs() < 1e-12);
    }

    #[test]
    fn rolling_mean_empty_returns_none() {
        let rm = RollingMean::new(5).unwrap();
        assert!(rm.value().is_none());
    }

    #[test]
    fn rolling_mean_zero_capacity_rejected() {
        assert!(matches!(
            RollingMean::new(0),
            Err(RillError::InvalidWindowSize)
        ));
    }

    #[test]
    fn rolling_variance_population() {
        let mut rv = RollingVariance::new(4, VarianceKind::Population).unwrap();
        for x in [1.0, 2.0, 3.0, 4.0] {
            rv.update(x).unwrap();
        }
        // population variance of [1,2,3,4] = 1.25
        assert!((rv.value().unwrap() - 1.25).abs() < 1e-12);
    }

    #[test]
    fn rolling_variance_evicts_correctly() {
        let mut rv = RollingVariance::new(3, VarianceKind::Population).unwrap();
        for x in [1.0, 2.0, 3.0, 4.0] {
            rv.update(x).unwrap();
        }
        // window is now [2, 3, 4], mean=3, var = (1+0+1)/3 = 0.6666...
        assert!((rv.value().unwrap() - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn rolling_variance_sample_insufficient() {
        let mut rv = RollingVariance::new(5, VarianceKind::Sample).unwrap();
        rv.update(1.0).unwrap();
        assert!(rv.value().is_none());
    }

    #[test]
    fn rolling_variance_zero_capacity_rejected() {
        assert!(matches!(
            RollingVariance::new(0, VarianceKind::Population),
            Err(RillError::InvalidWindowSize)
        ));
    }
}

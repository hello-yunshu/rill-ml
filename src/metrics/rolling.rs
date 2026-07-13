//! Rolling metrics with fixed-size windows.
//!
//! These store per-sample contributions and correctly maintain running sums
//! when the oldest contribution is evicted. Space complexity: `O(window_size)`.

use std::collections::VecDeque;

use crate::error::{RillError, checked_finite_add, ensure_finite, ensure_finite_target};
use crate::traits::Metric;

/// Rolling Mean Absolute Error.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RollingMae {
    errors: VecDeque<f64>,
    sum: f64,
    capacity: usize,
}

impl RollingMae {
    /// Create a new rolling MAE with the given window capacity.
    pub fn new(capacity: usize) -> Result<Self, RillError> {
        if capacity == 0 {
            return Err(RillError::InvalidWindowSize);
        }
        Ok(Self {
            errors: VecDeque::with_capacity(capacity),
            sum: 0.0,
            capacity,
        })
    }
}

impl Metric for RollingMae {
    type Truth = f64;
    type Prediction = f64;

    fn update(&mut self, truth: f64, prediction: f64) -> Result<(), RillError> {
        ensure_finite_target(truth)?;
        ensure_finite("prediction", prediction)?;
        let err = (truth - prediction).abs();
        ensure_finite("rolling absolute error", err)?;
        let base_sum = if self.errors.len() == self.capacity {
            checked_finite_add(
                self.sum,
                -self.errors.front().copied().unwrap_or(0.0),
                "rolling MAE sum",
            )?
        } else {
            self.sum
        };
        let next_sum = checked_finite_add(base_sum, err, "rolling MAE sum")?;
        if self.errors.len() == self.capacity {
            self.errors.pop_front();
        }
        self.errors.push_back(err);
        self.sum = next_sum;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.errors.is_empty() {
            None
        } else {
            Some(self.sum / self.errors.len() as f64)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.errors.len() as u64
    }

    fn reset(&mut self) {
        self.errors.clear();
        self.sum = 0.0;
    }
}

/// Rolling Mean Squared Error.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RollingMse {
    errors: VecDeque<f64>,
    sum: f64,
    capacity: usize,
}

impl RollingMse {
    /// Create a new rolling MSE.
    pub fn new(capacity: usize) -> Result<Self, RillError> {
        if capacity == 0 {
            return Err(RillError::InvalidWindowSize);
        }
        Ok(Self {
            errors: VecDeque::with_capacity(capacity),
            sum: 0.0,
            capacity,
        })
    }
}

impl Metric for RollingMse {
    type Truth = f64;
    type Prediction = f64;

    fn update(&mut self, truth: f64, prediction: f64) -> Result<(), RillError> {
        ensure_finite_target(truth)?;
        ensure_finite("prediction", prediction)?;
        let difference = truth - prediction;
        ensure_finite("rolling squared error input", difference)?;
        let err = difference.powi(2);
        ensure_finite("rolling squared error", err)?;
        let base_sum = if self.errors.len() == self.capacity {
            checked_finite_add(
                self.sum,
                -self.errors.front().copied().unwrap_or(0.0),
                "rolling MSE sum",
            )?
        } else {
            self.sum
        };
        let next_sum = checked_finite_add(base_sum, err, "rolling MSE sum")?;
        if self.errors.len() == self.capacity {
            self.errors.pop_front();
        }
        self.errors.push_back(err);
        self.sum = next_sum;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.errors.is_empty() {
            None
        } else {
            Some(self.sum / self.errors.len() as f64)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.errors.len() as u64
    }

    fn reset(&mut self) {
        self.errors.clear();
        self.sum = 0.0;
    }
}

/// Rolling Accuracy for binary classification.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RollingAccuracy {
    correct: VecDeque<bool>,
    sum: u64,
    capacity: usize,
}

impl RollingAccuracy {
    /// Create a new rolling accuracy.
    pub fn new(capacity: usize) -> Result<Self, RillError> {
        if capacity == 0 {
            return Err(RillError::InvalidWindowSize);
        }
        Ok(Self {
            correct: VecDeque::with_capacity(capacity),
            sum: 0,
            capacity,
        })
    }
}

impl Metric for RollingAccuracy {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        let is_correct = truth == prediction;
        if self.correct.len() == self.capacity {
            if let Some(old) = self.correct.pop_front() {
                if old {
                    self.sum -= 1;
                }
            }
        }
        self.correct.push_back(is_correct);
        if is_correct {
            self.sum += 1;
        }
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.correct.is_empty() {
            None
        } else {
            Some(self.sum as f64 / self.correct.len() as f64)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.correct.len() as u64
    }

    fn reset(&mut self) {
        self.correct.clear();
        self.sum = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_mae_evicts_correctly() {
        let mut m = RollingMae::new(2).unwrap();
        m.update(0.0, 2.0).unwrap(); // err=2
        m.update(0.0, 4.0).unwrap(); // err=4, window=[2,4], sum=6, mean=3
        assert!((m.value().unwrap() - 3.0).abs() < 1e-12);
        m.update(0.0, 6.0).unwrap(); // err=6, window=[4,6], sum=10, mean=5
        assert!((m.value().unwrap() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn rolling_mse_evicts_correctly() {
        let mut m = RollingMse::new(2).unwrap();
        m.update(0.0, 2.0).unwrap(); // sq_err=4
        m.update(0.0, 4.0).unwrap(); // sq_err=16, mean=(4+16)/2=10
        assert!((m.value().unwrap() - 10.0).abs() < 1e-12);
        m.update(0.0, 6.0).unwrap(); // sq_err=36, mean=(16+36)/2=26
        assert!((m.value().unwrap() - 26.0).abs() < 1e-12);
    }

    #[test]
    fn rolling_accuracy_evicts_correctly() {
        let mut m = RollingAccuracy::new(2).unwrap();
        m.update(true, true).unwrap(); // correct
        m.update(false, false).unwrap(); // correct
        assert!((m.value().unwrap() - 1.0).abs() < 1e-12);
        m.update(true, false).unwrap(); // incorrect, window=[correct, incorrect]
        assert!((m.value().unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn rolling_zero_capacity_rejected() {
        assert!(RollingMae::new(0).is_err());
        assert!(RollingMse::new(0).is_err());
        assert!(RollingAccuracy::new(0).is_err());
    }

    #[test]
    fn rolling_metrics_reject_overflow_without_mutating_state() {
        let mut mae = RollingMae::new(2).unwrap();
        let mut mse = RollingMse::new(2).unwrap();
        mae.update(0.0, 1.0).unwrap();
        mse.update(0.0, 1.0).unwrap();

        assert!(mae.update(f64::MAX, -f64::MAX).is_err());
        assert!(mse.update(f64::MAX, 0.0).is_err());

        assert_eq!(mae.samples_seen(), 1);
        assert_eq!(mse.samples_seen(), 1);
        assert_eq!(mae.value(), Some(1.0));
        assert_eq!(mse.value(), Some(1.0));
    }

    #[test]
    fn rolling_empty_returns_none() {
        assert!(RollingMae::new(5).unwrap().value().is_none());
    }
}

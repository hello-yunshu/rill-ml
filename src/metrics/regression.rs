//! Regression metrics: MAE, MSE, RMSE, R².

use crate::error::{RillError, ensure_finite, ensure_finite_target};
use crate::traits::Metric;

/// Mean Absolute Error.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mae {
    sum_abs_error: f64,
    count: u64,
}

impl Mae {
    /// Create a new MAE accumulator.
    pub const fn new() -> Self {
        Self {
            sum_abs_error: 0.0,
            count: 0,
        }
    }
}

impl Metric for Mae {
    type Truth = f64;
    type Prediction = f64;

    fn update(&mut self, truth: f64, prediction: f64) -> Result<(), RillError> {
        ensure_finite_target(truth)?;
        ensure_finite("prediction", prediction)?;
        self.sum_abs_error += (truth - prediction).abs();
        self.count += 1;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum_abs_error / self.count as f64)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.sum_abs_error = 0.0;
        self.count = 0;
    }
}

/// Mean Squared Error.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mse {
    sum_sq_error: f64,
    count: u64,
}

impl Mse {
    /// Create a new MSE accumulator.
    pub const fn new() -> Self {
        Self {
            sum_sq_error: 0.0,
            count: 0,
        }
    }
}

impl Metric for Mse {
    type Truth = f64;
    type Prediction = f64;

    fn update(&mut self, truth: f64, prediction: f64) -> Result<(), RillError> {
        ensure_finite_target(truth)?;
        ensure_finite("prediction", prediction)?;
        let err = truth - prediction;
        self.sum_sq_error += err * err;
        self.count += 1;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum_sq_error / self.count as f64)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.sum_sq_error = 0.0;
        self.count = 0;
    }
}

/// Root Mean Squared Error.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rmse {
    mse: Mse,
}

impl Rmse {
    /// Create a new RMSE accumulator.
    pub const fn new() -> Self {
        Self { mse: Mse::new() }
    }
}

impl Metric for Rmse {
    type Truth = f64;
    type Prediction = f64;

    fn update(&mut self, truth: f64, prediction: f64) -> Result<(), RillError> {
        self.mse.update(truth, prediction)
    }

    fn value(&self) -> Option<f64> {
        self.mse.value().map(|v| v.sqrt())
    }

    fn samples_seen(&self) -> u64 {
        self.mse.samples_seen()
    }

    fn reset(&mut self) {
        self.mse.reset();
    }
}

/// R² (coefficient of determination).
///
/// Computed online as `1 - SS_res / SS_tot`, where `SS_tot` uses the running
/// mean of the truth. Returns `None` when fewer than 2 samples have been seen
/// or when `SS_tot` is zero (constant truth).
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct R2 {
    ss_res: f64,
    sum_truth: f64,
    sum_truth_sq: f64,
    count: u64,
}

impl R2 {
    /// Create a new R² accumulator.
    pub const fn new() -> Self {
        Self {
            ss_res: 0.0,
            sum_truth: 0.0,
            sum_truth_sq: 0.0,
            count: 0,
        }
    }
}

impl Metric for R2 {
    type Truth = f64;
    type Prediction = f64;

    fn update(&mut self, truth: f64, prediction: f64) -> Result<(), RillError> {
        ensure_finite_target(truth)?;
        ensure_finite("prediction", prediction)?;
        let err = truth - prediction;
        self.ss_res += err * err;
        self.sum_truth += truth;
        self.sum_truth_sq += truth * truth;
        self.count += 1;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.count < 2 {
            return None;
        }
        let n = self.count as f64;
        let mean = self.sum_truth / n;
        let ss_tot = self.sum_truth_sq - n * mean * mean;
        if ss_tot.abs() < f64::EPSILON {
            return None;
        }
        Some(1.0 - self.ss_res / ss_tot)
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.ss_res = 0.0;
        self.sum_truth = 0.0;
        self.sum_truth_sq = 0.0;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mae_basic() {
        let mut m = Mae::new();
        m.update(3.0, 5.0).unwrap(); // err=2
        m.update(5.0, 4.0).unwrap(); // err=1
        assert!((m.value().unwrap() - 1.5).abs() < 1e-12);
    }

    #[test]
    fn mse_basic() {
        let mut m = Mse::new();
        m.update(3.0, 5.0).unwrap(); // err=2, sq=4
        m.update(5.0, 4.0).unwrap(); // err=1, sq=1
        assert!((m.value().unwrap() - 2.5).abs() < 1e-12);
    }

    #[test]
    fn rmse_basic() {
        let mut m = Rmse::new();
        m.update(3.0, 5.0).unwrap();
        m.update(5.0, 4.0).unwrap();
        assert!((m.value().unwrap() - 2.5_f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn r2_perfect_prediction_is_one() {
        let mut m = R2::new();
        m.update(1.0, 1.0).unwrap();
        m.update(2.0, 2.0).unwrap();
        m.update(3.0, 3.0).unwrap();
        assert!((m.value().unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn r2_mean_prediction_is_zero() {
        let mut m = R2::new();
        // predict the mean every time
        m.update(1.0, 2.0).unwrap();
        m.update(3.0, 2.0).unwrap();
        // mean=2, ss_res = 1+1=2, ss_tot = 1+1=2 -> R2=0
        assert!((m.value().unwrap()).abs() < 1e-9);
    }

    #[test]
    fn r2_insufficient_data_returns_none() {
        let mut m = R2::new();
        m.update(1.0, 1.0).unwrap();
        assert!(m.value().is_none());
    }

    #[test]
    fn r2_constant_truth_returns_none() {
        let mut m = R2::new();
        m.update(5.0, 3.0).unwrap();
        m.update(5.0, 4.0).unwrap();
        assert!(m.value().is_none());
    }

    #[test]
    fn non_finite_rejected() {
        let mut m = Mae::new();
        assert!(m.update(f64::NAN, 1.0).is_err());
        assert!(m.update(1.0, f64::INFINITY).is_err());
    }

    #[test]
    fn empty_metric_returns_none() {
        assert!(Mae::new().value().is_none());
        assert!(Mse::new().value().is_none());
        assert!(Rmse::new().value().is_none());
        assert!(R2::new().value().is_none());
    }
}

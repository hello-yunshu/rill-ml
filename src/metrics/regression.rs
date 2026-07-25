//! Regression metrics: MAE, MSE, RMSE, R².

use crate::error::{
    RillError, checked_finite_add, checked_increment, ensure_finite, ensure_finite_target,
};
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
        let error = truth - prediction;
        ensure_finite("absolute error", error)?;
        let next_sum = checked_finite_add(self.sum_abs_error, error.abs(), "MAE sum")?;
        let next_count = checked_increment(self.count, "MAE sample")?;
        self.sum_abs_error = next_sum;
        self.count = next_count;
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
        ensure_finite("squared error input", err)?;
        let squared_error = err * err;
        ensure_finite("squared error", squared_error)?;
        let next_sum = checked_finite_add(self.sum_sq_error, squared_error, "MSE sum")?;
        let next_count = checked_increment(self.count, "MSE sample")?;
        self.sum_sq_error = next_sum;
        self.count = next_count;
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
/// Uses Welford's online algorithm for the variance of the truth, avoiding
/// the catastrophic cancellation that `sum(y²) - n · mean(y)²` suffers on
/// large-offset, small-variance data. Returns `None` when fewer than 2
/// samples have been seen or when the truth variance is zero (constant
/// truth).
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct R2 {
    ss_res: f64,
    mean_truth: f64,
    /// Running `M2 = sum((y_i - mean)^2)` from Welford's algorithm.
    m2_truth: f64,
    count: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for R2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct R2State {
            ss_res: f64,
            mean_truth: f64,
            m2_truth: f64,
            count: u64,
        }

        let state = R2State::deserialize(deserializer)?;
        if !state.ss_res.is_finite() || state.ss_res < 0.0 {
            return Err(serde::de::Error::custom(
                "r2 ss_res must be finite and non-negative",
            ));
        }
        if !state.mean_truth.is_finite() {
            return Err(serde::de::Error::custom("r2 mean_truth must be finite"));
        }
        if !state.m2_truth.is_finite() || state.m2_truth < 0.0 {
            return Err(serde::de::Error::custom(
                "r2 m2_truth must be finite and non-negative",
            ));
        }
        Ok(R2 {
            ss_res: state.ss_res,
            mean_truth: state.mean_truth,
            m2_truth: state.m2_truth,
            count: state.count,
        })
    }
}

impl R2 {
    /// Create a new R² accumulator.
    pub const fn new() -> Self {
        Self {
            ss_res: 0.0,
            mean_truth: 0.0,
            m2_truth: 0.0,
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
        ensure_finite("R2 error", err)?;
        let squared_error = err * err;
        ensure_finite("R2 squared error", squared_error)?;

        // Welford update for the truth variance.
        let next_count = checked_increment(self.count, "R2 sample")?;
        let delta = truth - self.mean_truth;
        ensure_finite("R2 welford delta", delta)?;
        let next_mean = self.mean_truth + delta / next_count as f64;
        ensure_finite("R2 welford mean", next_mean)?;
        let delta2 = truth - next_mean;
        ensure_finite("R2 welford delta2", delta2)?;
        let m2_delta = delta * delta2;
        ensure_finite("R2 welford m2_delta", m2_delta)?;
        let next_m2 = checked_finite_add(self.m2_truth, m2_delta, "R2 m2_truth")?;

        let next_ss_res = checked_finite_add(self.ss_res, squared_error, "R2 residual sum")?;

        // Commit atomically.
        self.count = next_count;
        self.mean_truth = next_mean;
        self.m2_truth = next_m2;
        self.ss_res = next_ss_res;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.count < 2 {
            return None;
        }
        // M2 is the sum of squared deviations; treat floating-point noise
        // that drives it slightly negative as zero (constant truth).
        if self.m2_truth <= 0.0 {
            return None;
        }
        Some(1.0 - self.ss_res / self.m2_truth)
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.ss_res = 0.0;
        self.mean_truth = 0.0;
        self.m2_truth = 0.0;
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
    fn metrics_reject_overflow_without_mutating_state() {
        let mut mae = Mae::new();
        let mut mse = Mse::new();
        let mut r2 = R2::new();

        assert!(mae.update(f64::MAX, -f64::MAX).is_err());
        assert!(mse.update(f64::MAX, 0.0).is_err());
        assert!(r2.update(f64::MAX, 0.0).is_err());

        assert_eq!(mae.samples_seen(), 0);
        assert_eq!(mse.samples_seen(), 0);
        assert_eq!(r2.samples_seen(), 0);
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
    fn r2_welford_large_offset_small_variance() {
        // Large offset, tiny variance: the old `sum(y²) - n · mean(y)²`
        // formula lost all precision. Welford must remain accurate.
        let truths = [
            1_000_000_000_001.0,
            1_000_000_000_002.0,
            1_000_000_000_003.0,
        ];
        let mut m = R2::new();
        for y in truths {
            // Perfect prediction: R² should be 1.0.
            m.update(y, y).unwrap();
        }
        assert!((m.value().unwrap() - 1.0).abs() < 1e-9);

        // Predict the (known) mean every time: R² should be ~0.0.
        let mean = truths.iter().sum::<f64>() / truths.len() as f64;
        let mut m = R2::new();
        for y in truths {
            m.update(y, mean).unwrap();
        }
        assert!(m.value().unwrap().abs() < 1e-6);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn r2_partial_update_is_atomic() {
        // Restore a counter near overflow; the Welford update must fail
        // without mutating any state.
        let json = format!(
            "{{\"ss_res\":1.0,\"mean_truth\":1.0,\"m2_truth\":1.0,\"count\":{}}}",
            u64::MAX
        );
        let mut m: R2 = serde_json::from_str(&json).unwrap();
        let result = m.update(1.0, 1.0);
        assert!(result.is_err(), "expected counter overflow");
        assert_eq!(m.count, u64::MAX);
        assert_eq!(m.ss_res, 1.0);
        assert_eq!(m.mean_truth, 1.0);
        assert_eq!(m.m2_truth, 1.0);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn r2_serde_rejects_negative_m2() {
        let json = "{\"ss_res\":0.0,\"mean_truth\":0.0,\"m2_truth\":-1.0,\"count\":2}";
        assert!(serde_json::from_str::<R2>(json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn r2_serde_rejects_negative_ss_res() {
        // Regression: a malicious or corrupted state with negative residual
        // sum of squares must be rejected. ``ss_res`` is a sum of squares and
        // cannot be negative outside of floating-point corruption.
        let json = "{\"ss_res\":-0.5,\"mean_truth\":0.0,\"m2_truth\":1.0,\"count\":2}";
        assert!(serde_json::from_str::<R2>(json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn r2_serde_roundtrip_preserves_state() {
        let mut m = R2::new();
        m.update(1.0, 1.0).unwrap();
        m.update(2.0, 1.5).unwrap();
        m.update(3.0, 2.5).unwrap();
        let before = m.value();
        let json = serde_json::to_string(&m).unwrap();
        let restored: R2 = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.count, 3);
        assert_eq!(restored.value(), before);
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

//! Huber loss, robust to outliers.
//!
//! For `|residual| <= delta`: `0.5 * residual^2`.
//! For `|residual| > delta`: `delta * (|residual| - 0.5 * delta)`.
//!
//! The gradient w.r.t. the prediction is:
//! - `residual` if `|residual| <= delta`
//! - `delta * sign(residual)` otherwise

use crate::error::{RillError, ensure_finite};

/// Huber loss with configurable delta.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HuberLoss {
    delta: f64,
}

impl HuberLoss {
    /// Create a new Huber loss with the given delta.
    ///
    /// Returns an error if `delta` is not finite and strictly positive.
    pub fn new(delta: f64) -> Result<Self, RillError> {
        ensure_finite("delta", delta)?;
        if delta <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "delta",
                value: delta,
            });
        }
        Ok(Self { delta })
    }

    /// The configured delta.
    pub const fn delta(&self) -> f64 {
        self.delta
    }

    /// Compute the Huber loss.
    pub fn loss(&self, prediction: f64, target: f64) -> f64 {
        let residual = prediction - target;
        let abs_r = residual.abs();
        if abs_r <= self.delta {
            0.5 * residual * residual
        } else {
            self.delta * (abs_r - 0.5 * self.delta)
        }
    }

    /// Compute the gradient w.r.t. the prediction.
    pub fn gradient(&self, prediction: f64, target: f64) -> f64 {
        let residual = prediction - target;
        let abs_r = residual.abs();
        if abs_r <= self.delta {
            residual
        } else {
            self.delta * residual.signum()
        }
    }
}

impl Default for HuberLoss {
    fn default() -> Self {
        Self { delta: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadratic_region_matches_squared() {
        let h = HuberLoss::new(1.0).unwrap();
        // residual = 0.5, within delta
        assert!((h.loss(1.5, 1.0) - 0.5 * 0.25).abs() < 1e-12);
        assert!((h.gradient(1.5, 1.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn linear_region_clipped() {
        let h = HuberLoss::new(1.0).unwrap();
        // residual = 3, outside delta=1
        let expected_loss = 1.0 * (3.0 - 0.5);
        assert!((h.loss(4.0, 1.0) - expected_loss).abs() < 1e-12);
        assert!((h.gradient(4.0, 1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_delta_rejected() {
        assert!(HuberLoss::new(0.0).is_err());
        assert!(HuberLoss::new(-1.0).is_err());
        assert!(HuberLoss::new(f64::NAN).is_err());
    }
}

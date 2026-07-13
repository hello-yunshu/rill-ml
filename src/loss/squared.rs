//! Squared error loss: `0.5 * (y - y_hat)^2`.
//!
//! The gradient with respect to the prediction is `(y_hat - y)`.

use crate::error::{RillError, ensure_finite_target};

/// Squared error loss.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SquaredError;

impl SquaredError {
    /// Create a new squared error loss.
    pub const fn new() -> Self {
        Self
    }

    /// Compute `0.5 * (prediction - target)^2`.
    pub fn loss(prediction: f64, target: f64) -> f64 {
        let diff = prediction - target;
        0.5 * diff * diff
    }

    /// Compute the derivative w.r.t. prediction: `prediction - target`.
    pub fn gradient(prediction: f64, target: f64) -> f64 {
        prediction - target
    }

    /// Validate that both prediction and target are finite.
    pub fn validate(prediction: f64, target: f64) -> Result<(), RillError> {
        crate::error::ensure_finite("prediction", prediction)?;
        ensure_finite_target(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loss_at_zero_residual_is_zero() {
        assert_eq!(SquaredError::loss(5.0, 5.0), 0.0);
    }

    #[test]
    fn loss_value() {
        assert!((SquaredError::loss(3.0, 1.0) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn gradient_sign() {
        assert!((SquaredError::gradient(3.0, 1.0) - 2.0).abs() < 1e-12);
        assert!((SquaredError::gradient(1.0, 3.0) + 2.0).abs() < 1e-12);
    }
}

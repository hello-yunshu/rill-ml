//! Binary log loss (cross-entropy) for logistic regression.
//!
//! `loss = -(y * log(p) + (1 - y) * log(1 - p))`
//!
//! Probabilities are clipped to `[epsilon, 1 - epsilon]` for numerical
//! stability. The gradient w.r.t. the raw logit `z` is `(p - y)`.

use crate::error::{RillError, ensure_finite};

/// Default clipping epsilon for probabilities.
pub const DEFAULT_EPSILON: f64 = 1e-15;

/// Binary cross-entropy (log) loss.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BinaryLogLoss {
    epsilon: f64,
}

impl BinaryLogLoss {
    /// Create a new log loss with the default epsilon.
    pub const fn new() -> Self {
        Self {
            epsilon: DEFAULT_EPSILON,
        }
    }

    /// Create a new log loss with a custom epsilon.
    pub fn with_epsilon(epsilon: f64) -> Result<Self, RillError> {
        ensure_finite("epsilon", epsilon)?;
        if epsilon <= 0.0 || epsilon >= 0.5 {
            return Err(RillError::InvalidParameter {
                name: "epsilon",
                value: epsilon,
            });
        }
        Ok(Self { epsilon })
    }

    /// The configured epsilon.
    pub const fn epsilon(&self) -> f64 {
        self.epsilon
    }

    /// Clip a probability to `[epsilon, 1 - epsilon]`.
    pub fn clip(&self, p: f64) -> f64 {
        p.clamp(self.epsilon, 1.0 - self.epsilon)
    }

    /// Compute the log loss given a probability and boolean target.
    pub fn loss(&self, probability: f64, target: bool) -> f64 {
        let p = self.clip(probability);
        let y = if target { 1.0 } else { 0.0 };
        -(y * p.ln() + (1.0 - y) * (1.0 - p).ln())
    }

    /// Gradient of the loss w.r.t. the logit `z = log(p / (1 - p))`.
    ///
    /// This equals `p - y` where `p = sigmoid(z)`.
    pub fn gradient_wrt_logit(&self, probability: f64, target: bool) -> f64 {
        let y = if target { 1.0 } else { 0.0 };
        probability - y
    }
}

impl Default for BinaryLogLoss {
    fn default() -> Self {
        Self::new()
    }
}

/// Numerically stable sigmoid function.
///
/// For `z >= 0`: `1 / (1 + exp(-z))`.
/// For `z < 0`: `exp(z) / (1 + exp(z))`.
pub fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigmoid_bounds() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-12);
        assert!(sigmoid(-100.0) > 0.0 && sigmoid(-100.0) < 1e-10);
        assert!(sigmoid(100.0) <= 1.0 && sigmoid(100.0) > 1.0 - 1e-10);
        assert!(!sigmoid(1000.0).is_nan());
        assert!(!sigmoid(-1000.0).is_nan());
    }

    #[test]
    fn log_loss_correct_target() {
        let l = BinaryLogLoss::new();
        // p=0.9, target=true -> -log(0.9)
        assert!((l.loss(0.9, true) - (-0.9_f64.ln())).abs() < 1e-12);
        // p=0.1, target=false -> -log(0.9)
        assert!((l.loss(0.1, false) - (-0.9_f64.ln())).abs() < 1e-12);
    }

    #[test]
    fn log_loss_clips_extreme_probabilities() {
        let l = BinaryLogLoss::new();
        let loss = l.loss(0.0, true);
        assert!(loss.is_finite() && loss > 0.0);
        let loss = l.loss(1.0, false);
        assert!(loss.is_finite() && loss > 0.0);
    }

    #[test]
    fn gradient_wrt_logit_matches_probability_minus_target() {
        let l = BinaryLogLoss::new();
        assert!((l.gradient_wrt_logit(0.7, true) - (-0.3)).abs() < 1e-12);
        assert!((l.gradient_wrt_logit(0.7, false) - 0.7).abs() < 1e-12);
    }

    #[test]
    fn invalid_epsilon_rejected() {
        assert!(BinaryLogLoss::with_epsilon(0.0).is_err());
        assert!(BinaryLogLoss::with_epsilon(0.5).is_err());
        assert!(BinaryLogLoss::with_epsilon(-0.1).is_err());
    }
}

//! AdaGrad optimizer.
//!
//! Maintains a per-parameter sum of squared gradients and scales the learning
//! rate accordingly. Time complexity per step: `O(d)`. Space: `O(d)`.

use crate::error::{RillError, ensure_finite};

/// Configuration for [`AdaGrad`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdaGradConfig {
    /// Global learning rate. Must be finite and strictly positive.
    pub learning_rate: f64,
    /// L2 regularization strength.
    pub l2: f64,
    /// Small constant added to the denominator for numerical stability.
    pub epsilon: f64,
}

impl Default for AdaGradConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.1,
            l2: 0.0,
            epsilon: 1e-8,
        }
    }
}

/// AdaGrad optimizer.
///
/// Update rule:
/// ```text
/// g2_i += grad_i^2
/// w_i -= lr / sqrt(g2_i + epsilon) * (grad_i + l2 * w_i)
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdaGrad {
    feature_count: usize,
    config: AdaGradConfig,
    grad_sq_weights: Vec<f64>,
    grad_sq_intercept: f64,
    samples_seen: u64,
}

impl AdaGrad {
    /// Create a new AdaGrad optimizer.
    pub fn new(feature_count: usize, config: AdaGradConfig) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("learning_rate", config.learning_rate)?;
        ensure_finite("l2", config.l2)?;
        ensure_finite("epsilon", config.epsilon)?;
        if config.learning_rate <= 0.0 {
            return Err(RillError::InvalidLearningRate(config.learning_rate));
        }
        if config.l2 < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "l2",
                value: config.l2,
            });
        }
        if config.epsilon <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "epsilon",
                value: config.epsilon,
            });
        }
        Ok(Self {
            feature_count,
            config,
            grad_sq_weights: vec![0.0; feature_count],
            grad_sq_intercept: 0.0,
            samples_seen: 0,
        })
    }

    /// Number of parameters (features + intercept).
    pub const fn param_count(&self) -> usize {
        self.feature_count + 1
    }

    /// Number of samples processed.
    pub const fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    /// Apply one gradient step.
    pub fn step(
        &mut self,
        weights: &mut [f64],
        intercept: &mut f64,
        grad_weights: &[f64],
        grad_intercept: f64,
    ) -> Result<(), RillError> {
        if weights.len() != self.feature_count {
            return Err(RillError::DimensionMismatch {
                expected: self.feature_count,
                actual: weights.len(),
            });
        }
        if grad_weights.len() != self.feature_count {
            return Err(RillError::DimensionMismatch {
                expected: self.feature_count,
                actual: grad_weights.len(),
            });
        }
        let lr = self.config.learning_rate;
        let l2 = self.config.l2;
        let eps = self.config.epsilon;
        for (i, (w, &g)) in weights.iter_mut().zip(grad_weights).enumerate() {
            ensure_finite("grad_weight", g)?;
            self.grad_sq_weights[i] += g * g;
            let scale = (self.grad_sq_weights[i] + eps).sqrt();
            *w -= lr / scale * (g + l2 * *w);
        }
        ensure_finite("grad_intercept", grad_intercept)?;
        self.grad_sq_intercept += grad_intercept * grad_intercept;
        let scale = (self.grad_sq_intercept + eps).sqrt();
        *intercept -= lr / scale * grad_intercept;
        self.samples_seen += 1;
        Ok(())
    }

    /// Reset to initial state.
    pub fn reset(&mut self) {
        for g in &mut self.grad_sq_weights {
            *g = 0.0;
        }
        self.grad_sq_intercept = 0.0;
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adagrad_decreases_weights() {
        let mut opt = AdaGrad::new(2, AdaGradConfig::default()).unwrap();
        let mut w = vec![0.0, 0.0];
        let mut b = 0.0;
        opt.step(&mut w, &mut b, &[1.0, 2.0], 1.0).unwrap();
        // first step: scale = sqrt(g^2 + eps) ≈ |g|
        // w0 -= 0.1 / 1.0 * 1.0 = -0.1
        assert!(w[0] < 0.0);
        assert!(w[1] < 0.0);
    }

    #[test]
    fn adagrad_learning_rate_decreases() {
        let mut opt = AdaGrad::new(
            1,
            AdaGradConfig {
                learning_rate: 1.0,
                l2: 0.0,
                epsilon: 1e-12,
            },
        )
        .unwrap();
        let mut w = vec![0.0];
        let mut b = 0.0;
        opt.step(&mut w, &mut b, &[1.0], 0.0).unwrap();
        let step1 = w[0].abs();
        opt.step(&mut w, &mut b, &[1.0], 0.0).unwrap();
        let step2 = w[0].abs() - step1;
        // second step should be smaller than first
        assert!(step2 < step1);
    }

    #[test]
    fn invalid_config_rejected() {
        assert!(
            AdaGrad::new(
                1,
                AdaGradConfig {
                    learning_rate: 0.0,
                    l2: 0.0,
                    epsilon: 1e-8
                }
            )
            .is_err()
        );
        assert!(
            AdaGrad::new(
                1,
                AdaGradConfig {
                    learning_rate: 0.1,
                    l2: -1.0,
                    epsilon: 1e-8
                }
            )
            .is_err()
        );
        assert!(
            AdaGrad::new(
                1,
                AdaGradConfig {
                    learning_rate: 0.1,
                    l2: 0.0,
                    epsilon: 0.0
                }
            )
            .is_err()
        );
    }
}

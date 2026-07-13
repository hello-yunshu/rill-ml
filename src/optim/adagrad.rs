//! AdaGrad optimizer.
//!
//! Maintains a per-parameter sum of squared gradients and scales the learning
//! rate accordingly. Time complexity per step: `O(d)`. Space: `O(d)`.

use crate::error::{RillError, checked_finite_add, checked_increment, ensure_finite};

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
        for &gradient in grad_weights {
            ensure_finite("grad_weight", gradient)?;
        }
        ensure_finite("grad_intercept", grad_intercept)?;
        let next_samples = checked_increment(self.samples_seen, "AdaGrad sample")?;
        let lr = self.config.learning_rate;
        let l2 = self.config.l2;
        let eps = self.config.epsilon;

        let mut next_grad_sq_weights = Vec::with_capacity(self.feature_count);
        let mut next_weights = Vec::with_capacity(self.feature_count);
        for (i, (&weight, &gradient)) in weights.iter().zip(grad_weights).enumerate() {
            let squared_gradient = gradient * gradient;
            ensure_finite("squared gradient", squared_gradient)?;
            let accumulator = checked_finite_add(
                self.grad_sq_weights[i],
                squared_gradient,
                "AdaGrad accumulator",
            )?;
            let scale = (accumulator + eps).sqrt();
            ensure_finite("AdaGrad scale", scale)?;
            let regularized_gradient = gradient + l2 * weight;
            ensure_finite("regularized gradient", regularized_gradient)?;
            let next_weight = weight - lr / scale * regularized_gradient;
            ensure_finite("weight", next_weight)?;
            next_grad_sq_weights.push(accumulator);
            next_weights.push(next_weight);
        }

        let squared_intercept_gradient = grad_intercept * grad_intercept;
        ensure_finite("squared intercept gradient", squared_intercept_gradient)?;
        let next_grad_sq_intercept = checked_finite_add(
            self.grad_sq_intercept,
            squared_intercept_gradient,
            "AdaGrad intercept accumulator",
        )?;
        let intercept_scale = (next_grad_sq_intercept + eps).sqrt();
        ensure_finite("AdaGrad intercept scale", intercept_scale)?;
        let next_intercept = *intercept - lr / intercept_scale * grad_intercept;
        ensure_finite("intercept", next_intercept)?;

        self.grad_sq_weights = next_grad_sq_weights;
        self.grad_sq_intercept = next_grad_sq_intercept;
        weights.copy_from_slice(&next_weights);
        *intercept = next_intercept;
        self.samples_seen = next_samples;
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

    #[test]
    fn failed_step_is_atomic() {
        let mut opt = AdaGrad::new(2, AdaGradConfig::default()).unwrap();
        let mut weights = vec![1.0, 2.0];
        let mut intercept = 3.0;
        let result = opt.step(&mut weights, &mut intercept, &[1.0, f64::MAX], 1.0);
        assert!(result.is_err());
        assert_eq!(weights, vec![1.0, 2.0]);
        assert_eq!(intercept, 3.0);
        assert_eq!(opt.samples_seen(), 0);
        assert_eq!(opt.grad_sq_weights, vec![0.0, 0.0]);
    }
}

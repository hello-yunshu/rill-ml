//! Stochastic gradient descent optimizer.

use crate::error::{RillError, ensure_finite};

/// Configuration for [`Sgd`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SgdConfig {
    /// Learning rate. Must be finite and strictly positive.
    pub learning_rate: f64,
    /// L2 regularization strength. Must be finite and non-negative.
    pub l2: f64,
}

impl Default for SgdConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            l2: 0.0,
        }
    }
}

/// SGD optimizer with optional L2 regularization.
///
/// The update rule for each weight `w_i` is:
/// ```text
/// w_i -= lr * (grad_i + l2 * w_i)
/// ```
/// The intercept is not regularized.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sgd {
    feature_count: usize,
    config: SgdConfig,
    samples_seen: u64,
}

impl Sgd {
    /// Create a new SGD optimizer.
    pub fn new(feature_count: usize, config: SgdConfig) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("learning_rate", config.learning_rate)?;
        ensure_finite("l2", config.l2)?;
        if config.learning_rate <= 0.0 {
            return Err(RillError::InvalidLearningRate(config.learning_rate));
        }
        if config.l2 < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "l2",
                value: config.l2,
            });
        }
        Ok(Self {
            feature_count,
            config,
            samples_seen: 0,
        })
    }

    /// The configured learning rate.
    pub const fn learning_rate(&self) -> f64 {
        self.config.learning_rate
    }

    /// The configured L2 regularization.
    pub const fn l2(&self) -> f64 {
        self.config.l2
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
        for (w, &g) in weights.iter_mut().zip(grad_weights) {
            ensure_finite("grad_weight", g)?;
            *w -= lr * (g + l2 * *w);
        }
        ensure_finite("grad_intercept", grad_intercept)?;
        *intercept -= lr * grad_intercept;
        self.samples_seen += 1;
        Ok(())
    }

    /// Reset the sample counter.
    pub fn reset(&mut self) {
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgd_updates_weights() {
        let mut opt = Sgd::new(
            2,
            SgdConfig {
                learning_rate: 0.1,
                l2: 0.0,
            },
        )
        .unwrap();
        let mut w = vec![0.0, 0.0];
        let mut b = 0.0;
        opt.step(&mut w, &mut b, &[1.0, 2.0], 0.5).unwrap();
        // w -= 0.1 * grad -> [-0.1, -0.2]
        assert!((w[0] + 0.1).abs() < 1e-12);
        assert!((w[1] + 0.2).abs() < 1e-12);
        assert!((b + 0.05).abs() < 1e-12);
    }

    #[test]
    fn sgd_l2_regularization() {
        let mut opt = Sgd::new(
            1,
            SgdConfig {
                learning_rate: 0.1,
                l2: 1.0,
            },
        )
        .unwrap();
        let mut w = vec![10.0];
        let mut b = 0.0;
        opt.step(&mut w, &mut b, &[0.0], 0.0).unwrap();
        // w -= 0.1 * (0 + 1*10) = -1.0 -> w = 9.0
        assert!((w[0] - 9.0).abs() < 1e-12);
        // intercept not regularized -> b unchanged
        assert!((b - 0.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_learning_rate_rejected() {
        assert!(
            Sgd::new(
                1,
                SgdConfig {
                    learning_rate: 0.0,
                    l2: 0.0
                }
            )
            .is_err()
        );
        assert!(
            Sgd::new(
                1,
                SgdConfig {
                    learning_rate: -1.0,
                    l2: 0.0
                }
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_l2_rejected() {
        assert!(
            Sgd::new(
                1,
                SgdConfig {
                    learning_rate: 0.1,
                    l2: -1.0
                }
            )
            .is_err()
        );
    }
}

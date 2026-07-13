//! Online linear regression using SGD or AdaGrad.
//!
//! The model learns `y ≈ w·x + b` incrementally, one sample at a time.
//! Prediction is side-effect free; learning computes the gradient of the
//! configured loss and applies one optimizer step.

use crate::error::{RillError, ensure_finite_target, validate_features};
use crate::loss::RegressionLoss;
use crate::optim::Optimizer;
use crate::traits::OnlineRegressor;

/// Configuration for [`LinearRegression`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LinearRegressionConfig {
    /// The optimizer to use (SGD or AdaGrad).
    pub optimizer: Optimizer,
    /// The loss function (SquaredError or Huber).
    pub loss: RegressionLoss,
}

impl Default for LinearRegressionConfig {
    fn default() -> Self {
        Self {
            optimizer: Optimizer::sgd(1, Default::default()).expect("default optimizer"),
            loss: RegressionLoss::default(),
        }
    }
}

/// Online linear regression model.
///
/// # Examples
///
/// ```
/// use rill_ml::{
///     models::{LinearRegression, LinearRegressionConfig},
///     optim::{Optimizer, SgdConfig},
///     loss::RegressionLoss,
///     OnlineRegressor,
/// };
///
/// let feature_count = 2;
/// let mut model = LinearRegression::new(
///     feature_count,
///     LinearRegressionConfig {
///         optimizer: Optimizer::sgd(feature_count, SgdConfig {
///             learning_rate: 0.1,
///             l2: 0.0,
///         }).unwrap(),
///         loss: RegressionLoss::default(),
///     },
/// ).unwrap();
///
/// let prediction = model.predict(&[1.0, 2.0]).unwrap();
/// model.learn(&[1.0, 2.0], 3.0).unwrap();
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LinearRegression {
    feature_count: usize,
    weights: Vec<f64>,
    intercept: f64,
    optimizer: Optimizer,
    loss: RegressionLoss,
    samples_seen: u64,
}

impl LinearRegression {
    /// Create a new linear regression model.
    ///
    /// The optimizer's feature count must match `feature_count`.
    pub fn new(feature_count: usize, config: LinearRegressionConfig) -> Result<Self, RillError> {
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        if config.optimizer.param_count() != feature_count + 1 {
            return Err(RillError::DimensionMismatch {
                expected: feature_count + 1,
                actual: config.optimizer.param_count(),
            });
        }
        Ok(Self {
            feature_count,
            weights: vec![0.0; feature_count],
            intercept: 0.0,
            optimizer: config.optimizer,
            loss: config.loss,
            samples_seen: 0,
        })
    }

    /// The learned weights.
    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    /// The learned intercept (bias).
    pub const fn intercept(&self) -> f64 {
        self.intercept
    }

    /// Compute the prediction `w·x + b` without updating state.
    fn predict_inner(&self, features: &[f64]) -> Result<f64, RillError> {
        validate_features(self.feature_count, features)?;
        let dot = self
            .weights
            .iter()
            .zip(features.iter())
            .map(|(w, x)| w * x)
            .sum::<f64>();
        Ok(dot + self.intercept)
    }
}

impl OnlineRegressor for LinearRegression {
    fn feature_count(&self) -> usize {
        self.feature_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn predict(&self, features: &[f64]) -> Result<f64, RillError> {
        self.predict_inner(features)
    }

    fn learn(&mut self, features: &[f64], target: f64) -> Result<(), RillError> {
        validate_features(self.feature_count, features)?;
        ensure_finite_target(target)?;

        let prediction = self.predict_inner(features)?;
        let grad = self.loss.gradient(prediction, target);

        // gradient w.r.t. each weight w_i is grad * x_i
        let grad_weights: Vec<f64> = features.iter().map(|&x| grad * x).collect();
        let grad_intercept = grad;

        self.optimizer.step(
            &mut self.weights,
            &mut self.intercept,
            &grad_weights,
            grad_intercept,
        )?;
        self.samples_seen += 1;
        Ok(())
    }

    fn reset(&mut self) {
        for w in &mut self.weights {
            *w = 0.0;
        }
        self.intercept = 0.0;
        self.optimizer.reset();
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optim::{AdaGradConfig, SgdConfig};
    use rand::SeedableRng;

    fn make_sgd(lr: f64, l2: f64, d: usize) -> Optimizer {
        Optimizer::sgd(
            d,
            SgdConfig {
                learning_rate: lr,
                l2,
            },
        )
        .unwrap()
    }

    #[test]
    fn predict_cold_start_returns_intercept() {
        let model = LinearRegression::new(
            2,
            LinearRegressionConfig {
                optimizer: make_sgd(0.1, 0.0, 2),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        assert_eq!(model.predict(&[1.0, 2.0]).unwrap(), 0.0);
    }

    #[test]
    fn learn_reduces_loss_on_linear_data() {
        let mut model = LinearRegression::new(
            2,
            LinearRegressionConfig {
                optimizer: make_sgd(0.05, 0.0, 2),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        // y = 2*x1 - 0.5*x2 + 1
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
        let mut first_loss = 0.0;
        let mut last_loss = 0.0;
        for i in 0..500 {
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x2 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = 2.0 * x1 - 0.5 * x2 + 1.0;
            let pred = model.predict(&[x1, x2]).unwrap();
            let l = crate::loss::SquaredError::loss(pred, y);
            if i < 10 {
                first_loss += l;
            }
            if i >= 490 {
                last_loss += l;
            }
            model.learn(&[x1, x2], y).unwrap();
        }
        assert!(last_loss < first_loss, "loss should decrease");
        // weights should be approximately [2, -0.5]
        assert!((model.weights()[0] - 2.0).abs() < 0.3);
        assert!((model.weights()[1] + 0.5).abs() < 0.3);
        assert!((model.intercept() - 1.0).abs() < 0.3);
    }

    #[test]
    fn predict_does_not_update_state() {
        let model = LinearRegression::new(
            1,
            LinearRegressionConfig {
                optimizer: make_sgd(0.1, 0.0, 1),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        let _ = model.predict(&[1.0]).unwrap();
        assert_eq!(model.samples_seen(), 0);
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let mut model = LinearRegression::new(
            3,
            LinearRegressionConfig {
                optimizer: make_sgd(0.1, 0.0, 3),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        assert!(model.predict(&[1.0, 2.0]).is_err());
        assert!(model.learn(&[1.0, 2.0], 1.0).is_err());
    }

    #[test]
    fn optimizer_feature_count_mismatch_rejected() {
        let config = LinearRegressionConfig {
            optimizer: make_sgd(0.1, 0.0, 3),
            loss: RegressionLoss::default(),
        };
        assert!(LinearRegression::new(2, config).is_err());
    }

    #[test]
    fn adagrad_works() {
        let mut model = LinearRegression::new(
            1,
            LinearRegressionConfig {
                optimizer: Optimizer::adagrad(
                    1,
                    AdaGradConfig {
                        learning_rate: 0.5,
                        l2: 0.0,
                        epsilon: 1e-8,
                    },
                )
                .unwrap(),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        for _ in 0..200 {
            model.learn(&[1.0], 5.0).unwrap();
        }
        assert!((model.predict(&[1.0]).unwrap() - 5.0).abs() < 0.5);
    }

    #[test]
    fn reset_clears_state() {
        let mut model = LinearRegression::new(
            1,
            LinearRegressionConfig {
                optimizer: make_sgd(0.1, 0.0, 1),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        model.learn(&[1.0], 5.0).unwrap();
        model.reset();
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.predict(&[1.0]).unwrap(), 0.0);
    }

    #[test]
    fn non_finite_target_rejected() {
        let mut model = LinearRegression::new(
            1,
            LinearRegressionConfig {
                optimizer: make_sgd(0.1, 0.0, 1),
                loss: RegressionLoss::default(),
            },
        )
        .unwrap();
        assert!(model.learn(&[1.0], f64::NAN).is_err());
    }

    #[test]
    fn huber_loss_works() {
        let mut model = LinearRegression::new(
            1,
            LinearRegressionConfig {
                optimizer: make_sgd(0.1, 0.0, 1),
                loss: RegressionLoss::Huber(crate::loss::HuberLoss::new(1.0).unwrap()),
            },
        )
        .unwrap();
        model.learn(&[1.0], 1.0).unwrap();
        assert_eq!(model.samples_seen(), 1);
    }
}

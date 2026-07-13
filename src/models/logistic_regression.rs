//! Online logistic regression for binary classification.
//!
//! Uses a numerically stable sigmoid and binary cross-entropy (log) loss.

use crate::error::{
    RillError, checked_finite_add, checked_increment, ensure_finite, validate_features,
};
use crate::loss::log_loss::{BinaryLogLoss, sigmoid};
use crate::optim::Optimizer;
use crate::traits::OnlineBinaryClassifier;

/// Configuration for [`LogisticRegression`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LogisticRegressionConfig {
    /// The optimizer (SGD or AdaGrad).
    pub optimizer: Optimizer,
    /// The log loss configuration.
    pub loss: BinaryLogLoss,
}

impl Default for LogisticRegressionConfig {
    fn default() -> Self {
        Self {
            optimizer: Optimizer::sgd(1, Default::default()).expect("default optimizer"),
            loss: BinaryLogLoss::new(),
        }
    }
}

/// Online logistic regression model.
///
/// Predicts `P(y=1 | x) = sigmoid(w·x + b)`. Learning uses the gradient of
/// the binary log loss w.r.t. the logit, which simplifies to `p - y`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LogisticRegression {
    feature_count: usize,
    weights: Vec<f64>,
    intercept: f64,
    optimizer: Optimizer,
    loss: BinaryLogLoss,
    samples_seen: u64,
}

impl LogisticRegression {
    /// Create a new logistic regression model.
    pub fn new(feature_count: usize, config: LogisticRegressionConfig) -> Result<Self, RillError> {
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

    /// Compute the logit `w·x + b`.
    fn logit(&self, features: &[f64]) -> Result<f64, RillError> {
        validate_features(self.feature_count, features)?;
        let dot = self.weights.iter().zip(features.iter()).try_fold(
            0.0,
            |sum, (&weight, &feature)| {
                let term = weight * feature;
                ensure_finite("logit term", term)?;
                checked_finite_add(sum, term, "logit")
            },
        )?;
        checked_finite_add(dot, self.intercept, "logit")
    }
}

impl OnlineBinaryClassifier for LogisticRegression {
    fn feature_count(&self) -> usize {
        self.feature_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn predict_proba(&self, features: &[f64]) -> Result<f64, RillError> {
        let z = self.logit(features)?;
        Ok(sigmoid(z))
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        validate_features(self.feature_count, features)?;
        let next_samples = checked_increment(self.samples_seen, "logistic regression sample")?;
        let z = self.logit(features)?;
        let p = sigmoid(z);
        // gradient of log loss w.r.t. logit is (p - y)
        let grad = self.loss.gradient_wrt_logit(p, target);
        ensure_finite("loss gradient", grad)?;
        let grad_weights = features
            .iter()
            .map(|&feature| {
                let gradient = grad * feature;
                ensure_finite("weight gradient", gradient)?;
                Ok(gradient)
            })
            .collect::<Result<Vec<_>, RillError>>()?;
        let grad_intercept = grad;
        self.optimizer.step(
            &mut self.weights,
            &mut self.intercept,
            &grad_weights,
            grad_intercept,
        )?;
        self.samples_seen = next_samples;
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
    use crate::optim::SgdConfig;
    use rand::SeedableRng;

    fn make_model(d: usize, lr: f64) -> LogisticRegression {
        LogisticRegression::new(
            d,
            LogisticRegressionConfig {
                optimizer: Optimizer::sgd(
                    d,
                    SgdConfig {
                        learning_rate: lr,
                        l2: 0.0,
                    },
                )
                .unwrap(),
                loss: BinaryLogLoss::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn predict_proba_in_range() {
        let model = make_model(2, 0.1);
        let p = model.predict_proba(&[1.0, 2.0]).unwrap();
        assert!(p > 0.0 && p < 1.0);
        // cold start: weights=0, intercept=0 -> sigmoid(0) = 0.5
        assert!((p - 0.5).abs() < 1e-12);
    }

    #[test]
    fn learn_separable_data() {
        let mut model = make_model(2, 0.5);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(3);
        for _ in 0..1000 {
            // class 1: x1 > 0, class 0: x1 < 0
            let x1 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
            let x2 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = x1 > 0.0;
            model.learn(&[x1, x2], y).unwrap();
        }
        let p_pos = model.predict_proba(&[2.0, 0.0]).unwrap();
        let p_neg = model.predict_proba(&[-2.0, 0.0]).unwrap();
        assert!(p_pos > 0.7, "p_pos = {p_pos}");
        assert!(p_neg < 0.3, "p_neg = {p_neg}");
    }

    #[test]
    fn predict_does_not_update_state() {
        let model = make_model(1, 0.1);
        let _ = model.predict_proba(&[1.0]).unwrap();
        assert_eq!(model.samples_seen(), 0);
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let mut model = make_model(3, 0.1);
        assert!(model.predict_proba(&[1.0, 2.0]).is_err());
        assert!(model.learn(&[1.0, 2.0], true).is_err());
    }

    #[test]
    fn reset_clears_state() {
        let mut model = make_model(1, 0.1);
        model.learn(&[1.0], true).unwrap();
        model.reset();
        assert_eq!(model.samples_seen(), 0);
        assert!((model.predict_proba(&[1.0]).unwrap() - 0.5).abs() < 1e-12);
    }
}

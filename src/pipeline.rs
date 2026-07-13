//! Static two-segment pipelines: transformer + model.
//!
//! The learning contract is fixed:
//! - `predict(x)`: `transform(x)` → `model.predict()`. No state updates.
//! - `learn(x, y)`: `transformer.update(x)` → `transform(x)` → `model.learn()`.
//! - `learn_transactional` is the failure-atomic variant: neither stage is
//!   committed unless all three operations succeed.
//!
//! The transformer never sees the target `y`, so there is no label leakage in
//! the progressive-evaluation sense (the prediction for the current sample is
//! produced *before* any state update).

use crate::error::{RillError, ensure_finite_target};
use crate::traits::{OnlineBinaryClassifier, OnlineRegressor, Transformer};

/// A pipeline combining a transformer and a regressor.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RegressionPipeline<T, M> {
    transformer: T,
    model: M,
}

impl<T, M> RegressionPipeline<T, M>
where
    T: Transformer,
    M: OnlineRegressor,
{
    /// Create a new pipeline.
    ///
    /// Returns an error if the transformer's output dimension does not match
    /// the model's feature count.
    pub fn new(transformer: T, model: M) -> Result<Self, RillError> {
        if transformer.output_dim() != model.feature_count() {
            return Err(RillError::DimensionMismatch {
                expected: model.feature_count(),
                actual: transformer.output_dim(),
            });
        }
        Ok(Self { transformer, model })
    }

    /// Borrow the transformer.
    pub fn transformer(&self) -> &T {
        &self.transformer
    }

    /// Borrow the model.
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Learn one sample with all-or-nothing state changes.
    ///
    /// This clones both stages, applies the update to the clones, and commits
    /// them only after every operation succeeds. Prefer this at reliability
    /// boundaries; use [`OnlineRegressor::learn`] when avoiding the clone cost
    /// is more important and both stages already provide atomic updates.
    pub fn learn_transactional(&mut self, features: &[f64], target: f64) -> Result<(), RillError>
    where
        T: Clone,
        M: Clone,
    {
        let mut next_transformer = self.transformer.clone();
        let mut next_model = self.model.clone();
        next_transformer.update(features)?;
        let transformed = next_transformer.transform(features)?;
        next_model.learn(&transformed, target)?;
        self.transformer = next_transformer;
        self.model = next_model;
        Ok(())
    }
}

impl<T, M> OnlineRegressor for RegressionPipeline<T, M>
where
    T: Transformer,
    M: OnlineRegressor,
{
    fn feature_count(&self) -> usize {
        self.transformer.input_dim()
    }

    fn samples_seen(&self) -> u64 {
        self.transformer.samples_seen()
    }

    fn predict(&self, features: &[f64]) -> Result<f64, RillError> {
        let transformed = self.transformer.transform(features)?;
        self.model.predict(&transformed)
    }

    fn learn(&mut self, features: &[f64], target: f64) -> Result<(), RillError> {
        ensure_finite_target(target)?;
        self.transformer.update(features)?;
        let transformed = self.transformer.transform(features)?;
        self.model.learn(&transformed, target)
    }

    fn reset(&mut self) {
        self.transformer.reset();
        self.model.reset();
    }
}

/// A pipeline combining a transformer and a binary classifier.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClassificationPipeline<T, M> {
    transformer: T,
    model: M,
}

impl<T, M> ClassificationPipeline<T, M>
where
    T: Transformer,
    M: OnlineBinaryClassifier,
{
    /// Create a new classification pipeline.
    pub fn new(transformer: T, model: M) -> Result<Self, RillError> {
        if transformer.output_dim() != model.feature_count() {
            return Err(RillError::DimensionMismatch {
                expected: model.feature_count(),
                actual: transformer.output_dim(),
            });
        }
        Ok(Self { transformer, model })
    }

    /// Borrow the transformer.
    pub fn transformer(&self) -> &T {
        &self.transformer
    }

    /// Borrow the model.
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Learn one classification sample with all-or-nothing state changes.
    pub fn learn_transactional(&mut self, features: &[f64], target: bool) -> Result<(), RillError>
    where
        T: Clone,
        M: Clone,
    {
        let mut next_transformer = self.transformer.clone();
        let mut next_model = self.model.clone();
        next_transformer.update(features)?;
        let transformed = next_transformer.transform(features)?;
        next_model.learn(&transformed, target)?;
        self.transformer = next_transformer;
        self.model = next_model;
        Ok(())
    }
}

impl<T, M> OnlineBinaryClassifier for ClassificationPipeline<T, M>
where
    T: Transformer,
    M: OnlineBinaryClassifier,
{
    fn feature_count(&self) -> usize {
        self.transformer.input_dim()
    }

    fn samples_seen(&self) -> u64 {
        self.transformer.samples_seen()
    }

    fn predict_proba(&self, features: &[f64]) -> Result<f64, RillError> {
        let transformed = self.transformer.transform(features)?;
        self.model.predict_proba(&transformed)
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        self.transformer.update(features)?;
        let transformed = self.transformer.transform(features)?;
        self.model.learn(&transformed, target)
    }

    fn reset(&mut self) {
        self.transformer.reset();
        self.model.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Mae;
    use crate::models::{LinearRegression, LinearRegressionConfig};
    use crate::optim::{Optimizer, SgdConfig};
    use crate::preprocessing::StandardScaler;
    use crate::traits::Metric;
    use rand::SeedableRng;

    #[test]
    fn pipeline_predict_does_not_update_transformer() {
        let d = 2;
        let scaler = StandardScaler::new(d).unwrap();
        let model = LinearRegression::new(
            d,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(d, SgdConfig::default()).unwrap(),
                loss: Default::default(),
            },
        )
        .unwrap();
        let mut pipe = RegressionPipeline::new(scaler, model).unwrap();

        let _ = pipe.predict(&[1.0, 2.0]).unwrap();
        assert_eq!(pipe.transformer().samples_seen(), 0);

        pipe.learn(&[1.0, 2.0], 3.0).unwrap();
        assert_eq!(pipe.transformer().samples_seen(), 1);
    }

    #[test]
    fn failed_pipeline_learn_does_not_mutate_either_stage() {
        let scaler = StandardScaler::new(1).unwrap();
        let model = LinearRegression::new(
            1,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(1, SgdConfig::default()).unwrap(),
                loss: Default::default(),
            },
        )
        .unwrap();
        let mut pipe = RegressionPipeline::new(scaler, model).unwrap();

        assert!(pipe.learn_transactional(&[1.0], f64::NAN).is_err());
        assert_eq!(pipe.transformer().samples_seen(), 0);
        assert_eq!(pipe.model().samples_seen(), 0);
    }

    #[test]
    fn pipeline_dimension_mismatch_rejected() {
        let scaler = StandardScaler::new(3).unwrap();
        let model = LinearRegression::new(
            2,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(2, SgdConfig::default()).unwrap(),
                loss: Default::default(),
            },
        )
        .unwrap();
        assert!(RegressionPipeline::new(scaler, model).is_err());
    }

    #[test]
    fn pipeline_learns_linear_relation() {
        let d = 2;
        let scaler = StandardScaler::new(d).unwrap();
        let model = LinearRegression::new(
            d,
            LinearRegressionConfig {
                optimizer: Optimizer::sgd(
                    d,
                    SgdConfig {
                        learning_rate: 0.05,
                        l2: 0.0,
                    },
                )
                .unwrap(),
                loss: Default::default(),
            },
        )
        .unwrap();
        let mut pipe = RegressionPipeline::new(scaler, model).unwrap();
        let mut mae = Mae::default();

        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(11);
        for _ in 0..500 {
            let x1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
            let x2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
            let y = 3.0 * x1 + 2.0 * x2;
            let pred = pipe.predict(&[x1, x2]).unwrap();
            mae.update(y, pred).unwrap();
            pipe.learn(&[x1, x2], y).unwrap();
        }
        let final_mae = mae.value().unwrap();
        assert!(final_mae < 1.0, "final MAE too high: {final_mae}");
    }
}

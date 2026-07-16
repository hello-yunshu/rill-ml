//! FTRL-Proximal online learning for sparse features.
//!
//! Implements the Follow-The-Regularized-Leader Proximal algorithm,
//! which is well-suited for high-dimensional sparse data. L1
//! regularization produces sparse weight vectors, and the per-coordinate
//! learning rate adapts to feature frequency.
//!
//! See: McMahan et al., "Ad Click Prediction: a View from the Trenches"
//! (KDD 2013).
//!
//! # Per-coordinate learning rate
//!
//! `eta_i = alpha / (beta + sqrt(n_i))`
//!
//! # Weight computation
//!
//! For feature `i`:
//!
//! ```text
//! if |z_i| <= lambda1:
//!     w_i = 0
//! else:
//!     w_i = -(z_i - sign(z_i) * lambda1) / (lambda2 + (beta + sqrt(n_i)) / alpha)
//! ```
//!
//! The intercept uses `lambda1 = 0` (no L1 regularization).

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::loss::log_loss::sigmoid;
use crate::sparse::{FeatureId, SparseFeatures};
use crate::traits::{SparseClassifier, SparseRegressor};
use std::collections::BTreeMap;

/// Configuration for FTRL models.
///
/// Controls the per-coordinate learning rate and regularization strengths.
/// All fields must be finite; `alpha` must be strictly positive and the
/// regularization parameters must be non-negative.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FtrlConfig {
    /// Alpha: learning rate scaling. Must be `> 0`.
    pub alpha: f64,
    /// Beta: smoothing constant. Must be `>= 0`.
    pub beta: f64,
    /// L1 regularization strength. Must be `>= 0`.
    pub l1: f64,
    /// L2 regularization strength. Must be `>= 0`.
    pub l2: f64,
}

impl Default for FtrlConfig {
    fn default() -> Self {
        Self {
            alpha: 0.1,
            beta: 1.0,
            l1: 1.0,
            l2: 1.0,
        }
    }
}

/// Validate FTRL configuration parameters.
fn validate_config(config: &FtrlConfig) -> Result<(), RillError> {
    ensure_finite("alpha", config.alpha)?;
    ensure_finite("beta", config.beta)?;
    ensure_finite("l1", config.l1)?;
    ensure_finite("l2", config.l2)?;
    if config.alpha <= 0.0 {
        return Err(RillError::InvalidParameter {
            name: "alpha",
            value: config.alpha,
        });
    }
    if config.beta < 0.0 {
        return Err(RillError::InvalidParameter {
            name: "beta",
            value: config.beta,
        });
    }
    if config.l1 < 0.0 {
        return Err(RillError::InvalidParameter {
            name: "l1",
            value: config.l1,
        });
    }
    if config.l2 < 0.0 {
        return Err(RillError::InvalidParameter {
            name: "l2",
            value: config.l2,
        });
    }
    Ok(())
}

/// Per-feature FTRL state.
///
/// Tracks the sum of (sigma-corrected) gradients `z` and the sum of squared
/// gradients `n`. The per-coordinate adaptive learning rate is derived from
/// `n`: features seen more frequently get smaller steps.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FtrlParam {
    /// Sum of gradients (with sigma correction).
    z: f64,
    /// Sum of squared gradients.
    n: f64,
}

impl FtrlParam {
    /// Compute the FTRL weight given the config.
    ///
    /// Returns `0` when `|z| <= l1` (L1 soft-thresholding).
    fn weight(&self, config: &FtrlConfig) -> f64 {
        if self.z.abs() <= config.l1 {
            0.0
        } else {
            let sign = self.z.signum();
            let numerator = -(self.z - sign * config.l1);
            let denominator = config.l2 + (config.beta + self.n.sqrt()) / config.alpha;
            numerator / denominator
        }
    }

    /// Compute the intercept weight (no L1 regularization).
    ///
    /// Returns `0` when no gradient has been observed yet (`n == 0`),
    /// avoiding a potential `0 / 0` when `l2` and `beta` are both zero.
    fn intercept_weight(&self, config: &FtrlConfig) -> f64 {
        if self.n == 0.0 {
            0.0
        } else {
            let numerator = -self.z;
            let denominator = config.l2 + (config.beta + self.n.sqrt()) / config.alpha;
            numerator / denominator
        }
    }

    /// Update with a gradient and the pre-computed current weight.
    ///
    /// `sigma = (sqrt(n_new) - sqrt(n_old)) / alpha`
    /// `z += g - sigma * w`
    /// `n += g^2`
    fn update(&mut self, gradient: f64, weight: f64, config: &FtrlConfig) {
        let n_old = self.n;
        let n_new = n_old + gradient * gradient;
        let sigma = (n_new.sqrt() - n_old.sqrt()) / config.alpha;
        self.z += gradient - sigma * weight;
        self.n = n_new;
    }
}

/// Compute the dot product `w · x` over sparse features.
///
/// Iterates only over the features present in `features` (not all stored
/// params), looking up each feature's current FTRL weight. Feature values
/// are validated for finiteness.
fn compute_dot(
    params: &BTreeMap<FeatureId, FtrlParam>,
    config: &FtrlConfig,
    features: &SparseFeatures,
) -> Result<f64, RillError> {
    if features.is_empty() {
        return Err(RillError::EmptyFeatures);
    }
    let mut dot = 0.0;
    for &(id, value) in features.values() {
        ensure_finite("sparse_value", value)?;
        if let Some(param) = params.get(&id) {
            dot += param.weight(config) * value;
        }
    }
    Ok(dot)
}

/// FTRL regressor with squared loss.
///
/// Learns `y ≈ w · x + b` incrementally. The gradient of the squared loss
/// w.r.t. the prediction is `prediction - target`, so each feature's
/// gradient is `(prediction - target) * x_i`.
///
/// # Examples
///
/// ```
/// use rill_ml::models::{FtrlConfig, FtrlRegressor};
/// use rill_ml::sparse::SparseFeatures;
/// use rill_ml::SparseRegressor;
///
/// let mut model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
/// let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
/// let _pred = model.predict(&sf).unwrap();
/// model.learn(&sf, 3.0).unwrap();
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FtrlRegressor {
    config: FtrlConfig,
    params: BTreeMap<FeatureId, FtrlParam>,
    intercept: FtrlParam,
    samples_seen: u64,
}

impl FtrlRegressor {
    /// Create a new FTRL regressor.
    ///
    /// Returns an error if the configuration is invalid.
    pub fn new(config: FtrlConfig) -> Result<Self, RillError> {
        validate_config(&config)?;
        Ok(Self {
            config,
            params: BTreeMap::new(),
            intercept: FtrlParam::default(),
            samples_seen: 0,
        })
    }

    /// The model configuration.
    pub const fn config(&self) -> &FtrlConfig {
        &self.config
    }

    /// Return the current non-zero feature weights, sorted by `FeatureId`.
    ///
    /// Features whose FTRL weight is exactly zero (due to L1
    /// soft-thresholding or never having been updated) are excluded.
    pub fn weights(&self) -> Vec<(FeatureId, f64)> {
        self.params
            .iter()
            .map(|(&id, param)| (id, param.weight(&self.config)))
            .filter(|&(_, w)| w != 0.0)
            .collect()
    }

    /// Compute the current intercept (bias) weight.
    pub fn intercept(&self) -> f64 {
        self.intercept.intercept_weight(&self.config)
    }

    /// Number of distinct features the model has seen.
    pub fn feature_count(&self) -> usize {
        self.params.len()
    }

    /// Compute the raw prediction `w · x + b` without updating state.
    fn predict_inner(&self, features: &SparseFeatures) -> Result<f64, RillError> {
        let dot = compute_dot(&self.params, &self.config, features)?;
        Ok(dot + self.intercept.intercept_weight(&self.config))
    }
}

impl SparseRegressor for FtrlRegressor {
    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn predict(&self, features: &SparseFeatures) -> Result<f64, RillError> {
        self.predict_inner(features)
    }

    fn learn(&mut self, features: &SparseFeatures, target: f64) -> Result<(), RillError> {
        if features.is_empty() {
            return Err(RillError::EmptyFeatures);
        }
        ensure_finite("target", target)?;

        let prediction = self.predict_inner(features)?;
        let grad = prediction - target;

        // Update each feature's params. entry().or_default() creates new
        // feature state on demand, supporting dynamic feature growth.
        for &(id, value) in features.values() {
            ensure_finite("sparse_value", value)?;
            let g = grad * value;
            let param = self.params.entry(id).or_default();
            let w = param.weight(&self.config);
            param.update(g, w, &self.config);
        }

        // Update intercept with no L1 regularization.
        let w_b = self.intercept.intercept_weight(&self.config);
        self.intercept.update(grad, w_b, &self.config);

        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
        Ok(())
    }

    fn reset(&mut self) {
        self.params.clear();
        self.intercept = FtrlParam::default();
        self.samples_seen = 0;
    }
}

/// FTRL binary classifier with log loss.
///
/// Predicts `P(y=1 | x) = sigmoid(w · x + b)`. The gradient of the log loss
/// w.r.t. the logit simplifies to `probability - target`, so each feature's
/// gradient is `(probability - target) * x_i`.
///
/// # Examples
///
/// ```
/// use rill_ml::models::{FtrlClassifier, FtrlConfig};
/// use rill_ml::sparse::SparseFeatures;
/// use rill_ml::SparseClassifier;
///
/// let mut model = FtrlClassifier::new(FtrlConfig::default()).unwrap();
/// let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
/// let _proba = model.predict_proba(&sf).unwrap();
/// model.learn(&sf, true).unwrap();
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FtrlClassifier {
    config: FtrlConfig,
    params: BTreeMap<FeatureId, FtrlParam>,
    intercept: FtrlParam,
    samples_seen: u64,
}

impl FtrlClassifier {
    /// Create a new FTRL classifier.
    ///
    /// Returns an error if the configuration is invalid.
    pub fn new(config: FtrlConfig) -> Result<Self, RillError> {
        validate_config(&config)?;
        Ok(Self {
            config,
            params: BTreeMap::new(),
            intercept: FtrlParam::default(),
            samples_seen: 0,
        })
    }

    /// The model configuration.
    pub const fn config(&self) -> &FtrlConfig {
        &self.config
    }

    /// Return the current non-zero feature weights, sorted by `FeatureId`.
    ///
    /// Features whose FTRL weight is exactly zero (due to L1
    /// soft-thresholding or never having been updated) are excluded.
    pub fn weights(&self) -> Vec<(FeatureId, f64)> {
        self.params
            .iter()
            .map(|(&id, param)| (id, param.weight(&self.config)))
            .filter(|&(_, w)| w != 0.0)
            .collect()
    }

    /// Compute the current intercept (bias) weight.
    pub fn intercept(&self) -> f64 {
        self.intercept.intercept_weight(&self.config)
    }

    /// Number of distinct features the model has seen.
    pub fn feature_count(&self) -> usize {
        self.params.len()
    }

    /// Compute the probability `sigmoid(w · x + b)` without updating state.
    fn predict_proba_inner(&self, features: &SparseFeatures) -> Result<f64, RillError> {
        let dot = compute_dot(&self.params, &self.config, features)?;
        let logit = dot + self.intercept.intercept_weight(&self.config);
        Ok(sigmoid(logit))
    }
}

impl SparseClassifier for FtrlClassifier {
    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn predict_proba(&self, features: &SparseFeatures) -> Result<f64, RillError> {
        self.predict_proba_inner(features)
    }

    fn learn(&mut self, features: &SparseFeatures, target: bool) -> Result<(), RillError> {
        if features.is_empty() {
            return Err(RillError::EmptyFeatures);
        }

        let probability = self.predict_proba_inner(features)?;
        let y = if target { 1.0 } else { 0.0 };
        let grad = probability - y;

        for &(id, value) in features.values() {
            ensure_finite("sparse_value", value)?;
            let g = grad * value;
            let param = self.params.entry(id).or_default();
            let w = param.weight(&self.config);
            param.update(g, w, &self.config);
        }

        // Update intercept with no L1 regularization.
        let w_b = self.intercept.intercept_weight(&self.config);
        self.intercept.update(grad, w_b, &self.config);

        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
        Ok(())
    }

    fn reset(&mut self) {
        self.params.clear();
        self.intercept = FtrlParam::default();
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    // -----------------------------------------------------------------
    // FtrlRegressor tests
    // -----------------------------------------------------------------

    #[test]
    fn cold_start_returns_zero() {
        let model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let pred = model.predict(&sf).unwrap();
        assert!(pred.abs() < 1e-12);
    }

    #[test]
    fn learn_linear_data_converges() {
        // y = 2 * x, single feature
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let mut first_err = 0.0;
        let mut last_err = 0.0;
        for i in 0..500 {
            let x = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = 2.0 * x;
            let sf = SparseFeatures::from_sorted(vec![(0, x)]).unwrap();
            let pred = model.predict(&sf).unwrap();
            let err = (pred - y).abs();
            if i < 10 {
                first_err += err;
            }
            if i >= 490 {
                last_err += err;
            }
            model.learn(&sf, y).unwrap();
        }
        assert!(last_err < first_err, "error should decrease");
        let weights = model.weights();
        assert_eq!(weights.len(), 1);
        assert!(
            (weights[0].1 - 2.0).abs() < 0.5,
            "weight should approach 2.0"
        );
    }

    #[test]
    fn l1_produces_sparse_weights() {
        // High L1 should drive most weights to zero.
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.1,
            beta: 1.0,
            l1: 100.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        for _ in 0..200 {
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x2 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = 0.5 * x1;
            let sf = SparseFeatures::from_sorted(vec![(0, x1), (1, x2)]).unwrap();
            model.learn(&sf, y).unwrap();
        }
        let weights = model.weights();
        // With very high L1, all weights should be zero.
        assert!(
            weights.is_empty(),
            "weights should all be zero, got {weights:?}"
        );
    }

    #[test]
    fn dynamic_features() {
        let mut model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
        assert_eq!(model.feature_count(), 0);
        let sf1 = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        model.learn(&sf1, 1.0).unwrap();
        assert_eq!(model.feature_count(), 1);
        // A new feature id appears.
        let sf2 = SparseFeatures::from_sorted(vec![(5, 2.0)]).unwrap();
        model.learn(&sf2, 2.0).unwrap();
        assert_eq!(model.feature_count(), 2);
        // Feature 0 still present.
        assert!(model.params.contains_key(&0));
        assert!(model.params.contains_key(&5));
    }

    #[test]
    fn predict_does_not_update_state() {
        let mut model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let _ = model.predict(&sf).unwrap();
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
        // Learn once, then predict again.
        model.learn(&sf, 1.0).unwrap();
        let count_after_learn = model.feature_count();
        let _ = model.predict(&sf).unwrap();
        assert_eq!(model.feature_count(), count_after_learn);
        assert_eq!(model.samples_seen(), 1);
    }

    #[test]
    fn non_finite_value_rejected() {
        let model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
        // SparseFeatures::from_sorted rejects non-finite values at construction.
        assert!(SparseFeatures::from_sorted(vec![(0, f64::NAN)]).is_err());
        assert!(SparseFeatures::from_sorted(vec![(0, f64::INFINITY)]).is_err());
        assert!(SparseFeatures::from_sorted(vec![(0, f64::NEG_INFINITY)]).is_err());
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        assert!(model.predict(&sf).is_ok());
    }

    #[test]
    fn non_finite_target_rejected() {
        let mut model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        assert!(model.learn(&sf, f64::NAN).is_err());
        assert!(model.learn(&sf, f64::INFINITY).is_err());
        assert!(model.learn(&sf, f64::NEG_INFINITY).is_err());
        // State should not change on error.
        assert_eq!(model.samples_seen(), 0);
    }

    #[test]
    fn empty_features_rejected() {
        let mut model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::new();
        assert!(model.predict(&sf).is_err());
        assert!(model.learn(&sf, 1.0).is_err());
    }

    #[test]
    fn reset_clears_state() {
        let mut model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, 3.0).unwrap();
        model.learn(&sf, 3.0).unwrap();
        assert_eq!(model.samples_seen(), 2);
        assert_eq!(model.feature_count(), 2);
        model.reset();
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
        assert!(model.predict(&sf).unwrap().abs() < 1e-12);
    }

    #[test]
    fn invalid_config_rejected() {
        assert!(
            FtrlRegressor::new(FtrlConfig {
                alpha: 0.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlRegressor::new(FtrlConfig {
                alpha: -1.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlRegressor::new(FtrlConfig {
                beta: -1.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlRegressor::new(FtrlConfig {
                l1: -1.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlRegressor::new(FtrlConfig {
                l2: -1.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlRegressor::new(FtrlConfig {
                alpha: f64::NAN,
                ..FtrlConfig::default()
            })
            .is_err()
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.2,
            beta: 0.5,
            l1: 0.5,
            l2: 0.5,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (3, 2.0)]).unwrap();
        model.learn(&sf, 5.0).unwrap();
        let json = serde_json::to_string(&model).unwrap();
        let restored: FtrlRegressor = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), model.samples_seen());
        assert_eq!(restored.feature_count(), model.feature_count());
        let pred_orig = model.predict(&sf).unwrap();
        let pred_restored = restored.predict(&sf).unwrap();
        assert!((pred_orig - pred_restored).abs() < 1e-12);
    }

    #[test]
    fn weights_returns_nonzero_only() {
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        // Learn feature 0 strongly, feature 1 barely.
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 0.0001)]).unwrap();
        for _ in 0..50 {
            model.learn(&sf, 1.0).unwrap();
        }
        let weights = model.weights();
        // All returned weights should be non-zero.
        for &(_, w) in &weights {
            assert!(w != 0.0);
        }
        // Feature 0 should be in the list.
        assert!(weights.iter().any(|&(id, _)| id == 0));
    }

    #[test]
    fn multiple_features() {
        // y = 1.0 * x0 + (-1.0) * x1 + 0.5
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);
        for _ in 0..500 {
            let x0 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = 1.0 * x0 - 1.0 * x1 + 0.5;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            model.learn(&sf, y).unwrap();
        }
        let weights = model.weights();
        assert_eq!(weights.len(), 2);
        let w0 = weights
            .iter()
            .find(|&&(id, _)| id == 0)
            .map(|&(_, w)| w)
            .unwrap();
        let w1 = weights
            .iter()
            .find(|&&(id, _)| id == 1)
            .map(|&(_, w)| w)
            .unwrap();
        assert!((w0 - 1.0).abs() < 0.5, "w0 should approach 1.0, got {w0}");
        assert!((w1 + 1.0).abs() < 0.5, "w1 should approach -1.0, got {w1}");
        assert!(
            (model.intercept() - 0.5).abs() < 0.5,
            "intercept should approach 0.5"
        );
    }

    #[test]
    fn intercept_learned() {
        // y = 3.0 (constant), single feature with value 0.0 so that only
        // the intercept can learn (feature gradient is always 0).
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 0.0)]).unwrap();
        for _ in 0..300 {
            model.learn(&sf, 3.0).unwrap();
        }
        let pred = model.predict(&sf).unwrap();
        assert!(
            (pred - 3.0).abs() < 0.5,
            "prediction should approach 3.0, got {pred}"
        );
        assert!(
            (model.intercept() - 3.0).abs() < 0.5,
            "intercept should approach 3.0"
        );
        // Feature weight should be 0 (never updated since x=0).
        assert!(model.weights().is_empty());
    }

    #[test]
    fn high_dim_sparse() {
        // 1000 possible features, only 5 active per sample.
        // Target is a linear combination of the active features.
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.3,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
        // True weights for features 0..5.
        let true_w = [1.0, -0.5, 2.0, 0.3, -1.5];
        let mut first_err = 0.0;
        let mut last_err = 0.0;
        for i in 0..2000 {
            let mut active: Vec<(FeatureId, f64)> = Vec::with_capacity(5);
            for (j, &w) in true_w.iter().enumerate() {
                let x = rand::Rng::gen_range(&mut rng, -1.0..1.0);
                active.push((j as u64, x * w));
            }
            // Add some noise features with zero contribution.
            for k in 5..10 {
                let x = rand::Rng::gen_range(&mut rng, -1.0..1.0);
                active.push((k as u64 + 100, x));
            }
            active.sort_by_key(|(id, _)| *id);
            let sf = SparseFeatures::from_sorted(active.clone()).unwrap();
            let y: f64 = active.iter().take(5).map(|(_, v)| v).sum();
            let pred = model.predict(&sf).unwrap();
            let err = (pred - y).abs();
            if i < 20 {
                first_err += err;
            }
            if i >= 1980 {
                last_err += err;
            }
            model.learn(&sf, y).unwrap();
        }
        assert!(
            last_err < first_err,
            "error should decrease in high-dim sparse"
        );
    }

    // -----------------------------------------------------------------
    // FtrlClassifier tests
    // -----------------------------------------------------------------

    #[test]
    fn cold_start_returns_0_5() {
        let model = FtrlClassifier::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let p = model.predict_proba(&sf).unwrap();
        assert!((p - 0.5).abs() < 1e-12, "cold start should predict 0.5");
    }

    #[test]
    fn learn_separable_data() {
        // Linearly separable: class 1 when x0 > 0.
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(3);
        for _ in 0..1000 {
            let x0 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = x0 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            model.learn(&sf, y).unwrap();
        }
        let p_pos = model
            .predict_proba(&SparseFeatures::from_sorted(vec![(0, 2.0), (1, 0.0)]).unwrap())
            .unwrap();
        let p_neg = model
            .predict_proba(&SparseFeatures::from_sorted(vec![(0, -2.0), (1, 0.0)]).unwrap())
            .unwrap();
        assert!(p_pos > 0.7, "p_pos should be high, got {p_pos}");
        assert!(p_neg < 0.3, "p_neg should be low, got {p_neg}");
    }

    #[test]
    fn classifier_l1_produces_sparse_weights() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.1,
            beta: 1.0,
            l1: 100.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(5);
        for _ in 0..200 {
            let x0 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = x0 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            model.learn(&sf, y).unwrap();
        }
        let weights = model.weights();
        assert!(
            weights.is_empty(),
            "weights should all be zero with high L1, got {weights:?}"
        );
    }

    #[test]
    fn classifier_dynamic_features() {
        let mut model = FtrlClassifier::new(FtrlConfig::default()).unwrap();
        assert_eq!(model.feature_count(), 0);
        let sf1 = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        model.learn(&sf1, true).unwrap();
        assert_eq!(model.feature_count(), 1);
        let sf2 = SparseFeatures::from_sorted(vec![(10, 1.0)]).unwrap();
        model.learn(&sf2, false).unwrap();
        assert_eq!(model.feature_count(), 2);
    }

    #[test]
    fn classifier_predict_does_not_update_state() {
        let mut model = FtrlClassifier::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let _ = model.predict_proba(&sf).unwrap();
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
        model.learn(&sf, true).unwrap();
        let count = model.feature_count();
        let _ = model.predict_proba(&sf).unwrap();
        assert_eq!(model.feature_count(), count);
        assert_eq!(model.samples_seen(), 1);
    }

    #[test]
    fn classifier_non_finite_value_rejected() {
        let model = FtrlClassifier::new(FtrlConfig::default()).unwrap();
        assert!(SparseFeatures::from_sorted(vec![(0, f64::NAN)]).is_err());
        assert!(SparseFeatures::from_sorted(vec![(0, f64::INFINITY)]).is_err());
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        assert!(model.predict_proba(&sf).is_ok());
    }

    #[test]
    fn classifier_empty_features_rejected() {
        let mut model = FtrlClassifier::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::new();
        assert!(model.predict_proba(&sf).is_err());
        assert!(model.learn(&sf, true).is_err());
    }

    #[test]
    fn classifier_reset_clears_state() {
        let mut model = FtrlClassifier::new(FtrlConfig::default()).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        model.learn(&sf, true).unwrap();
        model.learn(&sf, false).unwrap();
        assert_eq!(model.samples_seen(), 2);
        assert!(model.feature_count() > 0);
        model.reset();
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
        let p = model.predict_proba(&sf).unwrap();
        assert!((p - 0.5).abs() < 1e-12);
    }

    #[test]
    fn classifier_invalid_config_rejected() {
        assert!(
            FtrlClassifier::new(FtrlConfig {
                alpha: 0.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlClassifier::new(FtrlConfig {
                beta: -0.1,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlClassifier::new(FtrlConfig {
                l1: -1.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlClassifier::new(FtrlConfig {
                l2: -1.0,
                ..FtrlConfig::default()
            })
            .is_err()
        );
        assert!(
            FtrlClassifier::new(FtrlConfig {
                alpha: f64::INFINITY,
                ..FtrlConfig::default()
            })
            .is_err()
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_roundtrip() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.3,
            beta: 0.5,
            l1: 0.1,
            l2: 0.2,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (2, -1.0)]).unwrap();
        model.learn(&sf, true).unwrap();
        model.learn(&sf, false).unwrap();
        let json = serde_json::to_string(&model).unwrap();
        let restored: FtrlClassifier = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), model.samples_seen());
        assert_eq!(restored.feature_count(), model.feature_count());
        let p1 = model.predict_proba(&sf).unwrap();
        let p2 = restored.predict_proba(&sf).unwrap();
        assert!((p1 - p2).abs() < 1e-12);
    }

    #[test]
    fn predict_proba_in_range() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(17);
        for _ in 0..200 {
            let x0 = rand::Rng::gen_range(&mut rng, -5.0..5.0);
            let x1 = rand::Rng::gen_range(&mut rng, -5.0..5.0);
            let y = x0 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            model.learn(&sf, y).unwrap();
            let p = model.predict_proba(&sf).unwrap();
            assert!(p > 0.0 && p < 1.0, "probability must be in (0,1), got {p}");
        }
    }

    #[test]
    fn learn_improves_accuracy() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(21);
        // Generate a fixed test set.
        let test_set: Vec<(SparseFeatures, bool)> = (0..100)
            .map(|_| {
                let x0 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
                let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
                let y = x0 + x1 > 0.0;
                (
                    SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap(),
                    y,
                )
            })
            .collect();

        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();

        // Accuracy before learning (always predicts 0.5 -> threshold 0.5 -> true).
        let acc_before: f64 = test_set
            .iter()
            .map(|(sf, y)| {
                let pred = model.predict(sf).unwrap();
                if pred == *y { 1.0 } else { 0.0 }
            })
            .sum::<f64>()
            / test_set.len() as f64;

        // Train on fresh data.
        for _ in 0..1000 {
            let x0 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = x0 + x1 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            model.learn(&sf, y).unwrap();
        }

        let acc_after: f64 = test_set
            .iter()
            .map(|(sf, y)| {
                let pred = model.predict(sf).unwrap();
                if pred == *y { 1.0 } else { 0.0 }
            })
            .sum::<f64>()
            / test_set.len() as f64;

        assert!(
            acc_after > acc_before,
            "accuracy should improve: {acc_before} -> {acc_after}"
        );
    }

    #[test]
    fn classifier_weights_returns_nonzero_only() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 0.0001)]).unwrap();
        for _ in 0..50 {
            model.learn(&sf, true).unwrap();
        }
        let weights = model.weights();
        for &(_, w) in &weights {
            assert!(w != 0.0);
        }
    }

    #[test]
    fn classifier_multiple_features() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(33);
        for _ in 0..1000 {
            let x0 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
            let x1 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
            let x2 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            // y = 1 if x0 + x1 > 0
            let y = x0 + x1 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1), (2, x2)]).unwrap();
            model.learn(&sf, y).unwrap();
        }
        let weights = model.weights();
        // Features 0 and 1 should have non-zero weights; feature 2 may or may not.
        assert!(weights.iter().any(|&(id, _)| id == 0));
        assert!(weights.iter().any(|&(id, _)| id == 1));
        // Verify prediction quality.
        let p_pos = model
            .predict_proba(
                &SparseFeatures::from_sorted(vec![(0, 3.0), (1, 3.0), (2, 0.0)]).unwrap(),
            )
            .unwrap();
        let p_neg = model
            .predict_proba(
                &SparseFeatures::from_sorted(vec![(0, -3.0), (1, -3.0), (2, 0.0)]).unwrap(),
            )
            .unwrap();
        assert!(p_pos > 0.8);
        assert!(p_neg < 0.2);
    }

    #[test]
    fn log_loss_converges() {
        // Average log loss should decrease over training.
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(55);
        let mut first_loss = 0.0;
        let mut last_loss = 0.0;
        for i in 0..1000 {
            let x0 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = x0 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            let p = model.predict_proba(&sf).unwrap();
            let y_f = if y { 1.0 } else { 0.0 };
            let loss = -(y_f * p.ln() + (1.0 - y_f) * (1.0 - p).ln());
            if i < 20 {
                first_loss += loss;
            }
            if i >= 980 {
                last_loss += loss;
            }
            model.learn(&sf, y).unwrap();
        }
        assert!(last_loss < first_loss, "log loss should decrease");
    }
}

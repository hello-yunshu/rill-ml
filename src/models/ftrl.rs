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
//!
//! # Failure atomicity
//!
//! [`FtrlRegressor::learn`] and [`FtrlClassifier::learn`] compute the next
//! state for every affected entry before mutating any field. If any
//! intermediate value is non-finite or the sample counter overflows, the
//! call returns `Err` and the model state is unchanged.
//!
//! # Dynamic feature growth
//!
//! By default the feature table grows without bound. Long-running
//! deployments handling untrusted streams should set
//! [`FtrlConfig::max_features`] to a finite value and pick a
//! [`NewFeaturePolicy`]. `None` preserves backwards compatibility but
//! allows memory growth proportional to the number of distinct
//! `FeatureId`s ever observed.

use crate::error::{RillError, checked_finite_add, checked_increment, ensure_finite};
use crate::loss::log_loss::sigmoid;
use crate::sparse::{FeatureId, SparseFeatures};
use crate::traits::{SparseClassifier, SparseRegressor};
use std::collections::BTreeMap;

/// Policy for handling new `FeatureId`s once [`FtrlConfig::max_features`]
/// has been reached.
///
/// `max_features` only constrains feature insertion; features already in
/// the model continue to train regardless of the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NewFeaturePolicy {
    /// Reject the entire `learn` call with [`RillError::InvalidState`] when
    /// adding the new features in the current sample would exceed
    /// `max_features`. The call is failure-atomic: no state changes.
    #[default]
    Reject,
    /// Keep training existing features but silently skip any new feature
    /// that would exceed `max_features`. The sample counter still
    /// advances and existing features still receive their gradient update.
    Ignore,
}

/// Configuration for FTRL models.
///
/// Controls the per-coordinate learning rate and regularization strengths.
/// All fields must be finite; `alpha` must be strictly positive and the
/// regularization parameters must be non-negative.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FtrlConfig {
    /// Alpha: learning rate scaling. Must be `> 0`.
    pub alpha: f64,
    /// Beta: smoothing constant. Must be `>= 0`.
    pub beta: f64,
    /// L1 regularization strength. Must be `>= 0`.
    pub l1: f64,
    /// L2 regularization strength. Must be `>= 0`.
    pub l2: f64,
    /// Maximum number of distinct features the model will store.
    ///
    /// `None` (the default) allows unbounded growth, which is
    /// backwards-compatible but unsafe for long-running services that
    /// consume untrusted feature streams. Set to a finite value to
    /// bound memory and trigger the [`NewFeaturePolicy`].
    pub max_features: Option<usize>,
    /// Policy applied when `max_features` is set and a new `FeatureId`
    /// would exceed the cap.
    pub new_feature_policy: NewFeaturePolicy,
}

impl Default for FtrlConfig {
    fn default() -> Self {
        Self {
            alpha: 0.1,
            beta: 1.0,
            l1: 1.0,
            l2: 1.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        }
    }
}

impl FtrlConfig {
    /// Validate configuration parameters.
    fn validate(&self) -> Result<(), RillError> {
        ensure_finite("alpha", self.alpha)?;
        ensure_finite("beta", self.beta)?;
        ensure_finite("l1", self.l1)?;
        ensure_finite("l2", self.l2)?;
        if self.alpha <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "alpha",
                value: self.alpha,
            });
        }
        if self.beta < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "beta",
                value: self.beta,
            });
        }
        if self.l1 < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "l1",
                value: self.l1,
            });
        }
        if self.l2 < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "l2",
                value: self.l2,
            });
        }
        if let Some(max_features) = self.max_features
            && max_features == 0
        {
            return Err(RillError::InvalidParameter {
                name: "max_features",
                value: 0.0,
            });
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FtrlConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct FtrlConfigState {
            alpha: f64,
            beta: f64,
            l1: f64,
            l2: f64,
            #[serde(default)]
            max_features: Option<usize>,
            #[serde(default)]
            new_feature_policy: NewFeaturePolicy,
        }

        let state = FtrlConfigState::deserialize(deserializer)?;
        let config = FtrlConfig {
            alpha: state.alpha,
            beta: state.beta,
            l1: state.l1,
            l2: state.l2,
            max_features: state.max_features,
            new_feature_policy: state.new_feature_policy,
        };
        config.validate().map_err(serde::de::Error::custom)?;
        Ok(config)
    }
}

/// Per-feature FTRL state.
///
/// Tracks the sum of (sigma-corrected) gradients `z` and the sum of squared
/// gradients `n`. The per-coordinate adaptive learning rate is derived from
/// `n`: features seen more frequently get smaller steps.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FtrlParam {
    /// Sum of gradients (with sigma correction).
    z: f64,
    /// Sum of squared gradients. Must remain finite and non-negative.
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

    /// Compute the FTRL feature weight with explicit config-aware safety
    /// checks.
    ///
    /// Unlike [`weight`](Self::weight), this method verifies every
    /// intermediate quantity (numerator, denominator, quotient) and rejects
    /// states where the config + param combination would produce a
    /// non-finite or non-computable weight. It is intended for the serde
    /// trust boundary and may also be used by the predict path.
    ///
    /// # Contract
    ///
    /// - `z` finite, `n` finite and `>= 0`.
    /// - L1 soft-thresholding path (`|z| <= l1`) returns `Ok(0.0)`.
    /// - Denominator must be finite and non-zero; a zero denominator
    ///   (e.g. `l2 = 0`, `beta = 0`, `sqrt(n) / alpha` underflows to 0)
    ///   is rejected explicitly rather than discovered via the final
    ///   quotient.
    /// - The final quotient must be finite.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn weight_checked(&self, config: &FtrlConfig) -> Result<f64, RillError> {
        ensure_finite("ftrl_z", self.z)?;
        ensure_finite("ftrl_n", self.n)?;
        if self.n < 0.0 {
            return Err(RillError::InvalidState(format!(
                "ftrl n must be non-negative, got {}",
                self.n
            )));
        }
        // L1 soft-thresholding: |z| <= l1 → weight is 0.
        if self.z.abs() <= config.l1 {
            return Ok(0.0);
        }
        let sign = self.z.signum();
        let numerator = -(self.z - sign * config.l1);
        ensure_finite("ftrl_weight_numerator", numerator)?;
        let sqrt_n = self.n.sqrt();
        ensure_finite("ftrl_weight_sqrt_n", sqrt_n)?;
        let denominator = config.l2 + (config.beta + sqrt_n) / config.alpha;
        ensure_finite("ftrl_weight_denominator", denominator)?;
        if denominator == 0.0 {
            return Err(RillError::InvalidState(format!(
                "ftrl weight denominator is zero (z={}, n={}, alpha={}, beta={}, l1={}, l2={})",
                self.z, self.n, config.alpha, config.beta, config.l1, config.l2
            )));
        }
        let weight = numerator / denominator;
        ensure_finite("ftrl_weight", weight)?;
        Ok(weight)
    }

    /// Compute the intercept weight with explicit config-aware safety
    /// checks.
    ///
    /// See [`weight_checked`](Self::weight_checked) for the rationale. The
    /// intercept uses `l1 = 0` (no L1 regularization) and short-circuits
    /// the cold-start case `n == 0, z == 0` to `Ok(0.0)`. A state with
    /// `n == 0` but `z != 0` is rejected because it cannot produce a
    /// finite intercept weight.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn intercept_weight_checked(&self, config: &FtrlConfig) -> Result<f64, RillError> {
        ensure_finite("ftrl_z", self.z)?;
        ensure_finite("ftrl_n", self.n)?;
        if self.n < 0.0 {
            return Err(RillError::InvalidState(format!(
                "ftrl n must be non-negative, got {}",
                self.n
            )));
        }
        // Cold start: n == 0. intercept_weight short-circuits to 0 only
        // when z == 0 as well; a non-zero z with n == 0 indicates a
        // corrupted or malicious state.
        if self.n == 0.0 {
            if self.z != 0.0 {
                return Err(RillError::InvalidState(format!(
                    "ftrl intercept has n=0 but z={} (non-zero); cannot produce a finite weight",
                    self.z
                )));
            }
            return Ok(0.0);
        }
        let numerator = -self.z;
        ensure_finite("ftrl_intercept_numerator", numerator)?;
        let sqrt_n = self.n.sqrt();
        ensure_finite("ftrl_intercept_sqrt_n", sqrt_n)?;
        let denominator = config.l2 + (config.beta + sqrt_n) / config.alpha;
        ensure_finite("ftrl_intercept_denominator", denominator)?;
        if denominator == 0.0 {
            return Err(RillError::InvalidState(format!(
                "ftrl intercept denominator is zero (z={}, n={}, alpha={}, beta={}, l2={})",
                self.z, self.n, config.alpha, config.beta, config.l2
            )));
        }
        let weight = numerator / denominator;
        ensure_finite("ftrl_intercept_weight", weight)?;
        Ok(weight)
    }

    /// Compute the next `(z, n)` state after applying a gradient, without
    /// mutating `self`.
    ///
    /// `sigma = (sqrt(n_new) - sqrt(n_old)) / alpha`
    /// `z_new = z + g - sigma * w`
    /// `n_new = n + g^2`
    ///
    /// Every intermediate result is checked for finiteness so a partial
    /// overflow cannot poison state. The caller is responsible for
    /// committing the returned values atomically.
    fn next_updated(
        &self,
        gradient: f64,
        weight: f64,
        config: &FtrlConfig,
    ) -> Result<(f64, f64), RillError> {
        let gradient_sq = gradient * gradient;
        ensure_finite("ftrl_gradient_squared", gradient_sq)?;
        // Detect underflow: a non-zero gradient whose square underflows to
        // zero. In that case `n_new == n_old` while `z` still advances by
        // `gradient`, which can produce an unusable state (`n == 0, z != 0`)
        // on cold-start features and a zero denominator in `weight` on the
        // next prediction. Reject explicitly rather than silently committing
        // a state that would make the next `predict()` fail.
        if gradient != 0.0 && gradient_sq == 0.0 {
            return Err(RillError::NonFiniteValue {
                field: "ftrl_gradient_squared",
                value: gradient_sq,
            });
        }
        let n_new = checked_finite_add(self.n, gradient_sq, "ftrl_n_new")?;
        let sigma = (n_new.sqrt() - self.n.sqrt()) / config.alpha;
        ensure_finite("ftrl_sigma", sigma)?;
        let sigma_w = sigma * weight;
        ensure_finite("ftrl_sigma_weight", sigma_w)?;
        let z_delta = gradient - sigma_w;
        ensure_finite("ftrl_z_delta", z_delta)?;
        let z_new = checked_finite_add(self.z, z_delta, "ftrl_z_new")?;
        Ok((z_new, n_new))
    }

    /// Validate that `z` is finite, `n` is finite and non-negative, and the
    /// state can produce a finite weight on the next `predict()` call.
    ///
    /// The FTRL weight formula divides by `l2 + (beta + sqrt(n)) / alpha`.
    /// When `n == 0` and `l2 == 0` and `beta == 0` the denominator is zero.
    /// `intercept_weight` already short-circuits `n == 0` to `0.0`, but
    /// `weight` does not. A state with `n == 0` and `z != 0` (where
    /// `|z| > l1`) would therefore produce `±inf` on the next prediction.
    /// Such a state can only arise from a non-zero gradient whose square
    /// underflows to zero (handled in [`FtrlParam::next_updated`]) or from
    /// a maliciously crafted serde payload; both must be rejected.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn validate(&self) -> Result<(), RillError> {
        ensure_finite("ftrl_z", self.z)?;
        ensure_finite("ftrl_n", self.n)?;
        if self.n < 0.0 {
            return Err(RillError::InvalidState(format!(
                "ftrl n must be non-negative, got {0}",
                self.n
            )));
        }
        if self.n == 0.0 && self.z != 0.0 {
            return Err(RillError::InvalidState(format!(
                "ftrl param has n=0 but z={0} (non-zero); this state cannot \
                 produce a finite weight",
                self.z
            )));
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FtrlParam {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct FtrlParamState {
            z: f64,
            n: f64,
        }

        let state = FtrlParamState::deserialize(deserializer)?;
        let param = FtrlParam {
            z: state.z,
            n: state.n,
        };
        param.validate().map_err(serde::de::Error::custom)?;
        Ok(param)
    }
}

/// Compute the dot product `w · x` over sparse features.
///
/// Iterates only over the features present in `features` (not all stored
/// params), looking up each feature's current FTRL weight. Each
/// contribution and the running sum are checked for finiteness so a
/// single overflowing term cannot poison the prediction.
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
            let w = param.weight(config);
            ensure_finite("ftrl_weight", w)?;
            let contribution = w * value;
            ensure_finite("ftrl_dot_contribution", contribution)?;
            dot = checked_finite_add(dot, contribution, "ftrl_dot")?;
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
        config.validate()?;
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
        let intercept = self.intercept.intercept_weight(&self.config);
        ensure_finite("ftrl_intercept", intercept)?;
        checked_finite_add(dot, intercept, "ftrl_prediction")
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FtrlRegressor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct FtrlRegressorState {
            config: FtrlConfig,
            params: BTreeMap<FeatureId, FtrlParam>,
            intercept: FtrlParam,
            samples_seen: u64,
        }

        let state = FtrlRegressorState::deserialize(deserializer)?;
        let model = FtrlRegressor {
            config: state.config,
            params: state.params,
            intercept: state.intercept,
            samples_seen: state.samples_seen,
        };
        // config and each FtrlParam are already validated by their own
        // Deserialize impls; only the top-level invariants remain.
        model
            .validate_invariants()
            .map_err(serde::de::Error::custom)?;
        Ok(model)
    }
}

impl FtrlRegressor {
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn validate_invariants(&self) -> Result<(), RillError> {
        // config and individual params are validated at deserialization;
        // here we additionally verify that the config + param combination
        // produces a finite, computable weight for every feature and the
        // intercept. This catches states that pass the basic z/n checks
        // but would still yield a zero-denominator or infinite weight
        // (e.g. very large alpha with tiny n, beta=0, l2=0).
        self.config.validate()?;
        // Top-level invariant: the stored feature count must not exceed
        // `max_features`. A malicious payload with `max_features = 1` but
        // two stored params violates the model contract and is rejected
        // here rather than silently accepted.
        if let Some(max_features) = self.config.max_features
            && self.params.len() > max_features
        {
            return Err(RillError::InvalidState(format!(
                "FTRL stored feature count {} exceeds max_features {}",
                self.params.len(),
                max_features
            )));
        }
        for (id, param) in &self.params {
            param.validate()?;
            param
                .weight_checked(&self.config)
                .map_err(|e| RillError::InvalidState(format!("ftrl feature {id} weight: {e}")))?;
        }
        self.intercept.validate()?;
        self.intercept
            .intercept_weight_checked(&self.config)
            .map_err(|e| RillError::InvalidState(format!("ftrl intercept weight: {e}")))?;
        Ok(())
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

        // Reserve the sample counter first. If it overflows, no state changes.
        let next_samples_seen = checked_increment(self.samples_seen, "samples_seen")?;

        let prediction = self.predict_inner(features)?;
        ensure_finite("ftrl_prediction", prediction)?;
        let grad = prediction - target;
        ensure_finite("ftrl_gradient", grad)?;

        // Pre-judge the max_features policy. We never insert a subset of
        // new features then fail: either all new features are accepted or
        // (under Ignore) all new features are skipped.
        let new_ids_count = features
            .values()
            .iter()
            .filter(|(id, _)| !self.params.contains_key(id))
            .count();
        let mut skip_new_features = false;
        if let Some(max_features) = self.config.max_features {
            let projected = self.params.len().saturating_add(new_ids_count);
            if projected > max_features {
                match self.config.new_feature_policy {
                    NewFeaturePolicy::Reject => {
                        return Err(RillError::InvalidState(format!(
                            "FTRL feature count {projected} exceeds max_features {max_features}"
                        )));
                    }
                    NewFeaturePolicy::Ignore => {
                        skip_new_features = true;
                    }
                }
            }
        }

        // Compute the next (z, n) for every feature without touching self.
        let mut updates: Vec<(FeatureId, f64, f64)> = Vec::with_capacity(features.len());
        for &(id, value) in features.values() {
            // Ignore policy must be evaluated before any arithmetic on the
            // new feature's value. Otherwise an oversized new feature whose
            // `grad * value` would overflow could fail the whole `learn()`
            // call even though the feature is supposed to be skipped.
            let is_new = !self.params.contains_key(&id);
            if is_new && skip_new_features {
                continue;
            }

            let g = grad * value;
            ensure_finite("ftrl_feature_gradient", g)?;

            let (new_z, new_n) = if let Some(param) = self.params.get(&id) {
                let w = param.weight(&self.config);
                param.next_updated(g, w, &self.config)?
            } else {
                let param = FtrlParam::default();
                let w = param.weight(&self.config);
                param.next_updated(g, w, &self.config)?
            };
            // Verify the next state produces a finite weight before
            // committing. This catches any path that would leave the model
            // in a state where the next `predict()` fails due to internal
            // state (e.g. a zero denominator in the weight formula).
            let next_w = FtrlParam { z: new_z, n: new_n }.weight(&self.config);
            ensure_finite("ftrl_next_weight", next_w)?;
            updates.push((id, new_z, new_n));
        }

        // Compute the next intercept state.
        let w_b = self.intercept.intercept_weight(&self.config);
        let (new_intercept_z, new_intercept_n) =
            self.intercept.next_updated(grad, w_b, &self.config)?;
        // Verify the next intercept produces a finite weight too.
        let next_intercept_w = FtrlParam {
            z: new_intercept_z,
            n: new_intercept_n,
        }
        .intercept_weight(&self.config);
        ensure_finite("ftrl_next_intercept_weight", next_intercept_w)?;

        // Commit atomically. No failure path beyond this point.
        for (id, new_z, new_n) in updates {
            let param = self.params.entry(id).or_default();
            param.z = new_z;
            param.n = new_n;
        }
        self.intercept.z = new_intercept_z;
        self.intercept.n = new_intercept_n;
        self.samples_seen = next_samples_seen;

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
/// The returned probability lies in `[0, 1]`. Extreme logits can produce
/// exactly `0.0` or `1.0` after `sigmoid`; downstream consumers such as
/// [`crate::loss::log_loss::BinaryLogLoss`] clip internally.
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
        config.validate()?;
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
        let intercept = self.intercept.intercept_weight(&self.config);
        ensure_finite("ftrl_intercept", intercept)?;
        let logit = checked_finite_add(dot, intercept, "ftrl_logit")?;
        Ok(sigmoid(logit))
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FtrlClassifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct FtrlClassifierState {
            config: FtrlConfig,
            params: BTreeMap<FeatureId, FtrlParam>,
            intercept: FtrlParam,
            samples_seen: u64,
        }

        let state = FtrlClassifierState::deserialize(deserializer)?;
        let model = FtrlClassifier {
            config: state.config,
            params: state.params,
            intercept: state.intercept,
            samples_seen: state.samples_seen,
        };
        model
            .validate_invariants()
            .map_err(serde::de::Error::custom)?;
        Ok(model)
    }
}

impl FtrlClassifier {
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn validate_invariants(&self) -> Result<(), RillError> {
        // See FtrlRegressor::validate_invariants for rationale.
        self.config.validate()?;
        // Top-level invariant: the stored feature count must not exceed
        // `max_features`. See FtrlRegressor::validate_invariants.
        if let Some(max_features) = self.config.max_features
            && self.params.len() > max_features
        {
            return Err(RillError::InvalidState(format!(
                "FTRL stored feature count {} exceeds max_features {}",
                self.params.len(),
                max_features
            )));
        }
        for (id, param) in &self.params {
            param.validate()?;
            param
                .weight_checked(&self.config)
                .map_err(|e| RillError::InvalidState(format!("ftrl feature {id} weight: {e}")))?;
        }
        self.intercept.validate()?;
        self.intercept
            .intercept_weight_checked(&self.config)
            .map_err(|e| RillError::InvalidState(format!("ftrl intercept weight: {e}")))?;
        Ok(())
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

        let next_samples_seen = checked_increment(self.samples_seen, "samples_seen")?;

        let probability = self.predict_proba_inner(features)?;
        ensure_finite("ftrl_probability", probability)?;
        let y = if target { 1.0 } else { 0.0 };
        let grad = probability - y;
        ensure_finite("ftrl_gradient", grad)?;

        let new_ids_count = features
            .values()
            .iter()
            .filter(|(id, _)| !self.params.contains_key(id))
            .count();
        let mut skip_new_features = false;
        if let Some(max_features) = self.config.max_features {
            let projected = self.params.len().saturating_add(new_ids_count);
            if projected > max_features {
                match self.config.new_feature_policy {
                    NewFeaturePolicy::Reject => {
                        return Err(RillError::InvalidState(format!(
                            "FTRL feature count {projected} exceeds max_features {max_features}"
                        )));
                    }
                    NewFeaturePolicy::Ignore => {
                        skip_new_features = true;
                    }
                }
            }
        }

        let mut updates: Vec<(FeatureId, f64, f64)> = Vec::with_capacity(features.len());
        for &(id, value) in features.values() {
            // Ignore policy must be evaluated before any arithmetic on the
            // new feature's value. Otherwise an oversized new feature whose
            // `grad * value` would overflow could fail the whole `learn()`
            // call even though the feature is supposed to be skipped.
            let is_new = !self.params.contains_key(&id);
            if is_new && skip_new_features {
                continue;
            }

            let g = grad * value;
            ensure_finite("ftrl_feature_gradient", g)?;

            let (new_z, new_n) = if let Some(param) = self.params.get(&id) {
                let w = param.weight(&self.config);
                param.next_updated(g, w, &self.config)?
            } else {
                let param = FtrlParam::default();
                let w = param.weight(&self.config);
                param.next_updated(g, w, &self.config)?
            };
            // Verify the next state produces a finite weight before committing.
            let next_w = FtrlParam { z: new_z, n: new_n }.weight(&self.config);
            ensure_finite("ftrl_next_weight", next_w)?;
            updates.push((id, new_z, new_n));
        }

        let w_b = self.intercept.intercept_weight(&self.config);
        let (new_intercept_z, new_intercept_n) =
            self.intercept.next_updated(grad, w_b, &self.config)?;
        // Verify the next intercept produces a finite weight too.
        let next_intercept_w = FtrlParam {
            z: new_intercept_z,
            n: new_intercept_n,
        }
        .intercept_weight(&self.config);
        ensure_finite("ftrl_next_intercept_weight", next_intercept_w)?;

        for (id, new_z, new_n) in updates {
            let param = self.params.entry(id).or_default();
            param.z = new_z;
            param.n = new_n;
        }
        self.intercept.z = new_intercept_z;
        self.intercept.n = new_intercept_n;
        self.samples_seen = next_samples_seen;

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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
        assert!(
            FtrlRegressor::new(FtrlConfig {
                max_features: Some(0),
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
            max_features: Some(100),
            new_feature_policy: NewFeaturePolicy::Reject,
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
    // FtrlRegressor: failure atomicity and overflow (ML-001/002)
    // -----------------------------------------------------------------

    #[test]
    fn regressor_overflow_does_not_mutate_state() {
        // Finite inputs but intermediate `gradient * value` or
        // `gradient^2` overflows. The whole learn call must fail and
        // leave the model untouched.
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.1,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 1e200)]).unwrap();
        let result = model.learn(&sf, 1e100);
        assert!(result.is_err(), "expected overflow error, got {result:?}");
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
        assert!(model.params.is_empty());
        assert_eq!(model.intercept.z, 0.0);
        assert_eq!(model.intercept.n, 0.0);
    }

    #[test]
    fn regressor_partial_update_is_atomic() {
        // Two features in one sample. Feature 0 would succeed on its own,
        // feature 1 overflows. Neither may be committed.
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.1,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 1e200)]).unwrap();
        assert!(model.learn(&sf, 1e100).is_err());
        // Feature 0 must NOT be inserted.
        assert!(!model.params.contains_key(&0));
        assert!(!model.params.contains_key(&1));
        assert_eq!(model.samples_seen(), 0);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_samples_seen_overflow_is_atomic() {
        let json = format!(
            "{{\"config\":{{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"}},\"params\":{{}},\"intercept\":{{\"z\":0.0,\"n\":0.0}},\"samples_seen\":{}}}",
            u64::MAX
        );
        let mut model: FtrlRegressor = serde_json::from_str(&json).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let result = model.learn(&sf, 1.0);
        assert!(result.is_err(), "expected counter overflow");
        assert_eq!(model.samples_seen(), u64::MAX);
        assert_eq!(model.feature_count(), 0);
        assert_eq!(model.intercept.z, 0.0);
        assert_eq!(model.intercept.n, 0.0);
    }

    // -----------------------------------------------------------------
    // FtrlRegressor: max_features boundary (ML-003)
    // -----------------------------------------------------------------

    #[test]
    fn regressor_max_features_reject_at_limit() {
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(2),
            new_feature_policy: NewFeaturePolicy::Reject,
        })
        .unwrap();
        // Reach exactly the limit.
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, 1.0).unwrap();
        assert_eq!(model.feature_count(), 2);
        // One more new feature: Reject must fail atomically.
        let sf_new = SparseFeatures::from_sorted(vec![(2, 3.0)]).unwrap();
        assert!(model.learn(&sf_new, 1.0).is_err());
        assert_eq!(model.feature_count(), 2);
        assert_eq!(model.samples_seen(), 1);
        // Existing features still train.
        let sf_existing = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf_existing, 1.0).unwrap();
        assert_eq!(model.feature_count(), 2);
        assert_eq!(model.samples_seen(), 2);
    }

    #[test]
    fn regressor_max_features_ignore_skips_new() {
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(2),
            new_feature_policy: NewFeaturePolicy::Ignore,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, 1.0).unwrap();
        // Sample with one existing and one new feature. Under Ignore the
        // new feature is skipped, the existing one is updated, and the
        // counter still advances.
        let sf_mixed = SparseFeatures::from_sorted(vec![(0, 1.0), (2, 3.0)]).unwrap();
        model.learn(&sf_mixed, 1.0).unwrap();
        assert_eq!(model.feature_count(), 2);
        assert!(!model.params.contains_key(&2));
        assert_eq!(model.samples_seen(), 2);
    }

    #[test]
    fn regressor_max_features_multi_new_prejudge() {
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(2),
            new_feature_policy: NewFeaturePolicy::Reject,
        })
        .unwrap();
        // A single sample with three new features. Reject fails atomically
        // without inserting any subset.
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0), (2, 3.0)]).unwrap();
        assert!(model.learn(&sf, 1.0).is_err());
        assert_eq!(model.feature_count(), 0);
        assert_eq!(model.samples_seen(), 0);
    }

    // -----------------------------------------------------------------
    // FtrlRegressor: serde validation (ML-004)
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_rejects_negative_n() {
        let json = "{\"config\":{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":-1.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlRegressor, _> = serde_json::from_str(json);
        assert!(result.is_err(), "negative n must be rejected");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_rejects_invalid_config() {
        let json = "{\"config\":{\"alpha\":-1.0,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlRegressor, _> = serde_json::from_str(json);
        assert!(result.is_err(), "invalid alpha must be rejected");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_accepts_missing_optional_fields() {
        // Old state without max_features/new_feature_policy must still load.
        let json = "{\"config\":{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0},\"params\":{},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let model: FtrlRegressor = serde_json::from_str(json).unwrap();
        assert!(model.config().max_features.is_none());
        assert_eq!(model.config().new_feature_policy, NewFeaturePolicy::Reject);
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: Some(100),
            new_feature_policy: NewFeaturePolicy::Reject,
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            assert!(
                (0.0..=1.0).contains(&p),
                "probability must be in [0,1], got {p}"
            );
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
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
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let loss_fn = crate::loss::log_loss::BinaryLogLoss::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(55);
        let mut first_loss = 0.0;
        let mut last_loss = 0.0;
        for i in 0..1000 {
            let x0 = rand::Rng::gen_range(&mut rng, -2.0..2.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = x0 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            let p = model.predict_proba(&sf).unwrap();
            let loss = loss_fn.loss(p, y);
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

    // -----------------------------------------------------------------
    // FtrlClassifier: failure atomicity and overflow (ML-001/002)
    // -----------------------------------------------------------------

    #[test]
    fn classifier_overflow_does_not_mutate_state() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.1,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 1e300)]).unwrap();
        // Cold-start probability is 0.5, grad = 0.5 - 0.0 = 0.5.
        // g for feature 1 = 0.5 * 1e300 = 5e299 (finite), g^2 = 2.5e599 = inf.
        let result = model.learn(&sf, false);
        assert!(result.is_err(), "expected overflow error, got {result:?}");
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
        assert!(model.params.is_empty());
        assert_eq!(model.intercept.z, 0.0);
        assert_eq!(model.intercept.n, 0.0);
    }

    #[test]
    fn classifier_partial_update_is_atomic() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.1,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 1e300)]).unwrap();
        assert!(model.learn(&sf, false).is_err());
        assert!(!model.params.contains_key(&0));
        assert!(!model.params.contains_key(&1));
        assert_eq!(model.samples_seen(), 0);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_samples_seen_overflow_is_atomic() {
        let json = format!(
            "{{\"config\":{{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"}},\"params\":{{}},\"intercept\":{{\"z\":0.0,\"n\":0.0}},\"samples_seen\":{}}}",
            u64::MAX
        );
        let mut model: FtrlClassifier = serde_json::from_str(&json).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let result = model.learn(&sf, true);
        assert!(result.is_err(), "expected counter overflow");
        assert_eq!(model.samples_seen(), u64::MAX);
        assert_eq!(model.feature_count(), 0);
        assert_eq!(model.intercept.z, 0.0);
        assert_eq!(model.intercept.n, 0.0);
    }

    // -----------------------------------------------------------------
    // FtrlClassifier: max_features boundary (ML-003)
    // -----------------------------------------------------------------

    #[test]
    fn classifier_max_features_reject_at_limit() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(2),
            new_feature_policy: NewFeaturePolicy::Reject,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, true).unwrap();
        assert_eq!(model.feature_count(), 2);
        let sf_new = SparseFeatures::from_sorted(vec![(2, 3.0)]).unwrap();
        assert!(model.learn(&sf_new, true).is_err());
        assert_eq!(model.feature_count(), 2);
        assert_eq!(model.samples_seen(), 1);
    }

    #[test]
    fn classifier_max_features_ignore_skips_new() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(2),
            new_feature_policy: NewFeaturePolicy::Ignore,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, true).unwrap();
        let sf_mixed = SparseFeatures::from_sorted(vec![(0, 1.0), (2, 3.0)]).unwrap();
        model.learn(&sf_mixed, false).unwrap();
        assert_eq!(model.feature_count(), 2);
        assert!(!model.params.contains_key(&2));
        assert_eq!(model.samples_seen(), 2);
    }

    #[test]
    fn classifier_max_features_multi_new_prejudge() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(2),
            new_feature_policy: NewFeaturePolicy::Reject,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0), (2, 3.0)]).unwrap();
        assert!(model.learn(&sf, true).is_err());
        assert_eq!(model.feature_count(), 0);
        assert_eq!(model.samples_seen(), 0);
    }

    // -----------------------------------------------------------------
    // FtrlClassifier: serde validation (ML-004)
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_rejects_negative_n() {
        let json = "{\"config\":{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":-1.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlClassifier, _> = serde_json::from_str(json);
        assert!(result.is_err(), "negative n must be rejected");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_rejects_invalid_config() {
        let json = "{\"config\":{\"alpha\":-1.0,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlClassifier, _> = serde_json::from_str(json);
        assert!(result.is_err(), "invalid alpha must be rejected");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_accepts_missing_optional_fields() {
        let json = "{\"config\":{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0},\"params\":{},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let model: FtrlClassifier = serde_json::from_str(json).unwrap();
        assert!(model.config().max_features.is_none());
        assert_eq!(model.config().new_feature_policy, NewFeaturePolicy::Reject);
    }

    // -----------------------------------------------------------------
    // Second-audit: FTRL underflow / zero-denominator / Ignore order
    // -----------------------------------------------------------------

    #[test]
    fn regressor_gradient_squared_underflow_is_atomic() {
        // Cold start: prediction = 0, target = -1e-200 → grad = 1e-200.
        // For feature 0 (value=1.0): g = 1e-200 (non-zero, finite).
        // g^2 = 1e-400 underflows to 0.0. Without the explicit underflow
        // check, n_new would stay at 0 while z advances, producing a state
        // whose next predict() divides by zero.
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 1.0,
            beta: 0.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let result = model.learn(&sf, -1e-200);
        assert!(result.is_err(), "expected underflow error, got {result:?}");
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
        assert!(model.params.is_empty());
        assert_eq!(model.intercept.z, 0.0);
        assert_eq!(model.intercept.n, 0.0);
    }

    #[test]
    fn classifier_gradient_squared_underflow_is_atomic() {
        // Cold start: probability = 0.5, target = false (y=0), grad = 0.5.
        // Feature 0 value = 1e-200: g = 0.5 * 1e-200 = 5e-201 (non-zero).
        // g^2 = 2.5e-401 underflows to 0.0.
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 1.0,
            beta: 0.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1e-200)]).unwrap();
        let result = model.learn(&sf, false);
        assert!(result.is_err(), "expected underflow error, got {result:?}");
        assert_eq!(model.samples_seen(), 0);
        assert_eq!(model.feature_count(), 0);
    }

    #[test]
    fn regressor_boundary_config_predict_after_learn_always_finite() {
        // beta=0, l2=0, l1=0 is the zero-denominator boundary. Every
        // successful learn must keep the model in a state where predict
        // returns a finite value.
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 1.0,
            beta: 0.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(77);
        for _ in 0..100 {
            let x0 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = 2.0 * x0 - x1;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            model.learn(&sf, y).unwrap();
            let pred = model.predict(&sf);
            assert!(
                pred.is_ok(),
                "predict failed after successful learn: {pred:?}"
            );
            assert!(
                pred.unwrap().is_finite(),
                "predict must return finite value after successful learn"
            );
        }
    }

    #[test]
    fn classifier_boundary_config_predict_proba_after_learn_always_finite() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 1.0,
            beta: 0.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(88);
        for _ in 0..100 {
            let x0 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x1 = rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let y = x0 > 0.0;
            let sf = SparseFeatures::from_sorted(vec![(0, x0), (1, x1)]).unwrap();
            model.learn(&sf, y).unwrap();
            let proba = model.predict_proba(&sf);
            assert!(proba.is_ok(), "predict_proba failed after learn: {proba:?}");
            let p = proba.unwrap();
            assert!(p.is_finite(), "probability must be finite, got {p}");
            assert!(
                (0.0..=1.0).contains(&p),
                "probability must be in [0,1], got {p}"
            );
        }
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_rejects_n_zero_z_nonzero() {
        // n=0, z=1.0: weight formula denominator = l2 + (beta + sqrt(0))/alpha.
        // With beta=0, l2=0, alpha=1: denominator = 0 → weight = inf.
        // validate() must reject this state.
        let json = "{\"config\":{\"alpha\":1.0,\"beta\":0.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":0.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlRegressor, _> = serde_json::from_str(json);
        assert!(result.is_err(), "n=0 && z!=0 must be rejected");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_rejects_n_zero_z_nonzero() {
        let json = "{\"config\":{\"alpha\":1.0,\"beta\":0.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":0.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlClassifier, _> = serde_json::from_str(json);
        assert!(result.is_err(), "n=0 && z!=0 must be rejected");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_predict_dot_plus_intercept_overflow() {
        // Craft a model where dot + intercept overflows f64.
        // config: alpha=1, beta=0, l1=0, l2=0 → weight = -z / sqrt(n)
        // z = -f64::MAX * 0.75, n = 1.0 → weight = f64::MAX * 0.75
        // dot = weight * 1.0 = f64::MAX * 0.75
        // intercept_weight = f64::MAX * 0.75
        // dot + intercept = f64::MAX * 1.5 → overflow to inf.
        let z = -f64::MAX * 0.75;
        let json = format!(
            "{{\"config\":{{\"alpha\":1.0,\"beta\":0.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"}},\"params\":{{\"0\":{{\"z\":{0},\"n\":1.0}}}},\"intercept\":{{\"z\":{0},\"n\":1.0}},\"samples_seen\":1}}",
            z
        );
        let model: FtrlRegressor = serde_json::from_str(&json).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        let result = model.predict(&sf);
        assert!(
            result.is_err(),
            "expected dot+intercept overflow error, got {result:?}"
        );
    }

    #[test]
    fn regressor_ignore_skips_overflowing_new_feature() {
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(1),
            new_feature_policy: NewFeaturePolicy::Ignore,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        model.learn(&sf, 1.0).unwrap();
        assert_eq!(model.feature_count(), 1);

        // New feature 1 with huge value. grad is O(1), so
        // g = grad * f64::MAX is finite, but g^2 overflows to inf.
        // Under Ignore, the new feature must be skipped BEFORE the
        // multiplication; otherwise the whole learn() fails.
        let sf_mixed = SparseFeatures::from_sorted(vec![(0, 1.0), (1, f64::MAX)]).unwrap();
        let result = model.learn(&sf_mixed, 1.0);
        assert!(
            result.is_ok(),
            "Ignore must skip overflowing new feature, got {result:?}"
        );
        assert_eq!(model.feature_count(), 1);
        assert!(!model.params.contains_key(&1));
        assert_eq!(model.samples_seen(), 2);
        assert!(model.predict(&sf).is_ok());
    }

    #[test]
    fn classifier_ignore_skips_overflowing_new_feature() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 0.5,
            beta: 1.0,
            l1: 0.0,
            l2: 0.0,
            max_features: Some(1),
            new_feature_policy: NewFeaturePolicy::Ignore,
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0)]).unwrap();
        model.learn(&sf, true).unwrap();
        assert_eq!(model.feature_count(), 1);

        let sf_mixed = SparseFeatures::from_sorted(vec![(0, 1.0), (1, f64::MAX)]).unwrap();
        let result = model.learn(&sf_mixed, true);
        assert!(
            result.is_ok(),
            "Ignore must skip overflowing new feature, got {result:?}"
        );
        assert_eq!(model.feature_count(), 1);
        assert!(!model.params.contains_key(&1));
        assert_eq!(model.samples_seen(), 2);
        assert!(model.predict_proba(&sf).is_ok());
    }

    // -----------------------------------------------------------------
    // §6.2: FTRL config-aware serde validation
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_rejects_config_dependent_zero_denominator() {
        // alpha = f64::MAX, beta = 0, l2 = 0, l1 = 0, n = 1e-300, z = 1.0.
        // sqrt(n) = 1e-150; denominator = (0 + 1e-150) / f64::MAX underflows
        // to 0.0. The basic validate() passes (n > 0, z finite) but
        // weight_checked must reject the zero denominator explicitly.
        let json = "{\"config\":{\"alpha\":1.7976931348623157e308,\"beta\":0.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":1e-300}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlRegressor, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "config-dependent zero denominator must be rejected"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_rejects_config_dependent_zero_denominator() {
        let json = "{\"config\":{\"alpha\":1.7976931348623157e308,\"beta\":0.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":1e-300}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":0}";
        let result: Result<FtrlClassifier, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "config-dependent zero denominator must be rejected"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_rejects_intercept_zero_denominator() {
        // Same dangerous config, but the malicious state is in the
        // intercept rather than a feature param.
        let json = "{\"config\":{\"alpha\":1.7976931348623157e308,\"beta\":0.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{},\"intercept\":{\"z\":1.0,\"n\":1e-300},\"samples_seen\":0}";
        let result: Result<FtrlRegressor, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "intercept zero denominator must be rejected"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_rejects_intercept_zero_denominator() {
        let json = "{\"config\":{\"alpha\":1.7976931348623157e308,\"beta\":0.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{},\"intercept\":{\"z\":1.0,\"n\":1e-300},\"samples_seen\":0}";
        let result: Result<FtrlClassifier, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "intercept zero denominator must be rejected"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_valid_boundary_state_roundtrips() {
        // Boundary config (beta=0, l2=0) with legitimately trained state
        // must round-trip through serde without being rejected by the new
        // config-aware checks.
        let mut model = FtrlRegressor::new(FtrlConfig {
            alpha: 1.0,
            beta: 0.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, 3.0).unwrap();
        model.learn(&sf, 5.0).unwrap();
        let json = serde_json::to_string(&model).unwrap();
        let restored: FtrlRegressor = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), model.samples_seen());
        assert_eq!(restored.feature_count(), model.feature_count());
        let p1 = model.predict(&sf).unwrap();
        let p2 = restored.predict(&sf).unwrap();
        assert!((p1 - p2).abs() < 1e-12);
        // weights() must not produce Infinity on the restored model.
        for (_, w) in restored.weights() {
            assert!(w.is_finite(), "restored weight must be finite, got {w}");
        }
        assert!(restored.intercept().is_finite());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_valid_boundary_state_roundtrips() {
        let mut model = FtrlClassifier::new(FtrlConfig {
            alpha: 1.0,
            beta: 0.0,
            l1: 0.0,
            l2: 0.0,
            max_features: None,
            new_feature_policy: NewFeaturePolicy::default(),
        })
        .unwrap();
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
        model.learn(&sf, true).unwrap();
        model.learn(&sf, false).unwrap();
        let json = serde_json::to_string(&model).unwrap();
        let restored: FtrlClassifier = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), model.samples_seen());
        assert_eq!(restored.feature_count(), model.feature_count());
        let p1 = model.predict_proba(&sf).unwrap();
        let p2 = restored.predict_proba(&sf).unwrap();
        assert!((p1 - p2).abs() < 1e-12);
        for (_, w) in restored.weights() {
            assert!(w.is_finite(), "restored weight must be finite, got {w}");
        }
        assert!(restored.intercept().is_finite());
    }

    // -----------------------------------------------------------------
    // Fourth-stage audit: FTRL max_features serde invariant (4-A-02)
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_rejects_params_above_max_features() {
        // config.max_features = 1, but two stored params. This violates
        // the model contract and must be rejected at deserialisation.
        let json = "{\"config\":{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":1,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":1.0},\"1\":{\"z\":1.0,\"n\":1.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":1}";
        let result: Result<FtrlRegressor, _> = serde_json::from_str(json);
        let err = match result {
            Ok(_) => panic!("expected serde error, got Ok"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("max_features") && msg.contains("feature count"),
            "error must mention feature count / max_features, got: {msg}"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_rejects_params_above_max_features() {
        let json = "{\"config\":{\"alpha\":0.1,\"beta\":1.0,\"l1\":1.0,\"l2\":1.0,\"max_features\":1,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":1.0,\"n\":1.0},\"1\":{\"z\":1.0,\"n\":1.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":1}";
        let result: Result<FtrlClassifier, _> = serde_json::from_str(json);
        let err = match result {
            Ok(_) => panic!("expected serde error, got Ok"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("max_features") && msg.contains("feature count"),
            "error must mention feature count / max_features, got: {msg}"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_accepts_params_equal_to_max_features() {
        // params.len() == max_features is legal. Round-trip must succeed
        // and weights()/predict() must remain finite.
        let json = "{\"config\":{\"alpha\":0.5,\"beta\":1.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":2,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":0.5,\"n\":1.0},\"1\":{\"z\":-0.25,\"n\":2.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":3}";
        let model: FtrlRegressor =
            serde_json::from_str(json).expect("equal count must be accepted");
        assert_eq!(model.feature_count(), 2);
        assert_eq!(model.samples_seen(), 3);
        for (_, w) in model.weights() {
            assert!(w.is_finite(), "weight must be finite, got {w}");
        }
        assert!(model.intercept().is_finite());
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 1.0)]).unwrap();
        let pred = model.predict(&sf).expect("predict must succeed");
        assert!(pred.is_finite(), "prediction must be finite, got {pred}");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_accepts_params_equal_to_max_features() {
        let json = "{\"config\":{\"alpha\":0.5,\"beta\":1.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":2,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":0.5,\"n\":1.0},\"1\":{\"z\":-0.25,\"n\":2.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":3}";
        let model: FtrlClassifier =
            serde_json::from_str(json).expect("equal count must be accepted");
        assert_eq!(model.feature_count(), 2);
        assert_eq!(model.samples_seen(), 3);
        for (_, w) in model.weights() {
            assert!(w.is_finite(), "weight must be finite, got {w}");
        }
        assert!(model.intercept().is_finite());
        let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 1.0)]).unwrap();
        let p = model
            .predict_proba(&sf)
            .expect("predict_proba must succeed");
        assert!(p.is_finite(), "probability must be finite, got {p}");
        assert!(
            (0.0..=1.0).contains(&p),
            "probability must be in [0,1], got {p}"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn regressor_serde_allows_unbounded_params_when_max_features_none() {
        // max_features = None must not constrain params.len(). Three
        // stored params with no cap must round-trip cleanly.
        let json = "{\"config\":{\"alpha\":0.5,\"beta\":1.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":0.5,\"n\":1.0},\"1\":{\"z\":-0.25,\"n\":2.0},\"2\":{\"z\":1.5,\"n\":3.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":6}";
        let model: FtrlRegressor =
            serde_json::from_str(json).expect("unbounded state must be accepted");
        assert_eq!(model.feature_count(), 3);
        for (_, w) in model.weights() {
            assert!(w.is_finite(), "weight must be finite, got {w}");
        }
        assert!(model.intercept().is_finite());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn classifier_serde_allows_unbounded_params_when_max_features_none() {
        let json = "{\"config\":{\"alpha\":0.5,\"beta\":1.0,\"l1\":0.0,\"l2\":0.0,\"max_features\":null,\"new_feature_policy\":\"Reject\"},\"params\":{\"0\":{\"z\":0.5,\"n\":1.0},\"1\":{\"z\":-0.25,\"n\":2.0},\"2\":{\"z\":1.5,\"n\":3.0}},\"intercept\":{\"z\":0.0,\"n\":0.0},\"samples_seen\":6}";
        let model: FtrlClassifier =
            serde_json::from_str(json).expect("unbounded state must be accepted");
        assert_eq!(model.feature_count(), 3);
        for (_, w) in model.weights() {
            assert!(w.is_finite(), "weight must be finite, got {w}");
        }
        assert!(model.intercept().is_finite());
    }
}

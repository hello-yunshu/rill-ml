//! Online Naive Bayes classifiers.
//!
//! Three variants are provided:
//! - [`GaussianNaiveBayes`]: for continuous features (assumes Gaussian distribution)
//! - [`BernoulliNaiveBayes`]: for binary features (0/1)
//! - [`MultinomialNaiveBayes`]: for count features (non-negative integers)
//!
//! All three implement [`OnlineBinaryClassifier`] for binary classification.
//! Multi-class support may be added in a future version.

use crate::error::{
    RillError, checked_finite_add, checked_increment, ensure_finite, validate_features,
};
use crate::loss::log_loss::sigmoid;
use crate::traits::OnlineBinaryClassifier;

/// Configuration for Naive Bayes classifiers.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct NaiveBayesConfig {
    /// Laplace smoothing parameter (alpha). Must be `> 0`.
    ///
    /// Default: `1.0` (standard Laplace smoothing).
    pub alpha: f64,
}

impl Default for NaiveBayesConfig {
    fn default() -> Self {
        Self { alpha: 1.0 }
    }
}

fn validate_config(config: &NaiveBayesConfig) -> Result<(), RillError> {
    ensure_finite("alpha", config.alpha)?;
    if config.alpha <= 0.0 {
        return Err(RillError::InvalidParameter {
            name: "alpha",
            value: config.alpha,
        });
    }
    Ok(())
}

/// Compare two non-negative finite floats for approximate equality using a
/// combination of absolute and relative tolerance, returning `true` when
/// they are "close enough" to be plausibly the same quantity.
///
/// This is used by [`MultinomialNaiveBayes`] to validate that
/// `sum(feature_sums)` matches the cached `total` despite floating-point
/// accumulation order. Strict equality would reject legitimate round-off;
/// an unbounded tolerance would accept clearly inconsistent state.
///
/// - `atol` is the absolute floor: differences below it are always accepted.
/// - `rtol` scales with the magnitude of the larger value.
///
/// Either `a` or `b` being non-finite makes the comparison return `false`;
/// callers should still run [`ensure_finite`] separately to produce a
/// precise diagnostic.
fn approx_equal_non_negative(a: f64, b: f64, atol: f64, rtol: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let abs_diff = (a - b).abs();
    if abs_diff <= atol {
        return true;
    }
    let larger = a.abs().max(b.abs());
    abs_diff <= rtol * larger
}

/// Tolerances used to verify Multinomial NB cache consistency.
///
/// `total_false` / `total_true` cache the sum of the corresponding
/// `feature_sums_*` vector. Floating-point accumulation order can introduce
/// small discrepancies, so a strict equality check would reject legitimate
/// state. These tolerances are tight enough to reject grossly inconsistent
/// (e.g. off-by-orders-of-magnitude) malicious state while tolerating
/// normal round-off.
const MULTINOMIAL_TOTAL_ATOL: f64 = 1e-9;
const MULTINOMIAL_TOTAL_RTOL: f64 = 1e-6;

/// Validate that a log-domain value is admissible.
///
/// `-Infinity` is allowed (it represents probability 0 for an impossible
/// event, e.g. a class with zero training samples). `NaN` and
/// `+Infinity` are rejected because they indicate an arithmetic breakdown
/// (such as `-Infinity - (-Infinity)`) that would propagate into the final
/// probability as `NaN`.
fn ensure_log_domain(field: &'static str, value: f64) -> Result<(), RillError> {
    if value.is_nan() || (value.is_infinite() && value > 0.0) {
        return Err(RillError::NonFiniteValue { field, value });
    }
    Ok(())
}

/// Add two log-domain values, rejecting `NaN` and `+Infinity` results.
///
/// `-Infinity` is preserved (probability 0 + anything = probability 0),
/// matching [`ensure_log_domain`].
fn checked_log_add(current: f64, delta: f64, field: &'static str) -> Result<f64, RillError> {
    let value = current + delta;
    ensure_log_domain(field, value)?;
    Ok(value)
}

/// Validate that features are finite and non-negative (for Bernoulli/Multinomial).
fn validate_non_negative(feature_count: usize, features: &[f64]) -> Result<(), RillError> {
    validate_features(feature_count, features)?;
    for &x in features {
        if x < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "feature",
                value: x,
            });
        }
    }
    Ok(())
}

// ============================================================================
// Gaussian Naive Bayes
// ============================================================================

/// Per-class Gaussian statistics (Welford algorithm per feature).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
struct GaussianClassStats {
    counts: Vec<u64>,
    means: Vec<f64>,
    m2s: Vec<f64>,
    class_count: u64,
}

impl GaussianClassStats {
    fn new(feature_count: usize) -> Self {
        Self {
            counts: vec![0; feature_count],
            means: vec![0.0; feature_count],
            m2s: vec![0.0; feature_count],
            class_count: 0,
        }
    }

    fn variance(&self, idx: usize) -> f64 {
        if self.counts[idx] < 2 {
            0.0
        } else {
            self.m2s[idx] / self.counts[idx] as f64
        }
    }

    fn reset(&mut self) {
        self.counts.fill(0);
        self.means.fill(0.0);
        self.m2s.fill(0.0);
        self.class_count = 0;
    }

    /// Validate internal consistency of this class's statistics.
    ///
    /// Does not check that the per-feature vector lengths match the parent
    /// model's `feature_count`; that is the parent's responsibility.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn validate_invariants(&self) -> Result<(), RillError> {
        let n = self.counts.len();
        if self.means.len() != n || self.m2s.len() != n {
            return Err(RillError::InvalidState(
                "gaussian class stats: counts/means/m2s length mismatch".to_owned(),
            ));
        }
        // Gaussian NB updates every feature on each learn, so every
        // per-feature count must equal the class_count.
        for &c in &self.counts {
            if c != self.class_count {
                return Err(RillError::InvalidState(format!(
                    "gaussian class stats: feature count {c} != class_count {}",
                    self.class_count
                )));
            }
        }
        for &m in &self.means {
            ensure_finite("gaussian mean", m)?;
        }
        for &m2 in &self.m2s {
            ensure_finite("gaussian m2", m2)?;
            if m2 < 0.0 {
                return Err(RillError::InvalidState(format!(
                    "gaussian m2 must be non-negative, got {m2}"
                )));
            }
        }
        // count == 0: no samples seen → mean and m2 must be exactly 0.
        if self.class_count == 0 {
            for &m in &self.means {
                if m != 0.0 {
                    return Err(RillError::InvalidState(format!(
                        "gaussian class_count=0 but mean={m} (must be 0)"
                    )));
                }
            }
            for &m2 in &self.m2s {
                if m2 != 0.0 {
                    return Err(RillError::InvalidState(format!(
                        "gaussian class_count=0 but m2={m2} (must be 0)"
                    )));
                }
            }
        }
        // count == 1: Welford M2 is exactly 0 after a single sample.
        if self.class_count == 1 {
            for &m2 in &self.m2s {
                if m2 != 0.0 {
                    return Err(RillError::InvalidState(format!(
                        "gaussian class_count=1 but m2={m2} (must be 0)"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GaussianClassStats {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct State {
            counts: Vec<u64>,
            means: Vec<f64>,
            m2s: Vec<f64>,
            class_count: u64,
        }
        let s = State::deserialize(deserializer)?;
        let stats = GaussianClassStats {
            counts: s.counts,
            means: s.means,
            m2s: s.m2s,
            class_count: s.class_count,
        };
        stats
            .validate_invariants()
            .map_err(serde::de::Error::custom)?;
        Ok(stats)
    }
}

/// Online Gaussian Naive Bayes classifier.
///
/// Assumes features are conditionally independent given the class,
/// and each feature follows a Gaussian distribution per class.
/// Uses Welford's algorithm for numerically stable variance updates.
///
/// # Examples
///
/// ```
/// use rill_ml::models::GaussianNaiveBayes;
/// use rill_ml::OnlineBinaryClassifier;
///
/// let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
/// model.learn(&[1.0, 2.0], true).unwrap();
/// model.learn(&[-1.0, -2.0], false).unwrap();
/// let proba = model.predict_proba(&[0.5, 1.0]).unwrap();
/// assert!(proba > 0.0 && proba < 1.0);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GaussianNaiveBayes {
    feature_count: usize,
    config: NaiveBayesConfig,
    class_false: GaussianClassStats,
    class_true: GaussianClassStats,
    samples_seen: u64,
}

impl GaussianNaiveBayes {
    /// Create a new Gaussian Naive Bayes classifier.
    ///
    /// `feature_count` must be greater than zero.
    pub fn new(feature_count: usize, config: NaiveBayesConfig) -> Result<Self, RillError> {
        validate_config(&config)?;
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        Ok(Self {
            feature_count,
            config,
            class_false: GaussianClassStats::new(feature_count),
            class_true: GaussianClassStats::new(feature_count),
            samples_seen: 0,
        })
    }

    /// The Laplace smoothing parameter.
    pub const fn alpha(&self) -> f64 {
        self.config.alpha
    }

    /// Gaussian log probability density function.
    ///
    /// Returns `0.0` when `variance <= 0.0` (constant or unseen feature),
    /// otherwise the standard Gaussian log-density. The result is always
    /// in `(-∞, 0]` for admissible inputs; callers still validate it via
    /// [`ensure_log_domain`] to catch `NaN` from malicious state.
    fn gaussian_log_pdf(x: f64, mean: f64, variance: f64) -> f64 {
        if variance <= 0.0 {
            return 0.0;
        }
        let sigma = variance.sqrt();
        -0.5 * ((x - mean) / sigma).powi(2) - sigma.ln() - 0.5 * (2.0 * std::f64::consts::PI).ln()
    }

    /// Validate all invariants required for a deserialized model to be safe.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn validate_invariants(&self) -> Result<(), RillError> {
        if self.feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        validate_config(&self.config)?;
        // Per-feature vector lengths must match feature_count.
        if self.class_false.counts.len() != self.feature_count
            || self.class_false.means.len() != self.feature_count
            || self.class_false.m2s.len() != self.feature_count
        {
            return Err(RillError::InvalidState(
                "gaussian class_false vector length != feature_count".to_owned(),
            ));
        }
        if self.class_true.counts.len() != self.feature_count
            || self.class_true.means.len() != self.feature_count
            || self.class_true.m2s.len() != self.feature_count
        {
            return Err(RillError::InvalidState(
                "gaussian class_true vector length != feature_count".to_owned(),
            ));
        }
        self.class_false.validate_invariants()?;
        self.class_true.validate_invariants()?;
        let total_class = self
            .class_false
            .class_count
            .checked_add(self.class_true.class_count)
            .ok_or_else(|| {
                RillError::InvalidState(format!(
                    "gaussian class_false({}) + class_true({}) overflow",
                    self.class_false.class_count, self.class_true.class_count
                ))
            })?;
        if total_class != self.samples_seen {
            return Err(RillError::InvalidState(format!(
                "gaussian samples_seen={} != class_false({}) + class_true({})",
                self.samples_seen, self.class_false.class_count, self.class_true.class_count
            )));
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GaussianNaiveBayes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct State {
            feature_count: usize,
            config: NaiveBayesConfig,
            class_false: GaussianClassStats,
            class_true: GaussianClassStats,
            samples_seen: u64,
        }
        let s = State::deserialize(deserializer)?;
        let model = GaussianNaiveBayes {
            feature_count: s.feature_count,
            config: s.config,
            class_false: s.class_false,
            class_true: s.class_true,
            samples_seen: s.samples_seen,
        };
        model
            .validate_invariants()
            .map_err(serde::de::Error::custom)?;
        Ok(model)
    }
}

impl OnlineBinaryClassifier for GaussianNaiveBayes {
    fn feature_count(&self) -> usize {
        self.feature_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn predict_proba(&self, features: &[f64]) -> Result<f64, RillError> {
        validate_features(self.feature_count, features)?;

        if self.samples_seen == 0 {
            return Ok(0.5);
        }

        let count_true = self.class_true.class_count as f64;
        let count_false = self.class_false.class_count as f64;
        let total = count_true + count_false;

        let log_prior_true = (count_true / total).ln();
        ensure_log_domain("nb_gaussian_log_prior_true", log_prior_true)?;
        let log_prior_false = (count_false / total).ln();
        ensure_log_domain("nb_gaussian_log_prior_false", log_prior_false)?;

        let mut log_likelihood_true = 0.0;
        let mut log_likelihood_false = 0.0;

        for (i, &x) in features.iter().enumerate() {
            let ll_true =
                Self::gaussian_log_pdf(x, self.class_true.means[i], self.class_true.variance(i));
            ensure_log_domain("nb_gaussian_log_likelihood_true", ll_true)?;
            log_likelihood_true = checked_log_add(
                log_likelihood_true,
                ll_true,
                "nb_gaussian_log_likelihood_true",
            )?;
            let ll_false =
                Self::gaussian_log_pdf(x, self.class_false.means[i], self.class_false.variance(i));
            ensure_log_domain("nb_gaussian_log_likelihood_false", ll_false)?;
            log_likelihood_false = checked_log_add(
                log_likelihood_false,
                ll_false,
                "nb_gaussian_log_likelihood_false",
            )?;
        }

        let log_p_true = checked_log_add(
            log_prior_true,
            log_likelihood_true,
            "nb_gaussian_log_p_true",
        )?;
        let log_p_false = checked_log_add(
            log_prior_false,
            log_likelihood_false,
            "nb_gaussian_log_p_false",
        )?;

        let log_odds = log_p_true - log_p_false;
        // `log_odds` may be `±Infinity` (one class dominates) — sigmoid maps
        // those to 0.0 or 1.0, which is valid per the closed `[0, 1]` trait
        // contract. Only `NaN` (both sides `-Infinity`) is unrecoverable.
        if log_odds.is_nan() {
            return Err(RillError::NonFiniteValue {
                field: "nb_gaussian_log_odds",
                value: log_odds,
            });
        }

        let probability = sigmoid(log_odds);
        ensure_finite("nb_gaussian_probability", probability)?;
        if !(0.0..=1.0).contains(&probability) {
            return Err(RillError::InvalidProbability(probability));
        }
        Ok(probability)
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        validate_features(self.feature_count, features)?;

        // Phase 1: compute the next per-feature state without mutating self.
        // If any feature overflows or produces a non-finite value, the
        // caller's `Err` leaves the model untouched.
        let stats = if target {
            &self.class_true
        } else {
            &self.class_false
        };
        let mut next_states: Vec<(usize, u64, f64, f64)> = Vec::with_capacity(features.len());
        for (i, &x) in features.iter().enumerate() {
            let n = checked_increment(stats.counts[i], "feature count")?;
            let delta = x - stats.means[i];
            ensure_finite("mean delta", delta)?;
            let new_mean = checked_finite_add(stats.means[i], delta / n as f64, "mean")?;
            let delta2 = x - new_mean;
            ensure_finite("mean delta2", delta2)?;
            let new_m2 = checked_finite_add(stats.m2s[i], delta * delta2, "m2")?;
            next_states.push((i, n, new_mean, new_m2));
        }
        let new_class_count = checked_increment(stats.class_count, "class_count")?;
        let new_samples_seen = checked_increment(self.samples_seen, "samples_seen")?;

        // Phase 2: commit atomically.
        let stats = if target {
            &mut self.class_true
        } else {
            &mut self.class_false
        };
        for (i, n, new_mean, new_m2) in next_states {
            stats.counts[i] = n;
            stats.means[i] = new_mean;
            stats.m2s[i] = new_m2;
        }
        stats.class_count = new_class_count;
        self.samples_seen = new_samples_seen;
        Ok(())
    }

    fn reset(&mut self) {
        self.class_false.reset();
        self.class_true.reset();
        self.samples_seen = 0;
    }
}

// ============================================================================
// Bernoulli Naive Bayes
// ============================================================================

/// Online Bernoulli Naive Bayes classifier.
///
/// Designed for binary features (0 or 1). Uses Laplace smoothing
/// for probability estimation.
///
/// # Examples
///
/// ```
/// use rill_ml::models::BernoulliNaiveBayes;
/// use rill_ml::OnlineBinaryClassifier;
///
/// let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
/// model.learn(&[1.0, 0.0, 1.0], true).unwrap();
/// model.learn(&[0.0, 1.0, 0.0], false).unwrap();
/// let proba = model.predict_proba(&[1.0, 0.0, 1.0]).unwrap();
/// assert!(proba > 0.5);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BernoulliNaiveBayes {
    feature_count: usize,
    config: NaiveBayesConfig,
    feature_true_counts_false: Vec<u64>,
    feature_true_counts_true: Vec<u64>,
    class_false_count: u64,
    class_true_count: u64,
    samples_seen: u64,
}

impl BernoulliNaiveBayes {
    /// Create a new Bernoulli Naive Bayes classifier.
    ///
    /// `feature_count` must be greater than zero.
    pub fn new(feature_count: usize, config: NaiveBayesConfig) -> Result<Self, RillError> {
        validate_config(&config)?;
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        Ok(Self {
            feature_count,
            config,
            feature_true_counts_false: vec![0; feature_count],
            feature_true_counts_true: vec![0; feature_count],
            class_false_count: 0,
            class_true_count: 0,
            samples_seen: 0,
        })
    }

    /// Compute log P(x_i | class) for a single Bernoulli feature.
    fn log_bernoulli(x: f64, p: f64) -> f64 {
        x * p.ln() + (1.0 - x) * (1.0 - p).ln()
    }

    /// Validate all invariants required for a deserialized model to be safe.
    ///
    /// Bernoulli NB tracks, per class, how many training samples had each
    /// feature "present" (`x > 0.5`). Each per-feature count must be at most
    /// the corresponding class count, and `samples_seen` must equal the sum
    /// of the two class counts. Lengths must match `feature_count`.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn validate_invariants(&self) -> Result<(), RillError> {
        if self.feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        validate_config(&self.config)?;
        if self.feature_true_counts_false.len() != self.feature_count
            || self.feature_true_counts_true.len() != self.feature_count
        {
            return Err(RillError::InvalidState(
                "bernoulli feature count vector length != feature_count".to_owned(),
            ));
        }
        if let Some(total) = self.class_false_count.checked_add(self.class_true_count) {
            if total != self.samples_seen {
                return Err(RillError::InvalidState(format!(
                    "bernoulli samples_seen={} != class_false({}) + class_true({})",
                    self.samples_seen, self.class_false_count, self.class_true_count
                )));
            }
        } else {
            return Err(RillError::InvalidState(format!(
                "bernoulli class_false({}) + class_true({}) overflow",
                self.class_false_count, self.class_true_count
            )));
        }
        for &c in &self.feature_true_counts_false {
            if c > self.class_false_count {
                return Err(RillError::InvalidState(format!(
                    "bernoulli feature_true_counts_false entry {c} > class_false_count {}",
                    self.class_false_count
                )));
            }
        }
        for &c in &self.feature_true_counts_true {
            if c > self.class_true_count {
                return Err(RillError::InvalidState(format!(
                    "bernoulli feature_true_counts_true entry {c} > class_true_count {}",
                    self.class_true_count
                )));
            }
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for BernoulliNaiveBayes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct State {
            feature_count: usize,
            config: NaiveBayesConfig,
            feature_true_counts_false: Vec<u64>,
            feature_true_counts_true: Vec<u64>,
            class_false_count: u64,
            class_true_count: u64,
            samples_seen: u64,
        }
        let s = State::deserialize(deserializer)?;
        let model = BernoulliNaiveBayes {
            feature_count: s.feature_count,
            config: s.config,
            feature_true_counts_false: s.feature_true_counts_false,
            feature_true_counts_true: s.feature_true_counts_true,
            class_false_count: s.class_false_count,
            class_true_count: s.class_true_count,
            samples_seen: s.samples_seen,
        };
        model
            .validate_invariants()
            .map_err(serde::de::Error::custom)?;
        Ok(model)
    }
}

impl OnlineBinaryClassifier for BernoulliNaiveBayes {
    fn feature_count(&self) -> usize {
        self.feature_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn predict_proba(&self, features: &[f64]) -> Result<f64, RillError> {
        validate_non_negative(self.feature_count, features)?;

        if self.samples_seen == 0 {
            return Ok(0.5);
        }

        let count_true = self.class_true_count as f64;
        let count_false = self.class_false_count as f64;
        let total = count_true + count_false;

        let log_prior_true = (count_true / total).ln();
        ensure_log_domain("nb_bernoulli_log_prior_true", log_prior_true)?;
        let log_prior_false = (count_false / total).ln();
        ensure_log_domain("nb_bernoulli_log_prior_false", log_prior_false)?;

        let mut log_likelihood_true = 0.0;
        let mut log_likelihood_false = 0.0;

        for (i, &x) in features.iter().enumerate() {
            let p_true = (self.feature_true_counts_true[i] as f64 + self.config.alpha)
                / (count_true + 2.0 * self.config.alpha);
            let p_false = (self.feature_true_counts_false[i] as f64 + self.config.alpha)
                / (count_false + 2.0 * self.config.alpha);
            let ll_true = Self::log_bernoulli(x, p_true);
            ensure_log_domain("nb_bernoulli_log_likelihood_true", ll_true)?;
            log_likelihood_true = checked_log_add(
                log_likelihood_true,
                ll_true,
                "nb_bernoulli_log_likelihood_true",
            )?;
            let ll_false = Self::log_bernoulli(x, p_false);
            ensure_log_domain("nb_bernoulli_log_likelihood_false", ll_false)?;
            log_likelihood_false = checked_log_add(
                log_likelihood_false,
                ll_false,
                "nb_bernoulli_log_likelihood_false",
            )?;
        }

        let log_p_true = checked_log_add(
            log_prior_true,
            log_likelihood_true,
            "nb_bernoulli_log_p_true",
        )?;
        let log_p_false = checked_log_add(
            log_prior_false,
            log_likelihood_false,
            "nb_bernoulli_log_p_false",
        )?;

        let log_odds = log_p_true - log_p_false;
        // See `GaussianNaiveBayes::predict_proba`: `±Infinity` is valid
        // (sigmoid maps to 0/1), only `NaN` is unrecoverable.
        if log_odds.is_nan() {
            return Err(RillError::NonFiniteValue {
                field: "nb_bernoulli_log_odds",
                value: log_odds,
            });
        }

        let probability = sigmoid(log_odds);
        ensure_finite("nb_bernoulli_probability", probability)?;
        if !(0.0..=1.0).contains(&probability) {
            return Err(RillError::InvalidProbability(probability));
        }
        Ok(probability)
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        validate_non_negative(self.feature_count, features)?;

        // Phase 1: compute the next per-feature counts without mutating self.
        // If any counter overflows, the caller's `Err` leaves the model
        // untouched.
        let mut next_feature_counts = if target {
            self.feature_true_counts_true.clone()
        } else {
            self.feature_true_counts_false.clone()
        };
        for (i, &x) in features.iter().enumerate() {
            if x > 0.5 {
                next_feature_counts[i] =
                    checked_increment(next_feature_counts[i], "feature_true_count")?;
            }
        }
        let next_class_count = if target {
            checked_increment(self.class_true_count, "class_true_count")?
        } else {
            checked_increment(self.class_false_count, "class_false_count")?
        };
        let next_samples_seen = checked_increment(self.samples_seen, "samples_seen")?;

        // Phase 2: commit atomically.
        if target {
            self.feature_true_counts_true = next_feature_counts;
            self.class_true_count = next_class_count;
        } else {
            self.feature_true_counts_false = next_feature_counts;
            self.class_false_count = next_class_count;
        }
        self.samples_seen = next_samples_seen;
        Ok(())
    }

    fn reset(&mut self) {
        self.feature_true_counts_false.fill(0);
        self.feature_true_counts_true.fill(0);
        self.class_false_count = 0;
        self.class_true_count = 0;
        self.samples_seen = 0;
    }
}

// ============================================================================
// Multinomial Naive Bayes
// ============================================================================

/// Online Multinomial Naive Bayes classifier.
///
/// Designed for count features (non-negative values). Uses Laplace
/// smoothing. Commonly used for text classification with word counts.
///
/// # Examples
///
/// ```
/// use rill_ml::models::MultinomialNaiveBayes;
/// use rill_ml::OnlineBinaryClassifier;
///
/// let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
/// model.learn(&[2.0, 1.0, 0.0], true).unwrap();
/// model.learn(&[0.0, 1.0, 3.0], false).unwrap();
/// let proba = model.predict_proba(&[1.0, 1.0, 0.0]).unwrap();
/// assert!(proba > 0.0 && proba < 1.0);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MultinomialNaiveBayes {
    feature_count: usize,
    config: NaiveBayesConfig,
    feature_sums_false: Vec<f64>,
    feature_sums_true: Vec<f64>,
    total_false: f64,
    total_true: f64,
    class_false_count: u64,
    class_true_count: u64,
    samples_seen: u64,
}

impl MultinomialNaiveBayes {
    /// Create a new Multinomial Naive Bayes classifier.
    ///
    /// `feature_count` must be greater than zero.
    pub fn new(feature_count: usize, config: NaiveBayesConfig) -> Result<Self, RillError> {
        validate_config(&config)?;
        if feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        Ok(Self {
            feature_count,
            config,
            feature_sums_false: vec![0.0; feature_count],
            feature_sums_true: vec![0.0; feature_count],
            total_false: 0.0,
            total_true: 0.0,
            class_false_count: 0,
            class_true_count: 0,
            samples_seen: 0,
        })
    }

    /// Validate all invariants required for a deserialized model to be safe.
    ///
    /// Multinomial NB caches `total_false` / `total_true` as the sum of the
    /// corresponding `feature_sums_*` vector. Floating-point accumulation
    /// order can introduce small discrepancies, so the cache is checked
    /// against the vector sum with a tight combined absolute / relative
    /// tolerance (see [`MULTINOMIAL_TOTAL_ATOL`] / [`MULTINOMIAL_TOTAL_RTOL`]).
    /// Grossly inconsistent (e.g. off-by-orders-of-magnitude) state is still
    /// rejected.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    fn validate_invariants(&self) -> Result<(), RillError> {
        if self.feature_count == 0 {
            return Err(RillError::EmptyFeatures);
        }
        validate_config(&self.config)?;
        if self.feature_sums_false.len() != self.feature_count
            || self.feature_sums_true.len() != self.feature_count
        {
            return Err(RillError::InvalidState(
                "multinomial feature sum vector length != feature_count".to_owned(),
            ));
        }
        for &s in &self.feature_sums_false {
            ensure_finite("multinomial feature_sums_false", s)?;
            if s < 0.0 {
                return Err(RillError::InvalidState(format!(
                    "multinomial feature_sums_false entry {s} is negative"
                )));
            }
        }
        for &s in &self.feature_sums_true {
            ensure_finite("multinomial feature_sums_true", s)?;
            if s < 0.0 {
                return Err(RillError::InvalidState(format!(
                    "multinomial feature_sums_true entry {s} is negative"
                )));
            }
        }
        ensure_finite("multinomial total_false", self.total_false)?;
        if self.total_false < 0.0 {
            return Err(RillError::InvalidState(format!(
                "multinomial total_false is negative: {}",
                self.total_false
            )));
        }
        ensure_finite("multinomial total_true", self.total_true)?;
        if self.total_true < 0.0 {
            return Err(RillError::InvalidState(format!(
                "multinomial total_true is negative: {}",
                self.total_true
            )));
        }
        if let Some(total) = self.class_false_count.checked_add(self.class_true_count) {
            if total != self.samples_seen {
                return Err(RillError::InvalidState(format!(
                    "multinomial samples_seen={} != class_false({}) + class_true({})",
                    self.samples_seen, self.class_false_count, self.class_true_count
                )));
            }
        } else {
            return Err(RillError::InvalidState(format!(
                "multinomial class_false({}) + class_true({}) overflow",
                self.class_false_count, self.class_true_count
            )));
        }
        // Cache consistency: sum(feature_sums_*) must match total_* within
        // tolerance. Use Vec::iter().sum::<f64>() as an independent
        // accumulation order from the learn() hot path.
        let summed_false: f64 = self.feature_sums_false.iter().sum();
        let summed_true: f64 = self.feature_sums_true.iter().sum();
        if !approx_equal_non_negative(
            summed_false,
            self.total_false,
            MULTINOMIAL_TOTAL_ATOL,
            MULTINOMIAL_TOTAL_RTOL,
        ) {
            return Err(RillError::InvalidState(format!(
                "multinomial sum(feature_sums_false)={summed_false} mismatches total_false={} beyond tolerance",
                self.total_false
            )));
        }
        if !approx_equal_non_negative(
            summed_true,
            self.total_true,
            MULTINOMIAL_TOTAL_ATOL,
            MULTINOMIAL_TOTAL_RTOL,
        ) {
            return Err(RillError::InvalidState(format!(
                "multinomial sum(feature_sums_true)={summed_true} mismatches total_true={} beyond tolerance",
                self.total_true
            )));
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for MultinomialNaiveBayes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct State {
            feature_count: usize,
            config: NaiveBayesConfig,
            feature_sums_false: Vec<f64>,
            feature_sums_true: Vec<f64>,
            total_false: f64,
            total_true: f64,
            class_false_count: u64,
            class_true_count: u64,
            samples_seen: u64,
        }
        let s = State::deserialize(deserializer)?;
        let model = MultinomialNaiveBayes {
            feature_count: s.feature_count,
            config: s.config,
            feature_sums_false: s.feature_sums_false,
            feature_sums_true: s.feature_sums_true,
            total_false: s.total_false,
            total_true: s.total_true,
            class_false_count: s.class_false_count,
            class_true_count: s.class_true_count,
            samples_seen: s.samples_seen,
        };
        model
            .validate_invariants()
            .map_err(serde::de::Error::custom)?;
        Ok(model)
    }
}

impl OnlineBinaryClassifier for MultinomialNaiveBayes {
    fn feature_count(&self) -> usize {
        self.feature_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn predict_proba(&self, features: &[f64]) -> Result<f64, RillError> {
        validate_non_negative(self.feature_count, features)?;

        if self.samples_seen == 0 {
            return Ok(0.5);
        }

        let count_true = self.class_true_count as f64;
        let count_false = self.class_false_count as f64;
        let total = count_true + count_false;

        let log_prior_true = (count_true / total).ln();
        ensure_log_domain("nb_multinomial_log_prior_true", log_prior_true)?;
        let log_prior_false = (count_false / total).ln();
        ensure_log_domain("nb_multinomial_log_prior_false", log_prior_false)?;

        let denom_true = self.total_true + self.config.alpha * self.feature_count as f64;
        let denom_false = self.total_false + self.config.alpha * self.feature_count as f64;

        let mut log_likelihood_true = 0.0;
        let mut log_likelihood_false = 0.0;

        for (i, &x) in features.iter().enumerate() {
            let p_true = (self.feature_sums_true[i] + self.config.alpha) / denom_true;
            let p_false = (self.feature_sums_false[i] + self.config.alpha) / denom_false;
            let ll_true = x * p_true.ln();
            ensure_log_domain("nb_multinomial_log_likelihood_true", ll_true)?;
            log_likelihood_true = checked_log_add(
                log_likelihood_true,
                ll_true,
                "nb_multinomial_log_likelihood_true",
            )?;
            let ll_false = x * p_false.ln();
            ensure_log_domain("nb_multinomial_log_likelihood_false", ll_false)?;
            log_likelihood_false = checked_log_add(
                log_likelihood_false,
                ll_false,
                "nb_multinomial_log_likelihood_false",
            )?;
        }

        let log_p_true = checked_log_add(
            log_prior_true,
            log_likelihood_true,
            "nb_multinomial_log_p_true",
        )?;
        let log_p_false = checked_log_add(
            log_prior_false,
            log_likelihood_false,
            "nb_multinomial_log_p_false",
        )?;

        let log_odds = log_p_true - log_p_false;
        // See `GaussianNaiveBayes::predict_proba`: `±Infinity` is valid
        // (sigmoid maps to 0/1), only `NaN` is unrecoverable.
        if log_odds.is_nan() {
            return Err(RillError::NonFiniteValue {
                field: "nb_multinomial_log_odds",
                value: log_odds,
            });
        }

        let probability = sigmoid(log_odds);
        ensure_finite("nb_multinomial_probability", probability)?;
        if !(0.0..=1.0).contains(&probability) {
            return Err(RillError::InvalidProbability(probability));
        }
        Ok(probability)
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        validate_non_negative(self.feature_count, features)?;

        // Phase 1: compute the next per-feature sums and total without
        // mutating self. If any sum overflows, the caller's `Err` leaves the
        // model untouched.
        let mut next_feature_sums = if target {
            self.feature_sums_true.clone()
        } else {
            self.feature_sums_false.clone()
        };
        let mut next_total = if target {
            self.total_true
        } else {
            self.total_false
        };
        for (i, &x) in features.iter().enumerate() {
            next_feature_sums[i] = checked_finite_add(next_feature_sums[i], x, "feature_sum")?;
            next_total = checked_finite_add(next_total, x, "total")?;
        }
        let next_class_count = if target {
            checked_increment(self.class_true_count, "class_true_count")?
        } else {
            checked_increment(self.class_false_count, "class_false_count")?
        };
        let next_samples_seen = checked_increment(self.samples_seen, "samples_seen")?;

        // Phase 2: commit atomically.
        if target {
            self.feature_sums_true = next_feature_sums;
            self.total_true = next_total;
            self.class_true_count = next_class_count;
        } else {
            self.feature_sums_false = next_feature_sums;
            self.total_false = next_total;
            self.class_false_count = next_class_count;
        }
        self.samples_seen = next_samples_seen;
        Ok(())
    }

    fn reset(&mut self) {
        self.feature_sums_false.fill(0.0);
        self.feature_sums_true.fill(0.0);
        self.total_false = 0.0;
        self.total_true = 0.0;
        self.class_false_count = 0;
        self.class_true_count = 0;
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    // ====================
    // GaussianNaiveBayes
    // ====================

    #[test]
    fn gaussian_cold_start_returns_0_5() {
        let model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        let p = model.predict_proba(&[1.0, 2.0]).unwrap();
        assert!((p - 0.5).abs() < 1e-12);
    }

    #[test]
    fn gaussian_learn_separable_data() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for _ in 0..200 {
            let x1 = 2.0 + rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x2 = 2.0 + rand::Rng::gen_range(&mut rng, -1.0..1.0);
            model.learn(&[x1, x2], true).unwrap();
            let x1 = -2.0 + rand::Rng::gen_range(&mut rng, -1.0..1.0);
            let x2 = -2.0 + rand::Rng::gen_range(&mut rng, -1.0..1.0);
            model.learn(&[x1, x2], false).unwrap();
        }
        let p_pos = model.predict_proba(&[2.0, 2.0]).unwrap();
        let p_neg = model.predict_proba(&[-2.0, -2.0]).unwrap();
        assert!(p_pos > 0.7, "p_pos = {p_pos}");
        assert!(p_neg < 0.3, "p_neg = {p_neg}");
    }

    #[test]
    fn gaussian_dimension_mismatch_rejected() {
        let mut model = GaussianNaiveBayes::new(3, Default::default()).unwrap();
        assert!(model.predict_proba(&[1.0, 2.0]).is_err());
        assert!(model.learn(&[1.0, 2.0], true).is_err());
    }

    #[test]
    fn gaussian_non_finite_rejected() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        assert!(model.learn(&[f64::NAN, 1.0], true).is_err());
        assert!(model.learn(&[1.0, f64::INFINITY], true).is_err());
    }

    #[test]
    fn gaussian_reset_clears_state() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 2.0], true).unwrap();
        model.learn(&[-1.0, -2.0], false).unwrap();
        model.reset();
        assert_eq!(model.samples_seen(), 0);
        assert!((model.predict_proba(&[1.0, 2.0]).unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn gaussian_invalid_alpha_rejected() {
        assert!(GaussianNaiveBayes::new(2, NaiveBayesConfig { alpha: 0.0 }).is_err());
        assert!(GaussianNaiveBayes::new(2, NaiveBayesConfig { alpha: -1.0 }).is_err());
        assert!(GaussianNaiveBayes::new(2, NaiveBayesConfig { alpha: f64::NAN }).is_err());
    }

    #[test]
    fn gaussian_predict_does_not_update_state() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 2.0], true).unwrap();
        let before = model.samples_seen();
        let _ = model.predict_proba(&[0.5, 0.5]).unwrap();
        assert_eq!(model.samples_seen(), before);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_roundtrip() {
        let mut model = GaussianNaiveBayes::new(2, NaiveBayesConfig { alpha: 0.5 }).unwrap();
        model.learn(&[1.0, 2.0], true).unwrap();
        model.learn(&[1.5, 2.5], true).unwrap();
        model.learn(&[-1.0, -2.0], false).unwrap();
        model.learn(&[-1.5, -2.5], false).unwrap();
        let json = serde_json::to_string(&model).unwrap();
        let restored: GaussianNaiveBayes = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), model.samples_seen());
        assert_eq!(restored.feature_count(), model.feature_count());
        let p1 = model.predict_proba(&[0.5, 0.5]).unwrap();
        let p2 = restored.predict_proba(&[0.5, 0.5]).unwrap();
        assert!((p1 - p2).abs() < 1e-12);
    }

    #[test]
    fn gaussian_predict_proba_in_range() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 2.0], true).unwrap();
        model.learn(&[3.0, 4.0], true).unwrap();
        model.learn(&[-1.0, -2.0], false).unwrap();
        model.learn(&[-3.0, -4.0], false).unwrap();
        let p = model.predict_proba(&[0.5, 1.0]).unwrap();
        assert!(p > 0.0 && p < 1.0, "p = {p}");
    }

    #[test]
    fn gaussian_zero_features_rejected() {
        assert!(GaussianNaiveBayes::new(0, Default::default()).is_err());
    }

    #[test]
    fn gaussian_learns_gaussian_distribution() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);
        for _ in 0..500 {
            let x1 = 3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            let x2 = 3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            model.learn(&[x1, x2], true).unwrap();
            let x1 = -3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            let x2 = -3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            model.learn(&[x1, x2], false).unwrap();
        }
        let mut correct = 0;
        let total = 100;
        for _ in 0..total {
            let x1 = 3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            let x2 = 3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            if model.predict(&[x1, x2]).unwrap() {
                correct += 1;
            }
            let x1 = -3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            let x2 = -3.0 + 0.5 * rand::Rng::gen_range(&mut rng, -3.0..3.0);
            if !model.predict(&[x1, x2]).unwrap() {
                correct += 1;
            }
        }
        let accuracy = correct as f64 / (total * 2) as f64;
        assert!(accuracy > 0.95, "accuracy = {accuracy}");
    }

    #[test]
    fn gaussian_single_class_predicts_that_class() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 2.0], true).unwrap();
        model.learn(&[1.5, 2.5], true).unwrap();
        let p = model.predict_proba(&[1.0, 2.0]).unwrap();
        assert!((p - 1.0).abs() < 1e-12, "p = {p}");
    }

    // ====================
    // BernoulliNaiveBayes
    // ====================

    #[test]
    fn bernoulli_cold_start_returns_0_5() {
        let model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
        let p = model.predict_proba(&[1.0, 0.0, 1.0]).unwrap();
        assert!((p - 0.5).abs() < 1e-12);
    }

    #[test]
    fn bernoulli_learn_separable_data() {
        let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for _ in 0..200 {
            let f0 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.9 {
                1.0
            } else {
                0.0
            };
            let f1 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.1 {
                1.0
            } else {
                0.0
            };
            let f2 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.5 {
                1.0
            } else {
                0.0
            };
            model.learn(&[f0, f1, f2], true).unwrap();
            let f0 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.1 {
                1.0
            } else {
                0.0
            };
            let f1 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.9 {
                1.0
            } else {
                0.0
            };
            let f2 = if rand::Rng::gen_range(&mut rng, 0.0..1.0) < 0.5 {
                1.0
            } else {
                0.0
            };
            model.learn(&[f0, f1, f2], false).unwrap();
        }
        let p_pos = model.predict_proba(&[1.0, 0.0, 0.0]).unwrap();
        let p_neg = model.predict_proba(&[0.0, 1.0, 0.0]).unwrap();
        assert!(p_pos > 0.7, "p_pos = {p_pos}");
        assert!(p_neg < 0.3, "p_neg = {p_neg}");
    }

    #[test]
    fn bernoulli_dimension_mismatch_rejected() {
        let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
        assert!(model.predict_proba(&[1.0, 0.0]).is_err());
        assert!(model.learn(&[1.0, 0.0], true).is_err());
    }

    #[test]
    fn bernoulli_non_finite_rejected() {
        let mut model = BernoulliNaiveBayes::new(2, Default::default()).unwrap();
        assert!(model.learn(&[f64::NAN, 1.0], true).is_err());
        assert!(model.learn(&[1.0, f64::INFINITY], true).is_err());
    }

    #[test]
    fn bernoulli_reset_clears_state() {
        let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
        model.learn(&[1.0, 0.0, 1.0], true).unwrap();
        model.learn(&[0.0, 1.0, 0.0], false).unwrap();
        model.reset();
        assert_eq!(model.samples_seen(), 0);
        assert!((model.predict_proba(&[1.0, 0.0, 1.0]).unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn bernoulli_invalid_alpha_rejected() {
        assert!(BernoulliNaiveBayes::new(3, NaiveBayesConfig { alpha: 0.0 }).is_err());
        assert!(BernoulliNaiveBayes::new(3, NaiveBayesConfig { alpha: -1.0 }).is_err());
        assert!(BernoulliNaiveBayes::new(3, NaiveBayesConfig { alpha: f64::NAN }).is_err());
    }

    #[test]
    fn bernoulli_predict_does_not_update_state() {
        let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
        model.learn(&[1.0, 0.0, 1.0], true).unwrap();
        let before = model.samples_seen();
        let _ = model.predict_proba(&[1.0, 0.0, 1.0]).unwrap();
        assert_eq!(model.samples_seen(), before);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn bernoulli_serde_roundtrip() {
        let mut model = BernoulliNaiveBayes::new(3, NaiveBayesConfig { alpha: 0.5 }).unwrap();
        model.learn(&[1.0, 0.0, 1.0], true).unwrap();
        model.learn(&[0.0, 1.0, 0.0], false).unwrap();
        model.learn(&[1.0, 1.0, 0.0], true).unwrap();
        let json = serde_json::to_string(&model).unwrap();
        let restored: BernoulliNaiveBayes = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), model.samples_seen());
        assert_eq!(restored.feature_count(), model.feature_count());
        let p1 = model.predict_proba(&[1.0, 0.0, 1.0]).unwrap();
        let p2 = restored.predict_proba(&[1.0, 0.0, 1.0]).unwrap();
        assert!((p1 - p2).abs() < 1e-12);
    }

    #[test]
    fn bernoulli_predict_proba_in_range() {
        let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
        model.learn(&[1.0, 0.0, 1.0], true).unwrap();
        model.learn(&[0.0, 1.0, 0.0], false).unwrap();
        let p = model.predict_proba(&[1.0, 0.0, 1.0]).unwrap();
        assert!(p > 0.0 && p < 1.0, "p = {p}");
    }

    #[test]
    fn bernoulli_zero_features_rejected() {
        assert!(BernoulliNaiveBayes::new(0, Default::default()).is_err());
    }

    #[test]
    fn bernoulli_rejects_negative_values() {
        let mut model = BernoulliNaiveBayes::new(2, Default::default()).unwrap();
        assert!(model.learn(&[-1.0, 0.0], true).is_err());
        assert!(model.predict_proba(&[-0.5, 0.0]).is_err());
    }

    // ====================
    // MultinomialNaiveBayes
    // ====================

    #[test]
    fn multinomial_cold_start_returns_0_5() {
        let model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
        let p = model.predict_proba(&[1.0, 2.0, 3.0]).unwrap();
        assert!((p - 0.5).abs() < 1e-12);
    }

    #[test]
    fn multinomial_learn_separable_data() {
        let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for _ in 0..200 {
            let f0 = rand::Rng::gen_range(&mut rng, 3.0..6.0);
            let f1 = rand::Rng::gen_range(&mut rng, 2.0..5.0);
            let f2 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
            model.learn(&[f0, f1, f2], true).unwrap();
            let f0 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
            let f1 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
            let f2 = rand::Rng::gen_range(&mut rng, 3.0..6.0);
            model.learn(&[f0, f1, f2], false).unwrap();
        }
        let p_pos = model.predict_proba(&[4.0, 3.0, 0.0]).unwrap();
        let p_neg = model.predict_proba(&[0.0, 0.0, 4.0]).unwrap();
        assert!(p_pos > 0.7, "p_pos = {p_pos}");
        assert!(p_neg < 0.3, "p_neg = {p_neg}");
    }

    #[test]
    fn multinomial_dimension_mismatch_rejected() {
        let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
        assert!(model.predict_proba(&[1.0, 2.0]).is_err());
        assert!(model.learn(&[1.0, 2.0], true).is_err());
    }

    #[test]
    fn multinomial_non_finite_rejected() {
        let mut model = MultinomialNaiveBayes::new(2, Default::default()).unwrap();
        assert!(model.learn(&[f64::NAN, 1.0], true).is_err());
        assert!(model.learn(&[1.0, f64::INFINITY], true).is_err());
    }

    #[test]
    fn multinomial_reset_clears_state() {
        let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
        model.learn(&[2.0, 1.0, 0.0], true).unwrap();
        model.learn(&[0.0, 1.0, 3.0], false).unwrap();
        model.reset();
        assert_eq!(model.samples_seen(), 0);
        assert!((model.predict_proba(&[1.0, 1.0, 1.0]).unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn multinomial_invalid_alpha_rejected() {
        assert!(MultinomialNaiveBayes::new(3, NaiveBayesConfig { alpha: 0.0 }).is_err());
        assert!(MultinomialNaiveBayes::new(3, NaiveBayesConfig { alpha: -1.0 }).is_err());
        assert!(MultinomialNaiveBayes::new(3, NaiveBayesConfig { alpha: f64::NAN }).is_err());
    }

    #[test]
    fn multinomial_predict_does_not_update_state() {
        let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
        model.learn(&[2.0, 1.0, 0.0], true).unwrap();
        let before = model.samples_seen();
        let _ = model.predict_proba(&[1.0, 1.0, 0.0]).unwrap();
        assert_eq!(model.samples_seen(), before);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_serde_roundtrip() {
        let mut model = MultinomialNaiveBayes::new(3, NaiveBayesConfig { alpha: 0.5 }).unwrap();
        model.learn(&[2.0, 1.0, 0.0], true).unwrap();
        model.learn(&[0.0, 1.0, 3.0], false).unwrap();
        model.learn(&[1.0, 2.0, 1.0], true).unwrap();
        let json = serde_json::to_string(&model).unwrap();
        let restored: MultinomialNaiveBayes = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), model.samples_seen());
        assert_eq!(restored.feature_count(), model.feature_count());
        let p1 = model.predict_proba(&[1.0, 1.0, 0.0]).unwrap();
        let p2 = restored.predict_proba(&[1.0, 1.0, 0.0]).unwrap();
        assert!((p1 - p2).abs() < 1e-12);
    }

    #[test]
    fn multinomial_predict_proba_in_range() {
        let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
        model.learn(&[2.0, 1.0, 0.0], true).unwrap();
        model.learn(&[0.0, 1.0, 3.0], false).unwrap();
        let p = model.predict_proba(&[1.0, 1.0, 0.0]).unwrap();
        assert!(p > 0.0 && p < 1.0, "p = {p}");
    }

    #[test]
    fn multinomial_zero_features_rejected() {
        assert!(MultinomialNaiveBayes::new(0, Default::default()).is_err());
    }

    #[test]
    fn multinomial_rejects_negative_values() {
        let mut model = MultinomialNaiveBayes::new(2, Default::default()).unwrap();
        assert!(model.learn(&[-1.0, 0.0], true).is_err());
        assert!(model.predict_proba(&[-0.5, 0.0]).is_err());
    }

    #[test]
    fn multinomial_handles_all_zero_features() {
        let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
        model.learn(&[0.0, 0.0, 0.0], true).unwrap();
        model.learn(&[0.0, 0.0, 0.0], false).unwrap();
        let p = model.predict_proba(&[0.0, 0.0, 0.0]).unwrap();
        assert!((p - 0.5).abs() < 1e-12, "p = {p}");
    }

    // ====================
    // 4.4: probability finiteness
    // ====================

    #[test]
    fn gaussian_predict_proba_rejects_extreme_features_causing_nan_log_odds() {
        // Train a balanced two-class model where both classes have small but
        // non-zero variance. Predicting with an extreme finite feature makes
        // both log-likelihoods underflow to -Infinity, which would yield
        // `log_odds = -Inf - (-Inf) = NaN` without the finiteness guard.
        let mut model = GaussianNaiveBayes::new(1, Default::default()).unwrap();
        model.learn(&[1.0], true).unwrap();
        model.learn(&[2.0], true).unwrap();
        model.learn(&[1.0], false).unwrap();
        model.learn(&[2.0], false).unwrap();
        let result = model.predict_proba(&[1e200]);
        assert!(result.is_err(), "expected Err for NaN log_odds, got Ok");
    }

    #[test]
    fn bernoulli_predict_proba_rejects_extreme_features_causing_nan_log_odds() {
        // Train both classes so the feature is "present" most of the time,
        // giving p > 0.5 for both. `log_bernoulli(x, p)` simplifies to
        // `x * ln(p/(1-p)) + ln(1-p)`; with p > 0.5 the slope is positive,
        // so a huge `x` drives both log-likelihoods to +Infinity.
        // `log_odds = +Inf - (+Inf) = NaN` must be rejected.
        let mut model = BernoulliNaiveBayes::new(1, Default::default()).unwrap();
        for _ in 0..10 {
            model.learn(&[1.0], true).unwrap();
            model.learn(&[1.0], false).unwrap();
        }
        let result = model.predict_proba(&[1e308]);
        assert!(result.is_err(), "expected Err for NaN log_odds, got Ok");
    }

    #[test]
    fn multinomial_predict_proba_rejects_extreme_features_causing_nan_log_odds() {
        // Train with large totals so the Laplace-smoothed p for the
        // zero-sum feature is ≈ 1/total (very small). With x = 1e308,
        // `x * ln(p)` overflows to -Infinity for both classes, making
        // log_odds = -Inf - (-Inf) = NaN.
        let mut model = MultinomialNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1e10, 0.0], true).unwrap();
        model.learn(&[0.0, 1e10], false).unwrap();
        let result = model.predict_proba(&[1e308, 1e308]);
        assert!(result.is_err(), "expected Err for NaN log_odds, got Ok");
    }

    #[test]
    fn gaussian_single_class_predict_proba_returns_0_or_1() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 2.0], true).unwrap();
        model.learn(&[1.5, 2.5], true).unwrap();
        let p = model.predict_proba(&[1.0, 2.0]).unwrap();
        assert!((p - 1.0).abs() < 1e-12, "p = {p}");
    }

    #[test]
    fn bernoulli_single_class_predict_proba_returns_0_or_1() {
        let mut model = BernoulliNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 0.0], true).unwrap();
        model.learn(&[0.0, 1.0], true).unwrap();
        let p = model.predict_proba(&[1.0, 0.0]).unwrap();
        assert!((p - 1.0).abs() < 1e-12, "p = {p}");
    }

    #[test]
    fn multinomial_single_class_predict_proba_returns_0_or_1() {
        let mut model = MultinomialNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 0.0], true).unwrap();
        model.learn(&[0.0, 1.0], true).unwrap();
        let p = model.predict_proba(&[1.0, 0.0]).unwrap();
        assert!((p - 1.0).abs() < 1e-12, "p = {p}");
    }

    #[test]
    fn gaussian_predict_proba_does_not_modify_state_on_error() {
        let mut model = GaussianNaiveBayes::new(1, Default::default()).unwrap();
        model.learn(&[1.0], true).unwrap();
        model.learn(&[2.0], true).unwrap();
        model.learn(&[1.0], false).unwrap();
        model.learn(&[2.0], false).unwrap();
        let before = model.samples_seen();
        let _ = model.predict_proba(&[1e200]);
        assert_eq!(model.samples_seen(), before);
    }

    // ====================
    // 4.5: failure atomicity
    // ====================

    #[test]
    fn gaussian_learn_failure_leaves_state_unchanged() {
        // Train one sample so feature 1 has non-zero mean; the second
        // learn call computes a huge m2 for feature 1 (overflow to Inf),
        // but feature 0's next state is valid. The old code would commit
        // feature 0 before feature 1 fails; the new code must reject the
        // entire call.
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 1.0], true).unwrap();
        let before_samples = model.samples_seen();
        let before_class_count = model.class_true.class_count;

        let result = model.learn(&[2.0, 1e200], true);
        assert!(result.is_err(), "expected overflow error");

        // State must be unchanged.
        assert_eq!(model.samples_seen(), before_samples);
        assert_eq!(model.class_true.class_count, before_class_count);
        assert_eq!(model.class_true.counts, vec![1, 1]);
        assert_eq!(model.class_true.means, vec![1.0, 1.0]);
        assert_eq!(model.class_true.m2s, vec![0.0, 0.0]);
    }

    #[test]
    fn gaussian_learn_succeeds_after_failed_attempt() {
        let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
        model.learn(&[1.0, 1.0], true).unwrap();
        // Failed call (overflow) must not corrupt state.
        let _ = model.learn(&[2.0, 1e200], true);
        // Subsequent valid call must work.
        model.learn(&[2.0, 3.0], true).unwrap();
        assert_eq!(model.samples_seen(), 2);
    }

    #[test]
    fn multinomial_learn_failure_leaves_state_unchanged() {
        // With a fresh model, learn([1e308, 1e308]) causes total to overflow
        // to Inf on the second feature. The old code commits feature 0 and
        // the first feature's contribution to total before failing; the new
        // code must reject the entire call.
        let mut model = MultinomialNaiveBayes::new(2, Default::default()).unwrap();
        let before_samples = model.samples_seen();

        let result = model.learn(&[1e308, 1e308], true);
        assert!(result.is_err(), "expected overflow error");

        // State must be unchanged.
        assert_eq!(model.samples_seen(), before_samples);
        assert_eq!(model.class_true_count, 0);
        assert_eq!(model.feature_sums_true, vec![0.0, 0.0]);
        assert_eq!(model.total_true, 0.0);
    }

    #[test]
    fn multinomial_learn_succeeds_after_failed_attempt() {
        let mut model = MultinomialNaiveBayes::new(2, Default::default()).unwrap();
        let _ = model.learn(&[1e308, 1e308], true);
        // Subsequent valid call must work.
        model.learn(&[1.0, 2.0], true).unwrap();
        assert_eq!(model.samples_seen(), 1);
        assert_eq!(model.feature_sums_true, vec![1.0, 2.0]);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn bernoulli_learn_failure_leaves_state_unchanged() {
        // Construct a model where feature 1's count is at u64::MAX via serde.
        // A learn call that tries to increment both features must fail on
        // feature 1 without modifying feature 0. The class_count must be
        // at least as large as the per-feature count to satisfy the
        // deserialization invariants (`feature_true_counts_true[i] <=
        // class_true_count`), so `class_true_count` and `samples_seen`
        // sit at u64::MAX as well.
        let json = r#"{
            "feature_count": 2,
            "config": {"alpha": 1.0},
            "feature_true_counts_false": [0, 0],
            "feature_true_counts_true": [0, 18446744073709551615],
            "class_false_count": 0,
            "class_true_count": 18446744073709551615,
            "samples_seen": 18446744073709551615
        }"#;
        let mut model: BernoulliNaiveBayes = serde_json::from_str(json).unwrap();
        let before_samples = model.samples_seen();

        let result = model.learn(&[1.0, 1.0], true);
        assert!(result.is_err(), "expected counter overflow");

        // State must be unchanged.
        assert_eq!(model.samples_seen(), before_samples);
        assert_eq!(model.feature_true_counts_true, vec![0u64, u64::MAX]);
        assert_eq!(model.class_true_count, u64::MAX);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_learn_failure_on_samples_seen_overflow_leaves_state_unchanged() {
        // Construct a valid model where the per-class counts are one below
        // the overflow boundary and `samples_seen` already sits at u64::MAX.
        // A successful feature update and class_count increment must not be
        // committed when `samples_seen` overflows.
        //
        // The Gaussian invariant requires every per-feature count to equal
        // `class_count`, so the original "class_count = u64::MAX, counts = 1"
        // fixture is no longer admissible. This repurposed fixture instead
        // pins `class_true.counts[0] == class_true.class_count == u64::MAX-1`
        // and `samples_seen = u64::MAX` so that the feature and class_count
        // increments both succeed but the samples_seen increment overflows.
        let max_minus_one = u64::MAX - 1;
        let json = format!(
            r#"{{
                "feature_count": 1,
                "config": {{"alpha": 1.0}},
                "class_false": {{
                    "counts": [1],
                    "means": [3.0],
                    "m2s": [0.0],
                    "class_count": 1
                }},
                "class_true": {{
                    "counts": [{max_minus_one}],
                    "means": [5.0],
                    "m2s": [0.0],
                    "class_count": {max_minus_one}
                }},
                "samples_seen": 18446744073709551615
            }}"#
        );
        let mut model: GaussianNaiveBayes = serde_json::from_str(&json).unwrap();
        let before_samples = model.samples_seen();

        let result = model.learn(&[6.0], true);
        assert!(result.is_err(), "expected samples_seen overflow");

        // State must be unchanged — feature 0's next state and the new
        // class_count were computed but not committed because samples_seen
        // overflowed.
        assert_eq!(model.samples_seen(), before_samples);
        assert_eq!(model.class_true.counts, vec![max_minus_one]);
        assert_eq!(model.class_true.means, vec![5.0]);
        assert_eq!(model.class_true.m2s, vec![0.0]);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_learn_failure_on_class_count_overflow_leaves_state_unchanged() {
        // `class_true_count` sits at u64::MAX so the class_count increment
        // overflows. The Multinomial invariants do not couple feature sums
        // to `class_true_count`, so `class_true_count = u64::MAX` is
        // admissible as long as `samples_seen` matches (here both equal
        // u64::MAX, since `class_false_count = 0`).
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "feature_sums_false": [0.0],
            "feature_sums_true": [5.0],
            "total_false": 0.0,
            "total_true": 5.0,
            "class_false_count": 0,
            "class_true_count": 18446744073709551615,
            "samples_seen": 18446744073709551615
        }"#;
        let mut model: MultinomialNaiveBayes = serde_json::from_str(json).unwrap();
        let before_samples = model.samples_seen();

        let result = model.learn(&[3.0], true);
        assert!(result.is_err(), "expected class_count overflow");

        assert_eq!(model.samples_seen(), before_samples);
        assert_eq!(model.feature_sums_true, vec![5.0]);
        assert_eq!(model.total_true, 5.0);
        assert_eq!(model.class_true_count, u64::MAX);
    }

    // ====================================================================
    // §6.1: Naive Bayes serde trust-boundary validation
    // ====================================================================

    #[test]
    fn approx_equal_non_negative_tolerances() {
        // Exact equality.
        assert!(approx_equal_non_negative(0.0, 0.0, 1e-9, 1e-6));
        assert!(approx_equal_non_negative(5.0, 5.0, 1e-9, 1e-6));
        // Absolute tolerance floor.
        assert!(approx_equal_non_negative(0.0, 1e-10, 1e-9, 1e-6));
        assert!(!approx_equal_non_negative(0.0, 1e-8, 1e-9, 1e-6));
        // Relative tolerance scales with magnitude.
        assert!(approx_equal_non_negative(
            1_000_000.0,
            1_000_000.5,
            1e-9,
            1e-6
        ));
        assert!(!approx_equal_non_negative(
            1_000_000.0,
            1_000_002.0,
            1e-9,
            1e-6
        ));
        // Non-finite inputs always compare false.
        assert!(!approx_equal_non_negative(f64::NAN, 0.0, 1e-9, 1e-6));
        assert!(!approx_equal_non_negative(
            f64::INFINITY,
            f64::INFINITY,
            1e-9,
            1e-6
        ));
        // Order-of-magnitude mismatch must be rejected.
        assert!(!approx_equal_non_negative(1.0, 1e6, 1e-9, 1e-6));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_feature_vector_length_mismatch() {
        // class_true vectors have length 1 while feature_count = 2.
        let json = r#"{
            "feature_count": 2,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [0, 0],
                "means": [0.0, 0.0],
                "m2s": [0.0, 0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "samples_seen": 0
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "feature vector length mismatch must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_negative_m2() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [2],
                "means": [5.0],
                "m2s": [-1.0],
                "class_count": 2
            },
            "samples_seen": 2
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "negative m2 must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_count_zero_with_nonzero_state() {
        // class_count == 0 but means/m2s are non-zero: this state can never
        // arise from a legitimate learn() path.
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [0],
                "means": [7.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "samples_seen": 0
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "class_count=0 with non-zero mean must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_count_one_with_nonzero_m2() {
        // After a single Welford update, M2 is exactly 0; a non-zero M2 at
        // count == 1 indicates a corrupted or malicious payload.
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [1],
                "means": [5.0],
                "m2s": [0.25],
                "class_count": 1
            },
            "samples_seen": 1
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "class_count=1 with non-zero m2 must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_samples_seen_mismatch() {
        // class_false.class_count + class_true.class_count = 1 + 1 = 2,
        // but samples_seen = 5.
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [1],
                "means": [3.0],
                "m2s": [0.0],
                "class_count": 1
            },
            "class_true": {
                "counts": [1],
                "means": [5.0],
                "m2s": [0.0],
                "class_count": 1
            },
            "samples_seen": 5
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "samples_seen mismatch must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_feature_count_not_equal_class_count() {
        // Gaussian NB updates every feature on each learn, so per-feature
        // counts must equal class_count. Here counts[0] = 3 but class_count
        // = 1.
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [3],
                "means": [5.0],
                "m2s": [0.5],
                "class_count": 1
            },
            "samples_seen": 1
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "per-feature count != class_count must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_invalid_alpha() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 0.0},
            "class_false": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "samples_seen": 0
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "invalid alpha must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_non_finite_state() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [1],
                "means": [NaN],
                "m2s": [0.0],
                "class_count": 1
            },
            "samples_seen": 1
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "non-finite mean must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_malicious_state_without_panic() {
        // A payload that, if accepted, would index out of bounds during
        // predict(). Validation must reject it cleanly instead of panicking.
        let json = r#"{
            "feature_count": 3,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "class_true": {
                "counts": [0],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 0
            },
            "samples_seen": 0
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "malicious length-mismatched state must be rejected, not panicked on"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn bernoulli_serde_rejects_feature_count_above_class_count() {
        // feature_true_counts_true[0] = 5 but class_true_count = 1: the
        // per-feature count cannot exceed the number of training samples
        // for that class.
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "feature_true_counts_false": [0],
            "feature_true_counts_true": [5],
            "class_false_count": 0,
            "class_true_count": 1,
            "samples_seen": 1
        }"#;
        let result: Result<BernoulliNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "feature count > class_count must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn bernoulli_serde_rejects_samples_seen_mismatch() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "feature_true_counts_false": [0],
            "feature_true_counts_true": [0],
            "class_false_count": 1,
            "class_true_count": 1,
            "samples_seen": 5
        }"#;
        let result: Result<BernoulliNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "samples_seen mismatch must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn bernoulli_serde_rejects_feature_vector_length_mismatch() {
        let json = r#"{
            "feature_count": 2,
            "config": {"alpha": 1.0},
            "feature_true_counts_false": [0],
            "feature_true_counts_true": [0, 0],
            "class_false_count": 0,
            "class_true_count": 0,
            "samples_seen": 0
        }"#;
        let result: Result<BernoulliNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "feature vector length mismatch must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn bernoulli_serde_rejects_invalid_alpha() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": -1.0},
            "feature_true_counts_false": [0],
            "feature_true_counts_true": [0],
            "class_false_count": 0,
            "class_true_count": 0,
            "samples_seen": 0
        }"#;
        let result: Result<BernoulliNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "invalid alpha must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_serde_rejects_negative_feature_sum() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "feature_sums_false": [0.0],
            "feature_sums_true": [-3.0],
            "total_false": 0.0,
            "total_true": -3.0,
            "class_false_count": 0,
            "class_true_count": 1,
            "samples_seen": 1
        }"#;
        let result: Result<MultinomialNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "negative feature sum must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_serde_rejects_total_mismatch() {
        // sum(feature_sums_true) = 1.0 + 2.0 = 3.0 but total_true = 100.0.
        // The discrepancy is orders of magnitude beyond tolerance.
        let json = r#"{
            "feature_count": 2,
            "config": {"alpha": 1.0},
            "feature_sums_false": [0.0, 0.0],
            "feature_sums_true": [1.0, 2.0],
            "total_false": 0.0,
            "total_true": 100.0,
            "class_false_count": 0,
            "class_true_count": 1,
            "samples_seen": 1
        }"#;
        let result: Result<MultinomialNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "total mismatch beyond tolerance must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_serde_rejects_samples_seen_mismatch() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "feature_sums_false": [0.0],
            "feature_sums_true": [0.0],
            "total_false": 0.0,
            "total_true": 0.0,
            "class_false_count": 1,
            "class_true_count": 1,
            "samples_seen": 5
        }"#;
        let result: Result<MultinomialNaiveBayes, _> = serde_json::from_str(json);
        assert!(result.is_err(), "samples_seen mismatch must be rejected");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_serde_accepts_tiny_total_roundoff() {
        // Floating-point accumulation can introduce tiny round-off between
        // the cached total and the recomputed sum. The tolerance must
        // accept legitimate round-off (here ~1e-12 on a total of ~3.0).
        let json = r#"{
            "feature_count": 2,
            "config": {"alpha": 1.0},
            "feature_sums_false": [0.0, 0.0],
            "feature_sums_true": [1.0, 2.0],
            "total_false": 0.0,
            "total_true": 3.000000000001,
            "class_false_count": 0,
            "class_true_count": 1,
            "samples_seen": 1
        }"#;
        let model: MultinomialNaiveBayes = serde_json::from_str(json).unwrap();
        assert_eq!(model.samples_seen(), 1);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_serde_rejects_feature_vector_length_mismatch() {
        let json = r#"{
            "feature_count": 2,
            "config": {"alpha": 1.0},
            "feature_sums_false": [0.0, 0.0],
            "feature_sums_true": [0.0],
            "total_false": 0.0,
            "total_true": 0.0,
            "class_false_count": 0,
            "class_true_count": 0,
            "samples_seen": 0
        }"#;
        let result: Result<MultinomialNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "feature vector length mismatch must be rejected"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn gaussian_serde_rejects_class_count_overflow() {
        // Two class counts near u64::MAX must not panic on addition;
        // the validator must reject via checked_add overflow path.
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "class_false": {
                "counts": [1],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 18446744073709551615
            },
            "class_true": {
                "counts": [1],
                "means": [0.0],
                "m2s": [0.0],
                "class_count": 18446744073709551615
            },
            "samples_seen": 18446744073709551614
        }"#;
        let result: Result<GaussianNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "u64 overflow on class_count sum must be rejected, not panic"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn bernoulli_serde_rejects_class_count_overflow() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "feature_true_counts_false": [1],
            "feature_true_counts_true": [1],
            "class_false_count": 18446744073709551615,
            "class_true_count": 18446744073709551615,
            "samples_seen": 18446744073709551614
        }"#;
        let result: Result<BernoulliNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "u64 overflow on class_count sum must be rejected, not panic"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn multinomial_serde_rejects_class_count_overflow() {
        let json = r#"{
            "feature_count": 1,
            "config": {"alpha": 1.0},
            "feature_sums_false": [1.0],
            "feature_sums_true": [1.0],
            "total_false": 1.0,
            "total_true": 1.0,
            "class_false_count": 18446744073709551615,
            "class_true_count": 18446744073709551615,
            "samples_seen": 18446744073709551614
        }"#;
        let result: Result<MultinomialNaiveBayes, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "u64 overflow on class_count sum must be rejected, not panic"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn naive_bayes_valid_roundtrip_preserves_prediction() {
        // Round-trip each variant through serde and confirm that predictions
        // on a held-out feature vector are bit-for-bit identical. This guards
        // against validation logic that is so strict it rejects legitimate
        // trained state, or so loose it alters the prediction path.
        let probe = [0.5, 1.0, 0.0];

        let mut g = GaussianNaiveBayes::new(3, NaiveBayesConfig { alpha: 0.5 }).unwrap();
        g.learn(&[1.0, 2.0, 0.0], true).unwrap();
        g.learn(&[1.5, 2.5, 1.0], true).unwrap();
        g.learn(&[-1.0, -2.0, 0.0], false).unwrap();
        g.learn(&[-1.5, -2.5, 1.0], false).unwrap();
        let g_json = serde_json::to_string(&g).unwrap();
        let g_restored: GaussianNaiveBayes = serde_json::from_str(&g_json).unwrap();
        assert_eq!(g_restored.samples_seen(), g.samples_seen());
        assert_eq!(g_restored.feature_count(), g.feature_count());
        let g_p1 = g.predict_proba(&probe).unwrap();
        let g_p2 = g_restored.predict_proba(&probe).unwrap();
        assert!((g_p1 - g_p2).abs() < 1e-12);

        let mut b = BernoulliNaiveBayes::new(3, NaiveBayesConfig { alpha: 0.5 }).unwrap();
        b.learn(&[1.0, 0.0, 1.0], true).unwrap();
        b.learn(&[0.0, 1.0, 0.0], false).unwrap();
        b.learn(&[1.0, 1.0, 0.0], true).unwrap();
        let b_json = serde_json::to_string(&b).unwrap();
        let b_restored: BernoulliNaiveBayes = serde_json::from_str(&b_json).unwrap();
        assert_eq!(b_restored.samples_seen(), b.samples_seen());
        assert_eq!(b_restored.feature_count(), b.feature_count());
        let b_p1 = b.predict_proba(&probe).unwrap();
        let b_p2 = b_restored.predict_proba(&probe).unwrap();
        assert!((b_p1 - b_p2).abs() < 1e-12);

        let mut m = MultinomialNaiveBayes::new(3, NaiveBayesConfig { alpha: 0.5 }).unwrap();
        m.learn(&[2.0, 1.0, 0.0], true).unwrap();
        m.learn(&[0.0, 1.0, 3.0], false).unwrap();
        m.learn(&[1.0, 2.0, 1.0], true).unwrap();
        let m_json = serde_json::to_string(&m).unwrap();
        let m_restored: MultinomialNaiveBayes = serde_json::from_str(&m_json).unwrap();
        assert_eq!(m_restored.samples_seen(), m.samples_seen());
        assert_eq!(m_restored.feature_count(), m.feature_count());
        let m_p1 = m.predict_proba(&probe).unwrap();
        let m_p2 = m_restored.predict_proba(&probe).unwrap();
        assert!((m_p1 - m_p2).abs() < 1e-12);
    }
}

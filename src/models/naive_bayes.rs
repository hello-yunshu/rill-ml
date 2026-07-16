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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

    fn update_feature(&mut self, idx: usize, value: f64) -> Result<(), RillError> {
        let n = checked_increment(self.counts[idx], "feature count")?;
        self.counts[idx] = n;
        let delta = value - self.means[idx];
        ensure_finite("mean delta", delta)?;
        self.means[idx] = checked_finite_add(self.means[idx], delta / n as f64, "mean")?;
        let delta2 = value - self.means[idx];
        ensure_finite("mean delta2", delta2)?;
        self.m2s[idx] = checked_finite_add(self.m2s[idx], delta * delta2, "m2")?;
        Ok(())
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    fn gaussian_log_pdf(x: f64, mean: f64, variance: f64) -> f64 {
        if variance <= 0.0 {
            return 0.0;
        }
        let sigma = variance.sqrt();
        -0.5 * ((x - mean) / sigma).powi(2) - sigma.ln() - 0.5 * (2.0 * std::f64::consts::PI).ln()
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
        let log_prior_false = (count_false / total).ln();

        let mut log_likelihood_true = 0.0;
        let mut log_likelihood_false = 0.0;

        for (i, &x) in features.iter().enumerate() {
            log_likelihood_true +=
                Self::gaussian_log_pdf(x, self.class_true.means[i], self.class_true.variance(i));
            log_likelihood_false +=
                Self::gaussian_log_pdf(x, self.class_false.means[i], self.class_false.variance(i));
        }

        let log_p_true = log_prior_true + log_likelihood_true;
        let log_p_false = log_prior_false + log_likelihood_false;

        let log_odds = log_p_true - log_p_false;
        Ok(sigmoid(log_odds).clamp(f64::EPSILON, 1.0 - f64::EPSILON))
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        validate_features(self.feature_count, features)?;
        let stats = if target {
            &mut self.class_true
        } else {
            &mut self.class_false
        };
        for (i, &x) in features.iter().enumerate() {
            stats.update_feature(i, x)?;
        }
        stats.class_count = checked_increment(stats.class_count, "class_count")?;
        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
        let log_prior_false = (count_false / total).ln();

        let mut log_likelihood_true = 0.0;
        let mut log_likelihood_false = 0.0;

        for (i, &x) in features.iter().enumerate() {
            let p_true = (self.feature_true_counts_true[i] as f64 + self.config.alpha)
                / (count_true + 2.0 * self.config.alpha);
            let p_false = (self.feature_true_counts_false[i] as f64 + self.config.alpha)
                / (count_false + 2.0 * self.config.alpha);
            log_likelihood_true += Self::log_bernoulli(x, p_true);
            log_likelihood_false += Self::log_bernoulli(x, p_false);
        }

        let log_p_true = log_prior_true + log_likelihood_true;
        let log_p_false = log_prior_false + log_likelihood_false;

        let log_odds = log_p_true - log_p_false;
        Ok(sigmoid(log_odds).clamp(f64::EPSILON, 1.0 - f64::EPSILON))
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        validate_non_negative(self.feature_count, features)?;
        if target {
            for (i, &x) in features.iter().enumerate() {
                if x > 0.5 {
                    self.feature_true_counts_true[i] =
                        checked_increment(self.feature_true_counts_true[i], "feature_true_count")?;
                }
            }
            self.class_true_count = checked_increment(self.class_true_count, "class_true_count")?;
        } else {
            for (i, &x) in features.iter().enumerate() {
                if x > 0.5 {
                    self.feature_true_counts_false[i] =
                        checked_increment(self.feature_true_counts_false[i], "feature_true_count")?;
                }
            }
            self.class_false_count =
                checked_increment(self.class_false_count, "class_false_count")?;
        }
        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
        let log_prior_false = (count_false / total).ln();

        let denom_true = self.total_true + self.config.alpha * self.feature_count as f64;
        let denom_false = self.total_false + self.config.alpha * self.feature_count as f64;

        let mut log_likelihood_true = 0.0;
        let mut log_likelihood_false = 0.0;

        for (i, &x) in features.iter().enumerate() {
            let p_true = (self.feature_sums_true[i] + self.config.alpha) / denom_true;
            let p_false = (self.feature_sums_false[i] + self.config.alpha) / denom_false;
            log_likelihood_true += x * p_true.ln();
            log_likelihood_false += x * p_false.ln();
        }

        let log_p_true = log_prior_true + log_likelihood_true;
        let log_p_false = log_prior_false + log_likelihood_false;

        let log_odds = log_p_true - log_p_false;
        Ok(sigmoid(log_odds).clamp(f64::EPSILON, 1.0 - f64::EPSILON))
    }

    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError> {
        validate_non_negative(self.feature_count, features)?;
        if target {
            for (i, &x) in features.iter().enumerate() {
                self.feature_sums_true[i] =
                    checked_finite_add(self.feature_sums_true[i], x, "feature_sum")?;
                self.total_true = checked_finite_add(self.total_true, x, "total")?;
            }
            self.class_true_count = checked_increment(self.class_true_count, "class_true_count")?;
        } else {
            for (i, &x) in features.iter().enumerate() {
                self.feature_sums_false[i] =
                    checked_finite_add(self.feature_sums_false[i], x, "feature_sum")?;
                self.total_false = checked_finite_add(self.total_false, x, "total")?;
            }
            self.class_false_count =
                checked_increment(self.class_false_count, "class_false_count")?;
        }
        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
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
}

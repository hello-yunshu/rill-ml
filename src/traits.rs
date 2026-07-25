//! Core traits shared across RillML.
//!
//! These traits are intentionally small and concrete. RillML avoids heavy
//! trait-object based polymorphism in favor of concrete, serializable types
//! for optimizers and losses.

use crate::error::RillError;
use crate::sparse::SparseFeatures;

/// An online regressor that produces real-valued predictions.
///
/// Implementations must keep `predict` side-effect free: calling `predict`
/// must never update internal state. State updates happen exclusively in
/// [`learn`](Self::learn).
pub trait OnlineRegressor {
    /// The number of features the model expects.
    fn feature_count(&self) -> usize;

    /// How many training samples the model has seen so far.
    fn samples_seen(&self) -> u64;

    /// Predict the target for the given feature slice.
    ///
    /// This method must not modify the model. If the feature dimension does
    /// not match [`feature_count`](Self::feature_count) or the values are
    /// not finite, an error is returned.
    fn predict(&self, features: &[f64]) -> Result<f64, RillError>;

    /// Update the model using a single labeled sample.
    fn learn(&mut self, features: &[f64], target: f64) -> Result<(), RillError>;

    /// Reset the model to its initial state, as if no samples had been seen.
    fn reset(&mut self);
}

/// An online binary classifier that produces a probability in `[0, 1]`.
///
/// `predict` (and `predict_proba`) must be side-effect free. The output
/// range is closed `[0, 1]` because floating-point sigmoid can return
/// exactly `0.0` or `1.0` for extreme logits; consumers that need an
/// open interval (e.g. log-loss) must clip internally.
pub trait OnlineBinaryClassifier {
    /// The number of features the model expects.
    fn feature_count(&self) -> usize;

    /// How many training samples the model has seen so far.
    fn samples_seen(&self) -> u64;

    /// Predict the probability of the positive class.
    fn predict_proba(&self, features: &[f64]) -> Result<f64, RillError>;

    /// Predict the boolean class label using a 0.5 threshold.
    fn predict(&self, features: &[f64]) -> Result<bool, RillError> {
        Ok(self.predict_proba(features)? >= 0.5)
    }

    /// Update the model using a single labeled sample.
    fn learn(&mut self, features: &[f64], target: bool) -> Result<(), RillError>;

    /// Reset the model to its initial state.
    fn reset(&mut self);
}

/// A stateful feature transformer.
///
/// The contract is:
/// - [`transform`](Self::transform) is read-only and must not update state.
/// - [`update`](Self::update) uses the raw features to refresh internal
///   statistics. It must not read the target label.
pub trait Transformer {
    /// Expected number of input features.
    fn input_dim(&self) -> usize;

    /// Number of features produced by [`transform`](Self::transform).
    fn output_dim(&self) -> usize;

    /// Transform features using the current internal state.
    fn transform(&self, features: &[f64]) -> Result<Vec<f64>, RillError>;

    /// Update internal statistics using raw features.
    fn update(&mut self, features: &[f64]) -> Result<(), RillError>;

    /// How many samples the transformer has seen.
    fn samples_seen(&self) -> u64;

    /// Reset the transformer to its initial state.
    fn reset(&mut self);
}

/// An online evaluation metric.
///
/// Metrics are updated sample-by-sample via [`update`](Self::update) and
/// queried via [`value`](Self::value). When insufficient data has been
/// observed, `value` returns `None` rather than a misleading zero.
pub trait Metric {
    /// The ground-truth type for this metric.
    type Truth;

    /// The prediction type for this metric.
    type Prediction;

    /// Incorporate a single observation.
    fn update(&mut self, truth: Self::Truth, prediction: Self::Prediction)
    -> Result<(), RillError>;

    /// Current metric value, or `None` if not enough data has been seen.
    fn value(&self) -> Option<f64>;

    /// How many observations have been incorporated.
    fn samples_seen(&self) -> u64;

    /// Reset the metric.
    fn reset(&mut self);
}

/// An online univariate statistic (mean, variance, etc.).
///
/// All implementations must use `O(1)` memory unless explicitly documented
/// otherwise (e.g. rolling statistics).
pub trait OnlineStatistic {
    /// Update the statistic with a new observation.
    ///
    /// Returns an error if `value` is not finite, unless the implementation
    /// explicitly opts in to a NaN-handling policy.
    fn update(&mut self, value: f64) -> Result<(), RillError>;

    /// How many observations have been incorporated.
    fn samples_seen(&self) -> u64;

    /// Reset the statistic.
    fn reset(&mut self);
}

/// An online regressor that accepts sparse features.
///
/// Implementations must keep `predict` side-effect free.
pub trait SparseRegressor {
    /// How many training samples the model has seen so far.
    fn samples_seen(&self) -> u64;

    /// Predict the target for the given sparse features.
    ///
    /// This method must not modify the model.
    fn predict(&self, features: &SparseFeatures) -> Result<f64, RillError>;

    /// Update the model using a single labeled sparse sample.
    fn learn(&mut self, features: &SparseFeatures, target: f64) -> Result<(), RillError>;

    /// Reset the model to its initial state.
    fn reset(&mut self);
}

/// An online binary classifier that accepts sparse features.
///
/// `predict` (and `predict_proba`) must be side-effect free. The output
/// range is closed `[0, 1]` — see [`OnlineBinaryClassifier`] for the
/// rationale.
pub trait SparseClassifier {
    /// How many training samples the model has seen so far.
    fn samples_seen(&self) -> u64;

    /// Predict the probability of the positive class.
    fn predict_proba(&self, features: &SparseFeatures) -> Result<f64, RillError>;

    /// Predict the boolean class label using a 0.5 threshold.
    fn predict(&self, features: &SparseFeatures) -> Result<bool, RillError> {
        Ok(self.predict_proba(features)? >= 0.5)
    }

    /// Update the model using a single labeled sparse sample.
    fn learn(&mut self, features: &SparseFeatures, target: bool) -> Result<(), RillError>;

    /// Reset the model to its initial state.
    fn reset(&mut self);
}

#[cfg(test)]
mod tests {
    //! Trait-level invariants shared by every `Metric` implementation.
    //!
    //! These tests enforce the contract documented in [`Metric`]:
    //! after `N` successful `update` calls, `samples_seen()` must equal `N`,
    //! and `reset()` must restore the metric to its initial state. A future
    //! regression that introduces an off-by-one in any metric's counter is
    //! caught here rather than only in per-metric unit tests.

    use super::*;
    use crate::metrics::{Accuracy, F1Score, LogLoss, Mae, Mse, Precision, R2, Recall, Rmse};

    /// Drive `metric` through `n` successful updates and assert the
    /// `samples_seen()` contract holds at every step.
    fn assert_samples_seen_contract<T: Metric>(
        metric: &mut T,
        n: u64,
        truth: impl Fn(u64) -> T::Truth,
        prediction: impl Fn(u64) -> T::Prediction,
    ) {
        assert_eq!(metric.samples_seen(), 0, "fresh metric must start at 0");
        for i in 0..n {
            metric.update(truth(i), prediction(i)).unwrap_or_else(|e| {
                panic!("update {i} must succeed in trait contract test: {e:?}")
            });
            assert_eq!(
                metric.samples_seen(),
                i + 1,
                "samples_seen must equal successful update count after {} updates",
                i + 1
            );
        }
        assert_eq!(metric.samples_seen(), n);
        metric.reset();
        assert_eq!(
            metric.samples_seen(),
            0,
            "reset must clear samples_seen for {}",
            std::any::type_name::<T>()
        );
    }

    #[test]
    fn mae_samples_seen_equals_successful_updates() {
        let mut m = Mae::new();
        assert_samples_seen_contract(&mut m, 5, |i| i as f64, |i| (i as f64) + 0.5);
    }

    #[test]
    fn mse_samples_seen_equals_successful_updates() {
        let mut m = Mse::new();
        assert_samples_seen_contract(&mut m, 5, |i| i as f64, |i| (i as f64) + 0.5);
    }

    #[test]
    fn rmse_samples_seen_equals_successful_updates() {
        let mut m = Rmse::new();
        assert_samples_seen_contract(&mut m, 5, |i| i as f64, |i| (i as f64) + 0.5);
    }

    #[test]
    fn r2_samples_seen_equals_successful_updates() {
        let mut m = R2::new();
        // Distinct truth values so m2_truth > 0 and value() is defined.
        assert_samples_seen_contract(&mut m, 5, |i| (i as f64) + 1.0, |i| (i as f64) + 1.1);
    }

    #[test]
    fn accuracy_samples_seen_equals_successful_updates() {
        let mut m = Accuracy::default();
        assert_samples_seen_contract(&mut m, 5, |i| i % 2 == 0, |i| i % 3 == 0);
    }

    #[test]
    fn precision_samples_seen_equals_successful_updates() {
        let mut m = Precision::default();
        // Mix of TP / FP / FN / TN so all internal counters move.
        assert_samples_seen_contract(&mut m, 4, |i| i % 2 == 0, |i| i % 3 == 0);
    }

    #[test]
    fn recall_samples_seen_equals_successful_updates() {
        let mut m = Recall::default();
        assert_samples_seen_contract(&mut m, 4, |i| i % 2 == 0, |i| i % 3 == 0);
    }

    #[test]
    fn f1_samples_seen_equals_successful_updates() {
        let mut m = F1Score::default();
        assert_samples_seen_contract(&mut m, 4, |i| i % 2 == 0, |i| i % 3 == 0);
    }

    #[test]
    fn log_loss_samples_seen_equals_successful_updates() {
        let mut m = LogLoss::default();
        // Predictions inside [0, 1] — the public trait contract.
        assert_samples_seen_contract(&mut m, 5, |i| i % 2 == 0, |i| 0.3 + 0.1 * (i as f64));
    }

    /// A failed `update` (non-finite input) must not advance `samples_seen`.
    /// This complements the per-metric atomicity tests by enforcing the
    /// contract at the trait level.
    #[test]
    fn failed_update_does_not_advance_samples_seen() {
        let mut m = Mae::new();
        m.update(1.0, 2.0).unwrap();
        assert_eq!(m.samples_seen(), 1);
        // Non-finite input must be rejected and must not change the count.
        assert!(m.update(f64::NAN, 1.0).is_err());
        assert_eq!(m.samples_seen(), 1, "failed update must not advance count");
    }
}

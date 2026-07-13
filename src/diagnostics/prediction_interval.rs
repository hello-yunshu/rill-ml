//! Prediction interval estimation.
//!
//! Maintains a bounded-memory estimate of prediction uncertainty based on the
//! exponentially weighted mean of absolute residuals. The interval is
//! `prediction ± k × recent_error`, where `recent_error` tracks the recent
//! average absolute error.
//!
//! Space complexity: `O(1)`.

use crate::error::{RillError, ensure_finite};
use crate::stats::ExponentiallyWeightedMean;
use crate::traits::OnlineStatistic;

/// An immutable prediction interval `[lower, upper]`.
///
/// Both bounds are inclusive.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PredictionInterval {
    lower: f64,
    upper: f64,
}

impl PredictionInterval {
    /// Lower bound of the interval (inclusive).
    pub const fn lower(&self) -> f64 {
        self.lower
    }

    /// Upper bound of the interval (inclusive).
    pub const fn upper(&self) -> f64 {
        self.upper
    }

    /// Whether `value` lies within `[lower, upper]` (inclusive on both ends).
    pub fn contains(&self, value: f64) -> bool {
        self.lower <= value && value <= self.upper
    }
}

/// Configuration for [`ResidualInterval`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResidualIntervalConfig {
    /// Multiplier applied to the recent error when forming the interval.
    ///
    /// Must be strictly positive. Larger values produce wider, more
    /// conservative intervals. Defaults to `1.0`.
    pub k: f64,

    /// Alpha for the exponentially weighted mean of absolute errors.
    ///
    /// Must be in `(0, 1]`. Smaller values give a longer memory.
    /// Defaults to `0.1`.
    pub alpha: f64,
}

impl Default for ResidualIntervalConfig {
    fn default() -> Self {
        Self { k: 1.0, alpha: 0.1 }
    }
}

/// Residual-based prediction interval estimator.
///
/// Tracks the exponentially weighted mean of absolute prediction errors and
/// forms intervals as `prediction ± k × recent_error`. Does not store raw
/// samples.
///
/// # Examples
///
/// ```
/// use rill_ml::diagnostics::{PredictionInterval, ResidualInterval};
///
/// let mut ri = ResidualInterval::default();
/// ri.observe(10.0, 11.0).unwrap();
/// ri.observe(10.0, 9.0).unwrap();
///
/// let interval: PredictionInterval = ri.interval(10.0).unwrap();
/// assert!(interval.contains(10.5));
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResidualInterval {
    config: ResidualIntervalConfig,
    error_ew: ExponentiallyWeightedMean,
}

impl ResidualInterval {
    /// Create a new residual interval estimator with the given configuration.
    ///
    /// Returns an error if `k` is not finite or not strictly positive, or if
    /// `alpha` is not in `(0, 1]` (the latter is validated by
    /// [`ExponentiallyWeightedMean::new`]).
    pub fn new(config: ResidualIntervalConfig) -> Result<Self, RillError> {
        ensure_finite("k", config.k)?;
        if config.k <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "k",
                value: config.k,
            });
        }
        Ok(Self {
            config: ResidualIntervalConfig {
                k: config.k,
                alpha: config.alpha,
            },
            error_ew: ExponentiallyWeightedMean::new(config.alpha)?,
        })
    }

    /// Observe a prediction and its ground truth, updating the error estimate.
    ///
    /// The absolute residual `|truth - prediction|` is fed to the internally
    /// tracked exponentially weighted mean. Non-finite inputs are rejected.
    pub fn observe(&mut self, prediction: f64, truth: f64) -> Result<(), RillError> {
        let abs_error = (truth - prediction).abs();
        self.error_ew.update(abs_error)
    }

    /// Compute the prediction interval centred on `prediction`.
    ///
    /// Returns [`RillError::InsufficientData`] if no observations have been
    /// recorded yet, and [`RillError::NonFiniteValue`] if `prediction` is not
    /// finite.
    pub fn interval(&self, prediction: f64) -> Result<PredictionInterval, RillError> {
        ensure_finite("prediction", prediction)?;
        if self.error_ew.count() == 0 {
            return Err(RillError::InsufficientData);
        }
        let margin = self.config.k * self.error_ew.value();
        Ok(PredictionInterval {
            lower: prediction - margin,
            upper: prediction + margin,
        })
    }

    /// Recent average absolute error, or `None` if no observations recorded.
    pub fn recent_error(&self) -> Option<f64> {
        if self.error_ew.count() == 0 {
            None
        } else {
            Some(self.error_ew.value())
        }
    }

    /// Number of observations recorded so far.
    pub fn samples_seen(&self) -> u64 {
        self.error_ew.samples_seen()
    }

    /// Reset the estimator to its initial state.
    pub fn reset(&mut self) {
        self.error_ew.reset();
    }
}

impl Default for ResidualInterval {
    fn default() -> Self {
        Self::new(ResidualIntervalConfig::default()).expect("default config is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_constructs_correctly() {
        let iv = PredictionInterval {
            lower: 1.0,
            upper: 3.0,
        };
        assert_eq!(iv.lower(), 1.0);
        assert_eq!(iv.upper(), 3.0);
    }

    #[test]
    fn contains_checks_bounds() {
        let iv = PredictionInterval {
            lower: 1.0,
            upper: 3.0,
        };
        assert!(iv.contains(1.0)); // lower bound inclusive
        assert!(iv.contains(3.0)); // upper bound inclusive
        assert!(iv.contains(2.0)); // middle
        assert!(!iv.contains(0.9)); // below
        assert!(!iv.contains(3.1)); // above
    }

    #[test]
    fn observe_then_interval() {
        let mut ri = ResidualInterval::default();
        ri.observe(10.0, 11.0).unwrap(); // |error| = 1.0
        ri.observe(10.0, 9.0).unwrap(); // |error| = 1.0
        let iv = ri.interval(10.0).unwrap();
        // EW mean: seed=1.0, then 0.1*1.0 + 0.9*1.0 = 1.0
        // margin = 1.0 (k) * 1.0 (recent) = 1.0
        assert!((iv.lower() - 9.0).abs() < 1e-9);
        assert!((iv.upper() - 11.0).abs() < 1e-9);
    }

    #[test]
    fn interval_without_observations_errors() {
        let ri = ResidualInterval::default();
        assert!(matches!(
            ri.interval(10.0),
            Err(RillError::InsufficientData)
        ));
    }

    #[test]
    fn recent_error_none_initially() {
        let ri = ResidualInterval::default();
        assert_eq!(ri.recent_error(), None);
        assert_eq!(ri.samples_seen(), 0);
    }

    #[test]
    fn custom_k_widens_interval() {
        let mut ri1 = ResidualInterval::new(ResidualIntervalConfig { k: 1.0, alpha: 0.1 }).unwrap();
        let mut ri2 = ResidualInterval::new(ResidualIntervalConfig { k: 2.0, alpha: 0.1 }).unwrap();
        ri1.observe(10.0, 12.0).unwrap(); // |error| = 2.0
        ri2.observe(10.0, 12.0).unwrap();
        let iv1 = ri1.interval(10.0).unwrap();
        let iv2 = ri2.interval(10.0).unwrap();
        let width1 = iv1.upper() - iv1.lower();
        let width2 = iv2.upper() - iv2.lower();
        assert!(width2 > width1);
        // k=2.0 doubles the margin, so the width doubles.
        assert!((width2 - 2.0 * width1).abs() < 1e-9);
    }

    #[test]
    fn alpha_affects_memory() {
        let mut ri = ResidualInterval::new(ResidualIntervalConfig { k: 1.0, alpha: 1.0 }).unwrap();
        ri.observe(0.0, 10.0).unwrap(); // |error| = 10.0
        ri.observe(0.0, 5.0).unwrap(); // |error| = 5.0
        ri.observe(0.0, 8.0).unwrap(); // |error| = 8.0
        // alpha=1.0 tracks the last value exactly.
        assert!((ri.recent_error().unwrap() - 8.0).abs() < 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut ri = ResidualInterval::default();
        ri.observe(10.0, 12.0).unwrap();
        assert!(ri.recent_error().is_some());
        ri.reset();
        assert_eq!(ri.recent_error(), None);
        assert_eq!(ri.samples_seen(), 0);
        assert!(matches!(
            ri.interval(10.0),
            Err(RillError::InsufficientData)
        ));
    }

    #[test]
    fn non_finite_prediction_rejected() {
        let mut ri = ResidualInterval::default();
        ri.observe(10.0, 11.0).unwrap();
        assert!(ri.interval(f64::NAN).is_err());
        assert!(ri.interval(f64::INFINITY).is_err());
        assert!(ri.interval(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn non_finite_truth_rejected() {
        let mut ri = ResidualInterval::default();
        assert!(ri.observe(10.0, f64::NAN).is_err());
        assert!(ri.observe(10.0, f64::INFINITY).is_err());
        assert!(ri.observe(10.0, f64::NEG_INFINITY).is_err());
        // No observations should have been recorded.
        assert_eq!(ri.samples_seen(), 0);
        assert_eq!(ri.recent_error(), None);
    }

    #[test]
    fn invalid_k_rejected() {
        let config = ResidualIntervalConfig { k: 0.0, alpha: 0.1 };
        assert!(ResidualInterval::new(config).is_err());
        let config = ResidualIntervalConfig {
            k: -1.0,
            alpha: 0.1,
        };
        assert!(ResidualInterval::new(config).is_err());
    }

    #[test]
    fn invalid_alpha_rejected() {
        let config = ResidualIntervalConfig { k: 1.0, alpha: 0.0 };
        assert!(ResidualInterval::new(config).is_err());
    }

    /// Deterministic pseudo-random number in `[0, 1)` using a simple LCG
    /// (Knuth MMIX constants) so the coverage test is reproducible.
    fn next_unit(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
    }

    #[test]
    fn interval_contains_subsequent_observation() {
        let config = ResidualIntervalConfig { k: 3.0, alpha: 0.1 };
        let mut ri = ResidualInterval::new(config).unwrap();
        let mut seed: u64 = 42;
        let prediction = 10.0;

        // Warm up the error estimate with bounded noise in [-1, 1].
        for _ in 0..50 {
            let noise = 2.0 * next_unit(&mut seed) - 1.0;
            ri.observe(prediction, prediction + noise).unwrap();
        }

        // Most subsequent observations should fall within the interval.
        let mut contained = 0u64;
        let total = 100u64;
        for _ in 0..total {
            let noise = 2.0 * next_unit(&mut seed) - 1.0;
            let truth = prediction + noise;
            ri.observe(prediction, truth).unwrap();
            let iv = ri.interval(prediction).unwrap();
            if iv.contains(truth) {
                contained += 1;
            }
        }
        // With k=3.0 and |error| <= 1.0, the EW mean (~0.5) gives a margin of
        // ~1.5, which comfortably covers the noise range.
        assert!(
            contained as f64 / total as f64 > 0.9,
            "only {}/{} observations contained",
            contained,
            total
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut ri = ResidualInterval::default();
        ri.observe(10.0, 12.0).unwrap();
        ri.observe(10.0, 9.0).unwrap();

        let json = serde_json::to_string(&ri).unwrap();
        let restored: ResidualInterval = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), 2);
        assert!((restored.recent_error().unwrap() - ri.recent_error().unwrap()).abs() < 1e-12);
        let iv = restored.interval(10.0).unwrap();
        assert!(iv.contains(10.0));
    }
}

//! Training summary statistics.
//!
//! Maintains bounded-memory summary statistics about the training process
//! without storing raw samples. Useful for diagnostics and monitoring.
//!
//! Space complexity: `O(1)`.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::stats::ExponentiallyWeightedMean;
use crate::traits::OnlineStatistic;

/// Configuration for [`TrainingSummary`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrainingSummaryConfig {
    /// Alpha for the exponentially weighted recent error.
    ///
    /// Must be in `(0, 1]`. Smaller values give a longer memory.
    pub error_alpha: f64,
}

impl Default for TrainingSummaryConfig {
    fn default() -> Self {
        Self { error_alpha: 0.1 }
    }
}

/// Bounded-memory summary of a training process.
///
/// Tracks counts, recent/best errors, model switches, resets, and load
/// failures. Does not store raw samples.
///
/// # Examples
///
/// ```
/// use rill_ml::diagnostics::TrainingSummary;
///
/// let mut summary = TrainingSummary::default();
/// summary.record_sample().unwrap();
/// summary.record_error(0.5).unwrap();
/// summary.set_baseline_error(0.8).unwrap();
///
/// assert_eq!(summary.total_samples(), 1);
/// assert!(summary.beats_baseline().unwrap());
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrainingSummary {
    total_samples: u64,
    rejected_samples: u64,
    error_ew: ExponentiallyWeightedMean,
    best_error: Option<f64>,
    baseline_error: Option<f64>,
    model_switches: u64,
    reset_count: u64,
    load_failures: u64,
}

impl TrainingSummary {
    /// Create a new training summary with the given configuration.
    pub fn new(config: TrainingSummaryConfig) -> Result<Self, RillError> {
        Ok(Self {
            total_samples: 0,
            rejected_samples: 0,
            error_ew: ExponentiallyWeightedMean::new(config.error_alpha)?,
            best_error: None,
            baseline_error: None,
            model_switches: 0,
            reset_count: 0,
            load_failures: 0,
        })
    }

    /// Record that a sample was processed.
    pub fn record_sample(&mut self) -> Result<(), RillError> {
        self.total_samples = checked_increment(self.total_samples, "total_samples")?;
        Ok(())
    }

    /// Record that an input was rejected (invalid, non-finite, etc.).
    pub fn record_rejection(&mut self) -> Result<(), RillError> {
        self.rejected_samples = checked_increment(self.rejected_samples, "rejected_samples")?;
        Ok(())
    }

    /// Record an error from a prediction.
    ///
    /// The absolute value is taken, so signed errors are accepted.
    /// Updates the recent error (EW mean) and the best (minimum) error.
    pub fn record_error(&mut self, error: f64) -> Result<(), RillError> {
        ensure_finite("error", error)?;
        let abs_error = error.abs();
        self.error_ew.update(abs_error)?;
        match self.best_error {
            None => self.best_error = Some(abs_error),
            Some(b) if abs_error < b => self.best_error = Some(abs_error),
            _ => {}
        }
        Ok(())
    }

    /// Set the baseline error for comparison.
    pub fn set_baseline_error(&mut self, error: f64) -> Result<(), RillError> {
        ensure_finite("baseline_error", error)?;
        self.baseline_error = Some(error.abs());
        Ok(())
    }

    /// Record that the active model was switched.
    pub fn record_switch(&mut self) -> Result<(), RillError> {
        self.model_switches = checked_increment(self.model_switches, "model_switches")?;
        Ok(())
    }

    /// Record that the model was reset.
    pub fn record_reset(&mut self) -> Result<(), RillError> {
        self.reset_count = checked_increment(self.reset_count, "reset_count")?;
        Ok(())
    }

    /// Record that a state load failed.
    pub fn record_load_failure(&mut self) -> Result<(), RillError> {
        self.load_failures = checked_increment(self.load_failures, "load_failures")?;
        Ok(())
    }

    /// Total samples processed.
    pub const fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// Samples rejected due to invalid input.
    pub const fn rejected_samples(&self) -> u64 {
        self.rejected_samples
    }

    /// Recent error (EW mean of absolute errors), or `None` if no errors recorded.
    pub fn recent_error(&self) -> Option<f64> {
        if self.error_ew.count() == 0 {
            None
        } else {
            Some(self.error_ew.value())
        }
    }

    /// Best (minimum) error observed, or `None` if no errors recorded.
    pub const fn best_error(&self) -> Option<f64> {
        self.best_error
    }

    /// Baseline error for comparison, or `None` if not set.
    pub const fn baseline_error(&self) -> Option<f64> {
        self.baseline_error
    }

    /// Number of times the active model was switched.
    pub const fn model_switches(&self) -> u64 {
        self.model_switches
    }

    /// Number of times the model was reset.
    pub const fn reset_count(&self) -> u64 {
        self.reset_count
    }

    /// Number of state load failures.
    pub const fn load_failures(&self) -> u64 {
        self.load_failures
    }

    /// Whether the model is currently beating the baseline.
    ///
    /// Returns `None` if either recent error or baseline error is unavailable.
    pub fn beats_baseline(&self) -> Option<bool> {
        match (self.recent_error(), self.baseline_error) {
            (Some(recent), Some(baseline)) => Some(recent < baseline),
            _ => None,
        }
    }

    /// Reset all summary statistics.
    pub fn reset(&mut self) {
        self.total_samples = 0;
        self.rejected_samples = 0;
        self.error_ew.reset();
        self.best_error = None;
        self.baseline_error = None;
        self.model_switches = 0;
        self.reset_count = 0;
        self.load_failures = 0;
    }
}

impl Default for TrainingSummary {
    fn default() -> Self {
        Self::new(TrainingSummaryConfig::default()).expect("default config is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_summary_has_no_data() {
        let s = TrainingSummary::default();
        assert_eq!(s.total_samples(), 0);
        assert_eq!(s.rejected_samples(), 0);
        assert_eq!(s.recent_error(), None);
        assert_eq!(s.best_error(), None);
        assert_eq!(s.baseline_error(), None);
        assert_eq!(s.beats_baseline(), None);
        assert_eq!(s.model_switches(), 0);
        assert_eq!(s.reset_count(), 0);
        assert_eq!(s.load_failures(), 0);
    }

    #[test]
    fn record_error_updates_recent_and_best() {
        let mut s = TrainingSummary::default();
        s.record_error(10.0).unwrap();
        s.record_error(5.0).unwrap();
        s.record_error(8.0).unwrap();
        assert_eq!(s.best_error(), Some(5.0));
        // EW mean with alpha=0.1: seed=10, then 0.1*5+0.9*10=9.5, then 0.1*8+0.9*9.5=9.35
        assert!((s.recent_error().unwrap() - 9.35).abs() < 1e-9);
    }

    #[test]
    fn beats_baseline_comparison() {
        let mut s = TrainingSummary::default();
        s.record_error(5.0).unwrap();
        s.set_baseline_error(10.0).unwrap();
        assert_eq!(s.beats_baseline(), Some(true));

        s.set_baseline_error(3.0).unwrap();
        assert_eq!(s.beats_baseline(), Some(false));
    }

    #[test]
    fn beats_baseline_none_without_errors() {
        let mut s = TrainingSummary::default();
        s.set_baseline_error(10.0).unwrap();
        assert_eq!(s.beats_baseline(), None);
    }

    #[test]
    fn counts_tracked_correctly() {
        let mut s = TrainingSummary::default();
        s.record_sample().unwrap();
        s.record_sample().unwrap();
        s.record_rejection().unwrap();
        s.record_switch().unwrap();
        s.record_switch().unwrap();
        s.record_reset().unwrap();
        s.record_load_failure().unwrap();
        s.record_load_failure().unwrap();
        s.record_load_failure().unwrap();
        assert_eq!(s.total_samples(), 2);
        assert_eq!(s.rejected_samples(), 1);
        assert_eq!(s.model_switches(), 2);
        assert_eq!(s.reset_count(), 1);
        assert_eq!(s.load_failures(), 3);
    }

    #[test]
    fn reset_clears_all() {
        let mut s = TrainingSummary::default();
        s.record_sample().unwrap();
        s.record_error(1.0).unwrap();
        s.set_baseline_error(2.0).unwrap();
        s.record_switch().unwrap();
        s.record_reset().unwrap();
        s.record_load_failure().unwrap();
        s.record_rejection().unwrap();
        s.reset();
        assert_eq!(s.total_samples(), 0);
        assert_eq!(s.rejected_samples(), 0);
        assert_eq!(s.recent_error(), None);
        assert_eq!(s.best_error(), None);
        assert_eq!(s.baseline_error(), None);
        assert_eq!(s.model_switches(), 0);
        assert_eq!(s.reset_count(), 0);
        assert_eq!(s.load_failures(), 0);
    }

    #[test]
    fn non_finite_error_rejected() {
        let mut s = TrainingSummary::default();
        assert!(s.record_error(f64::NAN).is_err());
        assert!(s.record_error(f64::INFINITY).is_err());
        assert!(s.record_error(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn non_finite_baseline_rejected() {
        let mut s = TrainingSummary::default();
        assert!(s.set_baseline_error(f64::NAN).is_err());
        assert!(s.set_baseline_error(f64::INFINITY).is_err());
    }

    #[test]
    fn invalid_alpha_rejected() {
        let config = TrainingSummaryConfig { error_alpha: 0.0 };
        assert!(TrainingSummary::new(config).is_err());
    }

    #[test]
    fn negative_error_uses_absolute_value() {
        let mut s = TrainingSummary::default();
        s.record_error(-5.0).unwrap();
        assert_eq!(s.best_error(), Some(5.0));
        assert!((s.recent_error().unwrap() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn custom_alpha_changes_memory() {
        let config = TrainingSummaryConfig { error_alpha: 1.0 };
        let mut s = TrainingSummary::new(config).unwrap();
        s.record_error(10.0).unwrap();
        s.record_error(5.0).unwrap();
        s.record_error(8.0).unwrap();
        // alpha=1.0 tracks last value
        assert!((s.recent_error().unwrap() - 8.0).abs() < 1e-12);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut s = TrainingSummary::default();
        s.record_sample().unwrap();
        s.record_sample().unwrap();
        s.record_error(2.0).unwrap();
        s.record_error(1.5).unwrap();
        s.set_baseline_error(3.0).unwrap();
        s.record_switch().unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let restored: TrainingSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.total_samples(), 2);
        assert_eq!(restored.best_error(), Some(1.5));
        assert_eq!(restored.baseline_error(), Some(3.0));
        assert_eq!(restored.model_switches(), 1);
        assert!(restored.beats_baseline().unwrap());
    }
}

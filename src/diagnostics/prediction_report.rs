//! Unified prediction report.
//!
//! Combines [`ResidualInterval`], [`WarmupTracker`], and [`TrainingSummary`]
//! into a single diagnostic wrapper that produces an immutable
//! [`PredictionReport`] for each prediction. This keeps the base model API
//! clean: a model returns a plain prediction, and the caller can wrap it with
//! [`PredictionReporter`] to obtain intervals, confidence levels, and
//! warmup/baseline comparisons.
//!
//! Space complexity: `O(1)`.

use crate::diagnostics::prediction_interval::{ResidualInterval, ResidualIntervalConfig};
use crate::diagnostics::training_summary::{TrainingSummary, TrainingSummaryConfig};
use crate::diagnostics::warmup::{WarmupConfig, WarmupState, WarmupTracker};
use crate::error::{RillError, ensure_finite};

/// Coarse confidence level derived from the warmup state and baseline
/// comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Confidence {
    /// No data has been observed yet.
    None,
    /// The model is still warming up or has degraded.
    Low,
    /// The model is usable but not yet stable.
    Medium,
    /// The model is stable and beating the baseline.
    High,
}

impl Confidence {
    /// Returns a short, stable string identifier.
    ///
    /// Possible return values: `"none"`, `"low"`, `"medium"`, `"high"`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Confidence::None => "none",
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
        }
    }
}

/// An immutable snapshot of diagnostics for a single prediction.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PredictionReport {
    prediction: f64,
    lower_bound: Option<f64>,
    upper_bound: Option<f64>,
    confidence: Confidence,
    samples_seen: u64,
    recent_error: Option<f64>,
    baseline_error: Option<f64>,
    warmup_state: WarmupState,
    beats_baseline: Option<bool>,
}

impl PredictionReport {
    /// The prediction that this report was generated for.
    pub const fn prediction(&self) -> f64 {
        self.prediction
    }

    /// Lower bound of the prediction interval, or `None` if insufficient data.
    pub const fn lower_bound(&self) -> Option<f64> {
        self.lower_bound
    }

    /// Upper bound of the prediction interval, or `None` if insufficient data.
    pub const fn upper_bound(&self) -> Option<f64> {
        self.upper_bound
    }

    /// Coarse confidence level for this prediction.
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Total number of samples observed so far.
    pub const fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    /// Recent (EW mean) absolute error, or `None` if no errors recorded.
    pub const fn recent_error(&self) -> Option<f64> {
        self.recent_error
    }

    /// Baseline error for comparison, or `None` if not set.
    pub const fn baseline_error(&self) -> Option<f64> {
        self.baseline_error
    }

    /// Current warmup state of the model.
    pub const fn warmup_state(&self) -> WarmupState {
        self.warmup_state
    }

    /// Whether the model is currently beating the baseline.
    ///
    /// Returns `None` if either recent error or baseline error is unavailable.
    pub const fn beats_baseline(&self) -> Option<bool> {
        self.beats_baseline
    }
}

/// Diagnostic wrapper that integrates interval estimation, warmup tracking,
/// and training summary statistics.
///
/// Produces a [`PredictionReport`] for each prediction without storing raw
/// samples. The underlying model API is not affected: callers feed
/// `(prediction, truth)` pairs via [`observe`](Self::observe) and request a
/// report via [`report`](Self::report) when needed.
///
/// # Examples
///
/// ```
/// use rill_ml::diagnostics::PredictionReporter;
///
/// let mut reporter = PredictionReporter::default();
/// reporter.observe(10.0, 11.0).unwrap();
/// reporter.observe(10.0, 9.0).unwrap();
///
/// let report = reporter.report(10.0).unwrap();
/// assert_eq!(report.prediction(), 10.0);
/// assert!(report.lower_bound().is_some());
/// assert_eq!(report.samples_seen(), 2);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PredictionReporter {
    interval: ResidualInterval,
    warmup: WarmupTracker,
    summary: TrainingSummary,
}

impl PredictionReporter {
    /// Create a new reporter with the given configurations.
    ///
    /// Each sub-component is constructed independently; configuration errors
    /// are propagated as [`RillError`].
    pub fn new(
        interval_config: ResidualIntervalConfig,
        warmup_config: WarmupConfig,
        summary_config: TrainingSummaryConfig,
    ) -> Result<Self, RillError> {
        Ok(Self {
            interval: ResidualInterval::new(interval_config)?,
            warmup: WarmupTracker::new(warmup_config)?,
            summary: TrainingSummary::new(summary_config)?,
        })
    }

    /// Observe a prediction and its ground truth.
    ///
    /// Updates the interval estimator, warmup tracker, and training summary.
    /// Non-finite inputs are rejected before any state is mutated.
    pub fn observe(&mut self, prediction: f64, truth: f64) -> Result<(), RillError> {
        self.interval.observe(prediction, truth)?;
        let error = (truth - prediction).abs();
        self.warmup.observe_sample(Some(error))?;
        self.summary.record_error(error)?;
        self.summary.record_sample()?;
        Ok(())
    }

    /// Set the baseline error for comparison.
    ///
    /// Propagates to both the warmup tracker and the training summary.
    pub fn set_baseline(&mut self, baseline: f64) -> Result<(), RillError> {
        self.warmup.set_baseline(baseline)?;
        self.summary.set_baseline_error(baseline)?;
        Ok(())
    }

    /// Build an immutable report for the given prediction.
    ///
    /// If the interval estimator has insufficient data, the bounds are set to
    /// `None` and no error is returned. Non-finite `prediction` values are
    /// rejected.
    pub fn report(&self, prediction: f64) -> Result<PredictionReport, RillError> {
        ensure_finite("prediction", prediction)?;

        let (lower_bound, upper_bound) = match self.interval.interval(prediction) {
            Ok(iv) => (Some(iv.lower()), Some(iv.upper())),
            Err(RillError::InsufficientData) => (None, None),
            Err(e) => return Err(e),
        };

        let warmup_state = self.warmup.state();
        let beats_baseline = self.summary.beats_baseline();
        let samples_seen = self.summary.total_samples();
        let recent_error = self.summary.recent_error();
        let baseline_error = self.summary.baseline_error();

        let confidence = match warmup_state {
            WarmupState::NoData => Confidence::None,
            WarmupState::WarmingUp | WarmupState::Degraded => Confidence::Low,
            WarmupState::Usable => Confidence::Medium,
            WarmupState::Stable => {
                if matches!(beats_baseline, Some(true)) {
                    Confidence::High
                } else {
                    Confidence::Medium
                }
            }
        };

        Ok(PredictionReport {
            prediction,
            lower_bound,
            upper_bound,
            confidence,
            samples_seen,
            recent_error,
            baseline_error,
            warmup_state,
            beats_baseline,
        })
    }

    /// Borrow the underlying training summary.
    pub fn summary(&self) -> &TrainingSummary {
        &self.summary
    }

    /// Current warmup state.
    pub fn warmup_state(&self) -> WarmupState {
        self.warmup.state()
    }

    /// Recent (EW mean) absolute error, or `None` if no errors recorded.
    pub fn recent_error(&self) -> Option<f64> {
        self.summary.recent_error()
    }

    /// Reset all three sub-components to their initial state.
    pub fn reset(&mut self) {
        self.interval.reset();
        self.warmup.reset();
        self.summary.reset();
    }
}

impl Default for PredictionReporter {
    fn default() -> Self {
        Self::new(
            ResidualIntervalConfig::default(),
            WarmupConfig::default(),
            TrainingSummaryConfig::default(),
        )
        .expect("default configs are valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_as_str() {
        assert_eq!(Confidence::None.as_str(), "none");
        assert_eq!(Confidence::Low.as_str(), "low");
        assert_eq!(Confidence::Medium.as_str(), "medium");
        assert_eq!(Confidence::High.as_str(), "high");
    }

    #[test]
    fn default_reporter_no_data() {
        let reporter = PredictionReporter::default();
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.prediction(), 0.0);
        assert_eq!(r.lower_bound(), None);
        assert_eq!(r.upper_bound(), None);
        assert_eq!(r.confidence(), Confidence::None);
        assert_eq!(r.warmup_state(), WarmupState::NoData);
        assert_eq!(r.samples_seen(), 0);
        assert_eq!(r.recent_error(), None);
        assert_eq!(r.baseline_error(), None);
        assert_eq!(r.beats_baseline(), None);
    }

    #[test]
    fn observe_then_report() {
        let mut reporter = PredictionReporter::default();
        reporter.observe(10.0, 11.0).unwrap(); // |error| = 1.0
        reporter.observe(10.0, 9.0).unwrap(); // |error| = 1.0
        let r = reporter.report(10.0).unwrap();
        assert_eq!(r.prediction(), 10.0);
        assert!(r.lower_bound().is_some());
        assert!(r.upper_bound().is_some());
        assert!(r.lower_bound().unwrap() < 10.0);
        assert!(r.upper_bound().unwrap() > 10.0);
        assert_eq!(r.samples_seen(), 2);
        assert!(r.recent_error().is_some());
    }

    #[test]
    fn set_baseline_enables_comparison() {
        let mut reporter = PredictionReporter::default();
        reporter.observe(0.0, 1.0).unwrap();
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.beats_baseline(), None);
        assert_eq!(r.baseline_error(), None);
        reporter.set_baseline(2.0).unwrap();
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.baseline_error(), Some(2.0));
        assert_eq!(r.beats_baseline(), Some(true)); // recent_error=1.0 < 2.0
    }

    #[test]
    fn confidence_progression() {
        let warmup_config = WarmupConfig {
            warming_up_threshold: 2,
            usable_threshold: 5,
            stable_threshold: 10,
            degraded_error_ratio: 2.0,
        };
        let summary_config = TrainingSummaryConfig { error_alpha: 1.0 };
        let mut reporter = PredictionReporter::new(
            ResidualIntervalConfig::default(),
            warmup_config,
            summary_config,
        )
        .unwrap();

        // NoData: no observations yet.
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.warmup_state(), WarmupState::NoData);
        assert_eq!(r.confidence(), Confidence::None);

        // WarmingUp: 1 sample (< warming_up_threshold=2).
        reporter.observe(0.0, 0.5).unwrap();
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.warmup_state(), WarmupState::WarmingUp);
        assert_eq!(r.confidence(), Confidence::Low);

        // Usable: 5 samples, no baseline.
        for _ in 0..4 {
            reporter.observe(0.0, 0.5).unwrap();
        }
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.warmup_state(), WarmupState::Usable);
        assert_eq!(r.confidence(), Confidence::Medium);

        // Set baseline; still Usable because samples < stable_threshold.
        reporter.set_baseline(1.0).unwrap();
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.warmup_state(), WarmupState::Usable);
        assert_eq!(r.confidence(), Confidence::Medium);

        // Stable: 10 samples and recent_error (0.5) <= baseline (1.0).
        for _ in 0..5 {
            reporter.observe(0.0, 0.5).unwrap();
        }
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.warmup_state(), WarmupState::Stable);
        assert_eq!(r.confidence(), Confidence::High);
    }

    #[test]
    fn degraded_state() {
        let mut reporter = PredictionReporter::default();
        reporter.set_baseline(0.4).unwrap();
        // error=1.0 > baseline(0.4) * ratio(2.0) = 0.8
        for _ in 0..5 {
            reporter.observe(0.0, 1.0).unwrap();
        }
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.warmup_state(), WarmupState::Degraded);
        assert_eq!(r.confidence(), Confidence::Low);
    }

    #[test]
    fn reset_clears_all() {
        let mut reporter = PredictionReporter::default();
        reporter.observe(10.0, 12.0).unwrap();
        reporter.set_baseline(2.0).unwrap();
        reporter.reset();
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.lower_bound(), None);
        assert_eq!(r.upper_bound(), None);
        assert_eq!(r.confidence(), Confidence::None);
        assert_eq!(r.warmup_state(), WarmupState::NoData);
        assert_eq!(r.samples_seen(), 0);
        assert_eq!(r.recent_error(), None);
        assert_eq!(r.baseline_error(), None);
        assert_eq!(r.beats_baseline(), None);
    }

    #[test]
    fn report_with_non_finite_prediction_errors() {
        let mut reporter = PredictionReporter::default();
        reporter.observe(0.0, 1.0).unwrap();
        assert!(reporter.report(f64::NAN).is_err());
        assert!(reporter.report(f64::INFINITY).is_err());
        assert!(reporter.report(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn observe_with_non_finite_rejected() {
        let mut reporter = PredictionReporter::default();
        assert!(reporter.observe(0.0, f64::NAN).is_err());
        assert!(reporter.observe(0.0, f64::INFINITY).is_err());
        assert!(reporter.observe(f64::NAN, 0.0).is_err());
        // No state should have been recorded.
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.samples_seen(), 0);
        assert_eq!(r.warmup_state(), WarmupState::NoData);
    }

    #[test]
    fn samples_seen_tracked() {
        let mut reporter = PredictionReporter::default();
        for i in 0..10 {
            reporter.observe(0.0, i as f64).unwrap();
        }
        let r = reporter.report(0.0).unwrap();
        assert_eq!(r.samples_seen(), 10);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut reporter = PredictionReporter::default();
        reporter.observe(10.0, 12.0).unwrap();
        reporter.observe(10.0, 9.0).unwrap();
        reporter.set_baseline(3.0).unwrap();

        let json = serde_json::to_string(&reporter).unwrap();
        let restored: PredictionReporter = serde_json::from_str(&json).unwrap();

        let r = restored.report(10.0).unwrap();
        assert_eq!(r.samples_seen(), 2);
        assert_eq!(r.baseline_error(), Some(3.0));
        assert!(r.beats_baseline().is_some());
    }
}

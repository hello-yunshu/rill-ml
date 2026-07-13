//! Warmup state tracking.
//!
//! Tracks the warmup state of an online model based on sample count and
//! error comparison against a baseline. Helps callers decide when a model
//! is ready for production use or when it has degraded.
//!
//! Space complexity: `O(1)`.

use crate::error::{RillError, ensure_finite};

/// Lifecycle state of a model during warmup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WarmupState {
    /// No samples have been observed yet.
    NoData,
    /// Not enough samples have been seen to make any decision.
    WarmingUp,
    /// The model can be used but has not yet stabilized.
    Usable,
    /// The model is stable and performing at least as well as the baseline.
    Stable,
    /// The recent error exceeds the baseline by too large a margin.
    Degraded,
}

impl WarmupState {
    /// Returns a short, stable string identifier for the state.
    ///
    /// Possible return values: `"no_data"`, `"warming_up"`, `"usable"`,
    /// `"stable"`, `"degraded"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            WarmupState::NoData => "no_data",
            WarmupState::WarmingUp => "warming_up",
            WarmupState::Usable => "usable",
            WarmupState::Stable => "stable",
            WarmupState::Degraded => "degraded",
        }
    }

    /// Returns `true` when the model may be used for decisions.
    ///
    /// Both [`WarmupState::Usable`] and [`WarmupState::Stable`] are considered ready.
    pub fn is_ready(&self) -> bool {
        matches!(self, WarmupState::Usable | WarmupState::Stable)
    }
}

/// Configuration for [`WarmupTracker`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WarmupConfig {
    /// Number of samples below which the model is considered [`WarmupState::WarmingUp`].
    pub warming_up_threshold: u64,
    /// Number of samples at which the model transitions from warming up to
    /// [`WarmupState::Usable`] (when no error comparison applies).
    pub usable_threshold: u64,
    /// Number of samples required (along with beating the baseline) for
    /// [`WarmupState::Stable`].
    pub stable_threshold: u64,
    /// Ratio by which the recent error may exceed the baseline before the
    /// model is considered [`WarmupState::Degraded`].
    pub degraded_error_ratio: f64,
}

impl Default for WarmupConfig {
    fn default() -> Self {
        Self {
            warming_up_threshold: 5,
            usable_threshold: 30,
            stable_threshold: 100,
            degraded_error_ratio: 2.0,
        }
    }
}

/// Bounded-memory warmup state tracker.
///
/// Tracks the number of observed samples, the most recent absolute error,
/// and a baseline error. From these it derives a [`WarmupState`].
///
/// # Examples
///
/// ```
/// use rill_ml::diagnostics::WarmupTracker;
///
/// let mut tracker = WarmupTracker::default();
/// assert_eq!(tracker.state().as_str(), "no_data");
///
/// for _ in 0..5 {
///     tracker.observe_sample(None).unwrap();
/// }
/// assert!(tracker.state().is_ready());
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WarmupTracker {
    config: WarmupConfig,
    samples: u64,
    recent_error: Option<f64>,
    baseline_error: Option<f64>,
}

impl WarmupTracker {
    /// Create a new tracker with the given configuration.
    ///
    /// Returns [`RillError::InvalidParameter`] if the thresholds are not ordered
    /// `warming_up_threshold < usable_threshold <= stable_threshold` or if
    /// `degraded_error_ratio` is not greater than `1.0`.
    pub fn new(config: WarmupConfig) -> Result<Self, RillError> {
        if config.warming_up_threshold >= config.usable_threshold {
            return Err(RillError::InvalidParameter {
                name: "warming_up_threshold",
                value: config.warming_up_threshold as f64,
            });
        }
        if config.usable_threshold > config.stable_threshold {
            return Err(RillError::InvalidParameter {
                name: "usable_threshold",
                value: config.usable_threshold as f64,
            });
        }
        if config.degraded_error_ratio.partial_cmp(&1.0) != Some(core::cmp::Ordering::Greater) {
            return Err(RillError::InvalidParameter {
                name: "degraded_error_ratio",
                value: config.degraded_error_ratio,
            });
        }
        Ok(Self {
            config,
            samples: 0,
            recent_error: None,
            baseline_error: None,
        })
    }

    /// Observe a sample, optionally with an error value.
    ///
    /// When `error` is `Some`, the value must be finite; its absolute value
    /// replaces the stored recent error. When `error` is `None`, only the
    /// sample counter is incremented.
    ///
    /// Returns [`RillError::NonFiniteValue`] if `error` is `Some` but not finite.
    /// In that case the tracker state is left unchanged.
    pub fn observe_sample(&mut self, error: Option<f64>) -> Result<(), RillError> {
        if let Some(e) = error {
            ensure_finite("error", e)?;
            self.recent_error = Some(e.abs());
        }
        self.samples += 1;
        Ok(())
    }

    /// Set the baseline error for comparison.
    ///
    /// The absolute value is stored, so signed errors are accepted.
    pub fn set_baseline(&mut self, baseline: f64) -> Result<(), RillError> {
        ensure_finite("baseline", baseline)?;
        self.baseline_error = Some(baseline.abs());
        Ok(())
    }

    /// Compute the current warmup state.
    ///
    /// The decision is made in priority order:
    ///
    /// 1. No samples seen → [`WarmupState::NoData`].
    /// 2. Samples below `warming_up_threshold` → [`WarmupState::WarmingUp`].
    /// 3. If both recent and baseline errors are available:
    ///    - recent > baseline × `degraded_error_ratio` → [`WarmupState::Degraded`].
    ///    - samples ≥ `stable_threshold` and recent ≤ baseline → [`WarmupState::Stable`].
    /// 4. Otherwise → [`WarmupState::Usable`].
    pub fn state(&self) -> WarmupState {
        if self.samples == 0 {
            return WarmupState::NoData;
        }
        if self.samples < self.config.warming_up_threshold {
            return WarmupState::WarmingUp;
        }
        match (self.recent_error, self.baseline_error) {
            (Some(r), Some(b)) if r > b * self.config.degraded_error_ratio => WarmupState::Degraded,
            (Some(r), Some(b)) if self.samples >= self.config.stable_threshold && r <= b => {
                WarmupState::Stable
            }
            _ => WarmupState::Usable,
        }
    }

    /// Number of samples observed so far.
    pub const fn samples(&self) -> u64 {
        self.samples
    }

    /// The most recent absolute error, or `None` if none was recorded.
    pub const fn recent_error(&self) -> Option<f64> {
        self.recent_error
    }

    /// The baseline error, or `None` if not set.
    pub const fn baseline_error(&self) -> Option<f64> {
        self.baseline_error
    }

    /// Reset the tracker to its initial (no-data) state.
    pub fn reset(&mut self) {
        self.samples = 0;
        self.recent_error = None;
        self.baseline_error = None;
    }
}

impl Default for WarmupTracker {
    fn default() -> Self {
        Self::new(WarmupConfig::default()).expect("default config is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_data_initially() {
        let t = WarmupTracker::default();
        assert_eq!(t.state(), WarmupState::NoData);
        assert_eq!(t.samples(), 0);
        assert_eq!(t.recent_error(), None);
        assert_eq!(t.baseline_error(), None);
    }

    #[test]
    fn warming_up_below_threshold() {
        let mut t = WarmupTracker::default();
        for _ in 0..4 {
            t.observe_sample(None).unwrap();
        }
        assert_eq!(t.state(), WarmupState::WarmingUp);
    }

    #[test]
    fn usable_after_warming_up() {
        let mut t = WarmupTracker::default();
        for _ in 0..5 {
            t.observe_sample(None).unwrap();
        }
        // samples >= warming_up (5) but < usable (30), no baseline -> Usable
        assert_eq!(t.state(), WarmupState::Usable);
    }

    #[test]
    fn stable_when_meets_threshold_and_beats_baseline() {
        let mut t = WarmupTracker::default();
        t.set_baseline(0.4).unwrap();
        for _ in 0..100 {
            t.observe_sample(Some(0.3)).unwrap();
        }
        // samples >= stable (100), recent (0.3) <= baseline (0.4) -> Stable
        assert_eq!(t.state(), WarmupState::Stable);
    }

    #[test]
    fn degraded_when_error_exceeds_ratio() {
        let mut t = WarmupTracker::default();
        t.set_baseline(0.4).unwrap();
        for _ in 0..5 {
            t.observe_sample(Some(1.0)).unwrap();
        }
        // 1.0 > 0.4 * 2.0 = 0.8 -> Degraded
        assert_eq!(t.state(), WarmupState::Degraded);
    }

    #[test]
    fn degraded_takes_precedence_over_stable() {
        let mut t = WarmupTracker::default();
        t.set_baseline(0.4).unwrap();
        for _ in 0..100 {
            t.observe_sample(Some(1.0)).unwrap();
        }
        // samples >= stable (100), but error (1.0) > baseline (0.4) * ratio (2.0)
        // Degraded is checked before Stable
        assert_eq!(t.state(), WarmupState::Degraded);
    }

    #[test]
    fn no_baseline_means_usable() {
        let mut t = WarmupTracker::default();
        for _ in 0..100 {
            t.observe_sample(Some(0.3)).unwrap();
        }
        // No baseline set, cannot be Stable or Degraded
        assert_eq!(t.state(), WarmupState::Usable);
    }

    #[test]
    fn set_baseline_stores_absolute() {
        let mut t = WarmupTracker::default();
        t.set_baseline(-3.0).unwrap();
        assert_eq!(t.baseline_error(), Some(3.0));
    }

    #[test]
    fn observe_sample_with_error() {
        let mut t = WarmupTracker::default();
        t.observe_sample(Some(0.5)).unwrap();
        assert_eq!(t.samples(), 1);
        assert_eq!(t.recent_error(), Some(0.5));
    }

    #[test]
    fn observe_sample_without_error() {
        let mut t = WarmupTracker::default();
        t.observe_sample(None).unwrap();
        assert_eq!(t.samples(), 1);
        assert_eq!(t.recent_error(), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut t = WarmupTracker::default();
        t.observe_sample(Some(0.5)).unwrap();
        t.set_baseline(0.4).unwrap();
        t.reset();
        assert_eq!(t.samples(), 0);
        assert_eq!(t.recent_error(), None);
        assert_eq!(t.baseline_error(), None);
        assert_eq!(t.state(), WarmupState::NoData);
    }

    #[test]
    fn invalid_config_rejected() {
        // warming_up >= usable
        let config = WarmupConfig {
            warming_up_threshold: 30,
            usable_threshold: 30,
            stable_threshold: 100,
            degraded_error_ratio: 2.0,
        };
        assert!(WarmupTracker::new(config).is_err());

        // usable > stable
        let config = WarmupConfig {
            warming_up_threshold: 5,
            usable_threshold: 101,
            stable_threshold: 100,
            degraded_error_ratio: 2.0,
        };
        assert!(WarmupTracker::new(config).is_err());

        // ratio <= 1.0
        let config = WarmupConfig {
            warming_up_threshold: 5,
            usable_threshold: 30,
            stable_threshold: 100,
            degraded_error_ratio: 1.0,
        };
        assert!(WarmupTracker::new(config).is_err());
    }

    #[test]
    fn non_finite_error_rejected() {
        let mut t = WarmupTracker::default();
        assert!(t.observe_sample(Some(f64::NAN)).is_err());
        assert_eq!(t.samples(), 0);
        assert_eq!(t.recent_error(), None);
        assert!(t.observe_sample(Some(f64::INFINITY)).is_err());
        assert_eq!(t.samples(), 0);
        assert!(t.observe_sample(Some(f64::NEG_INFINITY)).is_err());
        assert_eq!(t.samples(), 0);
    }

    #[test]
    fn state_as_str() {
        assert_eq!(WarmupState::NoData.as_str(), "no_data");
        assert_eq!(WarmupState::WarmingUp.as_str(), "warming_up");
        assert_eq!(WarmupState::Usable.as_str(), "usable");
        assert_eq!(WarmupState::Stable.as_str(), "stable");
        assert_eq!(WarmupState::Degraded.as_str(), "degraded");
    }

    #[test]
    fn state_is_ready() {
        assert!(!WarmupState::NoData.is_ready());
        assert!(!WarmupState::WarmingUp.is_ready());
        assert!(WarmupState::Usable.is_ready());
        assert!(WarmupState::Stable.is_ready());
        assert!(!WarmupState::Degraded.is_ready());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let config = WarmupConfig {
            warming_up_threshold: 1,
            usable_threshold: 2,
            stable_threshold: 3,
            degraded_error_ratio: 2.0,
        };
        let mut t = WarmupTracker::new(config).unwrap();
        t.observe_sample(Some(0.3)).unwrap();
        t.observe_sample(Some(0.3)).unwrap();
        t.observe_sample(Some(0.3)).unwrap();
        t.set_baseline(0.4).unwrap();

        let json = serde_json::to_string(&t).unwrap();
        let restored: WarmupTracker = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples(), 3);
        assert_eq!(restored.recent_error(), Some(0.3));
        assert_eq!(restored.baseline_error(), Some(0.4));
        assert_eq!(restored.state(), WarmupState::Stable);
    }
}

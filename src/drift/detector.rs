//! Core drift detector trait.
//!
//! All drift detectors in RillML implement [`DriftDetector`]. A detector
//! receives a scalar value (typically a prediction error or a target value)
//! and reports a [`DriftLevel`]: no drift, a warning, or a confirmed drift.
//!
//! Implementations must use bounded memory. They never store raw feature
//! vectors or labels — only scalar statistics derived from the stream.

use crate::error::RillError;

/// The severity level reported by a drift detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DriftLevel {
    /// No drift detected; the stream appears stable.
    #[default]
    None,
    /// A possible change has been detected but confidence is insufficient
    /// for a confirmed drift. Callers may wish to reduce confidence or
    /// increase monitoring.
    Warning,
    /// A confirmed drift has been detected. Callers should consider
    /// taking corrective action via a [`DriftStrategy`](crate::drift::DriftStrategy).
    Drift,
}

impl DriftLevel {
    /// Returns a short, stable string identifier.
    ///
    /// Possible values: `"none"`, `"warning"`, `"drift"`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            DriftLevel::None => "none",
            DriftLevel::Warning => "warning",
            DriftLevel::Drift => "drift",
        }
    }

    /// Returns `true` if the level indicates any kind of change
    /// (either `Warning` or `Drift`).
    pub const fn is_change(self) -> bool {
        matches!(self, DriftLevel::Warning | DriftLevel::Drift)
    }
}

/// Online drift detector trait.
///
/// Implementations track a scalar stream (prediction errors or target values)
/// and report when the stream's distribution appears to have changed. All
/// implementations must use bounded memory.
///
/// The detector is decoupled from any model: it only observes a scalar and
/// reports a level. The decision of what to do about a drift is delegated to
/// a [`DriftStrategy`](crate::drift::DriftStrategy).
pub trait DriftDetector {
    /// Update the detector with a new scalar observation.
    ///
    /// Returns the current [`DriftLevel`] after incorporating the value.
    /// Returns an error if the value is not finite.
    fn update(&mut self, value: f64) -> Result<DriftLevel, RillError>;

    /// Returns `true` if the detector currently reports a confirmed drift.
    fn detected(&self) -> bool;

    /// Returns `true` if the detector currently reports a warning.
    fn warning(&self) -> bool;

    /// The current drift level.
    fn level(&self) -> DriftLevel;

    /// Number of observations incorporated so far.
    fn samples_seen(&self) -> u64;

    /// Reset the detector to its initial (no-data) state.
    fn reset(&mut self);

    /// The detector-specific statistic value from the last update.
    ///
    /// Useful for diagnostics and logging in
    /// [`DriftEvent`](crate::drift::DriftEvent). For example, Page-Hinkley
    /// returns the cumulative-sum statistic, KSWIN returns the p-value.
    /// The default implementation returns `0.0`.
    fn last_value(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_level_as_str() {
        assert_eq!(DriftLevel::None.as_str(), "none");
        assert_eq!(DriftLevel::Warning.as_str(), "warning");
        assert_eq!(DriftLevel::Drift.as_str(), "drift");
    }

    #[test]
    fn drift_level_is_change() {
        assert!(!DriftLevel::None.is_change());
        assert!(DriftLevel::Warning.is_change());
        assert!(DriftLevel::Drift.is_change());
    }

    #[test]
    fn drift_level_default_is_none() {
        assert_eq!(DriftLevel::default(), DriftLevel::None);
    }
}

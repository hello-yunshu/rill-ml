//! Drift handling strategies.
//!
//! A [`DriftStrategy`] maps a [`DriftLevel`] to a [`DriftAction`]. This
//! decouples the detection (performed by a [`DriftDetector`](crate::drift::DriftDetector))
//! from the response (executed by
//! [`DriftAwareModel`](crate::drift::aware_model::DriftAwareModel)).

use crate::drift::action::DriftAction;
use crate::drift::detector::DriftLevel;

/// Strategy for deciding what action to take when a drift detector reports
/// a change.
///
/// Implementations may be stateless (like [`StaticStrategy`]) or stateful
/// (e.g. a strategy that escalates from `NotifyOnly` to `ResetModel` after
/// repeated warnings).
pub trait DriftStrategy {
    /// Decide the action for the given drift level and sample count.
    fn decide(&self, level: DriftLevel, samples_seen: u64) -> DriftAction;
}

/// A simple stateless strategy that maps each drift level to a fixed action.
///
/// `None` always maps to [`DriftAction::NotifyOnly`]. The caller configures
/// the actions for `Warning` and `Drift`.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::{DriftAction, DriftLevel, StaticStrategy};
/// use rill_ml::drift::DriftStrategy;
///
/// let strategy = StaticStrategy::new(
///     DriftAction::ReduceConfidence,
///     DriftAction::ResetModel,
/// );
/// assert_eq!(strategy.decide(DriftLevel::None, 100), DriftAction::NotifyOnly);
/// assert_eq!(strategy.decide(DriftLevel::Warning, 100), DriftAction::ReduceConfidence);
/// assert_eq!(strategy.decide(DriftLevel::Drift, 100), DriftAction::ResetModel);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StaticStrategy {
    /// Action to take when the detector reports a warning.
    pub warning_action: DriftAction,
    /// Action to take when the detector reports a confirmed drift.
    pub drift_action: DriftAction,
}

impl StaticStrategy {
    /// Create a new static strategy with the given warning and drift actions.
    pub const fn new(warning_action: DriftAction, drift_action: DriftAction) -> Self {
        Self {
            warning_action,
            drift_action,
        }
    }
}

impl Default for StaticStrategy {
    fn default() -> Self {
        // The safest default: never take destructive action automatically.
        Self {
            warning_action: DriftAction::NotifyOnly,
            drift_action: DriftAction::NotifyOnly,
        }
    }
}

impl DriftStrategy for StaticStrategy {
    fn decide(&self, level: DriftLevel, _samples_seen: u64) -> DriftAction {
        match level {
            DriftLevel::None => DriftAction::NotifyOnly,
            DriftLevel::Warning => self.warning_action,
            DriftLevel::Drift => self.drift_action,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_strategy_is_notify_only() {
        let s = StaticStrategy::default();
        assert_eq!(s.decide(DriftLevel::None, 0), DriftAction::NotifyOnly);
        assert_eq!(s.decide(DriftLevel::Warning, 10), DriftAction::NotifyOnly);
        assert_eq!(s.decide(DriftLevel::Drift, 100), DriftAction::NotifyOnly);
    }

    #[test]
    fn custom_strategy() {
        let s = StaticStrategy::new(DriftAction::ReduceConfidence, DriftAction::ResetModel);
        assert_eq!(s.decide(DriftLevel::None, 50), DriftAction::NotifyOnly);
        assert_eq!(
            s.decide(DriftLevel::Warning, 50),
            DriftAction::ReduceConfidence
        );
        assert_eq!(s.decide(DriftLevel::Drift, 50), DriftAction::ResetModel);
    }

    #[test]
    fn samples_seen_does_not_affect_static_strategy() {
        let s = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
        // Same level at different sample counts returns the same action.
        assert_eq!(
            s.decide(DriftLevel::Drift, 1),
            s.decide(DriftLevel::Drift, 1000)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let s = StaticStrategy::new(DriftAction::ReduceConfidence, DriftAction::ResetModel);
        let json = serde_json::to_string(&s).unwrap();
        let restored: StaticStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.warning_action, DriftAction::ReduceConfidence);
        assert_eq!(restored.drift_action, DriftAction::ResetModel);
    }
}

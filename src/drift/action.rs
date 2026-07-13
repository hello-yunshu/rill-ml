//! Drift action and event types.
//!
//! When a drift detector reports a change, a [`DriftAction`] describes what
//! the system should do about it. Actions are intentionally decoupled from
//! detectors: a detector only reports the level, and a
//! [`DriftStrategy`](crate::drift::strategy::DriftStrategy) decides the action.

use crate::drift::detector::DriftLevel;

/// The action to take when drift is detected.
///
/// This enum is returned by a [`DriftStrategy`](crate::drift::strategy::DriftStrategy)
/// and executed by [`DriftAwareModel`](crate::drift::aware_model::DriftAwareModel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DriftAction {
    /// Record the event but do not change model behavior. This is the
    /// safest default and should be used when the cost of a wrong reset
    /// exceeds the cost of a slow adaptation.
    #[default]
    NotifyOnly,
    /// Lower the confidence associated with subsequent predictions. The
    /// interpretation is left to the caller (e.g. widen prediction intervals
    /// or flag predictions as uncertain).
    ReduceConfidence,
    /// Reset the wrapped model's parameters to their initial state. Use
    /// when the concept drift is severe enough that relearning from scratch
    /// is faster than incremental adaptation.
    ResetModel,
    /// Reset the preprocessor's running statistics (e.g. StandardScaler
    /// mean and variance). Useful when feature distributions have shifted
    /// but the target relationship remains similar.
    ResetPreprocessor,
    /// Replace the current model with a baseline model. The replacement
    /// logic is handled by the caller; this action signals intent.
    ReplaceWithBaseline,
    /// Increase the model's adaptation rate (e.g. raise the learning rate)
    /// so it can relearn faster on the new distribution. The exact mechanism
    /// is model-dependent.
    IncreaseAdaptationRate,
}

impl DriftAction {
    /// Returns a short, stable string identifier.
    ///
    /// Possible values: `"notify_only"`, `"reduce_confidence"`,
    /// `"reset_model"`, `"reset_preprocessor"`, `"replace_with_baseline"`,
    /// `"increase_adaptation_rate"`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            DriftAction::NotifyOnly => "notify_only",
            DriftAction::ReduceConfidence => "reduce_confidence",
            DriftAction::ResetModel => "reset_model",
            DriftAction::ResetPreprocessor => "reset_preprocessor",
            DriftAction::ReplaceWithBaseline => "replace_with_baseline",
            DriftAction::IncreaseAdaptationRate => "increase_adaptation_rate",
        }
    }

    /// Returns `true` if this action modifies the model or preprocessor state.
    ///
    /// `NotifyOnly` and `ReduceConfidence` return `false`; all others return
    /// `true`.
    pub const fn is_destructive(self) -> bool {
        matches!(
            self,
            DriftAction::ResetModel
                | DriftAction::ResetPreprocessor
                | DriftAction::ReplaceWithBaseline
                | DriftAction::IncreaseAdaptationRate
        )
    }
}

/// An immutable record of a single drift event.
///
/// Produced by [`DriftAwareModel`](crate::drift::aware_model::DriftAwareModel)
/// whenever the detector reports a change (warning or drift).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DriftEvent {
    /// The sample index at which the event was triggered (0-based).
    pub sample_index: u64,
    /// The drift level that triggered the event.
    pub level: DriftLevel,
    /// The action that was taken in response.
    pub action: DriftAction,
    /// The detector-specific value at the time of triggering (e.g. cumulative
    /// sum for Page-Hinkley, KS statistic for KSWIN). Useful for diagnostics.
    pub detector_value: f64,
}

impl DriftEvent {
    /// Create a new drift event record.
    pub const fn new(
        sample_index: u64,
        level: DriftLevel,
        action: DriftAction,
        detector_value: f64,
    ) -> Self {
        Self {
            sample_index,
            level,
            action,
            detector_value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_as_str() {
        assert_eq!(DriftAction::NotifyOnly.as_str(), "notify_only");
        assert_eq!(DriftAction::ReduceConfidence.as_str(), "reduce_confidence");
        assert_eq!(DriftAction::ResetModel.as_str(), "reset_model");
        assert_eq!(
            DriftAction::ResetPreprocessor.as_str(),
            "reset_preprocessor"
        );
        assert_eq!(
            DriftAction::ReplaceWithBaseline.as_str(),
            "replace_with_baseline"
        );
        assert_eq!(
            DriftAction::IncreaseAdaptationRate.as_str(),
            "increase_adaptation_rate"
        );
    }

    #[test]
    fn action_is_destructive() {
        assert!(!DriftAction::NotifyOnly.is_destructive());
        assert!(!DriftAction::ReduceConfidence.is_destructive());
        assert!(DriftAction::ResetModel.is_destructive());
        assert!(DriftAction::ResetPreprocessor.is_destructive());
        assert!(DriftAction::ReplaceWithBaseline.is_destructive());
        assert!(DriftAction::IncreaseAdaptationRate.is_destructive());
    }

    #[test]
    fn action_default_is_notify_only() {
        assert_eq!(DriftAction::default(), DriftAction::NotifyOnly);
    }

    #[test]
    fn drift_event_construction() {
        let event = DriftEvent::new(42, DriftLevel::Drift, DriftAction::ResetModel, 1.23);
        assert_eq!(event.sample_index, 42);
        assert_eq!(event.level, DriftLevel::Drift);
        assert_eq!(event.action, DriftAction::ResetModel);
        assert!((event.detector_value - 1.23).abs() < 1e-12);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn action_serde_roundtrip() {
        for action in [
            DriftAction::NotifyOnly,
            DriftAction::ReduceConfidence,
            DriftAction::ResetModel,
            DriftAction::ResetPreprocessor,
            DriftAction::ReplaceWithBaseline,
            DriftAction::IncreaseAdaptationRate,
        ] {
            let json = serde_json::to_string(&action).unwrap();
            let restored: DriftAction = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, action);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn drift_event_serde_roundtrip() {
        let event = DriftEvent::new(10, DriftLevel::Warning, DriftAction::ReduceConfidence, 1.5);
        let json = serde_json::to_string(&event).unwrap();
        let restored: DriftEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.sample_index, 10);
        assert_eq!(restored.level, DriftLevel::Warning);
        assert_eq!(restored.action, DriftAction::ReduceConfidence);
        assert!((restored.detector_value - 1.5).abs() < 1e-12);
    }
}

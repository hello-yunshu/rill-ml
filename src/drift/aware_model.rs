//! A model wrapper that integrates drift detection into the predict → learn loop.
//!
//! [`DriftAwareModel`] combines an online regressor, a drift detector, and a
//! drift strategy into a single component. On each `learn` call, the prediction
//! error is fed to the detector; if the detector reports a change, the strategy
//! decides the response action and the event is recorded.
//!
//! ## What the wrapper does and does not do
//!
//! - **Does**: feed `|target - prediction|` to the detector, record
//!   [`DriftEvent`]s, and execute `ResetModel` when
//!   the strategy returns it.
//! - **Does not**: auto-reset the model by default. The default
//!   [`StaticStrategy`](crate::drift::StaticStrategy) returns `NotifyOnly` for
//!   both warning and drift, so the wrapper only logs events unless the caller
//!   configures a more aggressive strategy.
//! - **Does not**: execute `ResetPreprocessor`, `ReplaceWithBaseline`, or
//!   `IncreaseAdaptationRate`. These actions are recorded in
//!   [`last_action`](DriftAwareModel::last_action) for the caller to interpret.
//!
//! ## Space complexity
//!
//! `O(max_events)` for the event log, plus the space of the wrapped model,
//! detector, and strategy.

use crate::drift::action::{DriftAction, DriftEvent};
use crate::drift::detector::DriftDetector;
use crate::drift::strategy::DriftStrategy;
use crate::error::RillError;
use crate::traits::OnlineRegressor;

/// Default maximum number of retained drift events.
const DEFAULT_MAX_EVENTS: usize = 1000;

/// A wrapper that feeds prediction errors to a drift detector and applies
/// the strategy's action when drift is detected.
///
/// The wrapper is generic over the model `M`, detector `D`, and strategy `A`
/// to avoid trait-object overhead and preserve concrete types. See the
/// [module documentation](crate::drift::aware_model) for the full contract.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::{
///     DriftAction, DriftAwareModel, PageHinkley, StaticStrategy,
/// };
/// use rill_ml::models::{BaselineConfig, MeanRegressor};
///
/// let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
/// let detector = PageHinkley::default();
/// let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
/// let mut aware = DriftAwareModel::new(model, detector, strategy);
///
/// // Stable stream: no events.
/// for i in 0..50 {
///     let x = [i as f64 * 0.1];
///     aware.learn(&x, 1.0).unwrap();
/// }
/// assert!(aware.events().is_empty());
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DriftAwareModel<M, D, A>
where
    D: DriftDetector,
    A: DriftStrategy,
{
    model: M,
    detector: D,
    strategy: A,
    events: Vec<DriftEvent>,
    max_events: usize,
    samples_seen: u64,
    last_action: Option<DriftAction>,
}

impl<M, D, A> DriftAwareModel<M, D, A>
where
    M: OnlineRegressor,
    D: DriftDetector,
    A: DriftStrategy,
{
    /// Create a new drift-aware model with the default event log capacity
    /// (`1000` entries).
    pub fn new(model: M, detector: D, strategy: A) -> Self {
        Self {
            model,
            detector,
            strategy,
            events: Vec::new(),
            max_events: DEFAULT_MAX_EVENTS,
            samples_seen: 0,
            last_action: None,
        }
    }

    /// Create a new drift-aware model with a custom event log capacity.
    ///
    /// Returns an error if `max_events` is zero.
    pub fn with_max_events(
        model: M,
        detector: D,
        strategy: A,
        max_events: usize,
    ) -> Result<Self, RillError> {
        if max_events == 0 {
            return Err(RillError::InvalidCapacity(max_events));
        }
        Ok(Self {
            model,
            detector,
            strategy,
            events: Vec::with_capacity(max_events),
            max_events,
            samples_seen: 0,
            last_action: None,
        })
    }

    /// Predict the target for the given features.
    ///
    /// This is a pure delegation to the wrapped model's `predict` and does
    /// not update any state.
    pub fn predict(&self, features: &[f64]) -> Result<f64, RillError> {
        self.model.predict(features)
    }

    /// Update the model using a single labeled sample.
    ///
    /// The learning sequence is:
    /// 1. Predict with the current model (no state change).
    /// 2. Feed the absolute prediction error to the drift detector.
    /// 3. Ask the strategy for the response action.
    /// 4. If the detector reported a change, record a [`DriftEvent`].
    /// 5. If the action is `ResetModel`, reset the wrapped model.
    /// 6. Learn from the sample.
    pub fn learn(&mut self, features: &[f64], target: f64) -> Result<(), RillError> {
        let prediction = self.model.predict(features)?;
        let error = (target - prediction).abs();
        let level = self.detector.update(error)?;
        let action = self.strategy.decide(level, self.samples_seen);

        if level.is_change() {
            let event =
                DriftEvent::new(self.samples_seen, level, action, self.detector.last_value());
            self.events.push(event);
            while self.events.len() > self.max_events {
                self.events.remove(0);
            }
        }

        if action == DriftAction::ResetModel {
            self.model.reset();
        }
        self.last_action = Some(action);

        self.model.learn(features, target)?;
        self.samples_seen += 1;
        Ok(())
    }

    /// Borrow the wrapped model.
    pub const fn model(&self) -> &M {
        &self.model
    }

    /// Borrow the wrapped detector.
    pub const fn detector(&self) -> &D {
        &self.detector
    }

    /// Borrow the wrapped strategy.
    pub const fn strategy(&self) -> &A {
        &self.strategy
    }

    /// The recorded drift events, oldest first.
    ///
    /// The returned slice is guaranteed to be contiguous. When the event log
    /// exceeds `max_events`, the oldest entries are dropped.
    pub fn events(&self) -> &[DriftEvent] {
        &self.events
    }

    /// The most recent action taken by the wrapper, or `None` if `learn` has
    /// not been called yet.
    pub const fn last_action(&self) -> Option<DriftAction> {
        self.last_action
    }

    /// The number of samples processed by `learn`.
    pub const fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    /// The maximum number of retained drift events.
    pub const fn max_events(&self) -> usize {
        self.max_events
    }

    /// Reset the model, detector, and event log to their initial states.
    ///
    /// The strategy is not reset (it is typically stateless). The
    /// `max_events` capacity is preserved.
    pub fn reset(&mut self) {
        self.model.reset();
        self.detector.reset();
        self.events.clear();
        self.samples_seen = 0;
        self.last_action = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drift::detector::DriftLevel;
    use crate::drift::page_hinkley::PageHinkley;
    use crate::drift::strategy::StaticStrategy;
    use crate::models::{BaselineConfig, MeanRegressor};

    /// Build a minimal drift-aware model for tests.
    fn build(
        strategy: StaticStrategy,
    ) -> DriftAwareModel<MeanRegressor, PageHinkley, StaticStrategy> {
        let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let detector = PageHinkley::default();
        DriftAwareModel::new(model, detector, strategy)
    }

    #[test]
    fn predict_delegates_to_model() {
        let aware = build(StaticStrategy::default());
        // MeanRegressor with no data returns the initial prediction (0.0).
        let p = aware.predict(&[]).unwrap();
        assert!((p - 0.0).abs() < 1e-12);
    }

    #[test]
    fn learn_feeds_error_to_detector() {
        let mut aware = build(StaticStrategy::default());
        assert_eq!(aware.detector().samples_seen(), 0);
        aware.learn(&[], 1.0).unwrap();
        assert_eq!(aware.detector().samples_seen(), 1);
        aware.learn(&[], 2.0).unwrap();
        assert_eq!(aware.detector().samples_seen(), 2);
    }

    #[test]
    fn samples_seen_tracks_learn_calls() {
        let mut aware = build(StaticStrategy::default());
        assert_eq!(aware.samples_seen(), 0);
        for i in 0..10u64 {
            aware.learn(&[], i as f64).unwrap();
        }
        assert_eq!(aware.samples_seen(), 10);
    }

    #[test]
    fn last_action_updated_after_learn() {
        let mut aware = build(StaticStrategy::default());
        assert_eq!(aware.last_action(), None);
        aware.learn(&[], 1.0).unwrap();
        assert_eq!(aware.last_action(), Some(DriftAction::NotifyOnly));
    }

    #[test]
    fn default_strategy_does_not_reset_model() {
        let mut aware = build(StaticStrategy::default());
        // Feed several samples; model should accumulate them.
        for i in 0..20 {
            aware.learn(&[], i as f64).unwrap();
        }
        // With NotifyOnly, the model's samples_seen should keep growing.
        assert!(aware.model().samples_seen() > 0);
        let before = aware.model().samples_seen();
        aware.learn(&[], 100.0).unwrap();
        assert_eq!(aware.model().samples_seen(), before + 1);
    }

    #[test]
    fn reset_model_action_calls_model_reset() {
        // Strategy that returns ResetModel on drift.
        let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
        let mut aware = build(strategy);

        // Feed a stable stream first to accumulate model state.
        for _ in 0..30 {
            aware.learn(&[], 1.0).unwrap();
        }
        assert!(aware.model().samples_seen() > 0);

        // Now introduce a large shift to trigger drift.
        // PageHinkley should detect the sudden change and the strategy
        // should return ResetModel, which resets the model.
        let mut reset_happened = false;
        for _ in 0..100 {
            aware.learn(&[], 100.0).unwrap();
            // After a reset, model samples_seen would drop to a small value
            // relative to the total learn calls.
            if aware.model().samples_seen() < aware.samples_seen() {
                reset_happened = true;
                break;
            }
        }
        assert!(
            reset_happened,
            "ResetModel action should have reset the model"
        );
        // Events should have been recorded.
        assert!(!aware.events().is_empty());
    }

    #[test]
    fn detects_drift_and_records_event() {
        let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::NotifyOnly);
        let mut aware = build(strategy);

        // Stable stream: no events expected.
        for _ in 0..50 {
            aware.learn(&[], 1.0).unwrap();
        }
        assert!(aware.events().is_empty());

        // Sudden shift: keep feeding until a confirmed Drift (not just Warning)
        // is recorded. The first event may be a Warning because the PH
        // statistic exceeds the warning threshold before the drift threshold.
        let mut drift_recorded = false;
        for _ in 0..100 {
            aware.learn(&[], 50.0).unwrap();
            if let Some(last) = aware.events().last() {
                if last.level == DriftLevel::Drift {
                    drift_recorded = true;
                    break;
                }
            }
        }
        assert!(drift_recorded, "a drift event should have been recorded");
        let last_event = aware.events().last().unwrap();
        assert_eq!(last_event.level, DriftLevel::Drift);
    }

    #[test]
    fn replace_with_baseline_action_recorded() {
        // Use a strategy that returns ReplaceWithBaseline on drift.
        let strategy =
            StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ReplaceWithBaseline);
        let mut aware = build(strategy);

        // Stable baseline: the model learns to predict ~1.0 and PH settles.
        for _ in 0..50 {
            aware.learn(&[], 1.0).unwrap();
        }
        // Trigger drift with a large shift. The model still predicts ~1.0
        // initially, so the error jumps and PH detects the change.
        let mut seen_action = false;
        for _ in 0..200 {
            aware.learn(&[], 100.0).unwrap();
            if let Some(action) = aware.last_action() {
                if action == DriftAction::ReplaceWithBaseline {
                    seen_action = true;
                    break;
                }
            }
        }
        assert!(
            seen_action,
            "ReplaceWithBaseline action should have been recorded"
        );
    }

    #[test]
    fn increase_adaptation_rate_action_recorded() {
        let strategy =
            StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::IncreaseAdaptationRate);
        let mut aware = build(strategy);

        // Stable baseline.
        for _ in 0..50 {
            aware.learn(&[], 1.0).unwrap();
        }
        // Trigger drift.
        let mut seen_action = false;
        for _ in 0..200 {
            aware.learn(&[], 100.0).unwrap();
            if let Some(action) = aware.last_action() {
                if action == DriftAction::IncreaseAdaptationRate {
                    seen_action = true;
                    break;
                }
            }
        }
        assert!(
            seen_action,
            "IncreaseAdaptationRate action should have been recorded"
        );
    }

    #[test]
    fn events_bounded_by_max_events() {
        let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::NotifyOnly);
        let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let detector = PageHinkley::default();
        let mut aware = DriftAwareModel::with_max_events(model, detector, strategy, 3).unwrap();
        assert_eq!(aware.max_events(), 3);

        // Trigger many drift events by feeding a highly shifting stream.
        // Alternate between two very different means to trigger repeated drift.
        for i in 0..500 {
            let target = if i % 50 < 25 { 0.0 } else { 100.0 };
            aware.learn(&[], target).unwrap();
        }
        // The event log should never exceed max_events.
        assert!(aware.events().len() <= 3);
    }

    #[test]
    fn reset_clears_all_state() {
        let mut aware = build(StaticStrategy::default());
        for i in 0..20 {
            aware.learn(&[], i as f64).unwrap();
        }
        assert_eq!(aware.samples_seen(), 20);
        assert!(aware.last_action().is_some());

        aware.reset();
        assert_eq!(aware.samples_seen(), 0);
        assert_eq!(aware.last_action(), None);
        assert!(aware.events().is_empty());
        assert_eq!(aware.model().samples_seen(), 0);
        assert_eq!(aware.detector().samples_seen(), 0);
    }

    #[test]
    fn rejects_zero_max_events() {
        let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let detector = PageHinkley::default();
        let strategy = StaticStrategy::default();
        let result = DriftAwareModel::with_max_events(model, detector, strategy, 0);
        assert!(result.is_err());
    }

    #[test]
    fn model_detector_strategy_accessors() {
        let strategy = StaticStrategy::new(DriftAction::ReduceConfidence, DriftAction::ResetModel);
        let aware = build(strategy);
        assert_eq!(
            aware.strategy().decide(DriftLevel::Warning, 0),
            DriftAction::ReduceConfidence
        );
        assert_eq!(
            aware.strategy().decide(DriftLevel::Drift, 0),
            DriftAction::ResetModel
        );
        // Detector starts with no samples.
        assert_eq!(aware.detector().samples_seen(), 0);
        // Model starts with no samples.
        assert_eq!(aware.model().samples_seen(), 0);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
        let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let detector = PageHinkley::default();
        let mut aware = DriftAwareModel::new(model, detector, strategy);
        // Feed a few samples to populate state.
        for i in 0..10 {
            aware.learn(&[], i as f64).unwrap();
        }

        let json = serde_json::to_string(&aware).unwrap();
        let restored: DriftAwareModel<MeanRegressor, PageHinkley, StaticStrategy> =
            serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), aware.samples_seen());
        assert_eq!(restored.last_action(), aware.last_action());
        assert_eq!(restored.events().len(), aware.events().len());
    }
}

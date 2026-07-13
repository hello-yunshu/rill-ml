//! Online model selector with cooling period and minimum sample requirements.
//!
//! Wraps a [`BaselineComparator`] with additional constraints to prevent
//! frequent model switching:
//!
//! - **Cooling period**: after a switch, no new switch is allowed for a
//!   configurable number of samples recorded on the current best model.
//! - **Minimum samples**: a model must have at least a configurable number of
//!   recorded samples before it can be selected as the best.
//!
//! Space complexity: `O(n * window_size)` where `n` is the number of models.
//!
//! # Examples
//!
//! ```
//! use rill_ml::diagnostics::{OnlineModelSelector, SelectorConfig};
//!
//! let config = SelectorConfig::default();
//! let mut selector = OnlineModelSelector::new(&["model_a", "model_b"], config).unwrap();
//!
//! for _ in 0..20 {
//!     let truth = 1.0;
//!     selector.record(0, truth, truth + 1.0).unwrap();
//!     selector.record(1, truth, truth + 0.5).unwrap();
//! }
//!
//! let best = selector.select();
//! assert_eq!(best, Some(1));
//! ```

use crate::diagnostics::baseline_comparator::{BaselineComparator, ComparatorEntry};
use crate::error::RillError;

/// Configuration for [`OnlineModelSelector`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SelectorConfig {
    /// Rolling window size passed to the underlying [`BaselineComparator`].
    ///
    /// Must be greater than zero.
    pub window_size: usize,

    /// Number of samples that must be recorded on the current best model
    /// before another switch is allowed.
    pub cooling_period: u64,

    /// Minimum number of samples a model must have before it can be selected
    /// as the best.
    pub min_samples_before_switch: u64,
}

impl Default for SelectorConfig {
    fn default() -> Self {
        Self {
            window_size: 50,
            cooling_period: 20,
            min_samples_before_switch: 10,
        }
    }
}

/// Online model selector with cooling period and minimum sample requirements.
///
/// Wraps a [`BaselineComparator`] to prevent frequent model switching. The
/// selector delegates error tracking and best-entry detection to the
/// comparator, then applies two additional gates before committing to a
/// switch:
///
/// 1. The candidate must have at least [`SelectorConfig::min_samples_before_switch`]
///    total observations.
/// 2. At least [`SelectorConfig::cooling_period`] samples must have been
///    recorded on the *current* best model since the last switch.
///
/// # Examples
///
/// ```
/// use rill_ml::diagnostics::{OnlineModelSelector, SelectorConfig};
///
/// let mut selector = OnlineModelSelector::new(&["a", "b"], SelectorConfig::default()).unwrap();
/// selector.record(0, 1.0, 1.1).unwrap();
/// selector.record(1, 1.0, 0.9).unwrap();
/// let _ = selector.select();
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OnlineModelSelector {
    comparator: BaselineComparator,
    config: SelectorConfig,
    current_best: Option<usize>,
    samples_since_switch: u64,
}

impl OnlineModelSelector {
    /// Create a new model selector.
    ///
    /// # Errors
    ///
    /// Returns [`RillError::InvalidWindowSize`] if `config.window_size` is zero.
    /// Returns [`RillError::EmptyFeatures`] if `names` is empty (delegated to
    /// [`BaselineComparator::new`]).
    pub fn new(names: &[&str], config: SelectorConfig) -> Result<Self, RillError> {
        if config.window_size == 0 {
            return Err(RillError::InvalidWindowSize);
        }
        let comparator = BaselineComparator::new(names, config.window_size)?;
        Ok(Self {
            comparator,
            config,
            current_best: None,
            samples_since_switch: 0,
        })
    }

    /// Record a prediction from the model at `index`.
    ///
    /// If `index` matches the current best model, the sample counter is
    /// incremented (used for the cooling period).
    ///
    /// # Errors
    ///
    /// Returns [`RillError::DimensionMismatch`] if `index` is out of bounds,
    /// and propagates any finiteness error from the underlying metric.
    pub fn record(&mut self, index: usize, truth: f64, prediction: f64) -> Result<(), RillError> {
        self.comparator.record(index, truth, prediction)?;
        if self.current_best == Some(index) {
            self.samples_since_switch += 1;
        }
        Ok(())
    }

    /// Select the best model, applying cooling period and minimum sample
    /// constraints.
    ///
    /// Returns the index of the selected model, or `None` if no model has
    /// enough samples yet. The method is `&mut self` because it calls
    /// [`BaselineComparator::update_best`] internally.
    pub fn select(&mut self) -> Option<usize> {
        let new_best = match self.comparator.update_best() {
            None => return self.current_best,
            Some(idx) => idx,
        };

        // Check minimum samples for the candidate.
        let min_samples = self.config.min_samples_before_switch;
        let has_enough = self
            .comparator
            .entry(new_best)
            .map(|e| e.total_samples() >= min_samples)
            .unwrap_or(false);
        if !has_enough {
            return self.current_best;
        }

        match self.current_best {
            None => {
                // First selection: only the minimum-samples gate applies.
                self.current_best = Some(new_best);
                self.samples_since_switch = 0;
            }
            Some(current) if new_best == current => {
                // The comparator's best returned to the currently selected
                // model; no switch is needed.
                return self.current_best;
            }
            Some(_) => {
                // A different model is now the best. Enforce the cooling
                // period before committing to the switch.
                if self.samples_since_switch < self.config.cooling_period {
                    return self.current_best;
                }
                self.current_best = Some(new_best);
                self.samples_since_switch = 0;
            }
        }

        self.current_best
    }

    /// Returns the index of the currently selected best model, if any.
    pub const fn current_best(&self) -> Option<usize> {
        self.current_best
    }

    /// Returns the name of the currently selected best model, if any.
    pub fn current_best_name(&self) -> Option<&str> {
        let idx = self.current_best?;
        self.comparator.entry(idx).map(|e| e.name())
    }

    /// Number of times the underlying comparator has switched its best model.
    pub const fn switch_count(&self) -> u64 {
        self.comparator.switch_count()
    }

    /// Number of models tracked by the selector.
    pub fn entry_count(&self) -> usize {
        self.comparator.entry_count()
    }

    /// Returns the metrics entry for the model at `index`, if it exists.
    pub fn entry_metrics(&self, index: usize) -> Option<&ComparatorEntry> {
        self.comparator.entry(index)
    }

    /// Reset the selector and underlying comparator to their initial state.
    ///
    /// The number of tracked entries is preserved; only their data and the
    /// selection state are cleared.
    pub fn reset(&mut self) {
        self.comparator.reset();
        self.current_best = None;
        self.samples_since_switch = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_names_rejected() {
        let config = SelectorConfig::default();
        assert!(OnlineModelSelector::new(&[], config).is_err());
    }

    #[test]
    fn zero_window_rejected() {
        let config = SelectorConfig {
            window_size: 0,
            ..Default::default()
        };
        assert!(OnlineModelSelector::new(&["a"], config).is_err());
    }

    #[test]
    fn select_returns_none_initially() {
        let mut selector =
            OnlineModelSelector::new(&["a", "b"], SelectorConfig::default()).unwrap();
        assert_eq!(selector.select(), None);
        assert_eq!(selector.current_best(), None);
        assert_eq!(selector.current_best_name(), None);
    }

    #[test]
    fn select_after_min_samples() {
        let config = SelectorConfig {
            window_size: 50,
            cooling_period: 0,
            min_samples_before_switch: 5,
        };
        let mut selector = OnlineModelSelector::new(&["a", "b"], config).unwrap();

        for _ in 0..10 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        assert_eq!(selector.select(), Some(0));
    }

    #[test]
    fn cooling_period_prevents_switch() {
        let config = SelectorConfig {
            window_size: 50,
            cooling_period: 100,
            min_samples_before_switch: 2,
        };
        let mut selector = OnlineModelSelector::new(&["a", "b"], config).unwrap();

        // Model 0 is better.
        for _ in 0..10 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        assert_eq!(selector.select(), Some(0));

        // Model 1 becomes better, but cooling period prevents switch.
        for _ in 0..20 {
            selector.record(0, 1.0, 2.0).unwrap();
            selector.record(1, 1.0, 1.0).unwrap();
        }
        // samples_since_switch = 20 < 100, no switch.
        assert_eq!(selector.select(), Some(0));
    }

    #[test]
    fn min_samples_required() {
        let config = SelectorConfig {
            window_size: 50,
            cooling_period: 0,
            min_samples_before_switch: 100,
        };
        let mut selector = OnlineModelSelector::new(&["a", "b"], config).unwrap();

        for _ in 0..10 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        // Model 0 is best but has only 10 samples < 100.
        assert_eq!(selector.select(), None);
    }

    #[test]
    fn best_model_selected() {
        let mut selector =
            OnlineModelSelector::new(&["a", "b", "c"], SelectorConfig::default()).unwrap();

        for _ in 0..15 {
            selector.record(0, 1.0, 2.0).unwrap(); // error 1
            selector.record(1, 1.0, 1.0).unwrap(); // error 0
            selector.record(2, 1.0, 3.0).unwrap(); // error 2
        }
        assert_eq!(selector.select(), Some(1));
    }

    #[test]
    fn switch_count_tracked() {
        let mut selector =
            OnlineModelSelector::new(&["a", "b"], SelectorConfig::default()).unwrap();
        assert_eq!(selector.switch_count(), 0);

        for _ in 0..15 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        selector.select();
        // update_best detected a change (None -> Some(0)).
        assert_eq!(selector.switch_count(), 1);
    }

    #[test]
    fn reset_clears_all() {
        let mut selector =
            OnlineModelSelector::new(&["a", "b"], SelectorConfig::default()).unwrap();
        for _ in 0..15 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        selector.select();
        assert!(selector.current_best().is_some());

        selector.reset();
        assert_eq!(selector.current_best(), None);
        assert_eq!(selector.current_best_name(), None);
        assert_eq!(selector.switch_count(), 0);
        assert_eq!(selector.entry_count(), 2);
    }

    #[test]
    fn record_out_of_bounds_rejected() {
        let mut selector =
            OnlineModelSelector::new(&["a", "b"], SelectorConfig::default()).unwrap();
        assert!(selector.record(2, 1.0, 1.0).is_err());
        assert!(selector.record(0, 1.0, 1.0).is_ok());
    }

    #[test]
    fn two_models_alternating() {
        let config = SelectorConfig {
            window_size: 10,
            cooling_period: 100,
            min_samples_before_switch: 2,
        };
        let mut selector = OnlineModelSelector::new(&["a", "b"], config).unwrap();

        // Phase 1: model 0 is better.
        for _ in 0..3 {
            selector.record(0, 1.0, 1.0).unwrap(); // error 0
            selector.record(1, 1.0, 2.0).unwrap(); // error 1
        }
        assert_eq!(selector.select(), Some(0));

        // Phase 2: model 1 becomes better, but cooling period prevents switch.
        for _ in 0..7 {
            selector.record(0, 1.0, 2.0).unwrap(); // error 1
            selector.record(1, 1.0, 1.0).unwrap(); // error 0
        }
        // samples_since_switch = 7 < 100, no switch.
        assert_eq!(selector.select(), Some(0));

        // Phase 3: model 0 becomes better again.
        for _ in 0..10 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        // update_best returns Some(0), but current_best is already 0.
        assert_eq!(selector.select(), Some(0));
    }

    #[test]
    fn current_best_name() {
        let mut selector =
            OnlineModelSelector::new(&["alpha", "beta"], SelectorConfig::default()).unwrap();

        for _ in 0..15 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        selector.select();
        assert_eq!(selector.current_best(), Some(0));
        assert_eq!(selector.current_best_name(), Some("alpha"));
    }

    #[test]
    fn entry_metrics_access() {
        let mut selector =
            OnlineModelSelector::new(&["a", "b"], SelectorConfig::default()).unwrap();
        for _ in 0..5 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }

        let entry0 = selector.entry_metrics(0).expect("entry 0 should exist");
        assert_eq!(entry0.name(), "a");
        assert!(entry0.total_samples() >= 5);

        let entry1 = selector.entry_metrics(1).expect("entry 1 should exist");
        assert_eq!(entry1.name(), "b");
        assert!(entry1.total_samples() >= 5);

        assert!(selector.entry_metrics(2).is_none());
    }

    #[test]
    fn three_models_selection() {
        let mut selector =
            OnlineModelSelector::new(&["x", "y", "z"], SelectorConfig::default()).unwrap();

        for _ in 0..15 {
            selector.record(0, 1.0, 3.0).unwrap(); // error 2
            selector.record(1, 1.0, 1.0).unwrap(); // error 0
            selector.record(2, 1.0, 2.0).unwrap(); // error 1
        }
        assert_eq!(selector.select(), Some(1));
        assert_eq!(selector.current_best_name(), Some("y"));
        assert_eq!(selector.entry_count(), 3);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut selector =
            OnlineModelSelector::new(&["a", "b"], SelectorConfig::default()).unwrap();
        for _ in 0..15 {
            selector.record(0, 1.0, 1.0).unwrap();
            selector.record(1, 1.0, 2.0).unwrap();
        }
        selector.select();

        let json = serde_json::to_string(&selector).unwrap();
        let restored: OnlineModelSelector = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.current_best(), selector.current_best());
        assert_eq!(restored.entry_count(), selector.entry_count());
        assert_eq!(restored.switch_count(), selector.switch_count());
        assert_eq!(restored.current_best_name(), Some("a"));
    }
}

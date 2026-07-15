//! Baseline comparison across multiple models.
//!
//! Tracks progressive error metrics for several models in order to identify
//! the current best performer. The comparator stores only the rolling error
//! metrics, not the models themselves, keeping it fully decoupled from any
//! model trait.
//!
//! Space complexity: `O(n * window_size)` where `n` is the number of entries.
//!
//! # Examples
//!
//! ```
//! use rill_ml::diagnostics::BaselineComparator;
//!
//! let mut cmp = BaselineComparator::new(&["baseline", "candidate"], 16).unwrap();
//! cmp.record(0, 1.0, 1.4).unwrap(); // baseline error 0.4
//! cmp.record(1, 1.0, 1.1).unwrap(); // candidate error 0.1
//! assert_eq!(cmp.update_best(), Some(1));
//! assert_eq!(cmp.best_name(), Some("candidate"));
//! ```

use crate::error::RillError;
use crate::metrics::RollingMae;
use crate::traits::Metric;

/// Why the active best entry changed (or could not be determined).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SwitchReason {
    /// The new best entry has a strictly lower error than the previous best.
    LowerError,
    /// Not enough data has been observed to compare entries.
    InsufficientData,
    /// The new best entry ties with the previous best on error.
    Tie,
}

/// A single tracked model entry inside a [`BaselineComparator`].
///
/// Stores the entry's name, a rolling MAE over a fixed window, and the total
/// number of samples recorded against it.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComparatorEntry {
    name: String,
    rolling_mae: RollingMae,
    total_samples: u64,
}

impl ComparatorEntry {
    /// The name of this entry.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Current rolling MAE value, or `None` if no observations have been recorded.
    pub fn rolling_mae(&self) -> Option<f64> {
        self.rolling_mae.value()
    }

    /// Total number of observations recorded against this entry.
    pub const fn total_samples(&self) -> u64 {
        self.total_samples
    }
}

/// Compares multiple models by their rolling error and tracks the current best.
///
/// Each entry is identified by a name and maintains its own [`RollingMae`].
/// The comparator does not store models, only error metrics, so it can be
/// used alongside any predictor.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BaselineComparator {
    entries: Vec<ComparatorEntry>,
    best_index: Option<usize>,
    switch_count: u64,
    last_switch_reason: Option<SwitchReason>,
}

impl BaselineComparator {
    /// Create a new comparator with one entry per name and the given window size.
    ///
    /// # Errors
    ///
    /// Returns [`RillError::EmptyFeatures`] if `names` is empty, and
    /// [`RillError::InvalidParameter`] if `names` contains duplicates.
    /// A `window_size` of zero propagates [`RillError::InvalidWindowSize`]
    /// from the underlying [`RollingMae`].
    pub fn new(names: &[&str], window_size: usize) -> Result<Self, RillError> {
        if names.is_empty() {
            return Err(RillError::EmptyFeatures);
        }
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                if names[i] == names[j] {
                    return Err(RillError::InvalidParameter {
                        name: "names",
                        value: 0.0,
                    });
                }
            }
        }
        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            entries.push(ComparatorEntry {
                name: (*name).to_string(),
                rolling_mae: RollingMae::new(window_size)?,
                total_samples: 0,
            });
        }
        Ok(Self {
            entries,
            best_index: None,
            switch_count: 0,
            last_switch_reason: None,
        })
    }

    /// Record a single `(truth, prediction)` observation against the entry at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`RillError::DimensionMismatch`] if `index` is out of bounds,
    /// and propagates any finiteness error from the underlying metric.
    pub fn record(&mut self, index: usize, truth: f64, prediction: f64) -> Result<(), RillError> {
        let len = self.entries.len();
        let entry = self.entries.get_mut(index).ok_or({
            RillError::DimensionMismatch {
                expected: len,
                actual: index,
            }
        })?;
        entry.rolling_mae.update(truth, prediction)?;
        entry.total_samples += 1;
        Ok(())
    }

    /// Recompute the best entry and return the new best index if it changed.
    ///
    /// The best entry is the one with the lowest rolling MAE among entries
    /// that have at least one observation. If all entries are empty, the best
    /// index is cleared and the last switch reason is set to
    /// [`SwitchReason::InsufficientData`].
    ///
    /// Returns `Some(new_index)` when the best entry changes, and `None`
    /// otherwise.
    pub fn update_best(&mut self) -> Option<usize> {
        let mut new_best: Option<usize> = None;
        let mut best_err: f64 = f64::INFINITY;
        for (i, entry) in self.entries.iter().enumerate() {
            if let Some(err) = entry.rolling_mae()
                && err < best_err
            {
                best_err = err;
                new_best = Some(i);
            }
        }

        match new_best {
            None => {
                self.best_index = None;
                self.last_switch_reason = Some(SwitchReason::InsufficientData);
                None
            }
            Some(new_idx) if Some(new_idx) == self.best_index => None,
            Some(new_idx) => {
                self.switch_count += 1;
                let reason = match self.best_index {
                    None => SwitchReason::LowerError,
                    Some(old_idx) => match self.entries[old_idx].rolling_mae() {
                        None => SwitchReason::LowerError,
                        Some(old_err) => {
                            if best_err < old_err {
                                SwitchReason::LowerError
                            } else if best_err == old_err {
                                SwitchReason::Tie
                            } else {
                                // Unreachable: best_err is the minimum across all
                                // entries, so it cannot exceed the old best's error.
                                SwitchReason::LowerError
                            }
                        }
                    },
                };
                self.best_index = Some(new_idx);
                self.last_switch_reason = Some(reason);
                Some(new_idx)
            }
        }
    }

    /// Index of the current best entry, or `None` if no entry has data.
    pub const fn best_index(&self) -> Option<usize> {
        self.best_index
    }

    /// Name of the current best entry, or `None` if no entry has data.
    pub fn best_name(&self) -> Option<&str> {
        self.best_index.map(|i| self.entries[i].name.as_str())
    }

    /// Rolling MAE of the current best entry, or `None` if unavailable.
    pub fn best_error(&self) -> Option<f64> {
        self.best_index.and_then(|i| self.entries[i].rolling_mae())
    }

    /// Number of tracked entries.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// How many times the best entry has changed since construction or last reset.
    pub const fn switch_count(&self) -> u64 {
        self.switch_count
    }

    /// The reason for the most recent best-entry change (or `InsufficientData`).
    pub const fn last_switch_reason(&self) -> Option<SwitchReason> {
        self.last_switch_reason
    }

    /// Borrow the entry at `index`, or `None` if out of bounds.
    pub fn entry(&self, index: usize) -> Option<&ComparatorEntry> {
        self.entries.get(index)
    }

    /// Reset every entry's rolling metric and sample count, and clear all
    /// best-entry tracking state.
    pub fn reset(&mut self) {
        for entry in &mut self.entries {
            entry.rolling_mae.reset();
            entry.total_samples = 0;
        }
        self.best_index = None;
        self.switch_count = 0;
        self.last_switch_reason = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_names_rejected() {
        assert!(matches!(
            BaselineComparator::new(&[], 10),
            Err(RillError::EmptyFeatures)
        ));
    }

    #[test]
    fn duplicate_names_rejected() {
        assert!(BaselineComparator::new(&["a", "a"], 10).is_err());
        assert!(BaselineComparator::new(&["a", "b", "a"], 10).is_err());
    }

    #[test]
    fn zero_window_rejected() {
        assert!(BaselineComparator::new(&["a"], 0).is_err());
    }

    #[test]
    fn record_updates_entry() {
        let mut c = BaselineComparator::new(&["a"], 10).unwrap();
        assert_eq!(c.entry(0).unwrap().total_samples(), 0);
        assert!(c.entry(0).unwrap().rolling_mae().is_none());
        c.record(0, 1.0, 1.5).unwrap();
        assert_eq!(c.entry(0).unwrap().total_samples(), 1);
        assert_eq!(c.entry(0).unwrap().rolling_mae(), Some(0.5));
    }

    #[test]
    fn record_out_of_bounds_rejected() {
        let mut c = BaselineComparator::new(&["a"], 10).unwrap();
        assert!(c.record(1, 1.0, 1.0).is_err());
        assert!(c.record(0, 1.0, 1.0).is_ok());
    }

    #[test]
    fn update_best_selects_lowest_error() {
        let mut c = BaselineComparator::new(&["a", "b"], 10).unwrap();
        c.record(0, 1.0, 1.5).unwrap(); // error 0.5
        c.record(1, 1.0, 1.2).unwrap(); // error 0.2
        assert_eq!(c.update_best(), Some(1));
        assert_eq!(c.best_index(), Some(1));
        assert!((c.best_error().unwrap() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn update_best_returns_new_index_on_switch() {
        let mut c = BaselineComparator::new(&["a", "b"], 10).unwrap();
        c.record(0, 1.0, 1.5).unwrap(); // a: 0.5
        assert_eq!(c.update_best(), Some(0));
        c.record(1, 1.0, 1.1).unwrap(); // b: 0.1, lower
        assert_eq!(c.update_best(), Some(1));
    }

    #[test]
    fn update_best_returns_none_on_no_change() {
        let mut c = BaselineComparator::new(&["a", "b"], 10).unwrap();
        c.record(0, 1.0, 1.5).unwrap(); // a: 0.5
        c.record(1, 1.0, 1.9).unwrap(); // b: 0.9
        assert_eq!(c.update_best(), Some(0));
        assert_eq!(c.update_best(), None);
    }

    #[test]
    fn insufficient_data_when_all_empty() {
        let mut c = BaselineComparator::new(&["a", "b"], 10).unwrap();
        assert_eq!(c.update_best(), None);
        assert_eq!(c.best_index(), None);
        assert_eq!(c.last_switch_reason(), Some(SwitchReason::InsufficientData));
        assert_eq!(c.switch_count(), 0);
    }

    #[test]
    fn tie_records_tie_reason() {
        let mut c = BaselineComparator::new(&["a", "b"], 10).unwrap();
        // Record identical error for "b" first so it becomes the initial best.
        c.record(1, 1.0, 1.5).unwrap(); // error 0.5
        assert_eq!(c.update_best(), Some(1));
        assert_eq!(c.last_switch_reason(), Some(SwitchReason::LowerError));
        // Record identical error for "a" (index 0). Iteration picks the first
        // minimum, so the best switches from index 1 to index 0 with equal error.
        c.record(0, 1.0, 1.5).unwrap(); // error 0.5
        assert_eq!(c.update_best(), Some(0));
        assert_eq!(c.last_switch_reason(), Some(SwitchReason::Tie));
    }

    #[test]
    fn switch_count_increments() {
        let mut c = BaselineComparator::new(&["a", "b"], 10).unwrap();
        assert_eq!(c.switch_count(), 0);

        c.record(0, 1.0, 1.1).unwrap(); // a: 0.1
        assert_eq!(c.update_best(), Some(0));
        assert_eq!(c.switch_count(), 1);

        c.record(1, 1.0, 1.05).unwrap(); // b: 0.05 < 0.1
        assert_eq!(c.update_best(), Some(1));
        assert_eq!(c.switch_count(), 2);

        // a's rolling mae becomes (0.1 + 0.02) / 2 = 0.06, still worse than b's 0.05
        c.record(0, 1.0, 1.02).unwrap();
        assert_eq!(c.update_best(), None);
        assert_eq!(c.switch_count(), 2);

        // a's rolling mae becomes (0.1 + 0.02 + 0.0) / 3 ≈ 0.04 < 0.05
        c.record(0, 1.0, 1.0).unwrap();
        assert_eq!(c.update_best(), Some(0));
        assert_eq!(c.switch_count(), 3);
    }

    #[test]
    fn best_name_maps_index() {
        let mut c = BaselineComparator::new(&["alpha", "beta"], 10).unwrap();
        c.record(1, 1.0, 1.1).unwrap(); // beta: 0.1
        assert_eq!(c.update_best(), Some(1));
        assert_eq!(c.best_name(), Some("beta"));
        assert_eq!(c.entry(1).unwrap().name(), "beta");
    }

    #[test]
    fn entry_out_of_bounds() {
        let c = BaselineComparator::new(&["a"], 10).unwrap();
        assert!(c.entry(0).is_some());
        assert!(c.entry(99).is_none());
    }

    #[test]
    fn reset_clears_all() {
        let mut c = BaselineComparator::new(&["a", "b"], 10).unwrap();
        c.record(0, 1.0, 1.5).unwrap();
        c.record(1, 1.0, 1.3).unwrap();
        c.update_best();
        assert_eq!(c.switch_count(), 1);

        c.reset();
        assert_eq!(c.entry_count(), 2);
        assert_eq!(c.entry(0).unwrap().total_samples(), 0);
        assert!(c.entry(0).unwrap().rolling_mae().is_none());
        assert_eq!(c.entry(1).unwrap().total_samples(), 0);
        assert!(c.entry(1).unwrap().rolling_mae().is_none());
        assert_eq!(c.best_index(), None);
        assert_eq!(c.switch_count(), 0);
        assert_eq!(c.last_switch_reason(), None);
    }

    #[test]
    fn three_way_comparison() {
        let mut c = BaselineComparator::new(&["x", "y", "z"], 10).unwrap();
        c.record(0, 1.0, 1.5).unwrap(); // x: 0.5
        c.record(1, 1.0, 1.3).unwrap(); // y: 0.3
        c.record(2, 1.0, 1.1).unwrap(); // z: 0.1
        assert_eq!(c.update_best(), Some(2));
        assert_eq!(c.best_name(), Some("z"));
        assert!((c.best_error().unwrap() - 0.1).abs() < 1e-9);
        assert_eq!(c.entry_count(), 3);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut c = BaselineComparator::new(&["a", "b", "c"], 5).unwrap();
        c.record(0, 1.0, 1.5).unwrap();
        c.record(1, 1.0, 1.2).unwrap();
        c.record(2, 1.0, 1.4).unwrap();
        c.update_best();

        let json = serde_json::to_string(&c).unwrap();
        let restored: BaselineComparator = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.entry_count(), 3);
        assert_eq!(restored.best_index(), Some(1));
        assert_eq!(restored.best_name(), Some("b"));
        assert_eq!(restored.switch_count(), 1);
        assert_eq!(
            restored.last_switch_reason(),
            Some(SwitchReason::LowerError)
        );
        assert_eq!(restored.entry(0).unwrap().total_samples(), 1);
    }
}

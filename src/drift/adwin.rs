//! ADWIN (Adaptive Windowing) drift detector.
//!
//! ADWIN maintains a variable-length window of recent observations and
//! detects when the distribution of the window's two halves differs
//! significantly. When drift is detected, the older portion is dropped.
//!
//! ## Algorithm
//!
//! Based on Bifet & Gavaldà (2007). For each new observation:
//!
//! 1. Add the value to the window.
//! 2. For each possible split point `k` (dividing the window into `W0`
//!    and `W1`), compute the means `μ0` and `μ1`.
//! 3. Compute the Hoeffding bound:
//!    `ε = √(1/(2·m) · ln(4/δ'))` where `m = n0·n1/(n0+n1)` and
//!    `δ' = δ / ln(n)` (Bonferroni correction for repeated testing).
//! 4. If `|μ0 − μ1| > ε`, drop `W0` and signal drift.
//!
//! ## Space complexity
//!
//! `O(max_window)` — the window stores individual values up to
//! [`AdwinConfig::max_window`] elements. Prefix sums are maintained
//! incrementally so each update is `O(max_window)` in the worst case.

use crate::drift::detector::{DriftDetector, DriftLevel};
use crate::error::{RillError, ensure_finite};

/// Configuration for [`Adwin`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdwinConfig {
    /// Significance level for drift detection. Must be in `(0, 1)`.
    /// Smaller values reduce false positives. Defaults to `0.002`.
    pub delta: f64,

    /// Significance level for warnings. Must be in `(0, 1)` and
    /// greater than or equal to `delta`. Defaults to `0.01`.
    pub warning_delta: f64,

    /// Maximum window size (number of stored observations). Must be > 0.
    /// Larger values improve detection sensitivity but increase memory
    /// and computation. Defaults to `1000`.
    pub max_window: usize,

    /// Minimum number of samples before any detection is attempted.
    /// Must be greater than zero. Defaults to `10`.
    pub min_samples: u64,
}

impl Default for AdwinConfig {
    fn default() -> Self {
        Self {
            delta: 0.002,
            warning_delta: 0.01,
            max_window: 1000,
            min_samples: 10,
        }
    }
}

/// ADWIN (Adaptive Windowing) drift detector.
///
/// Maintains a variable-length window and detects distribution changes by
/// comparing the means of the window's two halves. See the
/// [module documentation](crate::drift::adwin) for the algorithm.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::{Adwin, DriftDetector, DriftLevel};
///
/// let mut adwin = Adwin::default();
///
/// // Stable stream.
/// for _ in 0..100 {
///     adwin.update(0.0).unwrap();
/// }
/// assert_eq!(adwin.level(), DriftLevel::None);
///
/// // Sudden shift. ADWIN's level is transient: after detecting drift and
/// // trimming the window, subsequent stable updates reset the level to
/// // None, so we check the level returned by each update.
/// let mut detected = false;
/// for _ in 0..100 {
///     let level = adwin.update(5.0).unwrap();
///     if level == DriftLevel::Drift {
///         detected = true;
///         break;
///     }
/// }
/// assert!(detected);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Adwin {
    config: AdwinConfig,
    window: std::collections::VecDeque<f64>,
    total: f64,
    samples: u64,
    current_level: DriftLevel,
}

impl Adwin {
    /// Create a new ADWIN detector with the given configuration.
    ///
    /// Returns an error if:
    /// - `delta` is not in `(0, 1)`.
    /// - `warning_delta` is not in `(0, 1)` or is less than `delta`.
    /// - `max_window` is zero.
    /// - `min_samples` is zero.
    pub fn new(config: AdwinConfig) -> Result<Self, RillError> {
        ensure_finite("delta", config.delta)?;
        if config.delta <= 0.0 || config.delta >= 1.0 {
            return Err(RillError::InvalidSignificanceLevel(config.delta));
        }
        ensure_finite("warning_delta", config.warning_delta)?;
        if config.warning_delta <= 0.0 || config.warning_delta >= 1.0 {
            return Err(RillError::InvalidSignificanceLevel(config.warning_delta));
        }
        if config.warning_delta < config.delta {
            return Err(RillError::InvalidParameter {
                name: "warning_delta",
                value: config.warning_delta,
            });
        }
        if config.max_window == 0 {
            return Err(RillError::InvalidCapacity(config.max_window));
        }
        if config.min_samples == 0 {
            return Err(RillError::InvalidParameter {
                name: "min_samples",
                value: 0.0,
            });
        }
        Ok(Self {
            window: std::collections::VecDeque::with_capacity(config.max_window),
            config,
            total: 0.0,
            samples: 0,
            current_level: DriftLevel::None,
        })
    }

    /// The number of values currently in the window.
    pub fn window_size(&self) -> usize {
        self.window.len()
    }

    /// The mean of all values currently in the window, or `0.0` if empty.
    pub fn window_mean(&self) -> f64 {
        if self.window.is_empty() {
            0.0
        } else {
            self.total / self.window.len() as f64
        }
    }

    /// The configuration of this detector.
    pub const fn config(&self) -> &AdwinConfig {
        &self.config
    }

    /// Compute the Hoeffding bound for a split with `n0` and `n1` elements
    /// at significance level `delta` with total stream length `n`.
    fn hoeffding_bound(n0: f64, n1: f64, n: u64, delta: f64) -> f64 {
        let m = n0 * n1 / (n0 + n1);
        let ln_n = (n as f64).ln().max(1.0);
        let delta_eff = delta / ln_n;
        (1.0 / (2.0 * m) * (4.0 / delta_eff).ln()).sqrt()
    }

    /// Check all split points and return the index where drift is detected,
    /// along with the level (Warning or Drift). Returns `None` if no split
    /// exceeds the threshold.
    fn check_splits(&self) -> Option<(usize, DriftLevel, f64)> {
        let n = self.window.len();
        if n < 2 {
            return None;
        }
        // Build prefix sums for O(1) mean computation per split.
        let mut prefix = Vec::with_capacity(n + 1);
        prefix.push(0.0_f64);
        let mut acc = 0.0;
        for &v in &self.window {
            acc += v;
            prefix.push(acc);
        }
        let total = prefix[n];
        let n_total = n as u64;

        let mut best_split: Option<(usize, DriftLevel, f64)> = None;
        // Check split points from the oldest end.
        for (k, &sum0) in prefix.iter().enumerate().take(n).skip(1) {
            let n0 = k as f64;
            let n1 = (n - k) as f64;
            let sum1 = total - sum0;
            let mean0 = sum0 / n0;
            let mean1 = sum1 / n1;
            let diff = (mean0 - mean1).abs();

            // Check drift threshold.
            let eps_drift = Self::hoeffding_bound(n0, n1, n_total, self.config.delta);
            if diff > eps_drift {
                return Some((k, DriftLevel::Drift, diff));
            }
            // Check warning threshold.
            let eps_warn = Self::hoeffding_bound(n0, n1, n_total, self.config.warning_delta);
            if diff > eps_warn && best_split.is_none() {
                best_split = Some((k, DriftLevel::Warning, diff));
            }
        }
        best_split
    }

    /// Trim the window by removing the oldest `count` elements.
    fn trim_front(&mut self, count: usize) {
        for _ in 0..count {
            if let Some(v) = self.window.pop_front() {
                self.total -= v;
            }
        }
    }
}

impl Default for Adwin {
    fn default() -> Self {
        Self::new(AdwinConfig::default()).expect("default config is valid")
    }
}

impl DriftDetector for Adwin {
    fn update(&mut self, value: f64) -> Result<DriftLevel, RillError> {
        ensure_finite("value", value)?;
        self.samples += 1;
        // Add the new value to the window.
        self.window.push_back(value);
        self.total += value;
        // Enforce the max window size by dropping the oldest element.
        if self.window.len() > self.config.max_window {
            if let Some(v) = self.window.pop_front() {
                self.total -= v;
            }
        }
        // Gate detection by minimum samples.
        if self.samples < self.config.min_samples || self.window.len() < 2 {
            self.current_level = DriftLevel::None;
            return Ok(DriftLevel::None);
        }
        // Check for drift.
        if let Some((split, level, _diff)) = self.check_splits() {
            // Trim the older portion when drift or warning is detected.
            // For Drift, trim aggressively. For Warning, keep the window intact.
            if level == DriftLevel::Drift {
                self.trim_front(split);
            }
            self.current_level = level;
        } else {
            self.current_level = DriftLevel::None;
        }
        Ok(self.current_level)
    }

    fn detected(&self) -> bool {
        self.current_level == DriftLevel::Drift
    }

    fn warning(&self) -> bool {
        self.current_level == DriftLevel::Warning
    }

    fn level(&self) -> DriftLevel {
        self.current_level
    }

    fn samples_seen(&self) -> u64 {
        self.samples
    }

    fn reset(&mut self) {
        self.window.clear();
        self.total = 0.0;
        self.samples = 0;
        self.current_level = DriftLevel::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random number in `[0, 1)` using a simple LCG.
    fn next_unit(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
    }

    #[test]
    fn default_config_is_valid() {
        let adwin = Adwin::default();
        assert_eq!(adwin.samples_seen(), 0);
        assert_eq!(adwin.level(), DriftLevel::None);
        assert_eq!(adwin.window_size(), 0);
    }

    #[test]
    fn detects_sudden_mean_shift() {
        let mut adwin = Adwin::new(AdwinConfig {
            delta: 0.05,
            warning_delta: 0.1,
            max_window: 500,
            min_samples: 5,
        })
        .unwrap();
        // Stable stream around 0.
        let mut seed = 42u64;
        for _ in 0..100 {
            let noise = 0.1 * (next_unit(&mut seed) - 0.5);
            adwin.update(noise).unwrap();
        }
        assert_eq!(adwin.level(), DriftLevel::None);
        // Sudden shift to mean 5.
        let mut detected = false;
        for _ in 0..200 {
            let noise = 0.1 * (next_unit(&mut seed) - 0.5);
            let level = adwin.update(5.0 + noise).unwrap();
            if level == DriftLevel::Drift {
                detected = true;
                break;
            }
        }
        assert!(detected, "ADWIN should detect the sudden mean shift");
    }

    #[test]
    fn no_false_positive_on_stable_stream() {
        let mut adwin = Adwin::new(AdwinConfig {
            delta: 0.002,
            warning_delta: 0.01,
            max_window: 500,
            min_samples: 10,
        })
        .unwrap();
        let mut seed = 7u64;
        for _ in 0..2000 {
            let noise = 0.5 * (next_unit(&mut seed) - 0.5);
            adwin.update(noise).unwrap();
        }
        assert!(
            !adwin.detected(),
            "false positive: drift reported on stable stream"
        );
    }

    #[test]
    fn detects_gradual_drift() {
        let mut adwin = Adwin::new(AdwinConfig {
            delta: 0.05,
            warning_delta: 0.1,
            max_window: 300,
            min_samples: 5,
        })
        .unwrap();
        // Start at mean 0, gradually increase to mean 5.
        let mut seed = 99u64;
        let mut detected = false;
        for i in 0..500 {
            let mean = (i as f64 / 100.0).min(5.0);
            let noise = 0.1 * (next_unit(&mut seed) - 0.5);
            let level = adwin.update(mean + noise).unwrap();
            if level == DriftLevel::Drift {
                detected = true;
                break;
            }
        }
        assert!(detected, "ADWIN should detect gradual drift");
    }

    #[test]
    fn window_trims_after_drift() {
        let mut adwin = Adwin::new(AdwinConfig {
            delta: 0.05,
            warning_delta: 0.1,
            max_window: 500,
            min_samples: 5,
        })
        .unwrap();
        // Build up a window of 100 samples at mean 0.
        for _ in 0..100 {
            adwin.update(0.0).unwrap();
        }
        let size_before = adwin.window_size();
        assert!(size_before > 0);
        // Shift to mean 10 to trigger drift.
        let mut trimmed = false;
        for _ in 0..200 {
            adwin.update(10.0).unwrap();
            if adwin.detected() {
                // After drift detection and trimming, the window should be
                // smaller than its peak (all 10.0 values are kept, old 0.0
                // values are trimmed).
                if adwin.window_size() < size_before + 200 {
                    trimmed = true;
                    break;
                }
            }
        }
        assert!(trimmed, "window should be trimmed after drift");
    }

    #[test]
    fn max_window_enforced() {
        let mut adwin = Adwin::new(AdwinConfig {
            max_window: 50,
            ..Default::default()
        })
        .unwrap();
        for i in 0..200u64 {
            adwin.update(i as f64).unwrap();
        }
        // Drift detection may trim the window below max_window; the invariant
        // is that the window never exceeds max_window.
        assert!(
            adwin.window_size() <= 50,
            "window should not exceed max_window, got {}",
            adwin.window_size()
        );
    }

    #[test]
    fn min_samples_gates_detection() {
        let mut adwin = Adwin::new(AdwinConfig {
            delta: 0.5,
            warning_delta: 0.5,
            max_window: 100,
            min_samples: 50,
        })
        .unwrap();
        // 48 zeros: samples_seen = 48 < min_samples = 50.
        for _ in 0..48 {
            adwin.update(0.0).unwrap();
        }
        // Sample 49: extreme value, but 49 < min_samples = 50 → no detection.
        adwin.update(100.0).unwrap();
        assert_eq!(adwin.level(), DriftLevel::None);
        // After min_samples, detection can trigger. The level is transient:
        // after drift is detected and the window trimmed, subsequent stable
        // updates reset the level to None. Track detection across the loop.
        let mut detected = false;
        for _ in 0..50 {
            let level = adwin.update(100.0).unwrap();
            if level.is_change() {
                detected = true;
            }
        }
        assert!(detected, "should have detected drift after min_samples");
    }

    #[test]
    fn reset_clears_state() {
        let mut adwin = Adwin::default();
        for _ in 0..50 {
            adwin.update(1.0).unwrap();
        }
        assert!(adwin.window_size() > 0);
        adwin.reset();
        assert_eq!(adwin.window_size(), 0);
        assert_eq!(adwin.samples_seen(), 0);
        assert_eq!(adwin.level(), DriftLevel::None);
        assert_eq!(adwin.window_mean(), 0.0);
    }

    #[test]
    fn rejects_non_finite_input() {
        let mut adwin = Adwin::default();
        assert!(adwin.update(f64::NAN).is_err());
        assert!(adwin.update(f64::INFINITY).is_err());
        assert!(adwin.update(f64::NEG_INFINITY).is_err());
        assert_eq!(adwin.samples_seen(), 0);
        assert_eq!(adwin.window_size(), 0);
    }

    #[test]
    fn rejects_invalid_config() {
        // delta out of range
        assert!(
            Adwin::new(AdwinConfig {
                delta: 0.0,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            Adwin::new(AdwinConfig {
                delta: 1.0,
                ..Default::default()
            })
            .is_err()
        );
        // warning_delta < delta
        assert!(
            Adwin::new(AdwinConfig {
                delta: 0.05,
                warning_delta: 0.01,
                ..Default::default()
            })
            .is_err()
        );
        // max_window == 0
        assert!(
            Adwin::new(AdwinConfig {
                max_window: 0,
                ..Default::default()
            })
            .is_err()
        );
        // min_samples == 0
        assert!(
            Adwin::new(AdwinConfig {
                min_samples: 0,
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn window_mean_correct() {
        let mut adwin = Adwin::new(AdwinConfig {
            max_window: 100,
            min_samples: 11, // prevent drift detection with only 10 samples
            ..Default::default()
        })
        .unwrap();
        for i in 1..=10 {
            adwin.update(i as f64).unwrap();
        }
        // mean of 1..=10 is 5.5
        assert!((adwin.window_mean() - 5.5).abs() < 1e-9);
    }

    #[test]
    fn hoeffding_bound_decreases_with_more_data() {
        // With more data, the bound should be tighter (smaller).
        let b1 = Adwin::hoeffding_bound(5.0, 5.0, 10, 0.01);
        let b2 = Adwin::hoeffding_bound(50.0, 50.0, 100, 0.01);
        assert!(
            b2 < b1,
            "bound should decrease with more data: {} vs {}",
            b2,
            b1
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut adwin = Adwin::new(AdwinConfig {
            delta: 0.01,
            warning_delta: 0.05,
            max_window: 200,
            min_samples: 5,
        })
        .unwrap();
        for i in 0..50 {
            adwin.update(i as f64 * 0.1).unwrap();
        }
        let json = serde_json::to_string(&adwin).unwrap();
        let restored: Adwin = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), 50);
        assert_eq!(restored.window_size(), adwin.window_size());
        assert!((restored.window_mean() - adwin.window_mean()).abs() < 1e-12);
        assert_eq!(restored.level(), adwin.level());
    }
}

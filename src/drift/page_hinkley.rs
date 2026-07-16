//! Page-Hinkley sequential change detection.
//!
//! The Page-Hinkley test detects sustained shifts in the mean of a scalar
//! stream. It is well suited for detecting average-value changes in target
//! values or prediction errors.
//!
//! ## Algorithm
//!
//! For each new observation `x_t`:
//!
//! 1. Update the running mean `x̄` incrementally.
//! 2. Update the cumulative sum: `S_t = α · S_{t-1} + (x_t − x̄ − δ)`
//!    where `α` is the forgetting factor and `δ` is the allowed drift
//!    magnitude.
//! 3. Track the running minimum: `m_t = min(m_{t-1}, S_t)`.
//! 4. Compute the test statistic: `PH_t = S_t − m_t`.
//! 5. Signal drift when `PH_t > threshold`.
//!
//! ## Space complexity
//!
//! `O(1)` — the detector stores only the running mean, cumulative sum,
//! minimum, and a counter.

use crate::drift::detector::{DriftDetector, DriftLevel};
use crate::error::{RillError, checked_increment, ensure_finite};

/// Configuration for [`PageHinkley`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PageHinkleyConfig {
    /// The detection threshold (λ). When the test statistic exceeds this
    /// value, a drift is reported. Must be finite and strictly positive.
    /// Larger values reduce false positives but increase detection latency.
    pub threshold: f64,

    /// The warning threshold. When the test statistic exceeds this value
    /// but not [`threshold`](Self::threshold), a warning is reported.
    /// Must be in `[0, threshold]`. Set to `0.0` to disable warnings.
    pub warning_threshold: f64,

    /// The forgetting factor (α) applied to the cumulative sum at each step.
    /// Must be in `(0, 1]`. Smaller values make the detector forget old
    /// observations faster. `1.0` gives the standard (non-forgetting)
    /// Page-Hinkley test.
    pub alpha: f64,

    /// The allowed drift magnitude (δ). The cumulative sum is penalised by
    /// this amount at each step, making the detector less sensitive to
    /// small fluctuations. Must be finite and non-negative.
    pub delta: f64,

    /// Minimum number of samples before any detection is reported.
    /// Must be greater than zero.
    pub min_samples: u64,
}

impl Default for PageHinkleyConfig {
    fn default() -> Self {
        Self {
            threshold: 50.0,
            warning_threshold: 25.0,
            alpha: 1.0,
            delta: 0.005,
            min_samples: 30,
        }
    }
}

/// Page-Hinkley sequential change detector.
///
/// Detects sustained mean shifts in a scalar stream. See the
/// [module documentation](crate::drift::page_hinkley) for the algorithm.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::{DriftDetector, DriftLevel, PageHinkley};
///
/// let mut ph = PageHinkley::default();
///
/// // Stable stream: no drift.
/// for _ in 0..200 {
///     ph.update(0.0).unwrap();
/// }
/// assert_eq!(ph.level(), DriftLevel::None);
///
/// // Sudden shift.
/// for _ in 0..100 {
///     ph.update(5.0).unwrap();
/// }
/// assert!(ph.detected());
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PageHinkley {
    config: PageHinkleyConfig,
    mean: f64,
    samples: u64,
    cum_sum: f64,
    min_cum_sum: f64,
    current_level: DriftLevel,
}

impl PageHinkley {
    /// Create a new Page-Hinkley detector with the given configuration.
    ///
    /// Returns an error if:
    /// - `threshold` is not finite or not strictly positive.
    /// - `warning_threshold` is negative or greater than `threshold`.
    /// - `alpha` is not in `(0, 1]`.
    /// - `delta` is not finite or is negative.
    /// - `min_samples` is zero.
    pub fn new(config: PageHinkleyConfig) -> Result<Self, RillError> {
        ensure_finite("threshold", config.threshold)?;
        if config.threshold <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "threshold",
                value: config.threshold,
            });
        }
        ensure_finite("warning_threshold", config.warning_threshold)?;
        if config.warning_threshold < 0.0 || config.warning_threshold > config.threshold {
            return Err(RillError::InvalidParameter {
                name: "warning_threshold",
                value: config.warning_threshold,
            });
        }
        ensure_finite("alpha", config.alpha)?;
        if config.alpha <= 0.0 || config.alpha > 1.0 {
            return Err(RillError::InvalidParameter {
                name: "alpha",
                value: config.alpha,
            });
        }
        ensure_finite("delta", config.delta)?;
        if config.delta < 0.0 {
            return Err(RillError::InvalidParameter {
                name: "delta",
                value: config.delta,
            });
        }
        if config.min_samples == 0 {
            return Err(RillError::InvalidParameter {
                name: "min_samples",
                value: 0.0,
            });
        }
        Ok(Self {
            config,
            mean: 0.0,
            samples: 0,
            cum_sum: 0.0,
            min_cum_sum: 0.0,
            current_level: DriftLevel::None,
        })
    }

    /// The current running mean of the observed stream.
    pub const fn mean(&self) -> f64 {
        self.mean
    }

    /// The current cumulative sum `S_t`.
    pub const fn cum_sum(&self) -> f64 {
        self.cum_sum
    }

    /// The current test statistic `PH_t = S_t − min(S)`.
    pub const fn ph_statistic(&self) -> f64 {
        self.cum_sum - self.min_cum_sum
    }

    /// The configuration of this detector.
    pub const fn config(&self) -> &PageHinkleyConfig {
        &self.config
    }
}

impl Default for PageHinkley {
    fn default() -> Self {
        Self::new(PageHinkleyConfig::default()).expect("default config is valid")
    }
}

impl DriftDetector for PageHinkley {
    fn update(&mut self, value: f64) -> Result<DriftLevel, RillError> {
        ensure_finite("value", value)?;
        self.samples = checked_increment(self.samples, "samples")?;
        // Incremental mean update.
        let delta = value - self.mean;
        self.mean += delta / self.samples as f64;
        // Cumulative sum with optional forgetting.
        self.cum_sum = self.config.alpha * self.cum_sum + (value - self.mean - self.config.delta);
        // Track the running minimum of the cumulative sum.
        if self.cum_sum < self.min_cum_sum {
            self.min_cum_sum = self.cum_sum;
        }
        // Determine the level, respecting the minimum-samples gate.
        if self.samples < self.config.min_samples {
            self.current_level = DriftLevel::None;
        } else {
            let stat = self.ph_statistic();
            if stat > self.config.threshold {
                self.current_level = DriftLevel::Drift;
            } else if stat > self.config.warning_threshold {
                self.current_level = DriftLevel::Warning;
            } else {
                self.current_level = DriftLevel::None;
            }
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
        self.mean = 0.0;
        self.samples = 0;
        self.cum_sum = 0.0;
        self.min_cum_sum = 0.0;
        self.current_level = DriftLevel::None;
    }

    fn last_value(&self) -> f64 {
        self.ph_statistic()
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
        let ph = PageHinkley::default();
        assert_eq!(ph.samples_seen(), 0);
        assert_eq!(ph.level(), DriftLevel::None);
        assert!(!ph.detected());
        assert!(!ph.warning());
    }

    #[test]
    fn detects_sudden_mean_shift() {
        let mut ph = PageHinkley::new(PageHinkleyConfig {
            threshold: 10.0,
            warning_threshold: 5.0,
            alpha: 1.0,
            delta: 0.01,
            min_samples: 10,
        })
        .unwrap();
        // Stable stream around 0.
        let mut seed = 42u64;
        for _ in 0..100 {
            let noise = 0.1 * (next_unit(&mut seed) - 0.5);
            ph.update(noise).unwrap();
        }
        assert_eq!(ph.level(), DriftLevel::None);
        // Sudden shift to mean 5.
        let mut detected = false;
        for _ in 0..100 {
            let noise = 0.1 * (next_unit(&mut seed) - 0.5);
            let level = ph.update(5.0 + noise).unwrap();
            if level == DriftLevel::Drift {
                detected = true;
                break;
            }
        }
        assert!(detected, "should detect the mean shift");
    }

    #[test]
    fn no_false_positive_on_stable_stream() {
        let mut ph = PageHinkley::new(PageHinkleyConfig {
            threshold: 20.0,
            warning_threshold: 10.0,
            alpha: 0.99,
            delta: 0.01,
            min_samples: 30,
        })
        .unwrap();
        // 1000 samples of Gaussian-ish noise around 0 with small variance.
        let mut seed = 7u64;
        for _ in 0..1000 {
            let noise = 0.5 * (next_unit(&mut seed) - 0.5);
            ph.update(noise).unwrap();
        }
        assert!(
            !ph.detected(),
            "false positive: drift reported on stable stream (stat={})",
            ph.ph_statistic()
        );
    }

    #[test]
    fn works_on_prediction_error_stream() {
        // Simulate prediction errors: initially small, then large after drift.
        let mut ph = PageHinkley::new(PageHinkleyConfig {
            threshold: 5.0,
            warning_threshold: 2.0,
            alpha: 1.0,
            delta: 0.0,
            min_samples: 5,
        })
        .unwrap();
        // Low-error phase.
        for _ in 0..50 {
            ph.update(0.1).unwrap();
        }
        assert_eq!(ph.level(), DriftLevel::None);
        // High-error phase.
        let mut detected_step = None;
        for i in 0..100 {
            let level = ph.update(2.0).unwrap();
            if level == DriftLevel::Drift {
                detected_step = Some(i);
                break;
            }
        }
        assert!(detected_step.is_some(), "should detect error increase");
    }

    #[test]
    fn warning_before_drift() {
        let mut ph = PageHinkley::new(PageHinkleyConfig {
            threshold: 100.0,
            warning_threshold: 0.5,
            alpha: 1.0,
            delta: 0.0,
            min_samples: 5,
        })
        .unwrap();
        // Baseline phase: feed 0.0 so the running mean settles at 0.
        for _ in 0..50 {
            ph.update(0.0).unwrap();
        }
        // Shift phase: feed 1.0; the mean lags so cum_sum grows.
        for _ in 0..100 {
            ph.update(1.0).unwrap();
            if ph.warning() || ph.detected() {
                break;
            }
        }
        assert!(
            ph.warning() || ph.detected(),
            "expected warning or drift, got {:?}, stat={}",
            ph.level(),
            ph.ph_statistic()
        );
    }

    #[test]
    fn min_samples_gates_detection() {
        let mut ph = PageHinkley::new(PageHinkleyConfig {
            threshold: 0.001,
            warning_threshold: 0.0,
            alpha: 1.0,
            delta: 0.0,
            min_samples: 100,
        })
        .unwrap();
        // Baseline phase: 98 zeros establish a mean near 0.
        for _ in 0..98 {
            ph.update(0.0).unwrap();
        }
        // Sample 99: shift to 1000.0, but 99 < min_samples=100 → no detection.
        ph.update(1000.0).unwrap();
        assert_eq!(ph.level(), DriftLevel::None);
        // Sample 100: ≥ min_samples, detection can now trigger.
        ph.update(1000.0).unwrap();
        assert!(ph.detected() || ph.warning());
    }

    #[test]
    fn reset_clears_state() {
        let mut ph = PageHinkley::new(PageHinkleyConfig {
            threshold: 1.0,
            warning_threshold: 0.5,
            alpha: 1.0,
            delta: 0.0,
            min_samples: 5,
        })
        .unwrap();
        // Baseline phase: 10 zeros establish a mean near 0.
        for _ in 0..10 {
            ph.update(0.0).unwrap();
        }
        // Shift phase: 10 tens trigger detection (mean lags behind).
        for _ in 0..10 {
            ph.update(10.0).unwrap();
        }
        assert!(ph.detected() || ph.warning());
        ph.reset();
        assert_eq!(ph.samples_seen(), 0);
        assert_eq!(ph.level(), DriftLevel::None);
        assert_eq!(ph.mean(), 0.0);
        assert_eq!(ph.cum_sum(), 0.0);
        assert_eq!(ph.ph_statistic(), 0.0);
    }

    #[test]
    fn rejects_non_finite_input() {
        let mut ph = PageHinkley::default();
        assert!(ph.update(f64::NAN).is_err());
        assert!(ph.update(f64::INFINITY).is_err());
        assert!(ph.update(f64::NEG_INFINITY).is_err());
        assert_eq!(ph.samples_seen(), 0);
    }

    #[test]
    fn rejects_invalid_config() {
        // threshold <= 0
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                threshold: 0.0,
                ..Default::default()
            })
            .is_err()
        );
        // threshold NaN
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                threshold: f64::NAN,
                ..Default::default()
            })
            .is_err()
        );
        // warning_threshold > threshold
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                threshold: 10.0,
                warning_threshold: 20.0,
                ..Default::default()
            })
            .is_err()
        );
        // warning_threshold < 0
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                warning_threshold: -1.0,
                ..Default::default()
            })
            .is_err()
        );
        // alpha <= 0
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                alpha: 0.0,
                ..Default::default()
            })
            .is_err()
        );
        // alpha > 1
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                alpha: 1.5,
                ..Default::default()
            })
            .is_err()
        );
        // delta < 0
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                delta: -1.0,
                ..Default::default()
            })
            .is_err()
        );
        // min_samples == 0
        assert!(
            PageHinkley::new(PageHinkleyConfig {
                min_samples: 0,
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn forgetting_factor_detects_drift() {
        // Both the forgetting (alpha < 1) and standard (alpha = 1) variants
        // should detect a sustained mean shift. The forgetting factor decays
        // old contributions, so for a single shift the standard variant is
        // typically faster — we only assert both detect the drift.
        let config_forgetting = PageHinkleyConfig {
            threshold: 5.0,
            warning_threshold: 0.0,
            alpha: 0.8,
            delta: 0.0,
            min_samples: 10,
        };
        let config_standard = PageHinkleyConfig {
            alpha: 1.0,
            ..config_forgetting
        };
        let mut ph_f = PageHinkley::new(config_forgetting).unwrap();
        let mut ph_s = PageHinkley::new(config_standard).unwrap();
        // Long stable phase at mean 0.
        for _ in 0..500 {
            ph_f.update(0.0).unwrap();
            ph_s.update(0.0).unwrap();
        }
        assert_eq!(ph_f.level(), DriftLevel::None);
        assert_eq!(ph_s.level(), DriftLevel::None);
        // Shift to mean 2.0.
        let mut steps_f = None;
        let mut steps_s = None;
        for i in 0..200 {
            let lv_f = ph_f.update(2.0).unwrap();
            let lv_s = ph_s.update(2.0).unwrap();
            if steps_f.is_none() && lv_f == DriftLevel::Drift {
                steps_f = Some(i);
            }
            if steps_s.is_none() && lv_s == DriftLevel::Drift {
                steps_s = Some(i);
            }
            if steps_f.is_some() && steps_s.is_some() {
                break;
            }
        }
        assert!(steps_f.is_some(), "forgetting variant should detect drift");
        assert!(steps_s.is_some(), "standard variant should detect drift");
    }

    #[test]
    fn ph_statistic_is_non_negative() {
        let mut ph = PageHinkley::default();
        let mut seed = 123u64;
        for _ in 0..200 {
            let v = next_unit(&mut seed) * 2.0 - 1.0;
            ph.update(v).unwrap();
            assert!(
                ph.ph_statistic() >= 0.0,
                "PH statistic should be non-negative, got {}",
                ph.ph_statistic()
            );
        }
    }

    #[test]
    fn mean_tracks_stream_average() {
        let mut ph = PageHinkley::default();
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];
        for &v in &values {
            ph.update(v).unwrap();
        }
        assert!((ph.mean() - 3.0).abs() < 1e-9);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut ph = PageHinkley::new(PageHinkleyConfig {
            threshold: 15.0,
            warning_threshold: 7.0,
            alpha: 0.95,
            delta: 0.02,
            min_samples: 20,
        })
        .unwrap();
        for i in 0..50 {
            ph.update(i as f64 * 0.1).unwrap();
        }
        let json = serde_json::to_string(&ph).unwrap();
        let restored: PageHinkley = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), 50);
        assert!((restored.mean() - ph.mean()).abs() < 1e-12);
        assert!((restored.cum_sum() - ph.cum_sum()).abs() < 1e-12);
        assert_eq!(restored.level(), ph.level());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn config_serde_roundtrip() {
        let config = PageHinkleyConfig {
            threshold: 42.0,
            warning_threshold: 21.0,
            alpha: 0.7,
            delta: 0.3,
            min_samples: 15,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: PageHinkleyConfig = serde_json::from_str(&json).unwrap();
        assert!((restored.threshold - 42.0).abs() < 1e-12);
        assert!((restored.warning_threshold - 21.0).abs() < 1e-12);
        assert!((restored.alpha - 0.7).abs() < 1e-12);
        assert!((restored.delta - 0.3).abs() < 1e-12);
        assert_eq!(restored.min_samples, 15);
    }
}

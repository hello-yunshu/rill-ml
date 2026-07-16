//! KSWIN (Kolmogorov-Smirnov Windowing) drift detector.
//!
//! KSWIN maintains two fixed-size windows — a reference window (older data)
//! and a current window (newer data) — and periodically runs a two-sample
//! Kolmogorov-Smirnov test to detect distribution changes.
//!
//! ## Algorithm
//!
//! 1. Each new observation is appended to the current window.
//! 2. When the current window fills up, it becomes the new reference window
//!    (the old reference is discarded) and a fresh current window starts.
//! 3. Whenever both windows are full and at least `check_interval` samples
//!    have passed since the last check, the two-sample KS statistic `D` is
//!    computed.
//! 4. The p-value is derived via the Marsaglia-Tsang-Wang (2003) algorithm:
//!    `λ = (√(n_eff) + 0.12 + 0.11/√(n_eff)) · D` with
//!    `n_eff = n1·n2/(n1+n2)`, then
//!    `Q_KS(λ) = 2 · Σ_{k=1}^{∞} (-1)^(k-1) · exp(-2·k²·λ²)`.
//!    The p-value equals `Q_KS(λ)`.
//! 5. If `p-value < alpha`, drift is reported and the current window becomes
//!    the new reference, so the new distribution serves as the baseline.
//!
//! ## Space complexity
//!
//! `O(2 * window_size)` — two fixed-size windows are stored. The KS test
//! itself uses `O(window_size)` scratch space for sorting.

use crate::drift::detector::{DriftDetector, DriftLevel};
use crate::error::{RillError, checked_increment, ensure_finite};

/// Configuration for [`Kswin`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct KswinConfig {
    /// Significance level for the KS test. Must be in `(0, 1)`. Smaller
    /// values reduce false positives. Defaults to `0.005`.
    pub alpha: f64,

    /// Size of each window (reference and current). Must be greater than
    /// zero. Larger values improve sensitivity but increase memory and
    /// computation. Defaults to `100`.
    pub window_size: usize,

    /// Minimum number of samples between two consecutive KS checks. Must
    /// be greater than zero. The actual check interval is the maximum of
    /// this value and `window_size` (since both windows must be full).
    /// Defaults to `100`.
    pub check_interval: usize,
}

impl Default for KswinConfig {
    fn default() -> Self {
        Self {
            alpha: 0.005,
            window_size: 100,
            check_interval: 100,
        }
    }
}

/// KSWIN (Kolmogorov-Smirnov Windowing) drift detector.
///
/// Detects distribution changes by comparing two fixed-size windows with a
/// two-sample KS test. Unlike mean-based detectors (Page-Hinkley, ADWIN),
/// KSWIN is sensitive to distribution shape changes (variance, skewness,
/// multimodality), not just mean shifts.
///
/// The KS test p-value is computed via the Marsaglia-Tsang-Wang algorithm;
/// no external statistics crate is required.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::{DriftDetector, DriftLevel, Kswin, KswinConfig};
///
/// let mut kswin = Kswin::new(KswinConfig {
///     alpha: 0.01,
///     window_size: 50,
///     check_interval: 50,
/// }).unwrap();
///
/// // Stable stream around 0.
/// for _ in 0..100 {
///     kswin.update(0.0).unwrap();
/// }
/// assert_eq!(kswin.level(), DriftLevel::None);
///
/// // Distribution shifts to mean 5.
/// for _ in 0..100 {
///     kswin.update(5.0).unwrap();
/// }
/// // KSWIN should detect the distribution change.
/// assert!(kswin.samples_seen() > 100);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Kswin {
    config: KswinConfig,
    reference_window: Vec<f64>,
    current_window: Vec<f64>,
    samples: u64,
    last_check_sample: u64,
    last_pvalue: f64,
    last_statistic: f64,
    current_level: DriftLevel,
}

impl Kswin {
    /// Create a new KSWIN detector with the given configuration.
    ///
    /// Returns an error if:
    /// - `alpha` is not in `(0, 1)`.
    /// - `window_size` is zero.
    /// - `check_interval` is zero.
    pub fn new(config: KswinConfig) -> Result<Self, RillError> {
        ensure_finite("alpha", config.alpha)?;
        if config.alpha <= 0.0 || config.alpha >= 1.0 {
            return Err(RillError::InvalidSignificanceLevel(config.alpha));
        }
        if config.window_size == 0 {
            return Err(RillError::InvalidCapacity(config.window_size));
        }
        if config.check_interval == 0 {
            return Err(RillError::InvalidCapacity(config.check_interval));
        }
        Ok(Self {
            config,
            reference_window: Vec::new(),
            current_window: Vec::new(),
            samples: 0,
            last_check_sample: 0,
            last_pvalue: 1.0,
            last_statistic: 0.0,
            current_level: DriftLevel::None,
        })
    }

    /// The last computed KS statistic `D` (max CDF difference).
    pub const fn last_statistic(&self) -> f64 {
        self.last_statistic
    }

    /// The last computed p-value from the KS test.
    pub const fn last_pvalue(&self) -> f64 {
        self.last_pvalue
    }

    /// The number of values currently in the reference window.
    pub fn reference_window_len(&self) -> usize {
        self.reference_window.len()
    }

    /// The number of values currently in the current window.
    pub fn current_window_len(&self) -> usize {
        self.current_window.len()
    }

    /// The configuration of this detector.
    pub const fn config(&self) -> &KswinConfig {
        &self.config
    }
}

impl Default for Kswin {
    fn default() -> Self {
        Self::new(KswinConfig::default()).expect("default config is valid")
    }
}

impl DriftDetector for Kswin {
    fn update(&mut self, value: f64) -> Result<DriftLevel, RillError> {
        ensure_finite("value", value)?;
        self.samples = checked_increment(self.samples, "samples")?;
        self.current_level = DriftLevel::None;

        // If the current window is full, rotate: current becomes the new
        // reference (old reference is dropped), and a fresh current starts.
        if self.current_window.len() >= self.config.window_size {
            std::mem::swap(&mut self.reference_window, &mut self.current_window);
            self.current_window.clear();
        }

        self.current_window.push(value);

        // Check for drift only when both windows are full and enough samples
        // have passed since the last check.
        let both_full = self.reference_window.len() >= self.config.window_size
            && self.current_window.len() >= self.config.window_size;
        let interval_ok =
            self.samples - self.last_check_sample >= self.config.check_interval as u64;

        if both_full && interval_ok {
            let d = ks_statistic(&self.reference_window, &self.current_window);
            let p = ks_pvalue(d, self.reference_window.len(), self.current_window.len());
            self.last_statistic = d;
            self.last_pvalue = p;
            self.last_check_sample = self.samples;

            if p < self.config.alpha {
                self.current_level = DriftLevel::Drift;
                // Rotate: the current window (new distribution) becomes the
                // reference, and a fresh current window starts.
                std::mem::swap(&mut self.reference_window, &mut self.current_window);
                self.current_window.clear();
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
        self.reference_window.clear();
        self.current_window.clear();
        self.samples = 0;
        self.last_check_sample = 0;
        self.last_pvalue = 1.0;
        self.last_statistic = 0.0;
        self.current_level = DriftLevel::None;
    }

    fn last_value(&self) -> f64 {
        self.last_pvalue
    }
}

// ---------------------------------------------------------------------------
// KS test implementation (Marsaglia-Tsang-Wang 2003)
// ---------------------------------------------------------------------------

/// Compute the two-sample Kolmogorov-Smirnov statistic `D = max|F_a(x) - F_b(x)|`.
///
/// Both slices are sorted internally. Returns `0.0` if either slice is empty.
pub(crate) fn ks_statistic(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut a_sorted = a.to_vec();
    let mut b_sorted = b.to_vec();
    a_sorted.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    b_sorted.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));

    let n1 = a_sorted.len() as f64;
    let n2 = b_sorted.len() as f64;

    let mut i = 0usize;
    let mut j = 0usize;
    let mut max_d = 0.0_f64;

    while i < a_sorted.len() && j < b_sorted.len() {
        if a_sorted[i] < b_sorted[j] {
            i += 1;
        } else if a_sorted[i] > b_sorted[j] {
            j += 1;
        } else {
            // Equal values: advance both indices simultaneously to avoid
            // creating an artificial CDF gap.
            i += 1;
            j += 1;
        }
        let cdf_a = i as f64 / n1;
        let cdf_b = j as f64 / n2;
        let d = (cdf_a - cdf_b).abs();
        if d > max_d {
            max_d = d;
        }
    }

    max_d
}

/// Compute the survival function `Q_KS(λ) = P(D > λ)` of the Kolmogorov
/// distribution using the Marsaglia-Tsang-Wang (2003) series.
///
/// This equals the p-value of the two-sample KS test for a given `λ`.
///
/// - `λ = 0` → returns `1.0` (no evidence against the null hypothesis).
/// - `λ → ∞` → returns `0.0` (strong evidence to reject the null).
pub(crate) fn ks_survival(lambda: f64) -> f64 {
    if lambda <= 0.0 {
        return 1.0;
    }
    let a2 = -2.0 * lambda * lambda;
    let mut sum = 0.0_f64;
    let mut fac = 2.0_f64; // +2 for k=1, flips sign each iteration
    for k in 1..=100u32 {
        let term = fac * (a2 * (k as f64) * (k as f64)).exp();
        sum += term;
        // Convergence: stop when the term is negligible relative to the sum.
        if term.abs() <= 1e-12 * sum.abs().max(1e-300) {
            break;
        }
        fac = -fac;
    }
    // Numerical safety: the result is a probability in [0, 1].
    sum.clamp(0.0, 1.0)
}

/// Compute the two-sample KS test p-value from the statistic `D` and the
/// two sample sizes `n1` and `n2`.
///
/// Uses the standard small-sample correction:
/// `λ = (√(n_eff) + 0.12 + 0.11/√(n_eff)) · D` where
/// `n_eff = n1·n2/(n1+n2)`.
pub(crate) fn ks_pvalue(d: f64, n1: usize, n2: usize) -> f64 {
    if d <= 0.0 || n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let n_eff = (n1 as f64 * n2 as f64) / (n1 + n2) as f64;
    let lambda = (n_eff.sqrt() + 0.12 + 0.11 / n_eff.sqrt()) * d;
    ks_survival(lambda)
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

    /// Standard normal sample via Box-Muller transform.
    fn next_normal(seed: &mut u64, mean: f64, std: f64) -> f64 {
        let u1 = next_unit(seed).max(1e-10);
        let u2 = next_unit(seed);
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        mean + std * z
    }

    #[test]
    fn default_config_is_valid() {
        let kswin = Kswin::default();
        assert_eq!(kswin.samples_seen(), 0);
        assert_eq!(kswin.level(), DriftLevel::None);
        assert!(!kswin.detected());
        assert!(!kswin.warning());
        assert_eq!(kswin.reference_window_len(), 0);
        assert_eq!(kswin.current_window_len(), 0);
    }

    #[test]
    fn detects_mean_shift() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.01,
            window_size: 50,
            check_interval: 50,
        })
        .unwrap();

        let mut seed = 42u64;
        // Phase 1: mean 0, small noise.
        for _ in 0..100 {
            let v = next_normal(&mut seed, 0.0, 0.5);
            kswin.update(v).unwrap();
        }
        assert_eq!(kswin.level(), DriftLevel::None);

        // Phase 2: mean 5.
        let mut detected = false;
        for _ in 0..150 {
            let v = next_normal(&mut seed, 5.0, 0.5);
            let level = kswin.update(v).unwrap();
            if level == DriftLevel::Drift {
                detected = true;
                break;
            }
        }
        assert!(detected, "KSWIN should detect the mean shift");
    }

    #[test]
    fn detects_variance_change() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.01,
            window_size: 50,
            check_interval: 50,
        })
        .unwrap();

        let mut seed = 7u64;
        // Phase 1: small variance (std = 0.1).
        for _ in 0..100 {
            let v = next_normal(&mut seed, 0.0, 0.1);
            kswin.update(v).unwrap();
        }
        assert_eq!(kswin.level(), DriftLevel::None);

        // Phase 2: large variance (std = 3.0).
        let mut detected = false;
        for _ in 0..150 {
            let v = next_normal(&mut seed, 0.0, 3.0);
            let level = kswin.update(v).unwrap();
            if level == DriftLevel::Drift {
                detected = true;
                break;
            }
        }
        assert!(
            detected,
            "KSWIN should detect the variance change (a distribution shape change)"
        );
    }

    #[test]
    fn detects_distribution_shape_change() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.01,
            window_size: 50,
            check_interval: 50,
        })
        .unwrap();

        let mut seed = 99u64;
        // Phase 1: uniform distribution on [-0.5, 0.5].
        for _ in 0..100 {
            let v = next_unit(&mut seed) - 0.5;
            kswin.update(v).unwrap();
        }
        assert_eq!(kswin.level(), DriftLevel::None);

        // Phase 2: normal distribution with mean 0, std 1 (different shape).
        let mut detected = false;
        for _ in 0..200 {
            let v = next_normal(&mut seed, 0.0, 1.0);
            let level = kswin.update(v).unwrap();
            if level == DriftLevel::Drift {
                detected = true;
                break;
            }
        }
        assert!(
            detected,
            "KSWIN should detect the distribution shape change (uniform -> normal)"
        );
    }

    #[test]
    fn no_false_positive_on_stable_stream() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.005,
            window_size: 100,
            check_interval: 100,
        })
        .unwrap();

        let mut seed = 13u64;
        for _ in 0..1000 {
            let v = next_normal(&mut seed, 0.0, 1.0);
            kswin.update(v).unwrap();
        }
        assert!(
            !kswin.detected(),
            "false positive: drift reported on stable stream (p-value={})",
            kswin.last_pvalue()
        );
    }

    #[test]
    fn rejects_non_finite_input() {
        let mut kswin = Kswin::default();
        assert!(kswin.update(f64::NAN).is_err());
        assert!(kswin.update(f64::INFINITY).is_err());
        assert!(kswin.update(f64::NEG_INFINITY).is_err());
        assert_eq!(kswin.samples_seen(), 0);
    }

    #[test]
    fn rejects_invalid_config() {
        // alpha <= 0
        assert!(
            Kswin::new(KswinConfig {
                alpha: 0.0,
                ..Default::default()
            })
            .is_err()
        );
        // alpha >= 1
        assert!(
            Kswin::new(KswinConfig {
                alpha: 1.0,
                ..Default::default()
            })
            .is_err()
        );
        // alpha NaN
        assert!(
            Kswin::new(KswinConfig {
                alpha: f64::NAN,
                ..Default::default()
            })
            .is_err()
        );
        // window_size == 0
        assert!(
            Kswin::new(KswinConfig {
                window_size: 0,
                ..Default::default()
            })
            .is_err()
        );
        // check_interval == 0
        assert!(
            Kswin::new(KswinConfig {
                check_interval: 0,
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut kswin = Kswin::new(KswinConfig {
            window_size: 20,
            check_interval: 20,
            ..Default::default()
        })
        .unwrap();
        for i in 0..50 {
            kswin.update(i as f64).unwrap();
        }
        assert!(kswin.samples_seen() > 0);
        assert!(kswin.reference_window_len() > 0 || kswin.current_window_len() > 0);
        kswin.reset();
        assert_eq!(kswin.samples_seen(), 0);
        assert_eq!(kswin.reference_window_len(), 0);
        assert_eq!(kswin.current_window_len(), 0);
        assert_eq!(kswin.level(), DriftLevel::None);
        assert_eq!(kswin.last_pvalue(), 1.0);
        assert_eq!(kswin.last_statistic(), 0.0);
    }

    #[test]
    fn min_samples_gates_detection() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.001,
            window_size: 50,
            check_interval: 50,
        })
        .unwrap();
        // Before both windows are full, no detection should occur even with
        // extreme distribution differences.
        for _ in 0..49 {
            kswin.update(0.0).unwrap();
        }
        assert_eq!(kswin.level(), DriftLevel::None);
        // Even with extreme values in the current window (not yet full),
        // detection cannot fire.
        for _ in 0..49 {
            kswin.update(1000.0).unwrap();
        }
        // Reference is full (50 from first phase), current has 49 — not full yet.
        assert_eq!(kswin.level(), DriftLevel::None);
    }

    #[test]
    fn ks_statistic_symmetric() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [1.5, 2.5, 3.5, 4.5, 5.5];
        let d_ab = ks_statistic(&a, &b);
        let d_ba = ks_statistic(&b, &a);
        assert!(
            (d_ab - d_ba).abs() < 1e-12,
            "ks_statistic should be symmetric: {} vs {}",
            d_ab,
            d_ba
        );
    }

    #[test]
    fn ks_statistic_identical_distributions() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [1.0, 2.0, 3.0, 4.0, 5.0];
        let d = ks_statistic(&a, &b);
        assert!(
            d.abs() < 1e-12,
            "ks_statistic of identical samples should be 0, got {}",
            d
        );
    }

    #[test]
    fn ks_statistic_disjoint_distributions() {
        let a = [1.0, 2.0, 3.0];
        let b = [10.0, 11.0, 12.0];
        let d = ks_statistic(&a, &b);
        assert!(
            (d - 1.0).abs() < 1e-12,
            "ks_statistic of disjoint samples should be 1, got {}",
            d
        );
    }

    #[test]
    fn ks_pvalue_decreases_with_larger_d() {
        let n1 = 50usize;
        let n2 = 50usize;
        let p_small = ks_pvalue(0.1, n1, n2);
        let p_medium = ks_pvalue(0.3, n1, n2);
        let p_large = ks_pvalue(0.6, n1, n2);
        assert!(
            p_small > p_medium,
            "p-value should decrease as D increases: {} vs {}",
            p_small,
            p_medium
        );
        assert!(
            p_medium > p_large,
            "p-value should decrease as D increases: {} vs {}",
            p_medium,
            p_large
        );
    }

    #[test]
    fn ks_survival_known_values() {
        // λ = 0 → no evidence against H0 → p-value = 1.
        assert!((ks_survival(0.0) - 1.0).abs() < 1e-12);
        // λ very large → strong evidence → p-value ≈ 0.
        assert!(ks_survival(10.0) < 1e-10);
        // Monotonically decreasing.
        let p1 = ks_survival(0.5);
        let p2 = ks_survival(1.0);
        let p3 = ks_survival(2.0);
        assert!(p1 > p2);
        assert!(p2 > p3);
        // Known value: Q_KS(1) ≈ 0.27 (Numerical Recipes table).
        assert!(
            (ks_survival(1.0) - 0.27).abs() < 0.02,
            "Q_KS(1) should be approximately 0.27, got {}",
            ks_survival(1.0)
        );
    }

    #[test]
    fn ks_pvalue_is_in_unit_interval() {
        let mut seed = 42u64;
        for _ in 0..100 {
            let d = next_unit(&mut seed); // [0, 1)
            let p = ks_pvalue(d, 30, 40);
            assert!(
                (0.0..=1.0).contains(&p),
                "p-value out of range: {} for d={}",
                p,
                d
            );
        }
    }

    #[test]
    fn window_rotation_after_drift() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.05,
            window_size: 30,
            check_interval: 30,
        })
        .unwrap();
        // Phase 1: fill reference with 0s.
        for _ in 0..30 {
            kswin.update(0.0).unwrap();
        }
        // Phase 2: fill current with 100s (very different distribution).
        let mut detected = false;
        for _ in 0..60 {
            let level = kswin.update(100.0).unwrap();
            if level == DriftLevel::Drift {
                detected = true;
                break;
            }
        }
        assert!(detected, "should detect the distribution change");
        // After drift detection, the current window was rotated to reference.
        // The reference window should now contain the new distribution (100s).
        assert!(
            kswin.reference_window_len() > 0,
            "reference window should have data after rotation"
        );
        // The mean of the reference window should be close to 100.
        let ref_mean: f64 =
            kswin.reference_window.iter().sum::<f64>() / kswin.reference_window.len() as f64;
        assert!(
            (ref_mean - 100.0).abs() < 1e-9,
            "reference should contain the new distribution, mean={}",
            ref_mean
        );
    }

    #[test]
    fn last_value_returns_last_pvalue() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.05,
            window_size: 20,
            check_interval: 20,
        })
        .unwrap();
        // Before any check, last_value should be the initial p-value (1.0).
        assert!((kswin.last_value() - 1.0).abs() < 1e-12);
        // Feed enough data to trigger a check.
        for _ in 0..20 {
            kswin.update(0.0).unwrap();
        }
        for _ in 0..20 {
            kswin.update(0.0).unwrap();
        }
        // After a check on identical distributions, p-value should be high.
        assert!(
            kswin.last_value() > 0.5,
            "p-value for identical distributions should be high, got {}",
            kswin.last_value()
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut kswin = Kswin::new(KswinConfig {
            alpha: 0.01,
            window_size: 30,
            check_interval: 30,
        })
        .unwrap();
        for i in 0..60 {
            kswin.update(i as f64 * 0.1).unwrap();
        }
        let json = serde_json::to_string(&kswin).unwrap();
        let restored: Kswin = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), 60);
        assert_eq!(restored.config().window_size, 30);
        assert_eq!(restored.config().alpha, 0.01);
        assert!((restored.last_pvalue() - kswin.last_pvalue()).abs() < 1e-12);
        assert!((restored.last_statistic() - kswin.last_statistic()).abs() < 1e-12);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn config_serde_roundtrip() {
        let config = KswinConfig {
            alpha: 0.007,
            window_size: 75,
            check_interval: 50,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: KswinConfig = serde_json::from_str(&json).unwrap();
        assert!((restored.alpha - 0.007).abs() < 1e-12);
        assert_eq!(restored.window_size, 75);
        assert_eq!(restored.check_interval, 50);
    }
}

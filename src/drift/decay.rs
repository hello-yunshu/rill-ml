//! Decay-aware learning utilities.
//!
//! This module provides three utilities for adapting to non-stationary
//! streams:
//!
//! - [`TimeDecayedMean`]: exponentially decays the weight of older
//!   observations, giving more influence to recent data.
//! - [`LearningRateScheduler`]: adjusts the learning rate based on the
//!   current drift level reported by a detector.
//! - [`FixedWindowBuffer`]: a bounded ring buffer that stores the most
//!   recent `N` observations for window-based training or replay.
//!
//! All three components use bounded memory and are independent of any
//! specific model or detector.

use crate::drift::detector::DriftLevel;
use crate::error::{RillError, checked_finite_add, ensure_finite};

// ---------------------------------------------------------------------------
// TimeDecayedMean
// ---------------------------------------------------------------------------

/// An exponentially time-decayed weighted mean.
///
/// Each observation `(t_i, v_i)` contributes `v_i · exp(-decay · (t_now - t_i))`
/// to the weighted sum. The mean is `Σ(v_i · w_i) / Σ(w_i)`. Older
/// observations receive exponentially smaller weights, making the statistic
/// responsive to recent changes.
///
/// Time complexity per update: `O(1)`. Space complexity: `O(1)`.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::TimeDecayedMean;
///
/// let mut m = TimeDecayedMean::new(0.1).unwrap();
/// m.update(0.0, 10.0).unwrap();
/// m.update(1.0, 20.0).unwrap();
/// m.update(2.0, 30.0).unwrap();
/// // The mean should be closer to 30 than to the simple average (20)
/// // because recent observations are weighted higher.
/// let v = m.value().unwrap();
/// assert!(v > 20.0, "recent data should dominate: {}", v);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TimeDecayedMean {
    decay: f64,
    weighted_sum: f64,
    weight_total: f64,
    last_time: Option<f64>,
}

impl TimeDecayedMean {
    /// Create a new time-decayed mean with the given decay rate.
    ///
    /// `decay` must be finite and strictly positive. Larger values cause
    /// faster forgetting.
    pub fn new(decay: f64) -> Result<Self, RillError> {
        ensure_finite("decay", decay)?;
        if decay <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "decay",
                value: decay,
            });
        }
        Ok(Self {
            decay,
            weighted_sum: 0.0,
            weight_total: 0.0,
            last_time: None,
        })
    }

    /// The configured decay rate.
    pub const fn decay(&self) -> f64 {
        self.decay
    }

    /// Update with a new observation at time `t` with value `v`.
    ///
    /// `t` must be finite and must not decrease (i.e., `t >= last_time`).
    /// `v` must be finite.
    pub fn update(&mut self, time: f64, value: f64) -> Result<(), RillError> {
        ensure_finite("time", time)?;
        ensure_finite("value", value)?;
        match self.last_time {
            None => {
                // First sample: seed directly.
                self.weighted_sum = value;
                self.weight_total = 1.0;
            }
            Some(prev) => {
                if time < prev {
                    return Err(RillError::InvalidParameter {
                        name: "time",
                        value: time,
                    });
                }
                let dt = time - prev;
                let factor = (-self.decay * dt).exp();
                self.weighted_sum =
                    checked_finite_add(factor * self.weighted_sum, value, "weighted_sum")?;
                self.weight_total =
                    checked_finite_add(factor * self.weight_total, 1.0, "weight_total")?;
            }
        }
        self.last_time = Some(time);
        Ok(())
    }

    /// The current decayed mean, or `None` if no observations have been seen.
    pub fn value(&self) -> Option<f64> {
        if self.weight_total > 0.0 {
            Some(self.weighted_sum / self.weight_total)
        } else {
            None
        }
    }

    /// The total weight accumulated so far.
    pub const fn weight_total(&self) -> f64 {
        self.weight_total
    }

    /// The last observation's timestamp, or `None` if no observations yet.
    pub const fn last_time(&self) -> Option<f64> {
        self.last_time
    }

    /// Reset to the initial (no-data) state.
    pub fn reset(&mut self) {
        self.weighted_sum = 0.0;
        self.weight_total = 0.0;
        self.last_time = None;
    }
}

// ---------------------------------------------------------------------------
// LearningRateScheduler
// ---------------------------------------------------------------------------

/// A learning-rate scheduler that adjusts the rate based on drift state.
///
/// - [`DriftLevel::None`]: use `base_lr`.
/// - [`DriftLevel::Warning`]: use `base_lr * warning_multiplier`.
/// - [`DriftLevel::Drift`]: use `base_lr * drift_multiplier`.
///
/// Space complexity: `O(1)`.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::{DriftLevel, LearningRateScheduler};
///
/// let mut sched = LearningRateScheduler::new(0.01, 2.0, 5.0).unwrap();
/// assert!((sched.current_lr() - 0.01).abs() < 1e-12);
///
/// sched.on_drift_level(DriftLevel::Warning);
/// assert!((sched.current_lr() - 0.02).abs() < 1e-12);
///
/// sched.on_drift_level(DriftLevel::Drift);
/// assert!((sched.current_lr() - 0.05).abs() < 1e-12);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LearningRateScheduler {
    base_lr: f64,
    warning_multiplier: f64,
    drift_multiplier: f64,
    current_state: DriftLevel,
}

impl LearningRateScheduler {
    /// Create a new scheduler.
    ///
    /// Returns an error if:
    /// - `base_lr` is not finite or not strictly positive.
    /// - `warning_multiplier` is not finite or less than 1.
    /// - `drift_multiplier` is not finite or less than `warning_multiplier`.
    pub fn new(
        base_lr: f64,
        warning_multiplier: f64,
        drift_multiplier: f64,
    ) -> Result<Self, RillError> {
        ensure_finite("base_lr", base_lr)?;
        ensure_finite("warning_multiplier", warning_multiplier)?;
        ensure_finite("drift_multiplier", drift_multiplier)?;
        if base_lr <= 0.0 {
            return Err(RillError::InvalidLearningRate(base_lr));
        }
        if warning_multiplier < 1.0 {
            return Err(RillError::InvalidParameter {
                name: "warning_multiplier",
                value: warning_multiplier,
            });
        }
        if drift_multiplier < warning_multiplier {
            return Err(RillError::InvalidParameter {
                name: "drift_multiplier",
                value: drift_multiplier,
            });
        }
        Ok(Self {
            base_lr,
            warning_multiplier,
            drift_multiplier,
            current_state: DriftLevel::None,
        })
    }

    /// The configured base learning rate.
    pub const fn base_lr(&self) -> f64 {
        self.base_lr
    }

    /// The configured warning multiplier.
    pub const fn warning_multiplier(&self) -> f64 {
        self.warning_multiplier
    }

    /// The configured drift multiplier.
    pub const fn drift_multiplier(&self) -> f64 {
        self.drift_multiplier
    }

    /// The current drift state.
    pub const fn current_state(&self) -> DriftLevel {
        self.current_state
    }

    /// Update the scheduler with the latest drift level from a detector.
    pub fn on_drift_level(&mut self, level: DriftLevel) {
        self.current_state = level;
    }

    /// The current learning rate, adjusted for the drift state.
    pub fn current_lr(&self) -> f64 {
        match self.current_state {
            DriftLevel::None => self.base_lr,
            DriftLevel::Warning => self.base_lr * self.warning_multiplier,
            DriftLevel::Drift => self.base_lr * self.drift_multiplier,
        }
    }

    /// Reset to the `None` drift state (use `base_lr`).
    pub fn reset(&mut self) {
        self.current_state = DriftLevel::None;
    }
}

impl Default for LearningRateScheduler {
    fn default() -> Self {
        Self::new(0.01, 2.0, 5.0).expect("default config is valid")
    }
}

// ---------------------------------------------------------------------------
// FixedWindowBuffer
// ---------------------------------------------------------------------------

/// A bounded ring buffer storing the most recent `capacity` observations.
///
/// When the buffer is full, pushing a new value overwrites the oldest entry.
/// Useful for fixed-window training or replay where only recent data matters.
///
/// Space complexity: `O(capacity)`.
///
/// # Examples
///
/// ```
/// use rill_ml::drift::FixedWindowBuffer;
///
/// let mut buf = FixedWindowBuffer::new(3).unwrap();
/// buf.push(1.0).unwrap();
/// buf.push(2.0).unwrap();
/// buf.push(3.0).unwrap();
/// assert_eq!(buf.mean(), Some(2.0));
///
/// // Overwrites the oldest entry (1.0).
/// buf.push(4.0).unwrap();
/// assert_eq!(buf.mean(), Some(3.0)); // (2 + 3 + 4) / 3
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FixedWindowBuffer {
    buffer: Vec<f64>,
    capacity: usize,
    head: usize,
    len: usize,
}

impl FixedWindowBuffer {
    /// Create a new buffer with the given capacity.
    ///
    /// `capacity` must be greater than zero.
    pub fn new(capacity: usize) -> Result<Self, RillError> {
        if capacity == 0 {
            return Err(RillError::InvalidCapacity(capacity));
        }
        Ok(Self {
            buffer: vec![0.0; capacity],
            capacity,
            head: 0,
            len: 0,
        })
    }

    /// The maximum number of elements the buffer can hold.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// The current number of elements stored.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the buffer is empty.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether the buffer is at capacity.
    pub const fn is_full(&self) -> bool {
        self.len == self.capacity
    }

    /// Push a new value, overwriting the oldest entry if full.
    ///
    /// `value` must be finite.
    pub fn push(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        self.buffer[self.head] = value;
        self.head = (self.head + 1) % self.capacity;
        if self.len < self.capacity {
            self.len += 1;
        }
        Ok(())
    }

    /// The mean of the stored values, or `None` if empty.
    pub fn mean(&self) -> Option<f64> {
        if self.len == 0 {
            return None;
        }
        let sum: f64 = self.iter().sum();
        if !sum.is_finite() {
            return None;
        }
        Some(sum / self.len as f64)
    }

    /// Iterate over the stored values in insertion order (oldest to newest).
    pub fn iter(&self) -> impl Iterator<Item = &f64> {
        let start = if self.is_full() { self.head } else { 0 };
        let len = self.len;
        let cap = self.capacity;
        (0..len).map(move |i| &self.buffer[(start + i) % cap])
    }

    /// Reset to the empty state.
    pub fn reset(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- TimeDecayedMean tests ---

    #[test]
    fn tdm_first_sample_seeds_mean() {
        let mut m = TimeDecayedMean::new(0.1).unwrap();
        m.update(0.0, 10.0).unwrap();
        assert!((m.value().unwrap() - 10.0).abs() < 1e-12);
    }

    #[test]
    fn tdm_decay_weights_old_samples() {
        let mut m = TimeDecayedMean::new(1.0).unwrap();
        m.update(0.0, 100.0).unwrap();
        m.update(10.0, 1.0).unwrap();
        // With decay=1.0 and dt=10, the old sample's weight is exp(-10) ≈ 4.5e-5.
        // The mean should be very close to 1.0 (the recent sample).
        let v = m.value().unwrap();
        assert!(
            (v - 1.0).abs() < 0.01,
            "recent sample should dominate, got {}",
            v
        );
    }

    #[test]
    fn tdm_value_correct() {
        let mut m = TimeDecayedMean::new(0.5).unwrap();
        m.update(0.0, 10.0).unwrap();
        m.update(1.0, 20.0).unwrap();
        // factor = exp(-0.5 * 1) ≈ 0.6065
        // weighted_sum = 0.6065 * 10 + 20 = 26.065
        // weight_total = 0.6065 + 1 = 1.6065
        // mean = 26.065 / 1.6065 ≈ 16.22
        let v = m.value().unwrap();
        assert!((v - 16.22).abs() < 0.1, "expected ~16.22, got {}", v);
    }

    #[test]
    fn tdm_reset_clears_state() {
        let mut m = TimeDecayedMean::new(0.1).unwrap();
        m.update(0.0, 10.0).unwrap();
        m.update(1.0, 20.0).unwrap();
        assert!(m.value().is_some());
        m.reset();
        assert!(m.value().is_none());
        assert_eq!(m.weight_total(), 0.0);
        assert_eq!(m.last_time(), None);
    }

    #[test]
    fn tdm_rejects_invalid_decay() {
        assert!(TimeDecayedMean::new(0.0).is_err());
        assert!(TimeDecayedMean::new(-1.0).is_err());
        assert!(TimeDecayedMean::new(f64::NAN).is_err());
        assert!(TimeDecayedMean::new(f64::INFINITY).is_err());
    }

    #[test]
    fn tdm_rejects_non_finite() {
        let mut m = TimeDecayedMean::new(0.1).unwrap();
        assert!(m.update(f64::NAN, 1.0).is_err());
        assert!(m.update(1.0, f64::NAN).is_err());
        assert!(m.update(f64::INFINITY, 1.0).is_err());
        assert!(m.update(1.0, f64::INFINITY).is_err());
    }

    #[test]
    fn tdm_rejects_negative_dt() {
        let mut m = TimeDecayedMean::new(0.1).unwrap();
        m.update(5.0, 10.0).unwrap();
        assert!(m.update(3.0, 20.0).is_err());
    }

    #[test]
    fn tdm_equal_time_no_decay() {
        let mut m = TimeDecayedMean::new(1.0).unwrap();
        m.update(0.0, 10.0).unwrap();
        m.update(0.0, 20.0).unwrap();
        // dt = 0, factor = exp(0) = 1, so this is a simple mean.
        assert!((m.value().unwrap() - 15.0).abs() < 1e-12);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn tdm_serde_roundtrip() {
        let mut m = TimeDecayedMean::new(0.5).unwrap();
        m.update(0.0, 10.0).unwrap();
        m.update(1.0, 20.0).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let restored: TimeDecayedMean = serde_json::from_str(&json).unwrap();
        assert!((restored.decay() - 0.5).abs() < 1e-12);
        assert!((restored.value().unwrap() - m.value().unwrap()).abs() < 1e-12);
    }

    // --- LearningRateScheduler tests ---

    #[test]
    fn lrs_default_lr() {
        let sched = LearningRateScheduler::default();
        assert!((sched.current_lr() - 0.01).abs() < 1e-12);
        assert_eq!(sched.current_state(), DriftLevel::None);
    }

    #[test]
    fn lrs_warning_increases_lr() {
        let mut sched = LearningRateScheduler::new(0.05, 2.0, 5.0).unwrap();
        sched.on_drift_level(DriftLevel::Warning);
        assert!((sched.current_lr() - 0.10).abs() < 1e-12);
    }

    #[test]
    fn lrs_drift_increases_more() {
        let mut sched = LearningRateScheduler::new(0.05, 2.0, 5.0).unwrap();
        sched.on_drift_level(DriftLevel::Drift);
        assert!((sched.current_lr() - 0.25).abs() < 1e-12);
    }

    #[test]
    fn lrs_reset_to_base() {
        let mut sched = LearningRateScheduler::new(0.05, 2.0, 5.0).unwrap();
        sched.on_drift_level(DriftLevel::Drift);
        sched.reset();
        assert_eq!(sched.current_state(), DriftLevel::None);
        assert!((sched.current_lr() - 0.05).abs() < 1e-12);
    }

    #[test]
    fn lrs_rejects_invalid_config() {
        // base_lr <= 0
        assert!(LearningRateScheduler::new(0.0, 2.0, 5.0).is_err());
        assert!(LearningRateScheduler::new(-1.0, 2.0, 5.0).is_err());
        // warning_multiplier < 1
        assert!(LearningRateScheduler::new(0.01, 0.5, 5.0).is_err());
        // drift_multiplier < warning_multiplier
        assert!(LearningRateScheduler::new(0.01, 3.0, 2.0).is_err());
        // NaN
        assert!(LearningRateScheduler::new(f64::NAN, 2.0, 5.0).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn lrs_serde_roundtrip() {
        let mut sched = LearningRateScheduler::new(0.02, 3.0, 7.0).unwrap();
        sched.on_drift_level(DriftLevel::Warning);
        let json = serde_json::to_string(&sched).unwrap();
        let restored: LearningRateScheduler = serde_json::from_str(&json).unwrap();
        assert!((restored.base_lr() - 0.02).abs() < 1e-12);
        assert!((restored.warning_multiplier() - 3.0).abs() < 1e-12);
        assert!((restored.drift_multiplier() - 7.0).abs() < 1e-12);
        assert_eq!(restored.current_state(), DriftLevel::Warning);
        assert!((restored.current_lr() - 0.06).abs() < 1e-12);
    }

    // --- FixedWindowBuffer tests ---

    #[test]
    fn fwb_push_below_capacity() {
        let mut buf = FixedWindowBuffer::new(5).unwrap();
        buf.push(1.0).unwrap();
        buf.push(2.0).unwrap();
        buf.push(3.0).unwrap();
        assert_eq!(buf.len(), 3);
        assert!(!buf.is_full());
        assert!(!buf.is_empty());
        let collected: Vec<f64> = buf.iter().copied().collect();
        assert_eq!(collected, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn fwb_push_overwrites_oldest() {
        let mut buf = FixedWindowBuffer::new(3).unwrap();
        buf.push(1.0).unwrap();
        buf.push(2.0).unwrap();
        buf.push(3.0).unwrap();
        assert!(buf.is_full());
        buf.push(4.0).unwrap();
        // After push, the oldest (1.0) is gone; order is [2, 3, 4].
        let collected: Vec<f64> = buf.iter().copied().collect();
        assert_eq!(collected, vec![2.0, 3.0, 4.0]);
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn fwb_mean_correct() {
        let mut buf = FixedWindowBuffer::new(4).unwrap();
        buf.push(1.0).unwrap();
        buf.push(2.0).unwrap();
        buf.push(3.0).unwrap();
        buf.push(4.0).unwrap();
        assert_eq!(buf.mean(), Some(2.5));
        buf.push(10.0).unwrap(); // overwrites 1.0
        assert_eq!(buf.mean(), Some((2.0 + 3.0 + 4.0 + 10.0) / 4.0));
    }

    #[test]
    fn fwb_iter_returns_in_order() {
        let mut buf = FixedWindowBuffer::new(3).unwrap();
        for v in &[10.0, 20.0, 30.0, 40.0, 50.0] {
            buf.push(*v).unwrap();
        }
        // After 5 pushes into capacity 3: the last 3 values are [30, 40, 50].
        let collected: Vec<f64> = buf.iter().copied().collect();
        assert_eq!(collected, vec![30.0, 40.0, 50.0]);
    }

    #[test]
    fn fwb_empty_buffer_mean_none() {
        let buf = FixedWindowBuffer::new(3).unwrap();
        assert_eq!(buf.mean(), None);
        assert!(buf.is_empty());
        assert!(!buf.is_full());
    }

    #[test]
    fn fwb_rejects_zero_capacity() {
        assert!(FixedWindowBuffer::new(0).is_err());
    }

    #[test]
    fn fwb_rejects_non_finite() {
        let mut buf = FixedWindowBuffer::new(3).unwrap();
        assert!(buf.push(f64::NAN).is_err());
        assert!(buf.push(f64::INFINITY).is_err());
        assert!(buf.push(f64::NEG_INFINITY).is_err());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn fwb_reset_clears() {
        let mut buf = FixedWindowBuffer::new(3).unwrap();
        buf.push(1.0).unwrap();
        buf.push(2.0).unwrap();
        buf.reset();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.mean(), None);
    }

    #[test]
    fn fwb_wrap_around_multiple_times() {
        let mut buf = FixedWindowBuffer::new(2).unwrap();
        for i in 1..=10 {
            buf.push(i as f64).unwrap();
        }
        assert_eq!(buf.len(), 2);
        assert!(buf.is_full());
        let collected: Vec<f64> = buf.iter().copied().collect();
        assert_eq!(collected, vec![9.0, 10.0]);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn fwb_serde_roundtrip() {
        let mut buf = FixedWindowBuffer::new(3).unwrap();
        buf.push(1.0).unwrap();
        buf.push(2.0).unwrap();
        buf.push(3.0).unwrap();
        buf.push(4.0).unwrap(); // overwrites 1.0
        let json = serde_json::to_string(&buf).unwrap();
        let restored: FixedWindowBuffer = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.capacity(), 3);
        assert_eq!(restored.len(), 3);
        assert!(restored.is_full());
        let collected: Vec<f64> = restored.iter().copied().collect();
        assert_eq!(collected, vec![2.0, 3.0, 4.0]);
    }
}

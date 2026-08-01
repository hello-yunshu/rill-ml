//! Bounded robust streaming summaries.
//!
//! [`ClippedMean`] is exact for a caller-selected fixed clipping interval. The
//! bounds must come from domain knowledge or an independently validated
//! calibration path; estimating them from this statistic would hide tail
//! behaviour and invalidate the stated semantics.
//!
//! [`RollingMedianMad`] is exact inside a bounded recent-observation window.
//! It is therefore a rolling approximation of an unbounded stream, not a
//! constant-memory estimate of the lifetime distribution. Updates are `O(1)`;
//! queries are `O(W log W)` and use one `O(W)` scratch vector.

use std::collections::VecDeque;

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::persistence::ValidateState;
use crate::traits::OnlineStatistic;

/// Maximum accepted window size for [`RollingMedianMad`].
///
/// This hard limit keeps both persisted state and query-time scratch memory
/// bounded even when configuration or restored state is untrusted.
pub const MAX_ROBUST_WINDOW_SIZE: usize = 65_536;

/// Normal-consistency multiplier commonly applied to MAD (`1 / 0.67448975...`).
pub const MAD_NORMAL_CONSISTENCY_SCALE: f64 = 1.482_602_218_505_602;

/// Normal-consistency factor used by the modified z-score.
pub const MODIFIED_Z_NORMAL_FACTOR: f64 = 0.674_489_750_196_081_7;

/// Exact median and median absolute deviation for the current rolling window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MedianMadSummary {
    samples: usize,
    median: f64,
    mad: f64,
}

impl MedianMadSummary {
    /// Number of observations represented by this summary.
    pub const fn samples(&self) -> usize {
        self.samples
    }

    /// Exact median of the current window.
    pub const fn median(&self) -> f64 {
        self.median
    }

    /// Exact median absolute deviation from [`Self::median`].
    pub const fn mad(&self) -> f64 {
        self.mad
    }

    /// MAD scaled for consistency with standard deviation under normal data.
    ///
    /// Returns an error when the finite raw MAD cannot be scaled into a finite
    /// `f64`. A zero MAD remains zero.
    pub fn normal_scaled_mad(&self) -> Result<f64, RillError> {
        let scaled = self.mad * MAD_NORMAL_CONSISTENCY_SCALE;
        ensure_finite("normal-scaled MAD", scaled)?;
        Ok(scaled)
    }
}

/// Result of comparing one observation with a rolling median/MAD summary.
///
/// The modified z-score is `0.67448975... * (x - median) / MAD`. A zero MAD
/// makes that expression undefined and is represented explicitly instead of
/// returning a non-finite number or silently inventing a fallback scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModifiedZScore {
    /// The score is finite and defined.
    Defined {
        /// Modified z-score for the supplied observation.
        score: f64,
        /// Median used to compute the score.
        median: f64,
        /// Raw MAD used to compute the score.
        mad: f64,
    },
    /// The current window has zero MAD, so no score is defined.
    ZeroMad {
        /// Median of the zero-scale window.
        median: f64,
        /// Observation the caller attempted to score.
        observation: f64,
    },
}

impl ModifiedZScore {
    /// Return the finite score, or `None` when the current MAD is zero.
    pub const fn score(&self) -> Option<f64> {
        match self {
            Self::Defined { score, .. } => Some(*score),
            Self::ZeroMad { .. } => None,
        }
    }
}

/// Exact median/MAD over a bounded FIFO window.
///
/// This Preview statistic retains at most [`MAX_ROBUST_WINDOW_SIZE`] finite
/// observations. `min_samples` is an explicit warm-up threshold: queries fail
/// with [`RillError::InsufficientData`] until the current window reaches it.
///
/// The window protects locality and memory, but it does not make arbitrary
/// contamination harmless. Median/MAD can be controlled when at least half of
/// the active window is replaced by adversarial values. Callers must choose a
/// window and alert policy appropriate to their stream.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RollingMedianMad {
    window: VecDeque<f64>,
    capacity: usize,
    min_samples: usize,
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for RollingMedianMad {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct State {
            window: BoundedRobustWindow,
            capacity: usize,
            min_samples: usize,
            samples_seen: u64,
        }

        let state = State::deserialize(deserializer)?;
        let statistic = Self {
            window: state.window.0,
            capacity: state.capacity,
            min_samples: state.min_samples,
            samples_seen: state.samples_seen,
        };
        statistic
            .validate_state()
            .map_err(serde::de::Error::custom)?;
        Ok(statistic)
    }
}

#[cfg(feature = "serde")]
struct BoundedRobustWindow(VecDeque<f64>);

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for BoundedRobustWindow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WindowVisitor;

        impl<'de> serde::de::Visitor<'de> for WindowVisitor {
            type Value = BoundedRobustWindow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    formatter,
                    "at most {MAX_ROBUST_WINDOW_SIZE} finite rolling-window values"
                )
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let initial_capacity = sequence
                    .size_hint()
                    .unwrap_or(0)
                    .min(MAX_ROBUST_WINDOW_SIZE);
                let mut window = VecDeque::with_capacity(initial_capacity);
                while let Some(value) = sequence.next_element::<f64>()? {
                    if window.len() == MAX_ROBUST_WINDOW_SIZE {
                        return Err(serde::de::Error::custom(format!(
                            "rolling median/MAD window exceeds maximum {MAX_ROBUST_WINDOW_SIZE}"
                        )));
                    }
                    if !value.is_finite() {
                        return Err(serde::de::Error::custom(
                            "rolling median/MAD window contains a non-finite value",
                        ));
                    }
                    window.push_back(value);
                }
                Ok(BoundedRobustWindow(window))
            }
        }

        deserializer.deserialize_seq(WindowVisitor)
    }
}

impl RollingMedianMad {
    /// Create a bounded rolling median/MAD statistic.
    ///
    /// `capacity` must be in `1..=MAX_ROBUST_WINDOW_SIZE`, and `min_samples`
    /// must be in `1..=capacity`.
    pub fn new(capacity: usize, min_samples: usize) -> Result<Self, RillError> {
        validate_rolling_median_mad_config(capacity, min_samples)?;
        Ok(Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
            min_samples,
            samples_seen: 0,
        })
    }

    /// Configured FIFO window capacity.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Minimum current-window sample count required by queries.
    pub const fn min_samples(&self) -> usize {
        self.min_samples
    }

    /// Number of observations currently retained in the window.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// Whether the current window is empty.
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Whether the current window has reached its query warm-up threshold.
    pub fn is_ready(&self) -> bool {
        self.window.len() >= self.min_samples
    }

    /// Compute the exact median and raw MAD of the current window.
    ///
    /// The method uses one scratch vector bounded by `capacity`. Extremely
    /// separated finite values are still accepted; an error is returned only
    /// if the selected median absolute deviation itself exceeds finite `f64`
    /// range.
    pub fn summary(&self) -> Result<MedianMadSummary, RillError> {
        if !self.is_ready() {
            return Err(RillError::InsufficientData);
        }

        let mut scratch = Vec::with_capacity(self.window.len());
        scratch.extend(self.window.iter().copied());
        scratch.sort_by(f64::total_cmp);
        let median = median_of_sorted(&scratch);

        for value in &mut scratch {
            *value = absolute_distance(*value, median);
        }
        scratch.sort_by(f64::total_cmp);
        let mad = median_of_sorted(&scratch);
        if !mad.is_finite() {
            return Err(RillError::InvalidState(
                "rolling MAD exceeds the finite f64 range".to_owned(),
            ));
        }

        Ok(MedianMadSummary {
            samples: scratch.len(),
            median,
            mad,
        })
    }

    /// Compute a modified robust z-score for `observation`.
    ///
    /// No outlier threshold is embedded in this statistic. The caller owns
    /// alert policy, including treatment of [`ModifiedZScore::ZeroMad`].
    pub fn modified_z_score(&self, observation: f64) -> Result<ModifiedZScore, RillError> {
        ensure_finite("observation", observation)?;
        let summary = self.summary()?;
        if summary.mad == 0.0 {
            return Ok(ModifiedZScore::ZeroMad {
                median: summary.median,
                observation,
            });
        }

        let direct_delta = observation - summary.median;
        let standardized = if direct_delta.is_finite() {
            direct_delta / summary.mad
        } else {
            observation / summary.mad - summary.median / summary.mad
        };
        let score = MODIFIED_Z_NORMAL_FACTOR * standardized;
        ensure_finite("modified z-score", score)?;
        Ok(ModifiedZScore::Defined {
            score,
            median: summary.median,
            mad: summary.mad,
        })
    }
}

impl OnlineStatistic for RollingMedianMad {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let next_samples_seen = checked_increment(self.samples_seen, "rolling median/MAD")?;
        if self.window.len() == self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(value);
        self.samples_seen = next_samples_seen;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        self.window.clear();
        self.samples_seen = 0;
    }
}

impl ValidateState for RollingMedianMad {
    fn validate_state(&self) -> Result<(), RillError> {
        validate_rolling_median_mad_config(self.capacity, self.min_samples)?;
        if self.window.len() > self.capacity {
            return Err(RillError::InvalidState(
                "rolling median/MAD window exceeds configured capacity".to_owned(),
            ));
        }
        let retained = usize::try_from(self.samples_seen)
            .unwrap_or(usize::MAX)
            .min(self.capacity);
        if self.window.len() != retained {
            return Err(RillError::InvalidState(
                "rolling median/MAD window length disagrees with samples_seen".to_owned(),
            ));
        }
        for value in &self.window {
            ensure_finite("rolling median/MAD window value", *value)?;
        }
        Ok(())
    }
}

fn validate_rolling_median_mad_config(
    capacity: usize,
    min_samples: usize,
) -> Result<(), RillError> {
    if capacity == 0 {
        return Err(RillError::InvalidCapacity(capacity));
    }
    if capacity > MAX_ROBUST_WINDOW_SIZE {
        return Err(RillError::InvalidState(format!(
            "rolling median/MAD capacity {capacity} exceeds maximum {MAX_ROBUST_WINDOW_SIZE}"
        )));
    }
    if min_samples == 0 || min_samples > capacity {
        return Err(RillError::InvalidState(format!(
            "rolling median/MAD min_samples {min_samples} must be in 1..={capacity}"
        )));
    }
    Ok(())
}

fn median_of_sorted(values: &[f64]) -> f64 {
    let midpoint = values.len() / 2;
    if values.len() % 2 == 1 {
        values[midpoint]
    } else {
        finite_midpoint(values[midpoint - 1], values[midpoint])
    }
}

fn finite_midpoint(lower: f64, upper: f64) -> f64 {
    if lower.is_sign_negative() == upper.is_sign_negative() {
        lower + (upper - lower) / 2.0
    } else {
        lower / 2.0 + upper / 2.0
    }
}

fn absolute_distance(value: f64, center: f64) -> f64 {
    let distance = (value - center).abs();
    if distance.is_finite() {
        distance
    } else {
        f64::INFINITY
    }
}

/// Exact online mean after clipping each observation to fixed bounds.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClippedMean {
    lower: f64,
    upper: f64,
    count: u64,
    mean: f64,
}

impl ClippedMean {
    /// Create a clipped mean with finite bounds satisfying `lower <= upper`.
    pub fn new(lower: f64, upper: f64) -> Result<Self, RillError> {
        ensure_finite("clipped mean lower", lower)?;
        ensure_finite("clipped mean upper", upper)?;
        if lower > upper {
            return Err(RillError::InvalidState(
                "clipped mean lower bound exceeds upper bound".to_owned(),
            ));
        }
        Ok(Self {
            lower,
            upper,
            count: 0,
            mean: 0.0,
        })
    }

    /// Fixed lower clipping bound.
    pub const fn lower(&self) -> f64 {
        self.lower
    }

    /// Fixed upper clipping bound.
    pub const fn upper(&self) -> f64 {
        self.upper
    }

    /// Current exact mean of clipped observations, or `0.0` when empty.
    pub const fn value(&self) -> f64 {
        self.mean
    }
}

impl OnlineStatistic for ClippedMean {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let clipped = value.clamp(self.lower, self.upper);
        let next_count = checked_increment(self.count, "clipped mean count")?;
        let delta = clipped - self.mean;
        ensure_finite("clipped mean delta", delta)?;
        let next_mean = self.mean + delta / next_count as f64;
        ensure_finite("clipped mean", next_mean)?;
        self.count = next_count;
        self.mean = next_mean;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.count = 0;
        self.mean = 0.0;
    }
}

impl ValidateState for ClippedMean {
    fn validate_state(&self) -> Result<(), RillError> {
        ClippedMean::new(self.lower, self.upper)?;
        ensure_finite("clipped mean", self.mean)?;
        if self.count == 0 && self.mean != 0.0 {
            return Err(RillError::InvalidState(
                "empty clipped mean must be zero".to_owned(),
            ));
        }
        if self.count > 0 && (self.mean < self.lower || self.mean > self.upper) {
            return Err(RillError::InvalidState(
                "clipped mean lies outside its clipping interval".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use proptest::collection::vec;
    use proptest::prelude::*;

    fn offline_median_mad(values: &[f64]) -> (f64, f64) {
        let mut ordered = values.to_vec();
        ordered.sort_by(f64::total_cmp);
        let median = if ordered.len() % 2 == 1 {
            ordered[ordered.len() / 2]
        } else {
            let middle = ordered.len() / 2;
            (ordered[middle - 1] + ordered[middle]) / 2.0
        };
        let mut deviations = values
            .iter()
            .map(|value| (value - median).abs())
            .collect::<Vec<_>>();
        deviations.sort_by(f64::total_cmp);
        let mad = if deviations.len() % 2 == 1 {
            deviations[deviations.len() / 2]
        } else {
            let middle = deviations.len() / 2;
            (deviations[middle - 1] + deviations[middle]) / 2.0
        };
        (median, mad)
    }

    #[test]
    fn clipped_mean_matches_offline_clipped_calculation() {
        let values = [-100.0, -1.0, 1.0, 2.0, 100.0];
        let mut statistic = ClippedMean::new(-2.0, 3.0).unwrap();
        for value in values {
            statistic.update(value).unwrap();
        }
        let expected = values
            .iter()
            .map(|value| value.clamp(-2.0, 3.0))
            .sum::<f64>()
            / values.len() as f64;
        assert!((statistic.value() - expected).abs() < 1e-12);
        statistic.validate_state().unwrap();
    }

    #[test]
    fn failure_is_atomic_and_reset_clears_state() {
        let mut statistic = ClippedMean::new(-10.0, 10.0).unwrap();
        statistic.update(1.0).unwrap();
        let before = statistic.clone();
        assert!(statistic.update(f64::INFINITY).is_err());
        assert_eq!(statistic, before);
        statistic.reset();
        assert_eq!(statistic.samples_seen(), 0);
        assert_eq!(statistic.value(), 0.0);
    }

    #[test]
    fn rolling_summary_and_modified_z_match_offline_reference() {
        let mut statistic = RollingMedianMad::new(5, 3).unwrap();
        for value in [1.0, 2.0, 100.0, 4.0, 5.0] {
            statistic.update(value).unwrap();
        }
        let summary = statistic.summary().unwrap();
        assert_eq!(summary.samples(), 5);
        assert_eq!(summary.median(), 4.0);
        assert_eq!(summary.mad(), 2.0);
        assert!((summary.normal_scaled_mad().unwrap() - 2.965_204_437_011_204).abs() < 1e-14);

        let score = statistic.modified_z_score(10.0).unwrap();
        assert_eq!(
            score,
            ModifiedZScore::Defined {
                score: MODIFIED_Z_NORMAL_FACTOR * 3.0,
                median: 4.0,
                mad: 2.0,
            }
        );
    }

    #[test]
    fn rolling_window_evicts_and_tracks_lifetime_samples() {
        let mut statistic = RollingMedianMad::new(3, 1).unwrap();
        for value in [1.0, 2.0, 3.0, 100.0] {
            statistic.update(value).unwrap();
        }
        let summary = statistic.summary().unwrap();
        assert_eq!(statistic.len(), 3);
        assert_eq!(statistic.samples_seen(), 4);
        assert_eq!(summary.median(), 3.0);
        assert_eq!(summary.mad(), 1.0);
        statistic.validate_state().unwrap();
    }

    #[test]
    fn zero_mad_is_explicit_and_never_non_finite() {
        let mut statistic = RollingMedianMad::new(5, 3).unwrap();
        for value in [7.0, 7.0, 7.0] {
            statistic.update(value).unwrap();
        }
        assert_eq!(statistic.summary().unwrap().mad(), 0.0);
        assert_eq!(
            statistic.modified_z_score(9.0).unwrap(),
            ModifiedZScore::ZeroMad {
                median: 7.0,
                observation: 9.0,
            }
        );
        assert_eq!(statistic.modified_z_score(9.0).unwrap().score(), None);
    }

    #[test]
    fn warmup_configuration_and_non_finite_updates_are_strict() {
        assert!(matches!(
            RollingMedianMad::new(0, 1),
            Err(RillError::InvalidCapacity(0))
        ));
        assert!(RollingMedianMad::new(10, 0).is_err());
        assert!(RollingMedianMad::new(10, 11).is_err());
        assert!(RollingMedianMad::new(MAX_ROBUST_WINDOW_SIZE + 1, 1).is_err());

        let mut statistic = RollingMedianMad::new(4, 3).unwrap();
        statistic.update(1.0).unwrap();
        statistic.update(2.0).unwrap();
        assert!(matches!(
            statistic.summary(),
            Err(RillError::InsufficientData)
        ));
        let before = statistic.clone();
        assert!(statistic.update(f64::NAN).is_err());
        assert_eq!(statistic, before);
        assert!(statistic.modified_z_score(f64::INFINITY).is_err());
        assert_eq!(statistic, before);
    }

    #[test]
    fn minority_extreme_contamination_does_not_control_the_summary() {
        let mut statistic = RollingMedianMad::new(101, 101).unwrap();
        for index in 0..52 {
            statistic.update([-1.0, 0.0, 1.0][index % 3]).unwrap();
        }
        for _ in 0..49 {
            statistic.update(f64::MAX).unwrap();
        }
        let summary = statistic.summary().unwrap();
        assert_eq!(summary.median(), 1.0);
        assert_eq!(summary.mad(), 2.0);
    }

    #[test]
    fn representable_extreme_summaries_remain_finite() {
        let mut symmetric = RollingMedianMad::new(2, 2).unwrap();
        symmetric.update(-f64::MAX).unwrap();
        symmetric.update(f64::MAX).unwrap();
        let summary = symmetric.summary().unwrap();
        assert_eq!(summary.median(), 0.0);
        assert_eq!(summary.mad(), f64::MAX);
        assert!(summary.normal_scaled_mad().is_err());

        let mut majority = RollingMedianMad::new(3, 3).unwrap();
        majority.update(-f64::MAX).unwrap();
        majority.update(f64::MAX).unwrap();
        majority.update(f64::MAX).unwrap();
        let summary = majority.summary().unwrap();
        assert_eq!(summary.median(), f64::MAX);
        assert_eq!(summary.mad(), 0.0);
    }

    proptest! {
        #[test]
        fn rolling_summary_matches_independent_offline_calculation(
            values in vec(-1_000_000.0f64..1_000_000.0, 1..160),
            capacity in 1usize..64,
        ) {
            let mut statistic = RollingMedianMad::new(capacity, 1).unwrap();
            let mut reference = VecDeque::new();
            for value in values {
                statistic.update(value).unwrap();
                if reference.len() == capacity {
                    reference.pop_front();
                }
                reference.push_back(value);
                let reference_values = reference.iter().copied().collect::<Vec<_>>();
                let (median, mad) = offline_median_mad(&reference_values);
                let summary = statistic.summary().unwrap();
                let median_tolerance = 1e-12 * median.abs().max(1.0);
                let mad_tolerance = 1e-12 * mad.abs().max(1.0);
                prop_assert!((summary.median() - median).abs() <= median_tolerance);
                prop_assert!((summary.mad() - mad).abs() <= mad_tolerance);
            }
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_preserves_future_updates() {
        let mut original = ClippedMean::new(-20.0, 20.0).unwrap();
        for value in 0..100 {
            original.update(value as f64).unwrap();
        }
        let json = serde_json::to_string(&original).unwrap();
        let mut restored: ClippedMean = serde_json::from_str(&json).unwrap();
        restored.validate_state().unwrap();
        for value in 100..200 {
            original.update(value as f64).unwrap();
            restored.update(value as f64).unwrap();
            assert_eq!(original, restored);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn rolling_serde_roundtrip_preserves_eviction_and_rejects_corrupt_state() {
        let mut original = RollingMedianMad::new(17, 5).unwrap();
        for value in 0..40 {
            original.update(value as f64).unwrap();
        }
        let json = serde_json::to_string(&original).unwrap();
        let mut restored: RollingMedianMad = serde_json::from_str(&json).unwrap();
        restored.validate_state().unwrap();
        for value in 40..80 {
            original.update(value as f64).unwrap();
            restored.update(value as f64).unwrap();
            assert_eq!(original, restored);
            assert_eq!(original.summary().unwrap(), restored.summary().unwrap());
        }

        let mut corrupt = serde_json::to_value(&original).unwrap();
        corrupt["capacity"] = serde_json::json!(0);
        assert!(serde_json::from_value::<RollingMedianMad>(corrupt).is_err());

        let mut unknown = serde_json::to_value(&original).unwrap();
        unknown["future_field"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RollingMedianMad>(unknown).is_err());

        let oversized = serde_json::json!({
            "window": vec![0.0; MAX_ROBUST_WINDOW_SIZE + 1],
            "capacity": MAX_ROBUST_WINDOW_SIZE,
            "min_samples": 1,
            "samples_seen": MAX_ROBUST_WINDOW_SIZE + 1,
        });
        assert!(serde_json::from_value::<RollingMedianMad>(oversized).is_err());
    }
}

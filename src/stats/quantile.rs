//! Constant-memory P-square (P²) online quantile estimation.
//!
//! P² tracks five adaptive markers per quantile and therefore uses `O(1)`
//! memory for one quantile and `O(k)` for `k` requested quantiles. It is a
//! heuristic estimator: unlike Greenwald-Khanna it has no deterministic
//! worst-case rank-error bound. It performs well on continuous stationary
//! streams, but callers needing an auditable epsilon rank bound, exact tail
//! quantiles, or abrupt multimodal changes should use a bounded-window exact
//! statistic or a sketch with a formal error guarantee.

use crate::error::{RillError, checked_increment, ensure_finite};
use crate::persistence::ValidateState;
use crate::traits::OnlineStatistic;

const MARKER_COUNT: usize = 5;
const MAX_TRACKED_QUANTILES: usize = 64;

/// Constant-memory P² estimator for one interior quantile.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct P2Quantile {
    quantile: f64,
    samples_seen: u64,
    initial: Vec<f64>,
    marker_heights: [f64; MARKER_COUNT],
    marker_positions: [u64; MARKER_COUNT],
    desired_positions: [f64; MARKER_COUNT],
}

impl P2Quantile {
    /// Create an estimator for `quantile` in the open interval `(0, 1)`.
    pub fn new(quantile: f64) -> Result<Self, RillError> {
        ensure_finite("quantile", quantile)?;
        if quantile <= 0.0 || quantile >= 1.0 {
            return Err(RillError::InvalidParameter {
                name: "quantile",
                value: quantile,
            });
        }
        Ok(Self {
            quantile,
            samples_seen: 0,
            initial: Vec::with_capacity(MARKER_COUNT),
            marker_heights: [0.0; MARKER_COUNT],
            marker_positions: [0; MARKER_COUNT],
            desired_positions: [0.0; MARKER_COUNT],
        })
    }

    /// Requested quantile.
    pub const fn quantile(&self) -> f64 {
        self.quantile
    }

    /// Estimated quantile, or `None` before the first observation.
    ///
    /// The first four samples use exact linear interpolation over the retained
    /// bootstrap values. From the fifth sample onward this returns the central
    /// P² marker.
    pub fn value(&self) -> Option<f64> {
        if self.samples_seen == 0 {
            return None;
        }
        if self.samples_seen < MARKER_COUNT as u64 {
            let mut sorted = self.initial.clone();
            sorted.sort_by(f64::total_cmp);
            return Some(exact_linear_quantile(&sorted, self.quantile));
        }
        Some(self.marker_heights[2])
    }

    /// Reset to the empty state while retaining the requested quantile.
    pub fn reset(&mut self) {
        self.samples_seen = 0;
        self.initial.clear();
        self.marker_heights = [0.0; MARKER_COUNT];
        self.marker_positions = [0; MARKER_COUNT];
        self.desired_positions = [0.0; MARKER_COUNT];
    }

    fn desired_increments(&self) -> [f64; MARKER_COUNT] {
        [
            0.0,
            self.quantile / 2.0,
            self.quantile,
            (1.0 + self.quantile) / 2.0,
            1.0,
        ]
    }

    fn update_inner(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        let next_samples = checked_increment(self.samples_seen, "P2 samples_seen")?;
        if self.samples_seen < MARKER_COUNT as u64 {
            self.initial.push(value);
            self.samples_seen = next_samples;
            if self.samples_seen == MARKER_COUNT as u64 {
                self.initial.sort_by(f64::total_cmp);
                self.marker_heights.copy_from_slice(&self.initial);
                self.marker_positions = [1, 2, 3, 4, 5];
                self.desired_positions = [
                    1.0,
                    1.0 + 2.0 * self.quantile,
                    1.0 + 4.0 * self.quantile,
                    3.0 + 2.0 * self.quantile,
                    5.0,
                ];
            }
            return Ok(());
        }

        let cell = if value < self.marker_heights[0] {
            self.marker_heights[0] = value;
            0
        } else if value < self.marker_heights[1] {
            0
        } else if value < self.marker_heights[2] {
            1
        } else if value < self.marker_heights[3] {
            2
        } else if value <= self.marker_heights[4] {
            3
        } else {
            self.marker_heights[4] = value;
            3
        };

        for position in self.marker_positions.iter_mut().skip(cell + 1) {
            *position = position
                .checked_add(1)
                .ok_or_else(|| RillError::InvalidState("P2 marker position overflow".to_owned()))?;
        }
        let increments = self.desired_increments();
        for (desired, increment) in self.desired_positions.iter_mut().zip(increments) {
            *desired += increment;
            ensure_finite("P2 desired marker position", *desired)?;
        }

        for index in 1..(MARKER_COUNT - 1) {
            let difference = self.desired_positions[index] - self.marker_positions[index] as f64;
            let can_move_up = difference >= 1.0
                && self.marker_positions[index + 1] - self.marker_positions[index] > 1;
            let can_move_down = difference <= -1.0
                && self.marker_positions[index] - self.marker_positions[index - 1] > 1;
            if !can_move_up && !can_move_down {
                continue;
            }
            let direction = if difference > 0.0 { 1_i8 } else { -1_i8 };
            let candidate = self.parabolic_candidate(index, direction)?;
            let next_height = if self.marker_heights[index - 1] < candidate
                && candidate < self.marker_heights[index + 1]
            {
                candidate
            } else {
                self.linear_candidate(index, direction)?
            };
            ensure_finite("P2 marker height", next_height)?;
            self.marker_heights[index] = next_height;
            if direction > 0 {
                self.marker_positions[index] += 1;
            } else {
                self.marker_positions[index] -= 1;
            }
        }
        self.samples_seen = next_samples;
        Ok(())
    }

    fn parabolic_candidate(&self, index: usize, direction: i8) -> Result<f64, RillError> {
        let n_prev = self.marker_positions[index - 1] as f64;
        let n = self.marker_positions[index] as f64;
        let n_next = self.marker_positions[index + 1] as f64;
        let q_prev = self.marker_heights[index - 1];
        let q = self.marker_heights[index];
        let q_next = self.marker_heights[index + 1];
        let d = f64::from(direction);
        let left = (n - n_prev + d) * (q_next - q) / (n_next - n);
        let right = (n_next - n - d) * (q - q_prev) / (n - n_prev);
        let candidate = q + d * (left + right) / (n_next - n_prev);
        ensure_finite("P2 parabolic marker", candidate)?;
        Ok(candidate)
    }

    fn linear_candidate(&self, index: usize, direction: i8) -> Result<f64, RillError> {
        let adjacent = if direction > 0 { index + 1 } else { index - 1 };
        let numerator = self.marker_heights[adjacent] - self.marker_heights[index];
        let denominator =
            self.marker_positions[adjacent] as f64 - self.marker_positions[index] as f64;
        let candidate = self.marker_heights[index] + f64::from(direction) * numerator / denominator;
        ensure_finite("P2 linear marker", candidate)?;
        Ok(candidate)
    }
}

impl OnlineStatistic for P2Quantile {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        // Five markers make cloning constant-cost and preserve failure
        // atomicity for arithmetic overflow or counter exhaustion.
        let mut next = self.clone();
        next.update_inner(value)?;
        *self = next;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        P2Quantile::reset(self);
    }
}

impl ValidateState for P2Quantile {
    fn validate_state(&self) -> Result<(), RillError> {
        P2Quantile::new(self.quantile)?;
        if self.samples_seen < MARKER_COUNT as u64 {
            if self.initial.len() != self.samples_seen as usize {
                return Err(RillError::InvalidState(
                    "P2 bootstrap length does not match samples_seen".to_owned(),
                ));
            }
            for &value in &self.initial {
                ensure_finite("P2 bootstrap value", value)?;
            }
            if self.marker_heights != [0.0; MARKER_COUNT]
                || self.marker_positions != [0; MARKER_COUNT]
                || self.desired_positions != [0.0; MARKER_COUNT]
            {
                return Err(RillError::InvalidState(
                    "P2 bootstrap state contains initialized markers".to_owned(),
                ));
            }
            return Ok(());
        }
        if self.initial.len() != MARKER_COUNT {
            return Err(RillError::InvalidState(
                "P2 initialized state must retain exactly five bootstrap values".to_owned(),
            ));
        }
        for &value in self.initial.iter().chain(&self.marker_heights) {
            ensure_finite("P2 retained value", value)?;
        }
        for &value in &self.desired_positions {
            ensure_finite("P2 desired position", value)?;
        }
        let observations_after_bootstrap = (self.samples_seen - MARKER_COUNT as u64) as f64;
        let increments = self.desired_increments();
        let initial_desired = [
            1.0,
            1.0 + 2.0 * self.quantile,
            1.0 + 4.0 * self.quantile,
            3.0 + 2.0 * self.quantile,
            5.0,
        ];
        let desired_matches_count = self
            .desired_positions
            .iter()
            .zip(initial_desired.iter().zip(increments))
            .all(|(&actual, (&initial, increment))| {
                let expected = initial + observations_after_bootstrap * increment;
                (actual - expected).abs() <= 1e-10 * expected.abs().max(1.0)
            });
        if self.initial.windows(2).any(|pair| pair[0] > pair[1])
            || self.marker_positions[0] != 1
            || self.marker_positions[4] != self.samples_seen
            || self
                .marker_positions
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self
                .marker_positions
                .iter()
                .any(|&position| position > self.samples_seen)
            || self.marker_heights.windows(2).any(|pair| pair[0] > pair[1])
            || self
                .desired_positions
                .windows(2)
                .any(|pair| pair[0] > pair[1])
            || !desired_matches_count
        {
            return Err(RillError::InvalidState(
                "P2 marker ordering or boundary positions are inconsistent".to_owned(),
            ));
        }
        Ok(())
    }
}

/// A bounded collection of independent P² estimators.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct P2Quantiles {
    estimators: Vec<P2Quantile>,
}

impl P2Quantiles {
    /// Create estimators for up to 64 distinct interior quantiles.
    pub fn new(quantiles: &[f64]) -> Result<Self, RillError> {
        if quantiles.is_empty() || quantiles.len() > MAX_TRACKED_QUANTILES {
            return Err(RillError::InvalidCapacity(quantiles.len()));
        }
        let mut estimators = Vec::with_capacity(quantiles.len());
        for (index, &quantile) in quantiles.iter().enumerate() {
            if quantiles[..index].contains(&quantile) {
                return Err(RillError::InvalidState(
                    "P2Quantiles requires distinct quantiles".to_owned(),
                ));
            }
            estimators.push(P2Quantile::new(quantile)?);
        }
        Ok(Self { estimators })
    }

    /// Update every configured estimator atomically.
    pub fn update(&mut self, value: f64) -> Result<(), RillError> {
        let mut next = self.clone();
        for estimator in &mut next.estimators {
            estimator.update_inner(value)?;
        }
        *self = next;
        Ok(())
    }

    /// `(requested quantile, current estimate)` pairs in constructor order.
    pub fn values(&self) -> Vec<(f64, Option<f64>)> {
        self.estimators
            .iter()
            .map(|estimator| (estimator.quantile(), estimator.value()))
            .collect()
    }

    /// Observations incorporated by every estimator.
    pub fn samples_seen(&self) -> u64 {
        self.estimators[0].samples_seen()
    }

    /// Reset every estimator.
    pub fn reset(&mut self) {
        for estimator in &mut self.estimators {
            estimator.reset();
        }
    }
}

impl ValidateState for P2Quantiles {
    fn validate_state(&self) -> Result<(), RillError> {
        if self.estimators.is_empty() || self.estimators.len() > MAX_TRACKED_QUANTILES {
            return Err(RillError::InvalidState(
                "P2Quantiles estimator count is out of bounds".to_owned(),
            ));
        }
        let samples = self.estimators[0].samples_seen();
        for (index, estimator) in self.estimators.iter().enumerate() {
            estimator.validate_state()?;
            if estimator.samples_seen() != samples
                || self.estimators[..index]
                    .iter()
                    .any(|previous| previous.quantile() == estimator.quantile())
            {
                return Err(RillError::InvalidState(
                    "P2Quantiles estimators are inconsistent".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

fn exact_linear_quantile(sorted: &[f64], quantile: f64) -> f64 {
    let position = (sorted.len() - 1) as f64 * quantile;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let weight = position - lower as f64;
        sorted[lower] + weight * (sorted[upper] - sorted[lower])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::{Rng, SeedableRng};

    #[test]
    fn bootstrap_values_are_exact() {
        let mut median = P2Quantile::new(0.5).unwrap();
        for value in [3.0, 1.0, 2.0, 4.0] {
            median.update(value).unwrap();
        }
        assert_eq!(median.value(), Some(2.5));
        assert_eq!(median.samples_seen(), 4);
    }

    #[test]
    fn monotonic_and_constant_streams() {
        let mut median = P2Quantile::new(0.5).unwrap();
        for value in 1..=1000 {
            median.update(value as f64).unwrap();
        }
        assert!((median.value().unwrap() - 500.0).abs() <= 2.0);

        let mut constant = P2Quantile::new(0.99).unwrap();
        for _ in 0..10_000 {
            constant.update(7.25).unwrap();
        }
        assert_eq!(constant.value(), Some(7.25));
    }

    #[test]
    fn seeded_random_stream_matches_offline_rank() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for quantile in [0.1, 0.5, 0.9, 0.99] {
            let mut estimator = P2Quantile::new(quantile).unwrap();
            let mut data = Vec::new();
            for _ in 0..20_000 {
                let value = rng.gen_range(-1000.0..1000.0);
                estimator.update(value).unwrap();
                data.push(value);
            }
            data.sort_by(f64::total_cmp);
            let estimate = estimator.value().unwrap();
            let rank = data.partition_point(|value| *value <= estimate) as f64 / data.len() as f64;
            assert!(
                (rank - quantile).abs() < 0.02,
                "q={quantile}, estimated rank={rank}, value={estimate}"
            );
        }
    }

    #[test]
    fn extreme_finite_values_and_non_finite_rejection_are_atomic() {
        let mut estimator = P2Quantile::new(0.5).unwrap();
        for value in [-1e150, -1e100, 0.0, 1e100, 1e150] {
            estimator.update(value).unwrap();
        }
        assert!(estimator.value().unwrap().is_finite());
        let before = estimator.clone();
        assert!(estimator.update(f64::NAN).is_err());
        assert_eq!(estimator, before);
    }

    #[test]
    fn multiple_quantiles_are_bounded_resettable_and_validated() {
        let mut quantiles = P2Quantiles::new(&[0.1, 0.5, 0.9]).unwrap();
        for value in 0..1000 {
            quantiles.update(value as f64).unwrap();
        }
        let values = quantiles.values();
        assert!(values[0].1.unwrap() < values[1].1.unwrap());
        assert!(values[1].1.unwrap() < values[2].1.unwrap());
        quantiles.validate_state().unwrap();
        quantiles.reset();
        assert_eq!(quantiles.samples_seen(), 0);
        assert!(quantiles.values().iter().all(|(_, value)| value.is_none()));
    }

    #[test]
    fn validation_rejects_corrupt_marker_counts_and_positions() {
        let mut estimator = P2Quantile::new(0.5).unwrap();
        for value in 0..20 {
            estimator.update(value as f64).unwrap();
        }
        let mut corrupt = estimator.clone();
        corrupt.desired_positions[2] += 10.0;
        assert!(corrupt.validate_state().is_err());
        let mut corrupt = estimator.clone();
        corrupt.marker_positions[4] -= 1;
        assert!(corrupt.validate_state().is_err());
        let mut corrupt = estimator;
        corrupt.marker_heights[2] = corrupt.marker_heights[1] - 1.0;
        assert!(corrupt.validate_state().is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_preserves_future_continuity() {
        let mut original = P2Quantile::new(0.75).unwrap();
        for value in 0..100 {
            original.update(value as f64).unwrap();
        }
        let json = serde_json::to_string(&original).unwrap();
        let mut restored: P2Quantile = serde_json::from_str(&json).unwrap();
        restored.validate_state().unwrap();
        for value in 100..200 {
            original.update(value as f64).unwrap();
            restored.update(value as f64).unwrap();
            assert_eq!(original, restored);
        }
    }

    proptest! {
        #[test]
        fn property_estimate_stays_within_observed_range(
            values in prop::collection::vec(-1e6_f64..1e6_f64, 1..500),
            quantile in 0.01_f64..0.99_f64,
        ) {
            let mut estimator = P2Quantile::new(quantile).unwrap();
            for &value in &values {
                estimator.update(value).unwrap();
            }
            let estimate = estimator.value().unwrap();
            let min = values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            prop_assert!(estimate >= min && estimate <= max);
            prop_assert!(estimate.is_finite());
            estimator.validate_state().unwrap();
        }
    }
}

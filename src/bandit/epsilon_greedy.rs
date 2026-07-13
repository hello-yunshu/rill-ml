//! Epsilon-Greedy bandit algorithm.
//!
//! The simplest multi-armed bandit strategy: with probability `epsilon`, select
//! a random arm (exploration); otherwise, select the arm with the highest
//! observed mean reward (exploitation).
//!
//! Epsilon can be fixed or decayed over time using exponential decay, which
//! reduces exploration as more data is collected.
//!
//! ## Complexity
//!
//! - `select`: `O(arm_count)` — must scan all arms to find the best.
//! - `update`: `O(1)`.
//! - Space: `O(arm_count)`.

use crate::bandit::stats::ArmStats;
use crate::bandit::{
    Bandit, checked_finite_add, checked_increment, validate_arm, validate_reward_finite,
    validate_sample_count,
};
use crate::error::RillError;
use rand::Rng;

/// Configuration for [`EpsilonGreedy`].
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::EpsilonGreedyConfig;
///
/// let config = EpsilonGreedyConfig {
///     epsilon: 0.1,
///     decay: 0.999,
///     min_epsilon: 0.01,
/// };
/// assert!((config.epsilon - 0.1).abs() < 1e-12);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EpsilonGreedyConfig {
    /// Initial exploration probability. Must be in `[0, 1]`.
    ///
    /// `0.0` means pure exploitation, `1.0` means pure exploration.
    pub epsilon: f64,

    /// Exponential decay factor applied to epsilon after each update.
    ///
    /// Set to `1.0` for no decay (fixed epsilon). Must be in `(0, 1]`.
    /// After each `update`, epsilon becomes `max(min_epsilon, epsilon * decay)`.
    pub decay: f64,

    /// Lower bound for epsilon after decay. Must be in `[0, epsilon]`.
    pub min_epsilon: f64,
}

impl Default for EpsilonGreedyConfig {
    fn default() -> Self {
        Self {
            epsilon: 0.1,
            decay: 1.0,
            min_epsilon: 0.01,
        }
    }
}

impl EpsilonGreedyConfig {
    /// Validate the configuration without constructing a bandit.
    pub fn validate(&self) -> Result<(), RillError> {
        if !(0.0..=1.0).contains(&self.epsilon) {
            return Err(RillError::InvalidEpsilon(self.epsilon));
        }
        if !(0.0 < self.decay && self.decay <= 1.0) {
            return Err(RillError::InvalidParameter {
                name: "decay",
                value: self.decay,
            });
        }
        if !(0.0..=self.epsilon).contains(&self.min_epsilon) {
            return Err(RillError::InvalidParameter {
                name: "min_epsilon",
                value: self.min_epsilon,
            });
        }
        Ok(())
    }
}

/// Epsilon-Greedy multi-armed bandit.
///
/// With probability `epsilon`, selects a random arm; otherwise selects the
/// arm with the highest observed mean reward. Supports optional epsilon decay.
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::{Bandit, EpsilonGreedy, EpsilonGreedyConfig};
/// use rand::SeedableRng;
/// use rand_chacha::ChaCha8Rng;
///
/// let mut rng = ChaCha8Rng::seed_from_u64(0);
/// let mut bandit = EpsilonGreedy::new(3, EpsilonGreedyConfig::default()).unwrap();
///
/// let arm = bandit.select(&mut rng).unwrap();
/// bandit.update(arm, 1.0).unwrap();
/// assert_eq!(bandit.samples_seen(), 1);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EpsilonGreedy {
    arm_count: usize,
    config: EpsilonGreedyConfig,
    /// Per-arm pull counts.
    pulls: Vec<u64>,
    /// Per-arm total rewards.
    total_rewards: Vec<f64>,
    /// Total number of updates.
    samples_seen: u64,
    /// Current epsilon (may differ from config.epsilon after decay).
    current_epsilon: f64,
}

impl EpsilonGreedy {
    /// Create a new epsilon-greedy bandit.
    ///
    /// # Errors
    ///
    /// Returns `RillError::InvalidArmCount` if `arm_count` is zero.
    /// Returns `RillError::InvalidEpsilon` if epsilon is not in `[0, 1]`.
    /// Returns `RillError::InvalidParameter` if decay is not in `(0, 1]`.
    pub fn new(arm_count: usize, config: EpsilonGreedyConfig) -> Result<Self, RillError> {
        if arm_count == 0 {
            return Err(RillError::InvalidArmCount(arm_count));
        }
        config.validate()?;

        Ok(Self {
            arm_count,
            current_epsilon: config.epsilon,
            config,
            pulls: vec![0; arm_count],
            total_rewards: vec![0.0; arm_count],
            samples_seen: 0,
        })
    }

    /// The current (possibly decayed) epsilon value.
    pub const fn current_epsilon(&self) -> f64 {
        self.current_epsilon
    }

    /// Per-arm pull counts (diagnostic).
    pub fn pulls(&self) -> &[u64] {
        &self.pulls
    }

    /// Per-arm total rewards (diagnostic).
    pub fn total_rewards(&self) -> &[f64] {
        &self.total_rewards
    }

    /// Validate all persisted state invariants.
    ///
    /// This is also run automatically during deserialization.
    pub fn validate(&self) -> Result<(), RillError> {
        if self.arm_count == 0 {
            return Err(RillError::InvalidArmCount(self.arm_count));
        }
        self.config.validate()?;
        if self.pulls.len() != self.arm_count || self.total_rewards.len() != self.arm_count {
            return Err(RillError::InvalidState(
                "arm_count does not match per-arm state lengths".to_owned(),
            ));
        }
        if self.total_rewards.iter().any(|value| !value.is_finite()) {
            return Err(RillError::InvalidState(
                "total_rewards must contain only finite values".to_owned(),
            ));
        }
        validate_sample_count(&self.pulls, self.samples_seen)?;
        if !self.current_epsilon.is_finite()
            || self.current_epsilon < self.config.min_epsilon
            || self.current_epsilon > self.config.epsilon
        {
            return Err(RillError::InvalidState(
                "current_epsilon is outside the configured bounds".to_owned(),
            ));
        }
        Ok(())
    }

    /// Find the arm with the highest mean reward.
    /// Ties are broken by choosing the lowest index.
    fn best_arm(&self) -> usize {
        let mut best = 0usize;
        let mut best_mean = f64::NEG_INFINITY;
        for (i, &pulls) in self.pulls.iter().enumerate() {
            let mean = if pulls > 0 {
                self.total_rewards[i] / pulls as f64
            } else {
                // Unpulled arms have unknown reward — treat as -inf so they
                // are only selected if all arms are unpulled.
                f64::NEG_INFINITY
            };
            if mean > best_mean {
                best_mean = mean;
                best = i;
            }
        }
        // If all arms are unpulled, best_arm returns 0.
        best
    }
}

impl Bandit for EpsilonGreedy {
    fn arm_count(&self) -> usize {
        self.arm_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn select(&self, rng: &mut impl Rng) -> Result<usize, RillError> {
        // If no arm has been pulled yet, select randomly to ensure exploration.
        if self.samples_seen == 0 || self.pulls.iter().all(|&p| p == 0) {
            let arm = rng.gen_range(0..self.arm_count);
            return Ok(arm);
        }

        // Exploration vs exploitation.
        let r: f64 = rng.gen_range(0.0..1.0);
        if r < self.current_epsilon {
            // Explore: pick a random arm.
            let arm = rng.gen_range(0..self.arm_count);
            Ok(arm)
        } else {
            // Exploit: pick the best arm.
            Ok(self.best_arm())
        }
    }

    fn update(&mut self, arm: usize, reward: f64) -> Result<(), RillError> {
        validate_arm(self.arm_count, arm)?;
        validate_reward_finite(reward)?;

        // Compute every fallible change before mutating state so an error never
        // leaves a partially updated model.
        let next_pulls = checked_increment(self.pulls[arm], "pulls")?;
        let next_total = checked_finite_add(self.total_rewards[arm], reward, "total_rewards")?;
        let next_samples = checked_increment(self.samples_seen, "samples_seen")?;

        // Apply epsilon decay.
        let next_epsilon = if self.config.decay < 1.0 {
            (self.current_epsilon * self.config.decay).max(self.config.min_epsilon)
        } else {
            self.current_epsilon
        };

        self.pulls[arm] = next_pulls;
        self.total_rewards[arm] = next_total;
        self.samples_seen = next_samples;
        self.current_epsilon = next_epsilon;

        Ok(())
    }

    fn reset(&mut self) {
        for p in &mut self.pulls {
            *p = 0;
        }
        for r in &mut self.total_rewards {
            *r = 0.0;
        }
        self.samples_seen = 0;
        self.current_epsilon = self.config.epsilon;
    }

    fn arm_stats(&self, arm: usize) -> Result<ArmStats, RillError> {
        validate_arm(self.arm_count, arm)?;
        ArmStats::new(self.pulls[arm], self.total_rewards[arm])
    }
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct EpsilonGreedyState {
    arm_count: usize,
    config: EpsilonGreedyConfig,
    pulls: Vec<u64>,
    total_rewards: Vec<f64>,
    samples_seen: u64,
    current_epsilon: f64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for EpsilonGreedy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let state = EpsilonGreedyState::deserialize(deserializer)?;
        let bandit = Self {
            arm_count: state.arm_count,
            config: state.config,
            pulls: state.pulls,
            total_rewards: state.total_rewards,
            samples_seen: state.samples_seen,
            current_epsilon: state.current_epsilon,
        };
        bandit.validate().map_err(serde::de::Error::custom)?;
        Ok(bandit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn make_bandit(epsilon: f64) -> EpsilonGreedy {
        EpsilonGreedy::new(
            3,
            EpsilonGreedyConfig {
                epsilon,
                decay: 1.0,
                min_epsilon: 0.0,
            },
        )
        .unwrap()
    }

    #[test]
    fn rejects_zero_arm_count() {
        let result = EpsilonGreedy::new(0, EpsilonGreedyConfig::default());
        assert!(matches!(result, Err(RillError::InvalidArmCount(0))));
    }

    #[test]
    fn rejects_invalid_epsilon() {
        let result = EpsilonGreedy::new(
            3,
            EpsilonGreedyConfig {
                epsilon: 1.5,
                decay: 1.0,
                min_epsilon: 0.0,
            },
        );
        assert!(matches!(result, Err(RillError::InvalidEpsilon(_))));
    }

    #[test]
    fn rejects_invalid_decay() {
        let result = EpsilonGreedy::new(
            3,
            EpsilonGreedyConfig {
                epsilon: 0.1,
                decay: 0.0,
                min_epsilon: 0.0,
            },
        );
        assert!(matches!(result, Err(RillError::InvalidParameter { .. })));
    }

    #[test]
    fn rejects_invalid_min_epsilon() {
        let result = EpsilonGreedy::new(
            3,
            EpsilonGreedyConfig {
                epsilon: 0.1,
                decay: 1.0,
                min_epsilon: 0.5, // > epsilon
            },
        );
        assert!(matches!(result, Err(RillError::InvalidParameter { .. })));
    }

    #[test]
    fn initial_state() {
        let b = make_bandit(0.1);
        assert_eq!(b.arm_count(), 3);
        assert_eq!(b.samples_seen(), 0);
        assert!((b.current_epsilon() - 0.1).abs() < 1e-12);
        for &pulls in b.pulls() {
            assert_eq!(pulls, 0);
        }
    }

    #[test]
    fn select_with_no_data_returns_valid_arm() {
        let b = make_bandit(0.1);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let arm = b.select(&mut rng).unwrap();
        assert!(arm < 3);
    }

    #[test]
    fn update_increments_samples_seen() {
        let mut b = make_bandit(0.1);
        b.update(0, 1.0).unwrap();
        b.update(1, 0.5).unwrap();
        b.update(2, 0.0).unwrap();
        assert_eq!(b.samples_seen(), 3);
    }

    #[test]
    fn update_rejects_invalid_arm() {
        let mut b = make_bandit(0.1);
        assert!(b.update(3, 1.0).is_err());
    }

    #[test]
    fn update_rejects_non_finite_reward() {
        let mut b = make_bandit(0.1);
        assert!(b.update(0, f64::NAN).is_err());
        assert!(b.update(0, f64::INFINITY).is_err());
    }

    #[test]
    fn update_rejects_overflow_without_mutating_state() {
        let mut b = make_bandit(0.1);
        b.update(0, f64::MAX).unwrap();
        let before = b.clone();
        assert!(b.update(0, f64::MAX).is_err());
        assert_eq!(b.pulls(), before.pulls());
        assert_eq!(b.total_rewards(), before.total_rewards());
        assert_eq!(b.samples_seen(), before.samples_seen());
    }

    #[test]
    fn arm_stats_after_updates() {
        let mut b = make_bandit(0.1);
        b.update(0, 1.0).unwrap();
        b.update(0, 3.0).unwrap();
        let stats = b.arm_stats(0).unwrap();
        assert_eq!(stats.pulls, 2);
        assert!((stats.total_reward - 4.0).abs() < 1e-12);
        assert!((stats.mean_reward - 2.0).abs() < 1e-12);
    }

    #[test]
    fn arm_stats_rejects_invalid_arm() {
        let b = make_bandit(0.1);
        assert!(b.arm_stats(5).is_err());
    }

    #[test]
    fn exploitation_picks_best_arm() {
        // With epsilon = 0, the bandit always exploits.
        let mut b = make_bandit(0.0);
        // Arm 1 has the highest mean reward.
        b.update(0, 0.1).unwrap();
        b.update(1, 0.9).unwrap();
        b.update(2, 0.5).unwrap();

        let mut rng = ChaCha8Rng::seed_from_u64(0);
        for _ in 0..20 {
            let arm = b.select(&mut rng).unwrap();
            assert_eq!(arm, 1, "pure exploitation should pick arm 1");
        }
    }

    #[test]
    fn exploration_with_epsilon_one() {
        // With epsilon = 1, the bandit always explores.
        let mut b = make_bandit(1.0);
        b.update(0, 0.1).unwrap();
        b.update(1, 0.9).unwrap();
        b.update(2, 0.5).unwrap();

        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let mut arms_seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let arm = b.select(&mut rng).unwrap();
            arms_seen.insert(arm);
        }
        // With pure exploration over 100 draws, all 3 arms should appear.
        assert_eq!(arms_seen.len(), 3);
    }

    #[test]
    fn epsilon_decay_reduces_exploration() {
        let mut b = EpsilonGreedy::new(
            3,
            EpsilonGreedyConfig {
                epsilon: 0.5,
                decay: 0.9,
                min_epsilon: 0.01,
            },
        )
        .unwrap();
        assert!((b.current_epsilon() - 0.5).abs() < 1e-12);

        for _ in 0..10 {
            b.update(0, 1.0).unwrap();
        }
        // After 10 decays: 0.5 * 0.9^10 ≈ 0.174
        assert!(b.current_epsilon() < 0.5);
        assert!(b.current_epsilon() > 0.01);
    }

    #[test]
    fn epsilon_decay_respects_min_epsilon() {
        let mut b = EpsilonGreedy::new(
            2,
            EpsilonGreedyConfig {
                epsilon: 0.5,
                decay: 0.1,
                min_epsilon: 0.2,
            },
        )
        .unwrap();
        // After one update: 0.5 * 0.1 = 0.05, but min is 0.2.
        b.update(0, 1.0).unwrap();
        assert!((b.current_epsilon() - 0.2).abs() < 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut b = EpsilonGreedy::new(
            3,
            EpsilonGreedyConfig {
                epsilon: 0.5,
                decay: 0.9,
                min_epsilon: 0.01,
            },
        )
        .unwrap();
        b.update(0, 1.0).unwrap();
        b.update(1, 0.5).unwrap();
        assert_eq!(b.samples_seen(), 2);
        assert!(b.current_epsilon() < 0.5);

        b.reset();
        assert_eq!(b.samples_seen(), 0);
        assert!((b.current_epsilon() - 0.5).abs() < 1e-12);
        for &pulls in b.pulls() {
            assert_eq!(pulls, 0);
        }
    }

    #[test]
    fn best_arm_tie_breaks_by_lowest_index() {
        let mut b = make_bandit(0.0);
        b.update(0, 1.0).unwrap();
        b.update(1, 1.0).unwrap();
        b.update(2, 0.5).unwrap();
        // Arms 0 and 1 have the same mean; arm 0 should be selected.
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let arm = b.select(&mut rng).unwrap();
        assert_eq!(arm, 0);
    }

    #[test]
    fn finds_best_arm_in_simulation() {
        let mut b = make_bandit(0.1);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        // Simulate: arm 0 has mean 0.8, arm 1 has mean 0.3, arm 2 has mean 0.5.
        for _ in 0..500 {
            let arm = b.select(&mut rng).unwrap();
            let reward = match arm {
                0 => 0.8,
                1 => 0.3,
                _ => 0.5,
            };
            b.update(arm, reward).unwrap();
        }

        // Arm 0 should have the highest mean reward.
        let stats0 = b.arm_stats(0).unwrap();
        let stats1 = b.arm_stats(1).unwrap();
        let stats2 = b.arm_stats(2).unwrap();
        assert!(stats0.mean_reward > stats1.mean_reward);
        assert!(stats0.mean_reward > stats2.mean_reward);
        // Arm 0 should be pulled most often.
        assert!(stats0.pulls > stats1.pulls);
        assert!(stats0.pulls > stats2.pulls);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut b = EpsilonGreedy::new(
            3,
            EpsilonGreedyConfig {
                epsilon: 0.2,
                decay: 0.95,
                min_epsilon: 0.01,
            },
        )
        .unwrap();
        b.update(0, 1.0).unwrap();
        b.update(1, 0.5).unwrap();

        let json = serde_json::to_string(&b).unwrap();
        let restored: EpsilonGreedy = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.arm_count(), b.arm_count());
        assert_eq!(restored.samples_seen(), b.samples_seen());
        assert!((restored.current_epsilon() - b.current_epsilon()).abs() < 1e-12);
        assert_eq!(restored.pulls(), b.pulls());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_malformed_state() {
        let json = r#"{
            "arm_count": 2,
            "config": {"epsilon": 0.1, "decay": 1.0, "min_epsilon": 0.01},
            "pulls": [1],
            "total_rewards": [1.0],
            "samples_seen": 1,
            "current_epsilon": 0.1
        }"#;
        assert!(serde_json::from_str::<EpsilonGreedy>(json).is_err());
    }
}

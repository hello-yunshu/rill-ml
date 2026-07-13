//! UCB1 (Upper Confidence Bound 1) bandit algorithm.
//!
//! UCB1 balances exploration and exploitation by selecting the arm that
//! maximizes:
//!
//! ```text
//! mean_reward + c * sqrt(2 * ln(total_pulls) / arm_pulls)
//! ```
//!
//! The first term is the exploitation term (estimated reward), and the second
//! term is the exploration bonus that decreases as an arm is pulled more often.
//! Arms that have never been pulled are selected first.
//!
//! ## Complexity
//!
//! - `select`: `O(arm_count)`.
//! - `update`: `O(1)`.
//! - Space: `O(arm_count)`.
//!
//! ## Reference
//!
//! Auer, Cesa-Bianchi, and Fischer. "Finite-time Analysis of the Multiarmed
//! Bandit Problem." Machine Learning, 2002.

use crate::bandit::stats::ArmStats;
use crate::bandit::{
    Bandit, checked_finite_add, checked_increment, validate_arm, validate_reward_01,
    validate_sample_count,
};
use crate::error::RillError;
use rand::Rng;

/// Configuration for [`Ucb1`].
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::Ucb1Config;
///
/// let config = Ucb1Config { exploration_constant: 2.0 };
/// assert!((config.exploration_constant - 2.0).abs() < 1e-12);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ucb1Config {
    /// Exploration constant `c` in the UCB formula. Controls the
    /// exploration/exploitation trade-off. Higher values favor exploration.
    ///
    /// Must be finite and positive. With the formula used by this type, the
    /// classic UCB1 value is `1.0`.
    pub exploration_constant: f64,
}

impl Default for Ucb1Config {
    fn default() -> Self {
        Self {
            exploration_constant: 1.0,
        }
    }
}

impl Ucb1Config {
    /// Validate the configuration without constructing a bandit.
    pub fn validate(&self) -> Result<(), RillError> {
        if !self.exploration_constant.is_finite() || self.exploration_constant <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "exploration_constant",
                value: self.exploration_constant,
            });
        }
        Ok(())
    }
}

/// UCB1 multi-armed bandit.
///
/// Selects arms using the upper confidence bound formula, which automatically
/// balances exploration and exploitation. Unpulled arms are always selected
/// first. Rewards passed to [`Bandit::update`] must be normalized to `[0, 1]`.
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::{Bandit, Ucb1, Ucb1Config};
/// use rand::SeedableRng;
/// use rand_chacha::ChaCha8Rng;
///
/// let mut rng = ChaCha8Rng::seed_from_u64(0);
/// let mut bandit = Ucb1::new(3, Ucb1Config::default()).unwrap();
///
/// let arm = bandit.select(&mut rng).unwrap();
/// bandit.update(arm, 1.0).unwrap();
/// assert_eq!(bandit.samples_seen(), 1);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Ucb1 {
    arm_count: usize,
    config: Ucb1Config,
    /// Per-arm pull counts.
    pulls: Vec<u64>,
    /// Per-arm total rewards.
    total_rewards: Vec<f64>,
    /// Total number of updates.
    samples_seen: u64,
}

impl Ucb1 {
    /// Create a new UCB1 bandit.
    ///
    /// # Errors
    ///
    /// Returns `RillError::InvalidArmCount` if `arm_count` is zero.
    /// Returns `RillError::InvalidParameter` if `exploration_constant` is not
    /// finite and positive.
    pub fn new(arm_count: usize, config: Ucb1Config) -> Result<Self, RillError> {
        if arm_count == 0 {
            return Err(RillError::InvalidArmCount(arm_count));
        }
        config.validate()?;

        Ok(Self {
            arm_count,
            config,
            pulls: vec![0; arm_count],
            total_rewards: vec![0.0; arm_count],
            samples_seen: 0,
        })
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
        validate_sample_count(&self.pulls, self.samples_seen)?;
        for (arm, (&pulls, &reward)) in self.pulls.iter().zip(self.total_rewards.iter()).enumerate()
        {
            if !reward.is_finite() || reward < 0.0 || reward > pulls as f64 {
                return Err(RillError::InvalidState(format!(
                    "total reward for arm {arm} is inconsistent with [0, 1] rewards"
                )));
            }
        }
        Ok(())
    }

    /// Compute the UCB value for a specific arm.
    ///
    /// Returns `f64::INFINITY` for unpulled arms (they are always selected first).
    fn ucb_value(&self, arm: usize) -> f64 {
        let pulls = self.pulls[arm];
        if pulls == 0 {
            return f64::INFINITY;
        }
        let mean = self.total_rewards[arm] / pulls as f64;
        // Exploration bonus: c * sqrt(2 * ln(N) / n_i)
        // where N = total pulls, n_i = arm pulls.
        let log_total = (self.samples_seen as f64).ln();
        let exploration =
            self.config.exploration_constant * (2.0 * log_total / pulls as f64).sqrt();
        mean + exploration
    }
}

impl Bandit for Ucb1 {
    fn arm_count(&self) -> usize {
        self.arm_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn select(&self, rng: &mut impl Rng) -> Result<usize, RillError> {
        // Compute UCB values for all arms.
        let mut best_arm = 0usize;
        let mut best_value = f64::NEG_INFINITY;
        let mut unexplored: Vec<usize> = Vec::new();

        for arm in 0..self.arm_count {
            if self.pulls[arm] == 0 {
                unexplored.push(arm);
                continue;
            }
            let value = self.ucb_value(arm);
            if value > best_value {
                best_value = value;
                best_arm = arm;
            }
        }

        // If there are unexplored arms, pick one at random.
        if !unexplored.is_empty() {
            let idx = rng.gen_range(0..unexplored.len());
            return Ok(unexplored[idx]);
        }

        Ok(best_arm)
    }

    fn update(&mut self, arm: usize, reward: f64) -> Result<(), RillError> {
        validate_arm(self.arm_count, arm)?;
        validate_reward_01(reward)?;

        let next_pulls = checked_increment(self.pulls[arm], "pulls")?;
        let next_total = checked_finite_add(self.total_rewards[arm], reward, "total_rewards")?;
        let next_samples = checked_increment(self.samples_seen, "samples_seen")?;
        self.pulls[arm] = next_pulls;
        self.total_rewards[arm] = next_total;
        self.samples_seen = next_samples;
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
    }

    fn arm_stats(&self, arm: usize) -> Result<ArmStats, RillError> {
        validate_arm(self.arm_count, arm)?;
        ArmStats::new(self.pulls[arm], self.total_rewards[arm])
    }
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct Ucb1State {
    arm_count: usize,
    config: Ucb1Config,
    pulls: Vec<u64>,
    total_rewards: Vec<f64>,
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Ucb1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let state = Ucb1State::deserialize(deserializer)?;
        let bandit = Self {
            arm_count: state.arm_count,
            config: state.config,
            pulls: state.pulls,
            total_rewards: state.total_rewards,
            samples_seen: state.samples_seen,
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

    fn make_bandit() -> Ucb1 {
        Ucb1::new(3, Ucb1Config::default()).unwrap()
    }

    #[test]
    fn rejects_zero_arm_count() {
        let result = Ucb1::new(0, Ucb1Config::default());
        assert!(matches!(result, Err(RillError::InvalidArmCount(0))));
    }

    #[test]
    fn rejects_invalid_exploration_constant() {
        for &bad in &[0.0, -1.0, f64::NAN, f64::INFINITY] {
            let result = Ucb1::new(
                3,
                Ucb1Config {
                    exploration_constant: bad,
                },
            );
            assert!(matches!(result, Err(RillError::InvalidParameter { .. })));
        }
    }

    #[test]
    fn initial_state() {
        let b = make_bandit();
        assert_eq!(b.arm_count(), 3);
        assert_eq!(b.samples_seen(), 0);
    }

    #[test]
    fn unpulled_arms_selected_first() {
        let b = make_bandit();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        // All arms unpulled — select should return a valid arm.
        let arm = b.select(&mut rng).unwrap();
        assert!(arm < 3);
    }

    #[test]
    fn unexplored_arms_prioritized() {
        let mut b = make_bandit();
        // Pull arm 0 and arm 1, leaving arm 2 unexplored.
        b.update(0, 1.0).unwrap();
        b.update(1, 0.5).unwrap();

        let mut rng = ChaCha8Rng::seed_from_u64(0);
        // Arm 2 should be selected (it's unexplored).
        let arm = b.select(&mut rng).unwrap();
        assert_eq!(arm, 2);
    }

    #[test]
    fn all_arms_explored_uses_ucb_formula() {
        let mut b = make_bandit();
        // Pull all arms at least once.
        b.update(0, 0.9).unwrap();
        b.update(1, 0.3).unwrap();
        b.update(2, 0.5).unwrap();

        // Arm 0 has the highest mean reward, and with equal pulls the
        // exploration bonus is the same, so arm 0 should be selected.
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let arm = b.select(&mut rng).unwrap();
        assert_eq!(arm, 0);
    }

    #[test]
    fn ucb_value_for_unpulled_arm_is_infinity() {
        let b = make_bandit();
        assert!(b.ucb_value(0).is_infinite());
    }

    #[test]
    fn ucb_value_decreases_with_more_pulls() {
        let mut b = make_bandit();
        // Pull all arms once first to establish a baseline (ln(total) > 0).
        b.update(0, 1.0).unwrap();
        b.update(1, 0.5).unwrap();
        b.update(2, 0.5).unwrap();
        let v1 = b.ucb_value(0);
        // Pull arm 0 several more times with the same reward.
        for _ in 0..10 {
            b.update(0, 1.0).unwrap();
        }
        let v2 = b.ucb_value(0);
        // More pulls on arm 0 → lower exploration bonus → lower UCB value.
        assert!(v2 < v1);
    }

    #[test]
    fn update_rejects_invalid_arm() {
        let mut b = make_bandit();
        assert!(b.update(3, 1.0).is_err());
    }

    #[test]
    fn update_rejects_reward_outside_unit_interval() {
        let mut b = make_bandit();
        assert!(b.update(0, f64::NAN).is_err());
        assert!(b.update(0, -0.1).is_err());
        assert!(b.update(0, 1.1).is_err());
    }

    #[test]
    fn reset_clears_state() {
        let mut b = make_bandit();
        b.update(0, 1.0).unwrap();
        b.update(1, 0.5).unwrap();
        assert_eq!(b.samples_seen(), 2);

        b.reset();
        assert_eq!(b.samples_seen(), 0);
        for &pulls in b.pulls() {
            assert_eq!(pulls, 0);
        }
    }

    #[test]
    fn finds_best_arm_in_simulation() {
        let mut b = make_bandit();
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

        // Arm 0 should be pulled most often.
        let stats0 = b.arm_stats(0).unwrap();
        let stats1 = b.arm_stats(1).unwrap();
        let stats2 = b.arm_stats(2).unwrap();
        assert!(stats0.pulls > stats1.pulls);
        assert!(stats0.pulls > stats2.pulls);
        assert!(stats0.mean_reward > stats1.mean_reward);
    }

    #[test]
    fn arm_stats_rejects_invalid_arm() {
        let b = make_bandit();
        assert!(b.arm_stats(5).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut b = Ucb1::new(
            3,
            Ucb1Config {
                exploration_constant: 2.0,
            },
        )
        .unwrap();
        b.update(0, 1.0).unwrap();
        b.update(1, 0.5).unwrap();

        let json = serde_json::to_string(&b).unwrap();
        let restored: Ucb1 = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.arm_count(), b.arm_count());
        assert_eq!(restored.samples_seen(), b.samples_seen());
        assert_eq!(restored.pulls(), b.pulls());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_malformed_state() {
        let json = r#"{
            "arm_count": 2,
            "config": {"exploration_constant": 1.0},
            "pulls": [1],
            "total_rewards": [1.0],
            "samples_seen": 1
        }"#;
        assert!(serde_json::from_str::<Ucb1>(json).is_err());
    }
}

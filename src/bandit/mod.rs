//! Online decision-making: multi-armed bandits and contextual bandits.
//!
//! This module provides bounded-memory bandit algorithms that learn to select
//! the best action (arm) from a fixed set by balancing exploration and
//! exploitation. Bandits are independent from the supervised learning models
//! in [`models`](crate::models): they do not predict a target value, they
//! select an action whose reward is observed after the fact.
//!
//! ## Overview
//!
//! - **Non-contextual bandits** ([`Bandit`] trait): select an arm without
//!   contextual features. Implementations: [`EpsilonGreedy`], [`Ucb1`],
//!   [`ThompsonSampling`].
//! - **Contextual bandits** ([`ContextualBandit`] trait): select an arm based
//!   on a context vector. Implementation: [`LinUcb`].
//! - **Diagnostics**: [`ArmStats`] summarizes per-arm statistics.
//!
//! ## Contract
//!
//! The bandit contract mirrors the supervised learning contract:
//!
//! 1. `select` is side-effect free — it does not update the bandit's learned
//!    state. It may use the provided RNG for exploration.
//! 2. `update` incorporates the observed reward for a previously selected arm.
//! 3. The caller is responsible for deciding what "reward" means (business
//!    layer). RillML only requires it to be a finite `f64`.
//!
//! ## Quick start
//!
//! ```rust
//! use rill_ml::bandit::{Bandit, EpsilonGreedy, EpsilonGreedyConfig};
//! use rand::SeedableRng;
//! use rand_chacha::ChaCha8Rng;
//!
//! let mut rng = ChaCha8Rng::seed_from_u64(42);
//! let mut bandit = EpsilonGreedy::new(
//!     3,
//!     EpsilonGreedyConfig::default(),
//! ).unwrap();
//!
//! // Select an arm, observe a reward, update.
//! for _ in 0..100 {
//!     let arm = bandit.select(&mut rng).unwrap();
//!     // Simulate reward: arm 0 has the highest expected reward.
//!     let reward = match arm {
//!         0 => 0.8,
//!         1 => 0.3,
//!         _ => 0.5,
//!     };
//!     bandit.update(arm, reward).unwrap();
//! }
//!
//! // After learning, the bandit should prefer arm 0.
//! let stats = bandit.arm_stats(0).unwrap();
//! assert!(stats.pulls > 0);
//! ```
//!
//! [`EpsilonGreedy`]: crate::bandit::epsilon_greedy::EpsilonGreedy
//! [`Ucb1`]: crate::bandit::ucb1::Ucb1
//! [`ThompsonSampling`]: crate::bandit::thompson::ThompsonSampling
//! [`LinUcb`]: crate::bandit::linucb::LinUcb

pub mod epsilon_greedy;
pub mod linucb;
pub mod stats;
pub mod thompson;
pub mod ucb1;

pub use epsilon_greedy::{EpsilonGreedy, EpsilonGreedyConfig};
pub use linucb::{LinUcb, LinUcbConfig};
pub use stats::ArmStats;
pub use thompson::{ThompsonConfig, ThompsonSampling};
pub use ucb1::{Ucb1, Ucb1Config};

use crate::error::RillError;
use rand::Rng;

/// Online multi-armed bandit (non-contextual).
///
/// Implementations maintain per-arm reward statistics and use an exploration
/// strategy to balance trying new arms vs. exploiting the best-known arm.
///
/// `select` is side-effect free: it does not modify the bandit's learned
/// state. State updates happen exclusively in [`update`](Self::update).
///
/// All implementations must use bounded memory: `O(arm_count)` for
/// non-contextual bandits.
pub trait Bandit {
    /// The number of arms (actions) available.
    fn arm_count(&self) -> usize;

    /// How many total samples (arm pulls) the bandit has observed.
    fn samples_seen(&self) -> u64;

    /// Select an arm using the provided RNG for exploration.
    ///
    /// This method must not modify the bandit's learned state. The caller
    /// should call [`update`](Self::update) after observing the reward.
    fn select(&self, rng: &mut impl Rng) -> Result<usize, RillError>;

    /// Update the bandit with the observed reward for a previously selected arm.
    ///
    /// Returns an error if `arm` is out of range or `reward` is not finite
    /// (or outside the valid range for the specific bandit type).
    fn update(&mut self, arm: usize, reward: f64) -> Result<(), RillError>;

    /// Reset the bandit to its initial (no-data) state.
    fn reset(&mut self);

    /// Per-arm diagnostic statistics.
    ///
    /// Returns an error if `arm` is out of range.
    fn arm_stats(&self, arm: usize) -> Result<ArmStats, RillError>;
}

/// Online contextual bandit.
///
/// Contextual bandits select an arm based on a context (feature) vector,
/// allowing different arms to be optimal in different situations.
///
/// `select` is side-effect free. State updates happen exclusively in
/// [`update`](Self::update).
///
/// Memory complexity is `O(arm_count * feature_count^2)` for implementations
/// that maintain per-arm models (e.g. LinUCB).
pub trait ContextualBandit {
    /// The number of arms (actions) available.
    fn arm_count(&self) -> usize;

    /// The number of features in the context vector.
    fn feature_count(&self) -> usize;

    /// How many total samples the bandit has observed.
    fn samples_seen(&self) -> u64;

    /// Select an arm given the context vector, using the provided RNG.
    ///
    /// This method must not modify the bandit's learned state.
    fn select(&self, context: &[f64], rng: &mut impl Rng) -> Result<usize, RillError>;

    /// Update the bandit with the observed reward for a previously selected arm.
    fn update(&mut self, arm: usize, context: &[f64], reward: f64) -> Result<(), RillError>;

    /// Reset the bandit to its initial (no-data) state.
    fn reset(&mut self);
}

/// Validate that an arm index is within range.
pub(crate) fn validate_arm(arm_count: usize, arm: usize) -> Result<(), RillError> {
    if arm >= arm_count {
        Err(RillError::InvalidArm {
            expected: arm_count,
            actual: arm,
        })
    } else {
        Ok(())
    }
}

/// Validate that a reward is finite.
pub(crate) fn validate_reward_finite(reward: f64) -> Result<(), RillError> {
    if reward.is_finite() {
        Ok(())
    } else {
        Err(RillError::InvalidReward(reward))
    }
}

/// Validate that a reward is in `[0, 1]` (for bounded-reward bandits).
pub(crate) fn validate_reward_01(reward: f64) -> Result<(), RillError> {
    if reward.is_finite() && (0.0..=1.0).contains(&reward) {
        Ok(())
    } else {
        Err(RillError::InvalidReward(reward))
    }
}

/// Add two finite values and reject an overflow into NaN or infinity.
pub(crate) fn checked_finite_add(
    current: f64,
    delta: f64,
    field: &'static str,
) -> Result<f64, RillError> {
    let value = current + delta;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(RillError::NonFiniteValue { field, value })
    }
}

/// Increment a persisted counter without allowing wraparound.
pub(crate) fn checked_increment(value: u64, field: &'static str) -> Result<u64, RillError> {
    value
        .checked_add(1)
        .ok_or_else(|| RillError::InvalidState(format!("{field} overflow")))
}

/// Verify that per-arm pull counts sum to the global sample count.
pub(crate) fn validate_sample_count(pulls: &[u64], samples_seen: u64) -> Result<(), RillError> {
    let total = pulls.iter().try_fold(0u64, |sum, &pulls| {
        sum.checked_add(pulls)
            .ok_or_else(|| RillError::InvalidState("pull count sum overflow".into()))
    })?;
    if total == samples_seen {
        Ok(())
    } else {
        Err(RillError::InvalidState(format!(
            "samples_seen is {samples_seen}, but per-arm pulls sum to {total}"
        )))
    }
}

//! Thompson Sampling bandit algorithm (Bernoulli rewards).
//!
//! Thompson Sampling maintains a Beta distribution for each arm and selects
//! the arm with the highest sampled value. For Bernoulli rewards (0 or 1),
//! each arm's posterior is `Beta(alpha, beta)` where `alpha = successes + prior`
//! and `beta = failures + prior`.
//!
//! This implementation uses the Marsaglia-Tsang method for Gamma distribution
//! sampling, combined via `Beta(a, b) = Gamma(a) / (Gamma(a) + Gamma(b))`.
//! No external statistics crate is required.
//!
//! ## Complexity
//!
//! - `select`: `O(arm_count)` — samples one Beta value per arm.
//! - `update`: `O(1)`.
//! - Space: `O(arm_count)`.
//!
//! ## Reference
//!
//! Russo, Van Roy, Kazerouni, Osband, Wen. "A Tutorial on Thompson Sampling."
//! Foundations and Trends in Machine Learning, 2018.

use crate::bandit::stats::ArmStats;
use crate::bandit::{
    Bandit, checked_finite_add, checked_increment, validate_arm, validate_reward_01,
    validate_sample_count,
};
use crate::error::RillError;
use rand::Rng;

/// Configuration for [`ThompsonSampling`].
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::ThompsonConfig;
///
/// let config = ThompsonConfig {
///     alpha_prior: 1.0,
///     beta_prior: 1.0,
/// };
/// assert!((config.alpha_prior - 1.0).abs() < 1e-12);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ThompsonConfig {
    /// Prior alpha (success) parameter for the Beta distribution.
    ///
    /// Must be finite and positive. The default `1.0` gives a uniform prior
    /// `Beta(1, 1)`.
    pub alpha_prior: f64,

    /// Prior beta (failure) parameter for the Beta distribution.
    ///
    /// Must be finite and positive. The default `1.0` gives a uniform prior
    /// `Beta(1, 1)`.
    pub beta_prior: f64,
}

impl Default for ThompsonConfig {
    fn default() -> Self {
        Self {
            alpha_prior: 1.0,
            beta_prior: 1.0,
        }
    }
}

impl ThompsonConfig {
    /// Validate the configuration without constructing a bandit.
    pub fn validate(&self) -> Result<(), RillError> {
        if !self.alpha_prior.is_finite() || self.alpha_prior <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "alpha_prior",
                value: self.alpha_prior,
            });
        }
        if !self.beta_prior.is_finite() || self.beta_prior <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "beta_prior",
                value: self.beta_prior,
            });
        }
        Ok(())
    }
}

/// Thompson Sampling multi-armed bandit (Bernoulli rewards).
///
/// Maintains a Beta posterior for each arm. On `select`, samples from each
/// arm's posterior and returns the arm with the highest sample. On `update`,
/// applies a soft update to the arm's alpha (success) and beta (failure)
/// parameters.
///
/// Rewards must be in `[0, 1]`. The update is `alpha += reward` and
/// `beta += 1 - reward`; for strict Bernoulli rewards this is the standard
/// success/failure update, while fractional rewards produce a weighted update.
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::{Bandit, ThompsonSampling, ThompsonConfig};
/// use rand::SeedableRng;
/// use rand_chacha::ChaCha8Rng;
///
/// let mut rng = ChaCha8Rng::seed_from_u64(0);
/// let mut bandit = ThompsonSampling::new(3, ThompsonConfig::default()).unwrap();
///
/// let arm = bandit.select(&mut rng).unwrap();
/// bandit.update(arm, 1.0).unwrap();
/// assert_eq!(bandit.samples_seen(), 1);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ThompsonSampling {
    arm_count: usize,
    config: ThompsonConfig,
    /// Per-arm alpha (successes + prior).
    alphas: Vec<f64>,
    /// Per-arm beta (failures + prior).
    betas: Vec<f64>,
    /// Per-arm pull counts.
    pulls: Vec<u64>,
    /// Per-arm total rewards (for diagnostics).
    total_rewards: Vec<f64>,
    /// Total number of updates.
    samples_seen: u64,
}

impl ThompsonSampling {
    /// Create a new Thompson Sampling bandit.
    ///
    /// # Errors
    ///
    /// Returns `RillError::InvalidArmCount` if `arm_count` is zero.
    /// Returns `RillError::InvalidParameter` if priors are not finite and positive.
    pub fn new(arm_count: usize, config: ThompsonConfig) -> Result<Self, RillError> {
        if arm_count == 0 {
            return Err(RillError::InvalidArmCount(arm_count));
        }
        config.validate()?;

        let alpha_prior = config.alpha_prior;
        let beta_prior = config.beta_prior;
        Ok(Self {
            arm_count,
            config,
            alphas: vec![alpha_prior; arm_count],
            betas: vec![beta_prior; arm_count],
            pulls: vec![0; arm_count],
            total_rewards: vec![0.0; arm_count],
            samples_seen: 0,
        })
    }

    /// Per-arm alpha parameters (diagnostic).
    pub fn alphas(&self) -> &[f64] {
        &self.alphas
    }

    /// Per-arm beta parameters (diagnostic).
    pub fn betas(&self) -> &[f64] {
        &self.betas
    }

    /// Per-arm pull counts (diagnostic).
    pub fn pulls(&self) -> &[u64] {
        &self.pulls
    }

    /// Validate all persisted state invariants.
    ///
    /// This is also run automatically during deserialization.
    pub fn validate(&self) -> Result<(), RillError> {
        if self.arm_count == 0 {
            return Err(RillError::InvalidArmCount(self.arm_count));
        }
        self.config.validate()?;
        if self.alphas.len() != self.arm_count
            || self.betas.len() != self.arm_count
            || self.pulls.len() != self.arm_count
            || self.total_rewards.len() != self.arm_count
        {
            return Err(RillError::InvalidState(
                "arm_count does not match per-arm state lengths".to_owned(),
            ));
        }
        validate_sample_count(&self.pulls, self.samples_seen)?;

        for arm in 0..self.arm_count {
            let pulls = self.pulls[arm] as f64;
            let total = self.total_rewards[arm];
            let alpha = self.alphas[arm];
            let beta = self.betas[arm];
            if !total.is_finite() || total < 0.0 || total > pulls {
                return Err(RillError::InvalidState(format!(
                    "total reward for arm {arm} is inconsistent with [0, 1] rewards"
                )));
            }
            let expected_alpha = self.config.alpha_prior + total;
            let expected_beta = self.config.beta_prior + pulls - total;
            let alpha_tolerance = 1e-9 * expected_alpha.abs().max(1.0);
            let beta_tolerance = 1e-9 * expected_beta.abs().max(1.0);
            if !alpha.is_finite()
                || !beta.is_finite()
                || (alpha - expected_alpha).abs() > alpha_tolerance
                || (beta - expected_beta).abs() > beta_tolerance
            {
                return Err(RillError::InvalidState(format!(
                    "posterior parameters for arm {arm} are inconsistent with observations"
                )));
            }
        }
        Ok(())
    }

    /// Sample from a Beta(alpha, beta) distribution using the
    /// Gamma ratio method.
    ///
    /// Beta(a, b) = Gamma(a) / (Gamma(a) + Gamma(b))
    fn sample_beta(rng: &mut impl Rng, alpha: f64, beta: f64) -> f64 {
        let x = Self::sample_gamma(rng, alpha);
        let y = Self::sample_gamma(rng, beta);
        // Handle degenerate case where both samples are 0.
        let denom = x + y;
        if denom <= 0.0 {
            // Fall back to 0.5 for the degenerate case.
            0.5
        } else {
            x / denom
        }
    }

    /// Sample from a Gamma(shape, scale=1) distribution using the
    /// Marsaglia-Tsang method.
    ///
    /// For shape >= 1, uses the standard acceptance-rejection method.
    /// For shape < 1, uses the boosting trick: sample Gamma(shape+1) then
    /// multiply by U^(1/shape).
    fn sample_gamma(rng: &mut impl Rng, shape: f64) -> f64 {
        if shape < 1.0 {
            // Boosting: Gamma(shape) = Gamma(shape + 1) * U^(1/shape)
            let u: f64 = rng.gen_range(1e-10..1.0);
            let g = Self::sample_gamma(rng, shape + 1.0);
            return g * u.powf(1.0 / shape);
        }

        // Marsaglia-Tsang for shape >= 1.
        let d = shape - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();

        loop {
            // Sample from Normal(0, 1) using Box-Muller.
            let (x, _unused) = Self::box_muller(rng);
            let v = (1.0 + c * x).powi(3);
            if v <= 0.0 {
                continue;
            }
            let u: f64 = rng.gen_range(0.0..1.0);
            if u < 1.0 - 0.0331 * x.powi(4) {
                return d * v;
            }
            if u.ln() < 0.5 * x * x + d * (1.0 - v + v.ln()) {
                return d * v;
            }
        }
    }

    /// Generate a pair of standard normal random variables using the
    /// Box-Muller transform. Returns (z0, z1).
    fn box_muller(rng: &mut impl Rng) -> (f64, f64) {
        let u1: f64 = rng.gen_range(1e-10..1.0);
        let u2: f64 = rng.gen_range(0.0..1.0);
        let mag = (-2.0 * u1.ln()).sqrt();
        let z0 = mag * (2.0 * std::f64::consts::PI * u2).cos();
        let z1 = mag * (2.0 * std::f64::consts::PI * u2).sin();
        (z0, z1)
    }
}

impl Bandit for ThompsonSampling {
    fn arm_count(&self) -> usize {
        self.arm_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn select(&self, rng: &mut impl Rng) -> Result<usize, RillError> {
        let mut best_arm = 0usize;
        let mut best_sample = f64::NEG_INFINITY;

        for arm in 0..self.arm_count {
            let sample = Self::sample_beta(rng, self.alphas[arm], self.betas[arm]);
            if sample > best_sample {
                best_sample = sample;
                best_arm = arm;
            }
        }

        Ok(best_arm)
    }

    fn update(&mut self, arm: usize, reward: f64) -> Result<(), RillError> {
        validate_arm(self.arm_count, arm)?;
        validate_reward_01(reward)?;

        // Soft update: alpha += reward, beta += (1 - reward).
        // For strict Bernoulli (0 or 1), this is equivalent to the standard
        // success/failure counting. For continuous rewards in [0, 1], this
        // provides a weighted update.
        let next_alpha = checked_finite_add(self.alphas[arm], reward, "alpha")?;
        let next_beta = checked_finite_add(self.betas[arm], 1.0 - reward, "beta")?;
        let next_pulls = checked_increment(self.pulls[arm], "pulls")?;
        let next_total = checked_finite_add(self.total_rewards[arm], reward, "total_rewards")?;
        let next_samples = checked_increment(self.samples_seen, "samples_seen")?;

        self.alphas[arm] = next_alpha;
        self.betas[arm] = next_beta;
        self.pulls[arm] = next_pulls;
        self.total_rewards[arm] = next_total;
        self.samples_seen = next_samples;
        Ok(())
    }

    fn reset(&mut self) {
        for a in &mut self.alphas {
            *a = self.config.alpha_prior;
        }
        for b in &mut self.betas {
            *b = self.config.beta_prior;
        }
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
struct ThompsonSamplingState {
    arm_count: usize,
    config: ThompsonConfig,
    alphas: Vec<f64>,
    betas: Vec<f64>,
    pulls: Vec<u64>,
    total_rewards: Vec<f64>,
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ThompsonSampling {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let state = ThompsonSamplingState::deserialize(deserializer)?;
        let bandit = Self {
            arm_count: state.arm_count,
            config: state.config,
            alphas: state.alphas,
            betas: state.betas,
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

    fn make_bandit() -> ThompsonSampling {
        ThompsonSampling::new(3, ThompsonConfig::default()).unwrap()
    }

    #[test]
    fn rejects_zero_arm_count() {
        let result = ThompsonSampling::new(0, ThompsonConfig::default());
        assert!(matches!(result, Err(RillError::InvalidArmCount(0))));
    }

    #[test]
    fn rejects_invalid_priors() {
        for &bad in &[0.0, -1.0, f64::NAN, f64::INFINITY] {
            let result = ThompsonSampling::new(
                3,
                ThompsonConfig {
                    alpha_prior: bad,
                    beta_prior: 1.0,
                },
            );
            assert!(matches!(result, Err(RillError::InvalidParameter { .. })));

            let result = ThompsonSampling::new(
                3,
                ThompsonConfig {
                    alpha_prior: 1.0,
                    beta_prior: bad,
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
        // Alpha and beta should be initialized to priors.
        for &a in b.alphas() {
            assert!((a - 1.0).abs() < 1e-12);
        }
        for &be in b.betas() {
            assert!((be - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn select_returns_valid_arm() {
        let b = make_bandit();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let arm = b.select(&mut rng).unwrap();
        assert!(arm < 3);
    }

    #[test]
    fn update_with_success_increases_alpha() {
        let mut b = make_bandit();
        b.update(0, 1.0).unwrap();
        // alpha += 1.0, beta += 0.0
        assert!((b.alphas()[0] - 2.0).abs() < 1e-12);
        assert!((b.betas()[0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn update_with_failure_increases_beta() {
        let mut b = make_bandit();
        b.update(0, 0.0).unwrap();
        // alpha += 0.0, beta += 1.0
        assert!((b.alphas()[0] - 1.0).abs() < 1e-12);
        assert!((b.betas()[0] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn update_with_continuous_reward() {
        let mut b = make_bandit();
        b.update(0, 0.7).unwrap();
        // alpha += 0.7, beta += 0.3
        assert!((b.alphas()[0] - 1.7).abs() < 1e-12);
        assert!((b.betas()[0] - 1.3).abs() < 1e-12);
    }

    #[test]
    fn update_rejects_invalid_arm() {
        let mut b = make_bandit();
        assert!(b.update(3, 1.0).is_err());
    }

    #[test]
    fn update_rejects_reward_out_of_range() {
        let mut b = make_bandit();
        assert!(b.update(0, 1.5).is_err());
        assert!(b.update(0, -0.1).is_err());
        assert!(b.update(0, f64::NAN).is_err());
    }

    #[test]
    fn reset_clears_state() {
        let mut b = make_bandit();
        b.update(0, 1.0).unwrap();
        b.update(1, 0.0).unwrap();
        assert_eq!(b.samples_seen(), 2);

        b.reset();
        assert_eq!(b.samples_seen(), 0);
        for &a in b.alphas() {
            assert!((a - 1.0).abs() < 1e-12);
        }
        for &be in b.betas() {
            assert!((be - 1.0).abs() < 1e-12);
        }
        for &p in b.pulls() {
            assert_eq!(p, 0);
        }
    }

    #[test]
    fn arm_stats_after_updates() {
        let mut b = make_bandit();
        b.update(0, 1.0).unwrap();
        b.update(0, 0.0).unwrap();
        b.update(0, 1.0).unwrap();
        let stats = b.arm_stats(0).unwrap();
        assert_eq!(stats.pulls, 3);
        assert!((stats.total_reward - 2.0).abs() < 1e-12);
    }

    #[test]
    fn arm_stats_rejects_invalid_arm() {
        let b = make_bandit();
        assert!(b.arm_stats(5).is_err());
    }

    #[test]
    fn finds_best_arm_in_simulation() {
        let mut b = make_bandit();
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        // Simulate Bernoulli rewards:
        // arm 0: p=0.8, arm 1: p=0.2, arm 2: p=0.5
        for _ in 0..1000 {
            let arm = b.select(&mut rng).unwrap();
            let p = match arm {
                0 => 0.8,
                1 => 0.2,
                _ => 0.5,
            };
            let reward = if rng.gen_range(0.0..1.0) < p {
                1.0
            } else {
                0.0
            };
            b.update(arm, reward).unwrap();
        }

        // Arm 0 should be pulled most often.
        let stats0 = b.arm_stats(0).unwrap();
        let stats1 = b.arm_stats(1).unwrap();
        let stats2 = b.arm_stats(2).unwrap();
        assert!(stats0.pulls > stats1.pulls);
        assert!(stats0.pulls > stats2.pulls);
        // Arm 0's mean reward should be close to 0.8.
        assert!(stats0.mean_reward > 0.6);
    }

    #[test]
    fn sample_beta_returns_value_in_unit_interval() {
        let mut rng = ChaCha8Rng::seed_from_u64(99);
        for _ in 0..1000 {
            let v = ThompsonSampling::sample_beta(&mut rng, 2.0, 5.0);
            assert!((0.0..=1.0).contains(&v), "Beta sample {v} out of [0, 1]");
        }
    }

    #[test]
    fn sample_gamma_returns_positive_value() {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        for shape in &[0.5, 1.0, 2.0, 5.0, 10.0] {
            for _ in 0..100 {
                let v = ThompsonSampling::sample_gamma(&mut rng, *shape);
                assert!(v > 0.0, "Gamma sample {v} not positive for shape {shape}");
            }
        }
    }

    #[test]
    fn sample_gamma_mean_converges() {
        // Gamma(shape, 1) has mean = shape.
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let shape = 5.0;
        let n = 10000;
        let mut sum = 0.0;
        for _ in 0..n {
            sum += ThompsonSampling::sample_gamma(&mut rng, shape);
        }
        let mean = sum / n as f64;
        // Allow 10% tolerance.
        assert!(
            (mean - shape).abs() / shape < 0.1,
            "Gamma mean {mean} too far from {shape}"
        );
    }

    #[test]
    fn sample_beta_mean_converges() {
        // Beta(2, 5) has mean = 2 / (2 + 5) ≈ 0.2857.
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let alpha = 2.0;
        let beta = 5.0;
        let n = 10000;
        let mut sum = 0.0;
        for _ in 0..n {
            sum += ThompsonSampling::sample_beta(&mut rng, alpha, beta);
        }
        let mean = sum / n as f64;
        let expected = alpha / (alpha + beta);
        // Allow 10% tolerance.
        assert!(
            (mean - expected).abs() / expected < 0.1,
            "Beta mean {mean} too far from {expected}"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut b = ThompsonSampling::new(
            3,
            ThompsonConfig {
                alpha_prior: 2.0,
                beta_prior: 3.0,
            },
        )
        .unwrap();
        b.update(0, 1.0).unwrap();
        b.update(1, 0.0).unwrap();

        let json = serde_json::to_string(&b).unwrap();
        let restored: ThompsonSampling = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.arm_count(), b.arm_count());
        assert_eq!(restored.samples_seen(), b.samples_seen());
        assert_eq!(restored.alphas(), b.alphas());
        assert_eq!(restored.betas(), b.betas());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_malformed_state() {
        let json = r#"{
            "arm_count": 2,
            "config": {"alpha_prior": 1.0, "beta_prior": 1.0},
            "alphas": [2.0],
            "betas": [1.0],
            "pulls": [1],
            "total_rewards": [1.0],
            "samples_seen": 1
        }"#;
        assert!(serde_json::from_str::<ThompsonSampling>(json).is_err());
    }
}

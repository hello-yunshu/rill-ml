//! Integration tests: bandit learning behavior.
//!
//! These tests verify that each bandit algorithm can learn to identify the
//! best arm in a controlled setting, and that error handling and reset
//! behavior work correctly.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use rill_ml::bandit::{
    Bandit, ContextualBandit, EpsilonGreedy, EpsilonGreedyConfig, LinUcb, LinUcbConfig,
    ThompsonConfig, ThompsonSampling, Ucb1, Ucb1Config,
};
use rill_ml::error::RillError;

/// Simulate a reward for a given arm. Arm 0 has the highest mean reward.
fn reward_for_arm(arm: usize, rng: &mut ChaCha8Rng) -> f64 {
    match arm {
        0 => 0.8 + rand::Rng::gen_range(rng, -0.05..0.05),
        1 => 0.3 + rand::Rng::gen_range(rng, -0.05..0.05),
        _ => 0.5 + rand::Rng::gen_range(rng, -0.05..0.05),
    }
}

/// Clamp a value into [0, 1] for ThompsonSampling (which requires [0, 1] rewards).
fn clamp_01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

#[test]
fn epsilon_greedy_finds_best_arm() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut bandit = EpsilonGreedy::new(
        3,
        EpsilonGreedyConfig {
            epsilon: 0.1,
            decay: 1.0,
            min_epsilon: 0.1,
        },
    )
    .unwrap();

    for _ in 0..1000 {
        let arm = bandit.select(&mut rng).unwrap();
        let reward = reward_for_arm(arm, &mut rng);
        bandit.update(arm, reward).unwrap();
    }

    let s0 = bandit.arm_stats(0).unwrap();
    let s1 = bandit.arm_stats(1).unwrap();
    let s2 = bandit.arm_stats(2).unwrap();
    assert!(
        s0.pulls > s1.pulls,
        "arm 0 should be pulled more than arm 1"
    );
    assert!(
        s0.pulls > s2.pulls,
        "arm 0 should be pulled more than arm 2"
    );
    assert!(
        s0.mean_reward > s1.mean_reward,
        "arm 0 should have higher mean reward"
    );
}

#[test]
fn ucb1_finds_best_arm() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut bandit = Ucb1::new(3, Ucb1Config::default()).unwrap();

    for _ in 0..1000 {
        let arm = bandit.select(&mut rng).unwrap();
        let reward = reward_for_arm(arm, &mut rng);
        bandit.update(arm, reward).unwrap();
    }

    let s0 = bandit.arm_stats(0).unwrap();
    let s1 = bandit.arm_stats(1).unwrap();
    let s2 = bandit.arm_stats(2).unwrap();
    assert!(
        s0.pulls > s1.pulls,
        "arm 0 should be pulled more than arm 1"
    );
    assert!(
        s0.pulls > s2.pulls,
        "arm 0 should be pulled more than arm 2"
    );
}

#[test]
fn thompson_sampling_finds_best_arm() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut bandit = ThompsonSampling::new(3, ThompsonConfig::default()).unwrap();

    for _ in 0..1000 {
        let arm = bandit.select(&mut rng).unwrap();
        let reward = clamp_01(reward_for_arm(arm, &mut rng));
        bandit.update(arm, reward).unwrap();
    }

    let s0 = bandit.arm_stats(0).unwrap();
    let s1 = bandit.arm_stats(1).unwrap();
    let s2 = bandit.arm_stats(2).unwrap();
    assert!(
        s0.pulls > s1.pulls,
        "arm 0 should be pulled more than arm 1"
    );
    assert!(
        s0.pulls > s2.pulls,
        "arm 0 should be pulled more than arm 2"
    );
}

#[test]
fn linucb_learns_contextual_policy() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut bandit = LinUcb::new(LinUcbConfig {
        alpha: 1.0,
        arm_count: 2,
        feature_count: 2,
    })
    .unwrap();

    // Train: arm 0 is optimal when context[0] > context[1].
    for _ in 0..500 {
        let c0: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let c1: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let context = [c0, c1];
        let optimal = if c0 > c1 { 0 } else { 1 };

        let arm = bandit.select(&context, &mut rng).unwrap();
        let reward = if arm == optimal { 1.0 } else { 0.1 };
        bandit.update(arm, &context, reward).unwrap();
    }

    // Evaluate: LinUCB should select the context-optimal arm most of the time.
    let mut correct = 0usize;
    let eval_steps = 200;
    for _ in 0..eval_steps {
        let c0: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let c1: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let context = [c0, c1];
        let optimal = if c0 > c1 { 0 } else { 1 };
        let arm = bandit.select(&context, &mut rng).unwrap();
        if arm == optimal {
            correct += 1;
        }
    }
    let accuracy = correct as f64 / eval_steps as f64;
    assert!(
        accuracy > 0.85,
        "LinUCB should achieve >85% accuracy, got {:.2}%",
        accuracy * 100.0
    );
}

#[test]
fn epsilon_greedy_reset_clears_state() {
    let mut bandit = EpsilonGreedy::new(3, EpsilonGreedyConfig::default()).unwrap();
    bandit.update(0, 1.0).unwrap();
    bandit.update(1, 0.5).unwrap();
    assert_eq!(bandit.samples_seen(), 2);

    bandit.reset();
    assert_eq!(bandit.samples_seen(), 0);
    let stats = bandit.arm_stats(0).unwrap();
    assert_eq!(stats.pulls, 0);
}

#[test]
fn ucb1_reset_clears_state() {
    let mut bandit = Ucb1::new(3, Ucb1Config::default()).unwrap();
    bandit.update(0, 1.0).unwrap();
    bandit.update(1, 0.5).unwrap();
    assert_eq!(bandit.samples_seen(), 2);

    bandit.reset();
    assert_eq!(bandit.samples_seen(), 0);
    let stats = bandit.arm_stats(0).unwrap();
    assert_eq!(stats.pulls, 0);
}

#[test]
fn thompson_reset_clears_state() {
    let mut bandit = ThompsonSampling::new(3, ThompsonConfig::default()).unwrap();
    bandit.update(0, 1.0).unwrap();
    bandit.update(1, 0.5).unwrap();
    assert_eq!(bandit.samples_seen(), 2);

    bandit.reset();
    assert_eq!(bandit.samples_seen(), 0);
    let stats = bandit.arm_stats(0).unwrap();
    assert_eq!(stats.pulls, 0);
}

#[test]
fn linucb_reset_clears_state() {
    let mut bandit = LinUcb::new(LinUcbConfig {
        alpha: 1.0,
        arm_count: 2,
        feature_count: 2,
    })
    .unwrap();
    bandit.update(0, &[1.0, 0.5], 1.0).unwrap();
    bandit.update(1, &[0.3, 0.7], 0.5).unwrap();
    assert_eq!(bandit.samples_seen(), 2);

    bandit.reset();
    assert_eq!(bandit.samples_seen(), 0);
    // A should be back to identity.
    let a = bandit.a_matrix(0).unwrap();
    assert!((a[0][0] - 1.0).abs() < 1e-12);
    assert!((a[0][1] - 0.0).abs() < 1e-12);
}

#[test]
fn epsilon_greedy_rejects_invalid_inputs() {
    let mut bandit = EpsilonGreedy::new(3, EpsilonGreedyConfig::default()).unwrap();
    // Invalid arm.
    assert!(matches!(
        bandit.update(5, 1.0),
        Err(RillError::InvalidArm { .. })
    ));
    // Non-finite reward.
    assert!(matches!(
        bandit.update(0, f64::NAN),
        Err(RillError::InvalidReward(_))
    ));
    // Invalid arm_stats.
    assert!(bandit.arm_stats(10).is_err());
}

#[test]
fn ucb1_rejects_invalid_inputs() {
    let mut bandit = Ucb1::new(3, Ucb1Config::default()).unwrap();
    assert!(matches!(
        bandit.update(5, 1.0),
        Err(RillError::InvalidArm { .. })
    ));
    assert!(matches!(
        bandit.update(0, f64::INFINITY),
        Err(RillError::InvalidReward(_))
    ));
}

#[test]
fn thompson_rejects_invalid_inputs() {
    let mut bandit = ThompsonSampling::new(3, ThompsonConfig::default()).unwrap();
    assert!(matches!(
        bandit.update(5, 0.5),
        Err(RillError::InvalidArm { .. })
    ));
    // ThompsonSampling requires reward in [0, 1].
    assert!(matches!(
        bandit.update(0, 2.0),
        Err(RillError::InvalidReward(_))
    ));
    assert!(matches!(
        bandit.update(0, -0.5),
        Err(RillError::InvalidReward(_))
    ));
}

#[test]
fn linucb_rejects_invalid_inputs() {
    let mut bandit = LinUcb::new(LinUcbConfig {
        alpha: 1.0,
        arm_count: 2,
        feature_count: 2,
    })
    .unwrap();
    // Invalid arm.
    assert!(matches!(
        bandit.update(5, &[1.0, 0.5], 1.0),
        Err(RillError::InvalidArm { .. })
    ));
    // Wrong context length.
    assert!(matches!(
        bandit.update(0, &[1.0], 1.0),
        Err(RillError::DimensionMismatch { .. })
    ));
    // Non-finite reward.
    assert!(matches!(
        bandit.update(0, &[1.0, 0.5], f64::NAN),
        Err(RillError::InvalidReward(_))
    ));
    // select with wrong context length.
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    assert!(bandit.select(&[1.0], &mut rng).is_err());
}

#[test]
fn bandit_select_returns_valid_arm() {
    let mut rng = ChaCha8Rng::seed_from_u64(0);

    let eg = EpsilonGreedy::new(3, EpsilonGreedyConfig::default()).unwrap();
    assert!(eg.select(&mut rng).unwrap() < 3);

    let ucb = Ucb1::new(3, Ucb1Config::default()).unwrap();
    assert!(ucb.select(&mut rng).unwrap() < 3);

    let ts = ThompsonSampling::new(3, ThompsonConfig::default()).unwrap();
    assert!(ts.select(&mut rng).unwrap() < 3);
}

#[test]
fn linucb_select_returns_valid_arm() {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let bandit = LinUcb::new(LinUcbConfig {
        alpha: 1.0,
        arm_count: 3,
        feature_count: 2,
    })
    .unwrap();
    let context = [0.5, 0.8];
    let arm = bandit.select(&context, &mut rng).unwrap();
    assert!(arm < 3);
}

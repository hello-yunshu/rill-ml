//! Bandit demo: multi-armed and contextual bandit algorithms.
//!
//! This example demonstrates the v0.5 online decision-making module. It shows:
//!
//! - Scenario A: Comparing EpsilonGreedy, UCB1, and ThompsonSampling on a
//!   fixed-reward multi-armed bandit problem.
//! - Scenario B: LinUCB contextual bandit selecting arms based on context
//!   features.
//! - Scenario C: Safe fallback strategy — using a default arm when the bandit
//!   has insufficient data.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example bandit_demo
//! ```

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use rill_ml::bandit::{
    Bandit, ContextualBandit, EpsilonGreedy, EpsilonGreedyConfig, LinUcb, LinUcbConfig,
    ThompsonConfig, ThompsonSampling, Ucb1, Ucb1Config,
};

/// Simulate a reward for a given arm from a fixed (deterministic + noise) distribution.
fn reward_for_arm(arm: usize, rng: &mut ChaCha8Rng) -> f64 {
    match arm {
        // Arm 0: high mean (0.8), low noise.
        0 => 0.8 + (rand::Rng::gen_range(rng, -0.1..0.1)),
        // Arm 1: low mean (0.3), low noise.
        1 => 0.3 + (rand::Rng::gen_range(rng, -0.1..0.1)),
        // Arm 2: medium mean (0.5), low noise.
        _ => 0.5 + (rand::Rng::gen_range(rng, -0.1..0.1)),
    }
}

/// Run a non-contextual bandit for `steps` steps and return (total_reward, arm_pull_counts).
fn run_non_contextual<B: Bandit>(
    bandit: &mut B,
    steps: usize,
    rng: &mut ChaCha8Rng,
) -> (f64, Vec<u64>) {
    let mut total_reward = 0.0;
    let mut pulls = vec![0u64; bandit.arm_count()];
    for _ in 0..steps {
        let arm = bandit.select(rng).unwrap();
        let reward = reward_for_arm(arm, rng);
        bandit.update(arm, reward).unwrap();
        total_reward += reward;
        pulls[arm] += 1;
    }
    (total_reward, pulls)
}

fn scenario_a_non_contextual() {
    println!("=== Scenario A: Non-contextual bandits (3 arms) ===\n");
    println!("True mean rewards: arm0=0.8, arm1=0.3, arm2=0.5\n");

    let steps = 500;

    // EpsilonGreedy with decay.
    let mut eg_rng = ChaCha8Rng::seed_from_u64(42);
    let mut eg = EpsilonGreedy::new(
        3,
        EpsilonGreedyConfig {
            epsilon: 0.2,
            decay: 0.995,
            min_epsilon: 0.01,
        },
    )
    .unwrap();
    let (eg_total, eg_pulls) = run_non_contextual(&mut eg, steps, &mut eg_rng);

    // UCB1.
    let mut ucb_rng = ChaCha8Rng::seed_from_u64(42);
    let mut ucb = Ucb1::new(3, Ucb1Config::default()).unwrap();
    let (ucb_total, ucb_pulls) = run_non_contextual(&mut ucb, steps, &mut ucb_rng);

    // ThompsonSampling.
    let mut ts_rng = ChaCha8Rng::seed_from_u64(42);
    let mut ts = ThompsonSampling::new(3, ThompsonConfig::default()).unwrap();
    let (ts_total, ts_pulls) = run_non_contextual(&mut ts, steps, &mut ts_rng);

    println!("After {steps} steps:");
    println!(
        "  EpsilonGreedy:    total={:.2}, avg={:.4}, pulls={:?}",
        eg_total,
        eg_total / steps as f64,
        eg_pulls
    );
    println!(
        "  UCB1:             total={:.2}, avg={:.4}, pulls={:?}",
        ucb_total,
        ucb_total / steps as f64,
        ucb_pulls
    );
    println!(
        "  ThompsonSampling: total={:.2}, avg={:.4}, pulls={:?}",
        ts_total,
        ts_total / steps as f64,
        ts_pulls
    );

    // All bandits should prefer arm 0 (the best arm).
    assert!(
        eg_pulls[0] > eg_pulls[1] && eg_pulls[0] > eg_pulls[2],
        "EpsilonGreedy should prefer arm 0"
    );
    assert!(
        ucb_pulls[0] > ucb_pulls[1] && ucb_pulls[0] > ucb_pulls[2],
        "UCB1 should prefer arm 0"
    );
    assert!(
        ts_pulls[0] > ts_pulls[1] && ts_pulls[0] > ts_pulls[2],
        "ThompsonSampling should prefer arm 0"
    );
    println!("\n  All bandits correctly identified arm 0 as the best.\n");
}

fn scenario_b_contextual() {
    println!("=== Scenario B: LinUCB contextual bandit ===\n");

    // 2 arms, 2-d context. Arm 0 is optimal when context[0] > context[1];
    // arm 1 is optimal when context[1] > context[0].
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut bandit = LinUcb::new(LinUcbConfig {
        alpha: 1.0,
        arm_count: 2,
        feature_count: 2,
    })
    .unwrap();

    let steps = 300;
    let mut correct = 0usize;
    let mut total_reward = 0.0f64;

    for _ in 0..steps {
        // Generate a random context.
        let c0: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let c1: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let context = [c0, c1];

        // The optimal arm depends on the context.
        let optimal = if c0 > c1 { 0 } else { 1 };

        let arm = bandit.select(&context, &mut rng).unwrap();
        if arm == optimal {
            correct += 1;
        }

        // Reward: 1.0 for the optimal arm, 0.1 for the suboptimal arm.
        let reward = if arm == optimal { 1.0 } else { 0.1 };
        bandit.update(arm, &context, reward).unwrap();
        total_reward += reward;
    }

    let accuracy = correct as f64 / steps as f64;
    println!("After {steps} steps:");
    println!("  Total reward: {:.2}", total_reward);
    println!("  Average reward: {:.4}", total_reward / steps as f64);
    println!(
        "  Context-optimal selection accuracy: {:.2}%",
        accuracy * 100.0
    );
    println!("  Samples seen: {}", bandit.samples_seen());

    // LinUCB should learn to select the context-optimal arm most of the time.
    assert!(
        accuracy > 0.8,
        "LinUCB should achieve >80% accuracy, got {:.2}%",
        accuracy * 100.0
    );
    println!("\n  LinUCB successfully learned the context-dependent optimal policy.\n");
}

fn scenario_c_safe_fallback() {
    println!("=== Scenario C: Safe fallback strategy ===\n");

    // When a bandit has seen very few samples, its decisions may be unreliable.
    // A safe fallback is to use a default arm until the bandit has collected
    // enough data.
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut bandit = Ucb1::new(3, Ucb1Config::default()).unwrap();
    let warmup_threshold = 10u64;
    let default_arm = 0usize;

    let steps = 50;
    let mut bandit_decisions = 0usize;
    let mut fallback_decisions = 0usize;

    for _ in 0..steps {
        let arm = if bandit.samples_seen() < warmup_threshold {
            // Not enough data: use the safe default arm.
            fallback_decisions += 1;
            default_arm
        } else {
            // Enough data: trust the bandit.
            bandit_decisions += 1;
            bandit.select(&mut rng).unwrap()
        };

        let reward = reward_for_arm(arm, &mut rng);
        bandit.update(arm, reward).unwrap();
    }

    println!("After {steps} steps (warmup threshold = {warmup_threshold}):");
    println!("  Fallback (default arm) decisions: {fallback_decisions}");
    println!("  Bandit decisions: {bandit_decisions}");
    println!("  Total samples seen: {}", bandit.samples_seen());
    println!("\n  The safe fallback ensures reliable behavior during cold-start,");
    println!("  then delegates to the bandit once it has learned enough.\n");
}

fn main() {
    println!("RillML v0.5 Bandit Demo\n");
    scenario_a_non_contextual();
    scenario_b_contextual();
    scenario_c_safe_fallback();
    println!("=== Demo complete ===");
}

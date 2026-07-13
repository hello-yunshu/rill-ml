//! Drift detection demo: Page-Hinkley, ADWIN, KSWIN, and DriftAwareModel.
//!
//! This example demonstrates the v0.4 drift detection module on synthetic
//! streams with concept drift. It shows how to:
//!
//! - Detect sudden mean shifts with Page-Hinkley, ADWIN, and KSWIN.
//! - Detect variance changes with KSWIN.
//! - Use DriftAwareModel to automatically reset a LinearRegression when
//!   drift is detected, and compare its rolling MAE against a plain model.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example drift_demo
//! ```

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use rill_ml::Metric;
use rill_ml::OnlineRegressor;
use rill_ml::drift::{
    Adwin, AdwinConfig, DriftAction, DriftAwareModel, DriftDetector, DriftLevel, Kswin,
    KswinConfig, PageHinkley, StaticStrategy,
};
use rill_ml::metrics::Mae;
use rill_ml::models::{LinearRegression, LinearRegressionConfig};
use rill_ml::optim::{Optimizer, SgdConfig};

/// Generate a normal-distributed sample using Box-Muller transform.
fn normal_sample(rng: &mut ChaCha8Rng, mean: f64, std: f64) -> f64 {
    let u1: f64 = rand::Rng::gen_range(rng, 1e-10..1.0);
    let u2: f64 = rand::Rng::gen_range(rng, 0.0..1.0);
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + std * z
}

fn main() {
    println!("=== RillML v0.4 Drift Detection Demo ===\n");

    scenario_a_mean_shift();
    scenario_b_variance_change();
    scenario_c_drift_aware_model();
}

/// Scenario A: sudden mean shift — compare detector trigger points.
fn scenario_a_mean_shift() {
    println!("--- Scenario A: Sudden Mean Shift ---");
    println!("Stream: 200 steps at mean 0, then 200 steps at mean 5.\n");

    let mut ph = PageHinkley::default();
    let mut adwin = Adwin::new(AdwinConfig {
        delta: 0.05,
        warning_delta: 0.1,
        max_window: 500,
        min_samples: 10,
    })
    .unwrap();
    let mut kswin = Kswin::new(KswinConfig {
        alpha: 0.01,
        window_size: 50,
        check_interval: 50,
    })
    .unwrap();

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut ph_trigger: Option<u64> = None;
    let mut adwin_trigger: Option<u64> = None;
    let mut kswin_trigger: Option<u64> = None;

    for step in 0..400u64 {
        let mean = if step < 200 { 0.0 } else { 5.0 };
        let v = normal_sample(&mut rng, mean, 0.3);

        let ph_level = ph.update(v).unwrap();
        let adwin_level = adwin.update(v).unwrap();
        let kswin_level = kswin.update(v).unwrap();

        if ph_trigger.is_none() && ph_level == DriftLevel::Drift {
            ph_trigger = Some(step);
        }
        if adwin_trigger.is_none() && adwin_level == DriftLevel::Drift {
            adwin_trigger = Some(step);
        }
        if kswin_trigger.is_none() && kswin_level == DriftLevel::Drift {
            kswin_trigger = Some(step);
        }
    }

    println!(
        "  Page-Hinkley detected drift at step: {}",
        ph_trigger.map_or("never".to_string(), |s| s.to_string())
    );
    println!(
        "  ADWIN detected drift at step:        {}",
        adwin_trigger.map_or("never".to_string(), |s| s.to_string())
    );
    println!(
        "  KSWIN detected drift at step:        {}\n",
        kswin_trigger.map_or("never".to_string(), |s| s.to_string())
    );
}

/// Scenario B: variance change — KSWIN's strength.
fn scenario_b_variance_change() {
    println!("--- Scenario B: Variance Change ---");
    println!("Stream: 200 steps at std=0.1, then 200 steps at std=3.0 (same mean).\n");

    let mut kswin = Kswin::new(KswinConfig {
        alpha: 0.01,
        window_size: 50,
        check_interval: 50,
    })
    .unwrap();
    let mut ph = PageHinkley::default();
    let mut adwin = Adwin::default();

    let mut rng = ChaCha8Rng::seed_from_u64(99);
    let mut kswin_trigger: Option<u64> = None;
    let mut ph_trigger: Option<u64> = None;
    let mut adwin_trigger: Option<u64> = None;

    for step in 0..400u64 {
        let std = if step < 200 { 0.1 } else { 3.0 };
        let v = normal_sample(&mut rng, 0.0, std);

        let k_level = kswin.update(v).unwrap();
        let ph_level = ph.update(v).unwrap();
        let ad_level = adwin.update(v).unwrap();

        if kswin_trigger.is_none() && k_level == DriftLevel::Drift {
            kswin_trigger = Some(step);
        }
        if ph_trigger.is_none() && ph_level == DriftLevel::Drift {
            ph_trigger = Some(step);
        }
        if adwin_trigger.is_none() && ad_level == DriftLevel::Drift {
            adwin_trigger = Some(step);
        }
    }

    println!(
        "  KSWIN detected variance drift at step:        {}",
        kswin_trigger.map_or("never".to_string(), |s| s.to_string())
    );
    println!(
        "  Page-Hinkley detected variance drift at step: {}",
        ph_trigger.map_or("never".to_string(), |s| s.to_string())
    );
    println!(
        "  ADWIN detected variance drift at step:        {}\n",
        adwin_trigger.map_or("never".to_string(), |s| s.to_string())
    );
}

/// Scenario C: DriftAwareModel vs plain model.
fn scenario_c_drift_aware_model() {
    println!("--- Scenario C: DriftAwareModel vs Plain Model ---");
    println!("Stream: y = 2x (200 steps), then y = 5x (200 steps).\n");

    let feature_count = 1;

    // Drift-aware model: LinearRegression + PageHinkley + ResetModel strategy.
    let aware_optimizer = Optimizer::sgd(
        feature_count,
        SgdConfig {
            learning_rate: 0.1,
            l2: 0.0,
        },
    )
    .unwrap();
    let aware_model = LinearRegression::new(
        feature_count,
        LinearRegressionConfig {
            optimizer: aware_optimizer,
            ..Default::default()
        },
    )
    .unwrap();
    let aware_detector = PageHinkley::default();
    let aware_strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
    let mut aware = DriftAwareModel::new(aware_model, aware_detector, aware_strategy);

    // Plain model: same LinearRegression, no drift awareness.
    let plain_optimizer = Optimizer::sgd(
        feature_count,
        SgdConfig {
            learning_rate: 0.1,
            l2: 0.0,
        },
    )
    .unwrap();
    let mut plain = LinearRegression::new(
        feature_count,
        LinearRegressionConfig {
            optimizer: plain_optimizer,
            ..Default::default()
        },
    )
    .unwrap();

    let mut aware_mae = Mae::default();
    let mut plain_mae = Mae::default();
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut drift_step: Option<u64> = None;

    for step in 0..400u64 {
        let slope = if step < 200 { 2.0 } else { 5.0 };
        let x: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let y = slope * x + normal_sample(&mut rng, 0.0, 0.05);

        // Drift-aware: predict → metric → learn.
        let aware_pred = aware.predict(&[x]).unwrap();
        aware_mae.update(y, aware_pred).unwrap();
        aware.learn(&[x], y).unwrap();

        // Plain: predict → metric → learn.
        let plain_pred = plain.predict(&[x]).unwrap();
        plain_mae.update(y, plain_pred).unwrap();
        plain.learn(&[x], y).unwrap();

        if drift_step.is_none() && !aware.events().is_empty() {
            drift_step = Some(step);
        }
    }

    println!(
        "  Drift detected at step: {}",
        drift_step.map_or("never".to_string(), |s| s.to_string())
    );
    println!(
        "  Drift-aware model final MAE: {:.4}",
        aware_mae.value().unwrap()
    );
    println!(
        "  Plain model final MAE:       {:.4}",
        plain_mae.value().unwrap()
    );
    println!("  Drift events recorded:       {}\n", aware.events().len());
}

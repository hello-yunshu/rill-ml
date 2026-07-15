//! Integration tests for the drift detection module.
//!
//! These tests exercise cross-module behavior: detector accuracy on
//! synthetic drift scenarios, DriftAwareModel event logging and action
//! execution, and decay-aware learning utilities. All random streams use
//! a fixed seed (`ChaCha8Rng::seed_from_u64`) for reproducibility.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use rill_ml::OnlineRegressor;
use rill_ml::drift::{
    Adwin, AdwinConfig, DriftAction, DriftAwareModel, DriftDetector, DriftLevel, DriftStrategy,
    FixedWindowBuffer, Kswin, KswinConfig, LearningRateScheduler, PageHinkley, PageHinkleyConfig,
    StaticStrategy, TimeDecayedMean,
};
use rill_ml::models::{BaselineConfig, LinearRegression, LinearRegressionConfig, MeanRegressor};
use rill_ml::optim::{Optimizer, SgdConfig};

/// Generate a normal-distributed sample using Box-Muller transform.
fn normal_sample(rng: &mut ChaCha8Rng, mean: f64, std: f64) -> f64 {
    let u1: f64 = rand::Rng::gen_range(rng, 1e-10..1.0);
    let u2: f64 = rand::Rng::gen_range(rng, 0.0..1.0);
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + std * z
}

#[test]
fn page_hinkley_detects_sudden_mean_shift() {
    let mut ph = PageHinkley::default();
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Stable stream around 0.
    for _ in 0..100 {
        let v = normal_sample(&mut rng, 0.0, 0.1);
        ph.update(v).unwrap();
    }
    assert_eq!(ph.level(), DriftLevel::None);

    // Sudden shift to mean 5.
    let mut detected = false;
    for _ in 0..100 {
        let v = normal_sample(&mut rng, 5.0, 0.1);
        let level = ph.update(v).unwrap();
        if level == DriftLevel::Drift {
            detected = true;
            break;
        }
    }
    assert!(detected, "Page-Hinkley should detect the sudden mean shift");
}

#[test]
fn adwin_detects_gradual_drift() {
    let mut adwin = Adwin::new(AdwinConfig {
        delta: 0.05,
        warning_delta: 0.1,
        max_window: 300,
        min_samples: 5,
    })
    .unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(99);

    // Gradual drift from mean 0 to mean 5.
    let mut detected = false;
    for i in 0..500 {
        let mean = (i as f64 / 100.0).min(5.0);
        let v = normal_sample(&mut rng, mean, 0.1);
        let level = adwin.update(v).unwrap();
        if level == DriftLevel::Drift {
            detected = true;
            break;
        }
    }
    assert!(detected, "ADWIN should detect gradual drift");
}

#[test]
fn kswin_detects_variance_change() {
    let mut kswin = Kswin::new(KswinConfig {
        alpha: 0.01,
        window_size: 50,
        check_interval: 50,
    })
    .unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(7);

    // Low-variance stream.
    for _ in 0..100 {
        let v = normal_sample(&mut rng, 0.0, 0.1);
        kswin.update(v).unwrap();
    }
    assert_eq!(kswin.level(), DriftLevel::None);

    // High-variance stream (same mean, different variance).
    let mut detected = false;
    for _ in 0..200 {
        let v = normal_sample(&mut rng, 0.0, 3.0);
        let level = kswin.update(v).unwrap();
        if level == DriftLevel::Drift {
            detected = true;
            break;
        }
    }
    assert!(detected, "KSWIN should detect the variance change");
}

#[test]
fn kswin_detects_distribution_shape_change() {
    let mut kswin = Kswin::new(KswinConfig {
        alpha: 0.01,
        window_size: 50,
        check_interval: 50,
    })
    .unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(123);

    // Uniform distribution in [0, 1).
    for _ in 0..100 {
        let v: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        kswin.update(v).unwrap();
    }

    // Normal distribution with mean 0.5, std 2.0 (same mean, very different
    // shape — much wider spread). The KS statistic between uniform[0,1) and
    // normal(0.5, 2.0) is ~0.40, which is large enough for detection with
    // 50 samples at alpha=0.01.
    let mut detected = false;
    for _ in 0..200 {
        let v = normal_sample(&mut rng, 0.5, 2.0);
        let level = kswin.update(v).unwrap();
        if level == DriftLevel::Drift {
            detected = true;
            break;
        }
    }
    assert!(
        detected,
        "KSWIN should detect the distribution shape change"
    );
}

#[test]
fn drift_aware_model_logs_events_on_drift() {
    let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let detector = PageHinkley::default();
    let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::NotifyOnly);
    let mut aware = DriftAwareModel::new(model, detector, strategy);

    // Stable stream: no events.
    for _ in 0..50 {
        aware.learn(&[], 1.0).unwrap();
    }
    assert!(aware.events().is_empty());

    // Sudden shift: keep feeding until a confirmed Drift (not just Warning)
    // is recorded. The first event may be a Warning because the PH statistic
    // exceeds the warning threshold before the drift threshold.
    let mut drift_recorded = false;
    for _ in 0..150 {
        aware.learn(&[], 50.0).unwrap();
        if let Some(last) = aware.events().last()
            && last.level == DriftLevel::Drift
        {
            drift_recorded = true;
            break;
        }
    }
    assert!(drift_recorded, "DriftAwareModel should log a drift event");
    let last = aware.events().last().unwrap();
    assert_eq!(last.level, DriftLevel::Drift);
}

#[test]
fn drift_aware_model_resets_on_reset_model_action() {
    let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let detector = PageHinkley::default();
    let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
    let mut aware = DriftAwareModel::new(model, detector, strategy);

    // Stable stream to build up model state.
    for _ in 0..50 {
        aware.learn(&[], 1.0).unwrap();
    }
    assert!(aware.model().samples_seen() > 0);

    // Trigger drift with a large shift.
    let mut reset_happened = false;
    for _ in 0..150 {
        aware.learn(&[], 100.0).unwrap();
        if aware.model().samples_seen() < aware.samples_seen() {
            reset_happened = true;
            break;
        }
    }
    assert!(
        reset_happened,
        "model should have been reset by ResetModel action"
    );
    assert!(!aware.events().is_empty());
}

#[test]
fn drift_aware_model_does_not_auto_reset_with_default_strategy() {
    let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let detector = PageHinkley::default();
    let strategy = StaticStrategy::default();
    let mut aware = DriftAwareModel::new(model, detector, strategy);

    for i in 0..100 {
        aware.learn(&[], i as f64).unwrap();
    }
    // With NotifyOnly, model samples_seen should equal total learn calls.
    assert_eq!(aware.model().samples_seen(), aware.samples_seen());
}

#[test]
fn drift_aware_model_replace_with_baseline_action_recorded() {
    let model = MeanRegressor::new(BaselineConfig::default()).unwrap();
    let detector = PageHinkley::default();
    let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ReplaceWithBaseline);
    let mut aware = DriftAwareModel::new(model, detector, strategy);

    // Stable stream first.
    for _ in 0..50 {
        aware.learn(&[], 1.0).unwrap();
    }

    // Trigger drift.
    let mut seen_action = false;
    for _ in 0..200 {
        aware.learn(&[], 100.0).unwrap();
        if let Some(action) = aware.last_action()
            && action == DriftAction::ReplaceWithBaseline
        {
            seen_action = true;
            break;
        }
    }
    assert!(
        seen_action,
        "ReplaceWithBaseline action should have been recorded"
    );
}

#[test]
fn time_decayed_mean_weights_recent_data() {
    let mut m = TimeDecayedMean::new(1.0).unwrap();
    // Old sample at t=0 with value 100.
    m.update(0.0, 100.0).unwrap();
    // Recent sample at t=10 with value 0.
    m.update(10.0, 0.0).unwrap();
    let v = m.value().unwrap();
    // The decay factor for the old sample is exp(-1.0 * 10) ≈ 4.5e-5,
    // so the mean should be very close to 0 (recent value).
    assert!(v < 1.0, "recent data should dominate: mean = {}", v);
}

#[test]
fn learning_rate_scheduler_increases_on_drift() {
    let mut sched = LearningRateScheduler::new(0.01, 2.0, 5.0).unwrap();
    let base = sched.current_lr();
    assert!((base - 0.01).abs() < 1e-12);

    sched.on_drift_level(DriftLevel::Warning);
    let warn_lr = sched.current_lr();
    assert!(warn_lr > base, "warning lr should be higher than base");

    sched.on_drift_level(DriftLevel::Drift);
    let drift_lr = sched.current_lr();
    assert!(
        drift_lr > warn_lr,
        "drift lr should be higher than warning lr"
    );
    assert!((drift_lr - 0.05).abs() < 1e-12);
}

#[test]
fn fixed_window_buffer_overwrites_oldest() {
    let mut buf = FixedWindowBuffer::new(3).unwrap();
    buf.push(1.0).unwrap();
    buf.push(2.0).unwrap();
    buf.push(3.0).unwrap();
    assert_eq!(buf.len(), 3);
    assert!((buf.mean().unwrap() - 2.0).abs() < 1e-12);

    // Push a 4th value; the oldest (1.0) should be overwritten.
    buf.push(10.0).unwrap();
    assert_eq!(buf.len(), 3);
    // Window should now contain [2.0, 3.0, 10.0], mean = 5.0.
    assert!(
        (buf.mean().unwrap() - 5.0).abs() < 1e-12,
        "mean should be 5.0, got {}",
        buf.mean().unwrap()
    );
}

#[test]
fn static_strategy_decides_correctly_per_level() {
    let s = StaticStrategy::new(DriftAction::ReduceConfidence, DriftAction::ResetModel);
    assert_eq!(s.decide(DriftLevel::None, 0), DriftAction::NotifyOnly);
    assert_eq!(
        s.decide(DriftLevel::Warning, 10),
        DriftAction::ReduceConfidence
    );
    assert_eq!(s.decide(DriftLevel::Drift, 100), DriftAction::ResetModel);
}

#[test]
fn drift_aware_model_with_linear_regression_and_adwin() {
    // Cross-component: LinearRegression + Adwin + ResetModel strategy.
    let feature_count = 1;
    let optimizer = Optimizer::sgd(
        feature_count,
        SgdConfig {
            learning_rate: 0.1,
            l2: 0.0,
        },
    )
    .unwrap();
    let model = LinearRegression::new(
        feature_count,
        LinearRegressionConfig {
            optimizer,
            ..Default::default()
        },
    )
    .unwrap();
    let detector = Adwin::new(AdwinConfig {
        delta: 0.05,
        warning_delta: 0.1,
        max_window: 200,
        min_samples: 10,
    })
    .unwrap();
    let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
    let mut aware = DriftAwareModel::new(model, detector, strategy);

    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Phase 1: stable linear relationship y = 2x + noise.
    for _ in 0..100 {
        let x: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let y = 2.0 * x + normal_sample(&mut rng, 0.0, 0.05);
        aware.learn(&[x], y).unwrap();
    }

    // Phase 2: drift to y = 5x (different slope).
    let mut reset_happened = false;
    for _ in 0..200 {
        let x: f64 = rand::Rng::gen_range(&mut rng, 0.0..1.0);
        let y = 5.0 * x + normal_sample(&mut rng, 0.0, 0.05);
        aware.learn(&[x], y).unwrap();
        if aware.model().samples_seen() < aware.samples_seen() {
            reset_happened = true;
            break;
        }
    }
    assert!(
        reset_happened,
        "DriftAwareModel with Adwin should detect slope drift and reset"
    );
}

#[test]
fn page_hinkley_config_validation() {
    // Zero threshold is invalid.
    assert!(
        PageHinkley::new(PageHinkleyConfig {
            threshold: 0.0,
            ..Default::default()
        })
        .is_err()
    );

    // Valid config succeeds.
    assert!(PageHinkley::new(PageHinkleyConfig::default()).is_ok());
}

#[test]
fn kswin_no_false_positive_on_stable_stream() {
    let mut kswin = Kswin::new(KswinConfig {
        alpha: 0.005,
        window_size: 100,
        check_interval: 100,
    })
    .unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(7);

    for _ in 0..2000 {
        let v = normal_sample(&mut rng, 0.0, 0.5);
        kswin.update(v).unwrap();
    }
    assert!(
        !kswin.detected(),
        "KSWIN should not report drift on a stable stream"
    );
}

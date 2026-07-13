#![cfg(feature = "serde")]

//! Integration tests: serialization round-trips for all serializable types.
//!
//! These tests only run when the `serde` feature is enabled.

use rill_ml::OnlineBinaryClassifier;
use rill_ml::OnlineRegressor;
use rill_ml::OnlineStatistic;
use rill_ml::Transformer;
use rill_ml::bandit::{
    Bandit, ContextualBandit, EpsilonGreedy, EpsilonGreedyConfig, LinUcb, LinUcbConfig,
    ThompsonConfig, ThompsonSampling, Ucb1, Ucb1Config,
};
use rill_ml::drift::{
    Adwin, AdwinConfig, DriftAction, DriftAwareModel, DriftDetector, DriftEvent, DriftLevel,
    FixedWindowBuffer, Kswin, KswinConfig, LearningRateScheduler, PageHinkley, PageHinkleyConfig,
    StaticStrategy, TimeDecayedMean,
};
use rill_ml::feature_hasher::FeatureHasher;
use rill_ml::loss::RegressionLoss;
use rill_ml::models::{
    BernoulliNaiveBayes, FtrlClassifier, FtrlConfig, FtrlRegressor, GaussianNaiveBayes,
    LinearRegression, LinearRegressionConfig, MeanRegressor, MultinomialNaiveBayes,
};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml::persistence::Snapshot;
use rill_ml::preprocessing::{
    ConstantImputer, ConstantImputerConfig, ForwardFill, FrequencyEncoder, MeanImputer,
    MissingIndicator, OneHotEncoder, OrdinalEncoder, StandardScaler,
};
use rill_ml::sparse::SparseFeatures;
use rill_ml::stats::{ExponentiallyWeightedMean, Mean, Variance, VarianceKind};
use rill_ml::{SparseClassifier, SparseRegressor};

fn assert_roundtrip<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string(value).unwrap();
    let _restored: T = serde_json::from_str(&json).expect("deserialization must succeed");
}

#[test]
fn mean_serialization_roundtrip() {
    let mut m = Mean::new();
    m.update(1.0).unwrap();
    m.update(2.5).unwrap();
    m.update(3.7).unwrap();
    assert_roundtrip(&m);
}

#[test]
fn variance_serialization_roundtrip() {
    let mut v = Variance::new(VarianceKind::Sample);
    for i in 0..20 {
        v.update(i as f64).unwrap();
    }
    assert_roundtrip(&v);
}

#[test]
fn ew_mean_serialization_roundtrip() {
    let mut ew = ExponentiallyWeightedMean::new(0.3).unwrap();
    for i in 0..10 {
        ew.update(i as f64 * 0.5).unwrap();
    }
    assert_roundtrip(&ew);
}

#[test]
fn standard_scaler_serialization_roundtrip() {
    let mut scaler = StandardScaler::new(3).unwrap();
    scaler.update(&[1.0, 10.0, 100.0]).unwrap();
    scaler.update(&[2.0, 20.0, 200.0]).unwrap();
    scaler.update(&[3.0, 30.0, 300.0]).unwrap();
    assert_roundtrip(&scaler);
}

#[test]
fn linear_regression_serialization_roundtrip() {
    let d = 2;
    let mut model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.1,
                    l2: 0.01,
                },
            )
            .unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    model.learn(&[1.0, 2.0], 3.0).unwrap();
    model.learn(&[4.0, 5.0], 6.0).unwrap();
    assert_roundtrip(&model);
}

#[test]
fn mean_regressor_serialization_roundtrip() {
    let mut model = MeanRegressor::default();
    model.learn(&[], 42.0).unwrap();
    model.learn(&[], 10.0).unwrap();
    assert_roundtrip(&model);
}

#[test]
fn snapshot_envelope_roundtrip() {
    let mut mean = Mean::new();
    mean.update(5.0).unwrap();
    mean.update(10.0).unwrap();
    mean.update(15.0).unwrap();

    let snap = Snapshot::new(mean);
    let json = serde_json::to_string(&snap).unwrap();
    let restored: Snapshot<Mean> = serde_json::from_str(&json).unwrap();

    assert_eq!(
        restored.format_version,
        rill_ml::persistence::SNAPSHOT_FORMAT_VERSION
    );
    let m = restored.into_model().unwrap();
    assert!((m.value() - 10.0).abs() < 1e-12);
}

#[test]
fn snapshot_rejects_incompatible_version() {
    let snap = Snapshot {
        format_version: 999,
        model: Mean::new(),
    };
    assert!(snap.into_model().is_err());
}

#[test]
fn snapshot_preserves_trained_model_predictions() {
    let d = 2;
    let mut model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.1,
                    l2: 0.0,
                },
            )
            .unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();
    for _ in 0..100 {
        model.learn(&[1.0, 1.0], 2.0).unwrap();
    }

    let pred_before = model.predict(&[1.0, 1.0]).unwrap();
    let snap = Snapshot::new(model);
    let json = serde_json::to_string(&snap).unwrap();
    let restored: Snapshot<LinearRegression> = serde_json::from_str(&json).unwrap();
    let model = restored.into_model().unwrap();
    let pred_after = model.predict(&[1.0, 1.0]).unwrap();

    assert!(
        (pred_before - pred_after).abs() < 1e-12,
        "pred_before = {pred_before}, pred_after = {pred_after}"
    );
}

#[test]
fn optimizer_serialization_roundtrip() {
    let opt = Optimizer::sgd(
        3,
        SgdConfig {
            learning_rate: 0.05,
            l2: 0.001,
        },
    )
    .unwrap();
    let json1 = serde_json::to_string(&opt).unwrap();
    let restored: Optimizer = serde_json::from_str(&json1).unwrap();
    let json2 = serde_json::to_string(&restored).unwrap();
    assert_eq!(json1, json2);
}

#[test]
fn regression_loss_serialization_roundtrip() {
    let losses = vec![
        RegressionLoss::default(),
        RegressionLoss::Huber(rill_ml::loss::HuberLoss::new(1.5).unwrap()),
    ];
    for loss in &losses {
        let json1 = serde_json::to_string(loss).unwrap();
        let restored: RegressionLoss = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&restored).unwrap();
        assert_eq!(json1, json2);
    }
}

// ===========================================================================
// Additional serialization round-trip tests for sparse, FTRL, Naive Bayes,
// and preprocessing types. Each test creates an instance, trains it with a
// few samples, then asserts the assert_roundtrip invariant.
// ===========================================================================

#[test]
fn sparse_features_serialization_roundtrip() {
    let sf = SparseFeatures::from_sorted(vec![(0, 1.5), (3, -2.0), (7, 0.25)]).unwrap();
    assert_roundtrip(&sf);

    // Also exercise the from_unsorted path to cover the merged representation.
    let sf_unsorted = SparseFeatures::from_unsorted(vec![(5, 1.0), (1, 2.0), (5, 0.5)]).unwrap();
    assert_roundtrip(&sf_unsorted);
}

#[test]
fn feature_hasher_serialization_roundtrip() {
    let hasher = FeatureHasher::new(32, 42).unwrap();
    // The hasher is stateless, but we still verify the round-trip.
    assert_roundtrip(&hasher);

    // Also verify that a restored hasher produces the same output as the
    // original for a fixed input.
    let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0)]).unwrap();
    let original_output = hasher.transform(&sf).unwrap();

    let json = serde_json::to_string(&hasher).unwrap();
    let restored: FeatureHasher = serde_json::from_str(&json).unwrap();
    let restored_output = restored.transform(&sf).unwrap();
    assert_eq!(original_output, restored_output);
}

#[test]
fn ftrl_regressor_serialization_roundtrip() {
    let mut model = FtrlRegressor::new(FtrlConfig {
        alpha: 0.3,
        beta: 0.5,
        l1: 0.1,
        l2: 0.2,
    })
    .unwrap();
    let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (3, 2.0)]).unwrap();
    model.learn(&sf, 5.0).unwrap();
    model.learn(&sf, -1.0).unwrap();
    assert_roundtrip(&model);
}

#[test]
fn ftrl_classifier_serialization_roundtrip() {
    let mut model = FtrlClassifier::new(FtrlConfig {
        alpha: 0.3,
        beta: 0.5,
        l1: 0.1,
        l2: 0.2,
    })
    .unwrap();
    let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (2, -1.0)]).unwrap();
    model.learn(&sf, true).unwrap();
    model.learn(&sf, false).unwrap();
    assert_roundtrip(&model);
}

#[test]
fn gaussian_naive_bayes_serialization_roundtrip() {
    let mut model = GaussianNaiveBayes::new(2, Default::default()).unwrap();
    model.learn(&[1.0, 2.0], true).unwrap();
    model.learn(&[1.5, 2.5], true).unwrap();
    model.learn(&[-1.0, -2.0], false).unwrap();
    model.learn(&[-1.5, -2.5], false).unwrap();
    assert_roundtrip(&model);
}

#[test]
fn bernoulli_naive_bayes_serialization_roundtrip() {
    let mut model = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
    model.learn(&[1.0, 0.0, 1.0], true).unwrap();
    model.learn(&[0.0, 1.0, 0.0], false).unwrap();
    model.learn(&[1.0, 1.0, 0.0], true).unwrap();
    assert_roundtrip(&model);
}

#[test]
fn multinomial_naive_bayes_serialization_roundtrip() {
    let mut model = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
    model.learn(&[2.0, 1.0, 0.0], true).unwrap();
    model.learn(&[0.0, 1.0, 3.0], false).unwrap();
    model.learn(&[1.0, 2.0, 1.0], true).unwrap();
    assert_roundtrip(&model);
}

#[test]
fn one_hot_encoder_serialization_roundtrip() {
    let mut enc = OneHotEncoder::new();
    enc.update_strs(&["b", "a", "c"]).unwrap();
    enc.update_strs(&["d"]).unwrap();
    assert_roundtrip(&enc);
}

#[test]
fn ordinal_encoder_serialization_roundtrip() {
    let mut enc = OrdinalEncoder::new();
    enc.update_strs(&["b", "a", "c"]).unwrap();
    enc.update_strs(&["a", "d"]).unwrap();
    assert_roundtrip(&enc);
}

#[test]
fn frequency_encoder_serialization_roundtrip() {
    let mut enc = FrequencyEncoder::new();
    enc.update_strs(&["a", "b", "a"]).unwrap();
    enc.update_strs(&["c"]).unwrap();
    enc.update_strs(&["a", "b"]).unwrap();
    assert_roundtrip(&enc);
}

#[test]
fn constant_imputer_serialization_roundtrip() {
    let mut imp =
        ConstantImputer::with_config(3, ConstantImputerConfig { fill_value: 7.0 }).unwrap();
    imp.update(&[1.0, f64::NAN, 3.0]).unwrap();
    imp.update(&[2.0, 4.0, f64::NAN]).unwrap();
    assert_roundtrip(&imp);
}

#[test]
fn mean_imputer_serialization_roundtrip() {
    let mut imp = MeanImputer::new(2).unwrap();
    imp.update(&[1.0, f64::NAN]).unwrap();
    imp.update(&[3.0, 5.0]).unwrap();
    imp.update(&[f64::NAN, 7.0]).unwrap();
    assert_roundtrip(&imp);
}

#[test]
fn forward_fill_serialization_roundtrip() {
    let mut ff = ForwardFill::new(2).unwrap();
    ff.update(&[1.0, f64::NAN]).unwrap();
    ff.update(&[3.0, 5.0]).unwrap();
    ff.update(&[f64::NAN, 7.0]).unwrap();
    assert_roundtrip(&ff);
}

#[test]
fn missing_indicator_serialization_roundtrip() {
    let mut mi = MissingIndicator::new(3).unwrap();
    mi.update(&[1.0, 2.0, 3.0]).unwrap();
    mi.update(&[f64::NAN, 4.0, 5.0]).unwrap();
    mi.update(&[1.0, f64::NAN, 6.0]).unwrap();
    assert_roundtrip(&mi);
}

// ---------------------------------------------------------------------------
// v0.4 drift detection types
// ---------------------------------------------------------------------------

#[test]
fn drift_level_serde_roundtrip() {
    for level in [DriftLevel::None, DriftLevel::Warning, DriftLevel::Drift] {
        let json = serde_json::to_string(&level).unwrap();
        let restored: DriftLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, level);
    }
}

#[test]
fn drift_action_serde_roundtrip() {
    for action in [
        DriftAction::NotifyOnly,
        DriftAction::ReduceConfidence,
        DriftAction::ResetModel,
        DriftAction::ResetPreprocessor,
        DriftAction::ReplaceWithBaseline,
        DriftAction::IncreaseAdaptationRate,
    ] {
        let json = serde_json::to_string(&action).unwrap();
        let restored: DriftAction = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, action);
    }
}

#[test]
fn drift_event_serde_roundtrip() {
    let event = DriftEvent::new(42, DriftLevel::Drift, DriftAction::ResetModel, 1.23);
    assert_roundtrip(&event);
}

#[test]
fn static_strategy_serde_roundtrip() {
    let s = StaticStrategy::new(DriftAction::ReduceConfidence, DriftAction::ResetModel);
    assert_roundtrip(&s);
}

#[test]
fn page_hinkley_config_serde_roundtrip() {
    let config = PageHinkleyConfig::default();
    assert_roundtrip(&config);
}

#[test]
fn page_hinkley_serde_roundtrip() {
    let mut ph = PageHinkley::default();
    for i in 0..50 {
        ph.update(i as f64 * 0.1).unwrap();
    }
    assert_roundtrip(&ph);
}

#[test]
fn adwin_config_serde_roundtrip() {
    let config = AdwinConfig::default();
    assert_roundtrip(&config);
}

#[test]
fn adwin_serde_roundtrip() {
    let mut adwin = Adwin::default();
    for i in 0..50 {
        adwin.update(i as f64 * 0.1).unwrap();
    }
    assert_roundtrip(&adwin);
}

#[test]
fn kswin_config_serde_roundtrip() {
    let config = KswinConfig::default();
    assert_roundtrip(&config);
}

#[test]
fn kswin_serde_roundtrip() {
    let mut kswin = Kswin::default();
    for i in 0..50 {
        kswin.update(i as f64 * 0.1).unwrap();
    }
    assert_roundtrip(&kswin);
}

#[test]
fn time_decayed_mean_serde_roundtrip() {
    let mut m = TimeDecayedMean::new(0.1).unwrap();
    m.update(0.0, 10.0).unwrap();
    m.update(1.0, 20.0).unwrap();
    m.update(2.0, 30.0).unwrap();
    assert_roundtrip(&m);
}

#[test]
fn learning_rate_scheduler_serde_roundtrip() {
    let mut sched = LearningRateScheduler::new(0.01, 2.0, 5.0).unwrap();
    sched.on_drift_level(DriftLevel::Warning);
    assert_roundtrip(&sched);
}

#[test]
fn fixed_window_buffer_serde_roundtrip() {
    let mut buf = FixedWindowBuffer::new(5).unwrap();
    for i in 0..10 {
        buf.push(i as f64).unwrap();
    }
    assert_roundtrip(&buf);
}

#[test]
fn drift_aware_model_serde_roundtrip() {
    let model = MeanRegressor::new(rill_ml::models::BaselineConfig::default()).unwrap();
    let detector = PageHinkley::default();
    let strategy = StaticStrategy::new(DriftAction::NotifyOnly, DriftAction::ResetModel);
    let mut aware = DriftAwareModel::new(model, detector, strategy);
    for i in 0..20 {
        aware.learn(&[], i as f64).unwrap();
    }
    assert_roundtrip(&aware);
}

// ---------------------------------------------------------------------------
// Bandit serialization round-trip tests (v0.5)
// ---------------------------------------------------------------------------

#[test]
fn epsilon_greedy_serialization_roundtrip() {
    let mut bandit = EpsilonGreedy::new(
        3,
        EpsilonGreedyConfig {
            epsilon: 0.2,
            decay: 0.99,
            min_epsilon: 0.05,
        },
    )
    .unwrap();
    bandit.update(0, 0.8).unwrap();
    bandit.update(1, 0.3).unwrap();
    bandit.update(0, 0.9).unwrap();

    let json = serde_json::to_string(&bandit).unwrap();
    let restored: EpsilonGreedy = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.arm_count(), bandit.arm_count());
    assert_eq!(restored.samples_seen(), bandit.samples_seen());
    for arm in 0..3 {
        let orig = bandit.arm_stats(arm).unwrap();
        let rest = restored.arm_stats(arm).unwrap();
        assert_eq!(orig.pulls, rest.pulls);
        assert!((orig.total_reward - rest.total_reward).abs() < 1e-12);
    }
}

#[test]
fn ucb1_serialization_roundtrip() {
    let mut bandit = Ucb1::new(
        3,
        Ucb1Config {
            exploration_constant: 2.0,
        },
    )
    .unwrap();
    bandit.update(0, 1.0).unwrap();
    bandit.update(1, 0.5).unwrap();
    bandit.update(2, 0.7).unwrap();

    let json = serde_json::to_string(&bandit).unwrap();
    let restored: Ucb1 = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.arm_count(), bandit.arm_count());
    assert_eq!(restored.samples_seen(), bandit.samples_seen());
    for arm in 0..3 {
        let orig = bandit.arm_stats(arm).unwrap();
        let rest = restored.arm_stats(arm).unwrap();
        assert_eq!(orig.pulls, rest.pulls);
    }
}

#[test]
fn thompson_sampling_serialization_roundtrip() {
    let mut bandit = ThompsonSampling::new(
        3,
        ThompsonConfig {
            alpha_prior: 1.0,
            beta_prior: 1.0,
        },
    )
    .unwrap();
    bandit.update(0, 1.0).unwrap();
    bandit.update(1, 0.0).unwrap();
    bandit.update(0, 0.7).unwrap();

    let json = serde_json::to_string(&bandit).unwrap();
    let restored: ThompsonSampling = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.arm_count(), bandit.arm_count());
    assert_eq!(restored.samples_seen(), bandit.samples_seen());
    for arm in 0..3 {
        let orig = bandit.arm_stats(arm).unwrap();
        let rest = restored.arm_stats(arm).unwrap();
        assert_eq!(orig.pulls, rest.pulls);
    }
}

#[test]
fn linucb_serialization_roundtrip() {
    let mut bandit = LinUcb::new(LinUcbConfig {
        alpha: 1.5,
        arm_count: 2,
        feature_count: 3,
    })
    .unwrap();
    bandit.update(0, &[1.0, 0.5, 0.2], 1.0).unwrap();
    bandit.update(1, &[0.3, 0.7, 0.9], 0.5).unwrap();
    bandit.update(0, &[0.8, 0.1, 0.4], 0.9).unwrap();

    let json = serde_json::to_string(&bandit).unwrap();
    let restored: LinUcb = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.arm_count(), bandit.arm_count());
    assert_eq!(restored.feature_count(), bandit.feature_count());
    assert_eq!(restored.samples_seen(), bandit.samples_seen());
    // Verify A matrices and b vectors are preserved.
    for arm in 0..2 {
        let orig_a = bandit.a_matrix(arm).unwrap();
        let rest_a = restored.a_matrix(arm).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert!((orig_a[i][j] - rest_a[i][j]).abs() < 1e-12);
            }
        }
        let orig_b = bandit.b_vector(arm).unwrap();
        let rest_b = restored.b_vector(arm).unwrap();
        for i in 0..3 {
            assert!((orig_b[i] - rest_b[i]).abs() < 1e-12);
        }
    }
}

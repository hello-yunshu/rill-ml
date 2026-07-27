#![cfg(feature = "serde")]

//! Generates v1.0 RC state fixtures from the current code for the 1.0 freeze.
//!
//! Writes JSON files to `tests/fixtures/state/v1/`.
//! Run with: `cargo test --features serde --test generate_v1_fixtures -- --nocapture`

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use rill_ml::OnlineBinaryClassifier;
use rill_ml::OnlineRegressor;
use rill_ml::OnlineStatistic;
use rill_ml::SparseClassifier;
use rill_ml::SparseRegressor;
use rill_ml::Transformer;
use rill_ml::bandit::{
    Bandit, ContextualBandit, EpsilonGreedy, EpsilonGreedyConfig, LinUcb, LinUcbConfig,
    ThompsonConfig, ThompsonSampling, Ucb1, Ucb1Config,
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

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("state")
        .join("v1")
}

fn write_fixture<T: serde::Serialize>(name: &str, value: &T) {
    let dir = fixture_dir();
    fs::create_dir_all(&dir).expect("create fixture dir");
    let path = dir.join(format!("{name}.json"));
    let json = serde_json::to_string_pretty(value).expect("serialize");
    let mut file = fs::File::create(&path).expect("create file");
    writeln!(file, "{json}").expect("write");
    println!("wrote {}", path.display());
}

fn write_snapshot_fixture<T: serde::Serialize>(name: &str, value: T) {
    let snap = Snapshot::new(value);
    write_fixture(name, &snap);
}

#[test]
fn generate_v1_fixtures() {
    // Stats
    let mut mean = Mean::new();
    for v in [1.0, 2.5, 3.7, 4.1] {
        mean.update(v).unwrap();
    }
    write_snapshot_fixture("mean", mean);

    let mut variance = Variance::new(VarianceKind::Sample);
    for i in 0..20 {
        variance.update(i as f64).unwrap();
    }
    write_snapshot_fixture("variance", variance);

    let mut ew_mean = ExponentiallyWeightedMean::new(0.3).unwrap();
    for i in 0..10 {
        ew_mean.update(i as f64 * 0.5).unwrap();
    }
    write_snapshot_fixture("ew_mean", ew_mean);

    // Preprocessing
    let mut scaler = StandardScaler::new(3).unwrap();
    scaler.update(&[1.0, 10.0, 100.0]).unwrap();
    scaler.update(&[2.0, 20.0, 200.0]).unwrap();
    scaler.update(&[3.0, 30.0, 300.0]).unwrap();
    write_snapshot_fixture("standard_scaler", scaler);

    let mut ohe = OneHotEncoder::new();
    ohe.update_strs(&["a", "b", "a", "c"]).unwrap();
    write_snapshot_fixture("one_hot_encoder", ohe);

    let mut ord = OrdinalEncoder::new();
    ord.update_strs(&["low", "mid", "high", "low"]).unwrap();
    write_snapshot_fixture("ordinal_encoder", ord);

    let mut freq = FrequencyEncoder::new();
    freq.update_strs(&["x", "x", "y", "x", "z"]).unwrap();
    write_snapshot_fixture("frequency_encoder", freq);

    let mut imp_cfg = ConstantImputerConfig::default();
    imp_cfg.fill_value = -1.0;
    let imputer = ConstantImputer::with_config(2, imp_cfg).unwrap();
    write_snapshot_fixture("constant_imputer", imputer);

    let mut mean_imp = MeanImputer::new(2).unwrap();
    mean_imp.update(&[1.0, 2.0]).unwrap();
    mean_imp.update(&[3.0, f64::NAN]).unwrap();
    write_snapshot_fixture("mean_imputer", mean_imp);

    let mut ff = ForwardFill::new(2).unwrap();
    ff.update(&[1.0, 2.0]).unwrap();
    ff.update(&[f64::NAN, 3.0]).unwrap();
    write_snapshot_fixture("forward_fill", ff);

    let mut mi = MissingIndicator::new(2).unwrap();
    mi.update(&[1.0, f64::NAN]).unwrap();
    write_snapshot_fixture("missing_indicator", mi);

    // Models — use Default::default() + field assignment because configs are #[non_exhaustive]
    let d = 2;
    let mut sgd = SgdConfig::default();
    sgd.learning_rate = 0.1;
    sgd.l2 = 0.01;
    let mut lr_cfg = LinearRegressionConfig::default();
    lr_cfg.optimizer = Optimizer::sgd(d, sgd).unwrap();
    let mut lr = LinearRegression::new(d, lr_cfg).unwrap();
    for (x, y) in [([1.0, 2.0], 3.0), ([4.0, 5.0], 6.0), ([2.0, 1.0], 4.0)] {
        lr.learn(&x, y).unwrap();
    }
    write_snapshot_fixture("linear_regression", lr);

    let mut mean_reg = MeanRegressor::default();
    for y in [42.0, 10.0, 30.0, 25.0] {
        mean_reg.learn(&[], y).unwrap();
    }
    write_snapshot_fixture("mean_regressor", mean_reg);

    // FTRL regressor (v1.0 API: takes only config, feature count is unbounded)
    let mut ftrl_r = FtrlRegressor::new(FtrlConfig::default()).unwrap();
    let sf_r = SparseFeatures::from_sorted(vec![(0, 1.0), (2, 2.0)]).unwrap();
    ftrl_r.learn(&sf_r, 5.0).unwrap();
    ftrl_r.learn(&sf_r, -1.0).unwrap();
    write_snapshot_fixture("ftrl_regressor", ftrl_r);

    // FTRL classifier
    let mut ftrl_c = FtrlClassifier::new(FtrlConfig::default()).unwrap();
    let sf_c = SparseFeatures::from_sorted(vec![(0, 1.0), (2, -1.0)]).unwrap();
    ftrl_c.learn(&sf_c, true).unwrap();
    ftrl_c.learn(&sf_c, false).unwrap();
    write_snapshot_fixture("ftrl_classifier", ftrl_c);

    // Naive Bayes — v1.0 API: new(feature_count, config), target is bool
    let mut gnb = GaussianNaiveBayes::new(2, Default::default()).unwrap();
    gnb.learn(&[1.0, 2.0], false).unwrap();
    gnb.learn(&[2.0, 3.0], true).unwrap();
    gnb.learn(&[1.5, 2.5], false).unwrap();
    write_snapshot_fixture("gaussian_naive_bayes", gnb);

    let mut bnb = BernoulliNaiveBayes::new(3, Default::default()).unwrap();
    bnb.learn(&[1.0, 0.0, 1.0], false).unwrap();
    bnb.learn(&[0.0, 1.0, 0.0], true).unwrap();
    write_snapshot_fixture("bernoulli_naive_bayes", bnb);

    let mut mnb = MultinomialNaiveBayes::new(3, Default::default()).unwrap();
    mnb.learn(&[2.0, 1.0, 0.0], false).unwrap();
    mnb.learn(&[0.0, 1.0, 2.0], true).unwrap();
    write_snapshot_fixture("multinomial_naive_bayes", mnb);

    // Optimizer and loss (directly)
    let opt = Optimizer::sgd(2, SgdConfig::default()).unwrap();
    write_fixture("optimizer_sgd", &opt);

    let loss = RegressionLoss::default();
    write_fixture("regression_loss", &loss);

    // Sparse features
    let sparse = SparseFeatures::from_sorted(vec![(0, 1.5), (3, -2.0), (7, 0.25)]).unwrap();
    write_fixture("sparse_features", &sparse);

    // Feature hasher
    let hasher = FeatureHasher::new(32, 42).unwrap();
    write_fixture("feature_hasher", &hasher);

    // Bandits — v1.0 configs are #[non_exhaustive], use Default + field assignment
    let mut eps_cfg = EpsilonGreedyConfig::default();
    eps_cfg.epsilon = 0.2;
    eps_cfg.decay = 0.99;
    eps_cfg.min_epsilon = 0.05;
    let mut eps = EpsilonGreedy::new(3, eps_cfg).unwrap();
    eps.update(0, 0.8).unwrap();
    eps.update(1, 0.3).unwrap();
    eps.update(0, 0.9).unwrap();
    write_snapshot_fixture("epsilon_greedy", eps);

    let mut ucb1_cfg = Ucb1Config::default();
    ucb1_cfg.exploration_constant = 2.0;
    let mut ucb1 = Ucb1::new(3, ucb1_cfg).unwrap();
    ucb1.update(0, 1.0).unwrap();
    ucb1.update(1, 0.5).unwrap();
    ucb1.update(2, 0.7).unwrap();
    write_snapshot_fixture("ucb1", ucb1);

    let mut thompson_cfg = ThompsonConfig::default();
    thompson_cfg.alpha_prior = 1.0;
    thompson_cfg.beta_prior = 1.0;
    let mut thompson = ThompsonSampling::new(3, thompson_cfg).unwrap();
    thompson.update(0, 1.0).unwrap();
    thompson.update(1, 0.0).unwrap();
    thompson.update(0, 0.7).unwrap();
    write_snapshot_fixture("thompson_sampling", thompson);

    let mut linucb_cfg = LinUcbConfig::default();
    linucb_cfg.alpha = 1.5;
    linucb_cfg.arm_count = 2;
    linucb_cfg.feature_count = 3;
    let mut linucb = LinUcb::new(linucb_cfg).unwrap();
    linucb.update(0, &[1.0, 0.5, 0.2], 1.0).unwrap();
    linucb.update(1, &[0.3, 0.7, 0.9], 0.5).unwrap();
    linucb.update(0, &[0.8, 0.1, 0.4], 0.9).unwrap();
    write_snapshot_fixture("linucb", linucb);

    println!("All v1 fixtures written to {}", fixture_dir().display());
}

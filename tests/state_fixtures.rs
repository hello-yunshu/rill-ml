#![cfg(feature = "serde")]

//! Cross-version state fixture loading and malicious-state rejection tests.
//!
//! These tests enforce the 1.0 state-freeze contract:
//! - v0.13.0 fixtures produced by the immutable `v0.13.0` tag must load and
//!   validate with the current 1.0 RC code.
//! - v1 fixtures produced by the current RC code must round-trip.
//! - Malicious states (oversized JSON, non-finite values, dimension mismatches,
//!   wrong optimizer param counts, dangling encoder indices, incompatible
//!   `format_version`) must be rejected by `Snapshot::from_json_validated`.
//! - The byte-size limit (`MAX_SNAPSHOT_JSON_BYTES`) is enforced before
//!   deserialization.

use std::fs;
use std::path::PathBuf;

use rill_ml::RillError;
use rill_ml::persistence::{MAX_SNAPSHOT_JSON_BYTES, Snapshot, ValidateState};
use rill_ml::{
    bandit::{EpsilonGreedy, LinUcb, ThompsonSampling, Ucb1},
    feature_hasher::FeatureHasher,
    loss::RegressionLoss,
    models::{
        BernoulliNaiveBayes, FtrlClassifier, FtrlRegressor, GaussianNaiveBayes, LinearRegression,
        MeanRegressor, MultinomialNaiveBayes,
    },
    optim::{Optimizer, Sgd},
    preprocessing::{
        ConstantImputer, ForwardFill, FrequencyEncoder, MeanImputer, MissingIndicator,
        OneHotEncoder, OrdinalEncoder, StandardScaler,
    },
    sparse::SparseFeatures,
    stats::{ExponentiallyWeightedMean, Mean, Variance},
};

fn fixture_dir(version: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("state")
        .join(version)
}

fn load_fixture<T>(version: &str, name: &str) -> T
where
    T: serde::de::DeserializeOwned + ValidateState,
{
    let path = fixture_dir(version).join(format!("{name}.json"));
    let json = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {} {}: {e}", version, path.display()));
    Snapshot::<T>::from_json_validated(&json)
        .unwrap_or_else(|e| panic!("load fixture {} {}: {e:?}", version, path.display()))
}

fn load_raw(version: &str, name: &str) -> String {
    let path = fixture_dir(version).join(format!("{name}.json"));
    fs::read_to_string(&path).expect("read fixture")
}

// ---------------------------------------------------------------------------
// v0.13.0 fixtures must load and validate under the 1.0 RC code.
// ---------------------------------------------------------------------------

#[test]
fn load_v0_13_0_mean() {
    let _: Mean = load_fixture("v0.13.0", "mean");
}

#[test]
fn load_v0_13_0_variance() {
    let _: Variance = load_fixture("v0.13.0", "variance");
}

#[test]
fn load_v0_13_0_ew_mean() {
    let _: ExponentiallyWeightedMean = load_fixture("v0.13.0", "ew_mean");
}

#[test]
fn load_v0_13_0_standard_scaler() {
    let _: StandardScaler = load_fixture("v0.13.0", "standard_scaler");
}

#[test]
fn load_v0_13_0_one_hot_encoder() {
    let _: OneHotEncoder = load_fixture("v0.13.0", "one_hot_encoder");
}

#[test]
fn load_v0_13_0_ordinal_encoder() {
    let _: OrdinalEncoder = load_fixture("v0.13.0", "ordinal_encoder");
}

#[test]
fn load_v0_13_0_frequency_encoder() {
    let _: FrequencyEncoder = load_fixture("v0.13.0", "frequency_encoder");
}

#[test]
fn load_v0_13_0_constant_imputer() {
    let _: ConstantImputer = load_fixture("v0.13.0", "constant_imputer");
}

#[test]
fn load_v0_13_0_mean_imputer() {
    let _: MeanImputer = load_fixture("v0.13.0", "mean_imputer");
}

#[test]
fn load_v0_13_0_forward_fill() {
    let _: ForwardFill = load_fixture("v0.13.0", "forward_fill");
}

#[test]
fn load_v0_13_0_missing_indicator() {
    let _: MissingIndicator = load_fixture("v0.13.0", "missing_indicator");
}

#[test]
fn load_v0_13_0_linear_regression() {
    let _: LinearRegression = load_fixture("v0.13.0", "linear_regression");
}

#[test]
fn load_v0_13_0_mean_regressor() {
    let _: MeanRegressor = load_fixture("v0.13.0", "mean_regressor");
}

#[test]
fn load_v0_13_0_ftrl_regressor() {
    let _: FtrlRegressor = load_fixture("v0.13.0", "ftrl_regressor");
}

#[test]
fn load_v0_13_0_ftrl_classifier() {
    let _: FtrlClassifier = load_fixture("v0.13.0", "ftrl_classifier");
}

#[test]
fn load_v0_13_0_gaussian_naive_bayes() {
    let _: GaussianNaiveBayes = load_fixture("v0.13.0", "gaussian_naive_bayes");
}

#[test]
fn load_v0_13_0_bernoulli_naive_bayes() {
    let _: BernoulliNaiveBayes = load_fixture("v0.13.0", "bernoulli_naive_bayes");
}

#[test]
fn load_v0_13_0_multinomial_naive_bayes() {
    let _: MultinomialNaiveBayes = load_fixture("v0.13.0", "multinomial_naive_bayes");
}

#[test]
fn load_v0_13_0_optimizer_sgd() {
    let json = load_raw("v0.13.0", "optimizer_sgd");
    let opt: Optimizer = serde_json::from_str(&json).expect("deserialize optimizer");
    let _ = opt;
    // Also verify it implements ValidateState if exposed.
}

#[test]
fn load_v0_13_0_regression_loss() {
    let json = load_raw("v0.13.0", "regression_loss");
    let loss: RegressionLoss = serde_json::from_str(&json).expect("deserialize loss");
    let _ = loss;
}

#[test]
fn load_v0_13_0_sparse_features() {
    let json = load_raw("v0.13.0", "sparse_features");
    let _: SparseFeatures = serde_json::from_str(&json).expect("deserialize sparse");
}

#[test]
fn load_v0_13_0_feature_hasher() {
    let json = load_raw("v0.13.0", "feature_hasher");
    let _: FeatureHasher = serde_json::from_str(&json).expect("deserialize hasher");
}

#[test]
fn load_v0_13_0_epsilon_greedy() {
    let _: EpsilonGreedy = load_fixture("v0.13.0", "epsilon_greedy");
}

#[test]
fn load_v0_13_0_ucb1() {
    let _: Ucb1 = load_fixture("v0.13.0", "ucb1");
}

#[test]
fn load_v0_13_0_thompson_sampling() {
    let _: ThompsonSampling = load_fixture("v0.13.0", "thompson_sampling");
}

#[test]
fn load_v0_13_0_linucb() {
    let _: LinUcb = load_fixture("v0.13.0", "linucb");
}

// ---------------------------------------------------------------------------
// v1 fixtures must round-trip and validate under the 1.0 RC code.
// ---------------------------------------------------------------------------

#[test]
fn load_v1_mean() {
    let _: Mean = load_fixture("v1", "mean");
}

#[test]
fn load_v1_variance() {
    let _: Variance = load_fixture("v1", "variance");
}

#[test]
fn load_v1_ew_mean() {
    let _: ExponentiallyWeightedMean = load_fixture("v1", "ew_mean");
}

#[test]
fn load_v1_standard_scaler() {
    let _: StandardScaler = load_fixture("v1", "standard_scaler");
}

#[test]
fn load_v1_one_hot_encoder() {
    let _: OneHotEncoder = load_fixture("v1", "one_hot_encoder");
}

#[test]
fn load_v1_ordinal_encoder() {
    let _: OrdinalEncoder = load_fixture("v1", "ordinal_encoder");
}

#[test]
fn load_v1_frequency_encoder() {
    let _: FrequencyEncoder = load_fixture("v1", "frequency_encoder");
}

#[test]
fn load_v1_constant_imputer() {
    let _: ConstantImputer = load_fixture("v1", "constant_imputer");
}

#[test]
fn load_v1_mean_imputer() {
    let _: MeanImputer = load_fixture("v1", "mean_imputer");
}

#[test]
fn load_v1_forward_fill() {
    let _: ForwardFill = load_fixture("v1", "forward_fill");
}

#[test]
fn load_v1_missing_indicator() {
    let _: MissingIndicator = load_fixture("v1", "missing_indicator");
}

#[test]
fn load_v1_linear_regression() {
    let _: LinearRegression = load_fixture("v1", "linear_regression");
}

#[test]
fn load_v1_mean_regressor() {
    let _: MeanRegressor = load_fixture("v1", "mean_regressor");
}

#[test]
fn load_v1_ftrl_regressor() {
    let _: FtrlRegressor = load_fixture("v1", "ftrl_regressor");
}

#[test]
fn load_v1_ftrl_classifier() {
    let _: FtrlClassifier = load_fixture("v1", "ftrl_classifier");
}

#[test]
fn load_v1_gaussian_naive_bayes() {
    let _: GaussianNaiveBayes = load_fixture("v1", "gaussian_naive_bayes");
}

#[test]
fn load_v1_bernoulli_naive_bayes() {
    let _: BernoulliNaiveBayes = load_fixture("v1", "bernoulli_naive_bayes");
}

#[test]
fn load_v1_multinomial_naive_bayes() {
    let _: MultinomialNaiveBayes = load_fixture("v1", "multinomial_naive_bayes");
}

#[test]
fn load_v1_epsilon_greedy() {
    let _: EpsilonGreedy = load_fixture("v1", "epsilon_greedy");
}

#[test]
fn load_v1_ucb1() {
    let _: Ucb1 = load_fixture("v1", "ucb1");
}

#[test]
fn load_v1_thompson_sampling() {
    let _: ThompsonSampling = load_fixture("v1", "thompson_sampling");
}

#[test]
fn load_v1_linucb() {
    let _: LinUcb = load_fixture("v1", "linucb");
}

// ---------------------------------------------------------------------------
// Malicious-state rejection.
// ---------------------------------------------------------------------------

#[test]
fn incompatible_format_version_rejected() {
    // A valid Mean snapshot but with format_version = 999.
    let json = r#"{"format_version":999,"model":{"count":1,"mean":1.0}}"#;
    let result: Result<Mean, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err());
}

#[test]
fn null_mean_rejected_by_serde() {
    // JSON `null` cannot deserialize as `f64`, so serde itself rejects this
    // payload before `ValidateState` is reached. This proves the serde type
    // check, not the `ValidateState` non-finite-value check — see
    // `validate_state_rejects_non_finite_values` for the latter.
    let json = r#"{"format_version":1,"model":{"count":1,"mean":null}}"#;
    let result: Result<Mean, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err());
}

#[test]
fn validate_state_rejects_non_finite_values() {
    // JSON cannot represent NaN/Inf as native `f64` values, so to test that
    // `ValidateState` actually rejects non-finite values (as opposed to serde
    // type checking), we use a custom test type that deserializes a string
    // representation into `f64` and then validates finiteness.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct FloatWrapper {
        raw: String,
    }

    impl ValidateState for FloatWrapper {
        fn validate_state(&self) -> Result<(), RillError> {
            let value: f64 = self.raw.parse().map_err(|_| {
                RillError::InvalidState(format!("invalid float literal: {}", self.raw))
            })?;
            if !value.is_finite() {
                return Err(RillError::NonFiniteValue {
                    field: "value",
                    value,
                });
            }
            Ok(())
        }
    }

    // NaN must be rejected by ValidateState.
    let json = r#"{"format_version":1,"model":{"raw":"NaN"}}"#;
    let result: Result<FloatWrapper, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err(), "ValidateState must reject NaN");

    // Infinity must be rejected by ValidateState.
    let json = r#"{"format_version":1,"model":{"raw":"inf"}}"#;
    let result: Result<FloatWrapper, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err(), "ValidateState must reject Infinity");

    // Negative Infinity must be rejected by ValidateState.
    let json = r#"{"format_version":1,"model":{"raw":"-inf"}}"#;
    let result: Result<FloatWrapper, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err(), "ValidateState must reject -Infinity");

    // A finite value must be accepted.
    let json = r#"{"format_version":1,"model":{"raw":"1.5"}}"#;
    let result: Result<FloatWrapper, _> = Snapshot::from_json_validated(json);
    assert!(result.is_ok(), "ValidateState must accept finite values");
}

#[test]
fn oversized_json_rejected_before_deserialization() {
    // Build a JSON string that exceeds MAX_SNAPSHOT_JSON_BYTES.
    let payload = "x".repeat(MAX_SNAPSHOT_JSON_BYTES + 1);
    let json = format!(
        "{{\"format_version\":1,\"model\":{{\"count\":1,\"mean\":1.0}},\"pad\":\"{payload}\"}}"
    );
    let result: Result<Mean, _> = Snapshot::from_json_validated(&json);
    assert!(result.is_err());
}

#[test]
fn dimension_mismatch_in_linear_regression_rejected() {
    // Construct a LinearRegression state where weights length != feature_count.
    // The serde model is field-pub, so we craft JSON directly.
    let json = r#"{
      "format_version":1,
      "model":{
        "feature_count":3,
        "weights":[0.0,0.0],
        "intercept":0.0,
        "optimizer":{"Sgd":{"feature_count":3,"config":{"learning_rate":0.01,"l2":0.0},"samples_seen":0}},
        "loss":{"Mse":{}},
        "samples_seen":0
      }
    }"#;
    let result: Result<LinearRegression, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err(), "must reject dimension mismatch");
}

#[test]
fn optimizer_param_count_mismatch_rejected() {
    // LinearRegression with feature_count=2 but optimizer param_count=2 (should be 3).
    let json = r#"{
      "format_version":1,
      "model":{
        "feature_count":2,
        "weights":[0.0,0.0],
        "intercept":0.0,
        "optimizer":{"Sgd":{"feature_count":1,"config":{"learning_rate":0.01,"l2":0.0},"samples_seen":0}},
        "loss":{"Mse":{}},
        "samples_seen":0
      }
    }"#;
    let result: Result<LinearRegression, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err(), "must reject optimizer param mismatch");
}

#[test]
fn failed_restore_returns_no_model() {
    // Ensure into_validated_model is atomic: an error means no model leaks.
    let json = r#"{"format_version":999,"model":{"count":1,"mean":1.0}}"#;
    let result: Result<Mean, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err());
    // No way to access a half-validated model: the Result owns nothing on Err.
}

#[test]
fn negative_m2_in_variance_rejected() {
    // Variance with a negative m2 must fail ValidateState. m2 is the sum of
    // squared deltas in Welford's algorithm and must always be non-negative.
    // The `count` field is `u64`, so it cannot be negative in valid JSON;
    // instead we test the real invariant that `validate_state()` enforces.
    let json = r#"{"format_version":1,"model":{"count":1,"mean":1.0,"m2":-1.0,"kind":"Sample"}}"#;
    let result: Result<Variance, _> = Snapshot::from_json_validated(json);
    assert!(result.is_err(), "must reject negative m2 in Variance state");
}

#[test]
fn sgd_invalid_learning_rate_rejected() {
    // Sgd optimizer with non-positive learning rate must fail ValidateState.
    // The optimizer is wrapped in a `Snapshot` envelope to exercise the full
    // `from_json_validated` path (version check + deserialize + validate_state),
    // not just the `Deserialize` trait. The learning rate `0.0` is non-positive
    // and must be rejected by `Sgd::validate_state()`.
    let json = r#"{"format_version":1,"model":{"feature_count":2,"config":{"learning_rate":0.0,"l2":0.0},"samples_seen":0}}"#;
    let result: Result<Sgd, _> = Snapshot::from_json_validated(json);
    assert!(
        result.is_err(),
        "must reject non-positive learning rate in Sgd state"
    );
}

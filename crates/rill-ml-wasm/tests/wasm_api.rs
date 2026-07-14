//! Functional tests for the WASM bindings.
//!
//! These tests run under `wasm32-unknown-unknown` via `wasm-pack test --node`
//! (or `cargo test --target wasm32-unknown-unknown`). They exercise the
//! JavaScript-facing API surface to verify that the bindings correctly wrap
//! the core RillML types.
//!
//! The host-side compile check (`tests/host_compile_check.rs`) is kept so that
//! `cargo check --workspace` and `cargo clippy --workspace` still work without
//! a wasm toolchain installed locally.

#![cfg(target_arch = "wasm32")]

use js_sys::Float64Array;
use rill_ml_wasm::*;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn wasm_mean_update_and_value() {
    let mut m = WasmMean::new();
    m.update(1.0).unwrap();
    m.update(2.0).unwrap();
    m.update(3.0).unwrap();
    assert!((m.value() - 2.0).abs() < 1e-12);
    assert_eq!(m.count(), 3);
}

#[wasm_bindgen_test]
fn wasm_mean_roundtrip() {
    let mut m = WasmMean::new();
    m.update(10.0).unwrap();
    m.update(20.0).unwrap();
    let json = m.to_json().unwrap();
    let m2 = WasmMean::from_json(&json).unwrap();
    assert!((m2.value() - 15.0).abs() < 1e-9);
    assert_eq!(m2.count(), 2);
}

#[wasm_bindgen_test]
fn wasm_variance_update() {
    let mut v = WasmVariance::new("population").unwrap();
    assert!(v.value().is_none());
    for x in [1.0, 2.0, 3.0, 4.0, 5.0] {
        v.update(x).unwrap();
    }
    assert!((v.value().unwrap() - 2.0).abs() < 1e-12);
    assert!((v.std_dev().unwrap() - 2.0_f64.sqrt()).abs() < 1e-12);
    assert_eq!(v.count(), 5);
}

#[wasm_bindgen_test]
fn wasm_variance_roundtrip() {
    let mut v = WasmVariance::new("population").unwrap();
    for x in [1.0, 2.0, 3.0] {
        v.update(x).unwrap();
    }
    let json = v.to_json().unwrap();
    let v2 = WasmVariance::from_json(&json).unwrap();
    assert!((v2.value().unwrap() - v.value().unwrap()).abs() < 1e-12);
    assert_eq!(v2.count(), 3);
}

#[wasm_bindgen_test]
fn wasm_ewmean() {
    let mut ew = WasmEWMean::new(0.5).unwrap();
    ew.update(10.0).unwrap();
    ew.update(20.0).unwrap();
    assert!((ew.value() - 15.0).abs() < 1e-9);
}

#[wasm_bindgen_test]
fn wasm_ewmean_roundtrip() {
    let mut ew = WasmEWMean::new(0.5).unwrap();
    ew.update(10.0).unwrap();
    let json = ew.to_json().unwrap();
    let ew2 = WasmEWMean::from_json(&json).unwrap();
    assert!((ew2.value() - ew.value()).abs() < 1e-9);
}

#[wasm_bindgen_test]
fn wasm_standard_scaler() {
    let mut sc = WasmStandardScaler::new(2).unwrap();
    let x = Float64Array::new_with_length(2);
    x.set_index(0, 1.0);
    x.set_index(1, 2.0);
    sc.update(&x).unwrap();
    let x2 = Float64Array::new_with_length(2);
    x2.set_index(0, 3.0);
    x2.set_index(1, 4.0);
    sc.update(&x2).unwrap();
    assert_eq!(sc.samples_seen(), 2);

    let query = Float64Array::new_with_length(2);
    query.set_index(0, 2.0);
    query.set_index(1, 3.0);
    let out = sc.transform(&query).unwrap();
    assert_eq!(out.length(), 2);
}

#[wasm_bindgen_test]
fn wasm_standard_scaler_roundtrip() {
    let mut sc = WasmStandardScaler::new(2).unwrap();
    let x = Float64Array::new_with_length(2);
    x.set_index(0, 1.0);
    x.set_index(1, 2.0);
    sc.update(&x).unwrap();
    let json = sc.to_json().unwrap();
    let sc2 = WasmStandardScaler::from_json(&json).unwrap();
    assert_eq!(sc2.samples_seen(), 1);
}

#[wasm_bindgen_test]
fn wasm_linear_regression_learns() {
    let mut lr = WasmLinearRegression::new(1, 0.05).unwrap();
    let x = Float64Array::new_with_length(1);
    x.set_index(0, 2.0);
    for _ in 0..100 {
        lr.learn(&x, 10.0).unwrap();
    }
    let pred = lr.predict(&x).unwrap();
    assert!((pred - 10.0).abs() < 1.0);
    assert_eq!(lr.samples_seen(), 100);
    assert_eq!(lr.weights().length(), 1);
}

#[wasm_bindgen_test]
fn wasm_linear_regression_roundtrip() {
    let mut lr = WasmLinearRegression::new(1, 0.1).unwrap();
    let x = Float64Array::new_with_length(1);
    x.set_index(0, 1.0);
    lr.learn(&x, 5.0).unwrap();
    let json = lr.to_json().unwrap();
    let lr2 = WasmLinearRegression::from_json(&json).unwrap();
    assert_eq!(lr2.samples_seen(), 1);
}

#[wasm_bindgen_test]
fn wasm_logistic_regression() {
    let mut logr = WasmLogisticRegression::new(2, 0.1).unwrap();
    let pos = Float64Array::new_with_length(2);
    pos.set_index(0, 1.0);
    pos.set_index(1, 2.0);
    logr.learn(&pos, true).unwrap();
    let neg = Float64Array::new_with_length(2);
    neg.set_index(0, -1.0);
    neg.set_index(1, -2.0);
    logr.learn(&neg, false).unwrap();
    let _pred = logr.predict(&pos).unwrap();
    let proba = logr.predict_proba(&pos).unwrap();
    assert!(proba >= 0.0 && proba <= 1.0);
}

#[wasm_bindgen_test]
fn wasm_logistic_regression_roundtrip() {
    let mut logr = WasmLogisticRegression::new(1, 0.1).unwrap();
    let x = Float64Array::new_with_length(1);
    x.set_index(0, 1.0);
    logr.learn(&x, true).unwrap();
    let json = logr.to_json().unwrap();
    let logr2 = WasmLogisticRegression::from_json(&json).unwrap();
    assert_eq!(logr2.samples_seen(), 1);
}

#[wasm_bindgen_test]
fn wasm_regression_pipeline() {
    let mut pipe = WasmRegressionPipeline::new(2, 0.05).unwrap();
    let x = Float64Array::new_with_length(2);
    x.set_index(0, 0.1);
    x.set_index(1, 0.2);
    pipe.learn(&x, 0.5).unwrap();
    let pred = pipe.predict(&x).unwrap();
    assert!(pred.is_finite());
    assert_eq!(pipe.samples_seen(), 1);
}

#[wasm_bindgen_test]
fn wasm_regression_pipeline_roundtrip() {
    let mut pipe = WasmRegressionPipeline::new(2, 0.05).unwrap();
    let x = Float64Array::new_with_length(2);
    x.set_index(0, 0.1);
    x.set_index(1, 0.2);
    pipe.learn(&x, 0.5).unwrap();
    let json = pipe.to_json().unwrap();
    let pipe2 = WasmRegressionPipeline::from_json(&json).unwrap();
    assert_eq!(pipe2.samples_seen(), 1);
}

#[wasm_bindgen_test]
fn wasm_classification_pipeline() {
    let mut pipe = WasmClassificationPipeline::new(2, 0.05).unwrap();
    let pos = Float64Array::new_with_length(2);
    pos.set_index(0, 0.1);
    pos.set_index(1, 0.2);
    pipe.learn(&pos, true).unwrap();
    let neg = Float64Array::new_with_length(2);
    neg.set_index(0, -0.1);
    neg.set_index(1, -0.2);
    pipe.learn(&neg, false).unwrap();
    let proba = pipe.predict_proba(&pos).unwrap();
    assert!(proba >= 0.0 && proba <= 1.0);
}

#[wasm_bindgen_test]
fn wasm_classification_pipeline_roundtrip() {
    let mut pipe = WasmClassificationPipeline::new(2, 0.05).unwrap();
    let x = Float64Array::new_with_length(2);
    x.set_index(0, 0.1);
    x.set_index(1, 0.2);
    pipe.learn(&x, true).unwrap();
    let json = pipe.to_json().unwrap();
    let pipe2 = WasmClassificationPipeline::from_json(&json).unwrap();
    assert_eq!(pipe2.samples_seen(), 1);
}

#[wasm_bindgen_test]
fn wasm_snapshot_format_version() {
    assert_eq!(WasmSnapshot::format_version(), 1);
}

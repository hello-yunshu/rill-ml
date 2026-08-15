//! Functional tests for the `rill-ml-ffi` opaque-handle C ABI.
//!
//! These tests call the exported `#[no_mangle] extern "C"` entry points
//! directly, the same way a C / C++ / JNI / Swift binding would. They cover
//! the full lifecycle (new → learn/update → predict/transform → to_json →
//! from_json → destroy) plus the error-code contract:
//!
//! - NULL / destroyed handles report `RILL_ML_ERR_INVALID_HANDLE`
//! - undersized output buffers report `RILL_ML_ERR_BUFFER_TOO_SMALL`
//! - invalid arguments (e.g. a binary target of 2) report
//!   `RILL_ML_ERR_INVALID_ARGUMENT`
//! - every error also writes a human-readable message into the error buffer

use std::ffi::{c_char, c_double, c_int, c_void};
use std::ptr;

use rill_ml_ffi::{
    RILL_ML_ERR_BUFFER_TOO_SMALL, RILL_ML_ERR_INVALID_ARGUMENT, RILL_ML_ERR_INVALID_HANDLE,
    RILL_ML_OK,
};

// --------------------------------------------------------------------------- //
// Extern declarations (mirror include/rill_ml.h)
// --------------------------------------------------------------------------- //

// Not every mirrored symbol is exercised by a test; the full set is declared
// to keep this file an exact reflection of the C header.
#[allow(dead_code)]
unsafe extern "C" {
    fn rill_ml_version(
        buf: *mut c_char,
        buf_len: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_snapshot_format_version() -> i32;

    fn rill_ml_mean_new(error_buf: *mut c_char, error_buf_len: usize) -> *mut c_void;
    fn rill_ml_mean_update(
        handle: *mut c_void,
        value: f64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_mean_value(
        handle: *mut c_void,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_mean_count(
        handle: *mut c_void,
        out: *mut u64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_mean_to_json(
        handle: *mut c_void,
        buf: *mut c_char,
        buf_len: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_mean_from_json(
        json: *const c_char,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_mean_destroy(
        handle: *mut c_void,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;

    fn rill_ml_linear_regression_new(
        feature_count: usize,
        learning_rate: f64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_linear_regression_predict(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_linear_regression_learn(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        target: f64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_linear_regression_weights(
        handle: *mut c_void,
        out: *mut c_double,
        out_len: *mut usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_linear_regression_intercept(
        handle: *mut c_void,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_linear_regression_samples_seen(
        handle: *mut c_void,
        out: *mut u64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_linear_regression_to_json(
        handle: *mut c_void,
        buf: *mut c_char,
        buf_len: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_linear_regression_from_json(
        json: *const c_char,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_linear_regression_destroy(
        handle: *mut c_void,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;

    fn rill_ml_logistic_regression_new(
        feature_count: usize,
        learning_rate: f64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_logistic_regression_predict_proba(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_logistic_regression_predict(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        out: *mut c_int,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_logistic_regression_learn(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        target: i32,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_logistic_regression_weights(
        handle: *mut c_void,
        out: *mut c_double,
        out_len: *mut usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_logistic_regression_intercept(
        handle: *mut c_void,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_logistic_regression_samples_seen(
        handle: *mut c_void,
        out: *mut u64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_logistic_regression_to_json(
        handle: *mut c_void,
        buf: *mut c_char,
        buf_len: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_logistic_regression_from_json(
        json: *const c_char,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_logistic_regression_destroy(
        handle: *mut c_void,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;

    fn rill_ml_standard_scaler_new(
        feature_count: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_standard_scaler_update(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_standard_scaler_transform(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_standard_scaler_samples_seen(
        handle: *mut c_void,
        out: *mut u64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_standard_scaler_to_json(
        handle: *mut c_void,
        buf: *mut c_char,
        buf_len: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_standard_scaler_from_json(
        json: *const c_char,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_standard_scaler_destroy(
        handle: *mut c_void,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;

    fn rill_ml_regression_pipeline_new(
        feature_count: usize,
        learning_rate: f64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_regression_pipeline_predict(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_regression_pipeline_learn(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        target: f64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_regression_pipeline_samples_seen(
        handle: *mut c_void,
        out: *mut u64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_regression_pipeline_to_json(
        handle: *mut c_void,
        buf: *mut c_char,
        buf_len: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_regression_pipeline_from_json(
        json: *const c_char,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_regression_pipeline_destroy(
        handle: *mut c_void,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;

    fn rill_ml_classification_pipeline_new(
        feature_count: usize,
        learning_rate: f64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_classification_pipeline_predict_proba(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        out: *mut c_double,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_classification_pipeline_learn(
        handle: *mut c_void,
        features: *const c_double,
        feature_count: usize,
        target: i32,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_classification_pipeline_samples_seen(
        handle: *mut c_void,
        out: *mut u64,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_classification_pipeline_to_json(
        handle: *mut c_void,
        buf: *mut c_char,
        buf_len: usize,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
    fn rill_ml_classification_pipeline_from_json(
        json: *const c_char,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> *mut c_void;
    fn rill_ml_classification_pipeline_destroy(
        handle: *mut c_void,
        error_buf: *mut c_char,
        error_buf_len: usize,
    ) -> i32;
}

// --------------------------------------------------------------------------- //
// Test helpers
// --------------------------------------------------------------------------- //

const ERR_CAP: usize = 256;

/// Converts a `c_char` to its byte value.
///
/// `c_char` is `i8` on macOS and x86_64 Linux but `u8` on ARM/aarch64 Linux,
/// so the explicit cast is required for portability; clippy must not flag it.
#[allow(clippy::unnecessary_cast)]
fn c_char_to_u8(c: c_char) -> u8 {
    c as u8
}

/// A reusable error buffer for FFI calls.
struct ErrBuf {
    data: [c_char; ERR_CAP],
}

impl ErrBuf {
    fn new() -> Self {
        Self {
            data: [0 as c_char; ERR_CAP],
        }
    }

    fn ptr(&mut self) -> *mut c_char {
        self.data.as_mut_ptr()
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    /// Reads the NUL-terminated message back as a `String`.
    fn message(&self) -> String {
        let bytes: Vec<u8> = self
            .data
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c_char_to_u8(c))
            .collect();
        String::from_utf8(bytes).expect("FFI error messages are UTF-8")
    }
}

/// Reads a NUL-terminated string from an FFI output buffer.
fn read_c_string(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c_char_to_u8(c))
        .collect();
    String::from_utf8(bytes).expect("FFI output strings are UTF-8")
}

/// Asserts an FFI call succeeded and returns the error message buffer (empty).
fn expect_ok(code: i32, err: &ErrBuf) {
    assert_eq!(
        code,
        RILL_ML_OK,
        "expected RILL_ML_OK, got {code} with message: {}",
        err.message()
    );
}

/// Asserts an FFI call failed with the given code and wrote a non-empty
/// message into the error buffer.
fn expect_err(code: i32, expected: i32, err: &ErrBuf) {
    assert_eq!(code, expected, "got {code}, expected {expected}");
    let msg = err.message();
    assert!(
        !msg.is_empty(),
        "error path must write a human-readable message"
    );
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// --------------------------------------------------------------------------- //
// Version / format helpers
// --------------------------------------------------------------------------- //

#[test]
fn version_string_matches_crate_version() {
    let mut err = ErrBuf::new();
    let mut buf = [0 as c_char; 32];
    let code = unsafe { rill_ml_version(buf.as_mut_ptr(), buf.len(), err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert_eq!(read_c_string(&buf), env!("CARGO_PKG_VERSION"));
}

#[test]
fn snapshot_format_version_is_stable() {
    let version = unsafe { rill_ml_snapshot_format_version() };
    assert_eq!(version, 1);
}

// --------------------------------------------------------------------------- //
// Mean
// --------------------------------------------------------------------------- //

#[test]
fn mean_lifecycle() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_mean_new(err.ptr(), err.len()) };
    assert!(!handle.is_null(), "mean_new failed: {}", err.message());

    for v in [1.0, 2.0, 3.0, 4.0] {
        let code = unsafe { rill_ml_mean_update(handle, v, err.ptr(), err.len()) };
        expect_ok(code, &err);
    }

    let mut value = 0.0;
    let code = unsafe { rill_ml_mean_value(handle, &mut value, err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert!(approx(value, 2.5), "mean = {value}, expected 2.5");

    let mut count: u64 = 0;
    let code = unsafe { rill_ml_mean_count(handle, &mut count, err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert_eq!(count, 4);

    let code = unsafe { rill_ml_mean_destroy(handle, err.ptr(), err.len()) };
    expect_ok(code, &err);
}

#[test]
fn mean_roundtrip_snapshot() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_mean_new(err.ptr(), err.len()) };
    assert!(!handle.is_null());

    for v in [10.0, 20.0, 30.0] {
        let code = unsafe { rill_ml_mean_update(handle, v, err.ptr(), err.len()) };
        expect_ok(code, &err);
    }

    let mut json = [0 as c_char; 512];
    let code = unsafe {
        rill_ml_mean_to_json(handle, json.as_mut_ptr(), json.len(), err.ptr(), err.len())
    };
    expect_ok(code, &err);
    let snapshot = read_c_string(&json);
    assert!(
        snapshot.contains("format_version"),
        "snapshot has a version envelope"
    );

    let restored = unsafe { rill_ml_mean_from_json(json.as_ptr(), err.ptr(), err.len()) };
    assert!(
        !restored.is_null(),
        "mean_from_json failed: {}",
        err.message()
    );

    let mut value = 0.0;
    let code = unsafe { rill_ml_mean_value(restored, &mut value, err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert!(approx(value, 20.0));

    let mut count: u64 = 0;
    let code = unsafe { rill_ml_mean_count(restored, &mut count, err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert_eq!(count, 3);

    unsafe {
        rill_ml_mean_destroy(handle, err.ptr(), err.len());
        rill_ml_mean_destroy(restored, err.ptr(), err.len());
    }
}

// --------------------------------------------------------------------------- //
// LinearRegression
// --------------------------------------------------------------------------- //

#[test]
fn linear_regression_learns_and_predicts() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_linear_regression_new(1, 0.05, err.ptr(), err.len()) };
    assert!(
        !handle.is_null(),
        "linear_regression_new failed: {}",
        err.message()
    );

    let x = [2.0];
    for _ in 0..100 {
        let code = unsafe {
            rill_ml_linear_regression_learn(handle, x.as_ptr(), 1, 10.0, err.ptr(), err.len())
        };
        expect_ok(code, &err);
    }

    let mut pred = 0.0;
    let code = unsafe {
        rill_ml_linear_regression_predict(handle, x.as_ptr(), 1, &mut pred, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert!((pred - 10.0).abs() < 1.0, "prediction {pred} far from 10.0");

    let mut count: u64 = 0;
    let code =
        unsafe { rill_ml_linear_regression_samples_seen(handle, &mut count, err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert_eq!(count, 100);

    let mut intercept = 0.0;
    let code = unsafe {
        rill_ml_linear_regression_intercept(handle, &mut intercept, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert!(intercept.is_finite());

    // Weights query mode: out == NULL returns the required element count.
    let mut needed: usize = 0;
    let code = unsafe {
        rill_ml_linear_regression_weights(
            handle,
            ptr::null_mut(),
            &mut needed,
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);
    assert_eq!(needed, 1);

    // Copy mode.
    let mut weights = [0.0; 1];
    let code = unsafe {
        rill_ml_linear_regression_weights(
            handle,
            weights.as_mut_ptr(),
            &mut needed,
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);
    assert!(weights[0].is_finite());

    let code = unsafe { rill_ml_linear_regression_destroy(handle, err.ptr(), err.len()) };
    expect_ok(code, &err);
}

#[test]
fn linear_regression_roundtrip_snapshot() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_linear_regression_new(2, 0.1, err.ptr(), err.len()) };
    assert!(!handle.is_null());

    let x = [1.0, 1.0];
    for _ in 0..10 {
        let code = unsafe {
            rill_ml_linear_regression_learn(handle, x.as_ptr(), 2, 5.0, err.ptr(), err.len())
        };
        expect_ok(code, &err);
    }

    let mut json = [0 as c_char; 2048];
    let code = unsafe {
        rill_ml_linear_regression_to_json(
            handle,
            json.as_mut_ptr(),
            json.len(),
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);

    let restored =
        unsafe { rill_ml_linear_regression_from_json(json.as_ptr(), err.ptr(), err.len()) };
    assert!(
        !restored.is_null(),
        "linear_regression_from_json failed: {}",
        err.message()
    );

    let mut count: u64 = 0;
    let code = unsafe {
        rill_ml_linear_regression_samples_seen(restored, &mut count, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert_eq!(count, 10);

    unsafe {
        rill_ml_linear_regression_destroy(handle, err.ptr(), err.len());
        rill_ml_linear_regression_destroy(restored, err.ptr(), err.len());
    }
}

// --------------------------------------------------------------------------- //
// LogisticRegression
// --------------------------------------------------------------------------- //

#[test]
fn logistic_regression_learns_and_predicts() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_logistic_regression_new(2, 0.1, err.ptr(), err.len()) };
    assert!(
        !handle.is_null(),
        "logistic_regression_new failed: {}",
        err.message()
    );

    let pos = [1.0, 2.0];
    let neg = [-1.0, -2.0];
    for _ in 0..50 {
        let code = unsafe {
            rill_ml_logistic_regression_learn(handle, pos.as_ptr(), 2, 1, err.ptr(), err.len())
        };
        expect_ok(code, &err);
        let code = unsafe {
            rill_ml_logistic_regression_learn(handle, neg.as_ptr(), 2, 0, err.ptr(), err.len())
        };
        expect_ok(code, &err);
    }

    let mut proba = 0.0;
    let code = unsafe {
        rill_ml_logistic_regression_predict_proba(
            handle,
            pos.as_ptr(),
            2,
            &mut proba,
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);
    assert!((0.0..=1.0).contains(&proba), "proba {proba} outside [0, 1]");
    assert!(
        proba > 0.5,
        "positive sample should be classified as positive"
    );

    let mut class: i32 = -1;
    let code = unsafe {
        rill_ml_logistic_regression_predict(
            handle,
            pos.as_ptr(),
            2,
            &mut class,
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);
    assert_eq!(class, 1);

    let mut count: u64 = 0;
    let code = unsafe {
        rill_ml_logistic_regression_samples_seen(handle, &mut count, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert_eq!(count, 100);

    let code = unsafe { rill_ml_logistic_regression_destroy(handle, err.ptr(), err.len()) };
    expect_ok(code, &err);
}

#[test]
fn logistic_regression_roundtrip_snapshot() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_logistic_regression_new(1, 0.1, err.ptr(), err.len()) };
    assert!(!handle.is_null());

    let x = [1.0];
    for _ in 0..5 {
        let code = unsafe {
            rill_ml_logistic_regression_learn(handle, x.as_ptr(), 1, 1, err.ptr(), err.len())
        };
        expect_ok(code, &err);
    }

    let mut json = [0 as c_char; 2048];
    let code = unsafe {
        rill_ml_logistic_regression_to_json(
            handle,
            json.as_mut_ptr(),
            json.len(),
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);

    let restored =
        unsafe { rill_ml_logistic_regression_from_json(json.as_ptr(), err.ptr(), err.len()) };
    assert!(
        !restored.is_null(),
        "logistic_regression_from_json failed: {}",
        err.message()
    );

    let mut count: u64 = 0;
    let code = unsafe {
        rill_ml_logistic_regression_samples_seen(restored, &mut count, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert_eq!(count, 5);

    unsafe {
        rill_ml_logistic_regression_destroy(handle, err.ptr(), err.len());
        rill_ml_logistic_regression_destroy(restored, err.ptr(), err.len());
    }
}

// --------------------------------------------------------------------------- //
// StandardScaler
// --------------------------------------------------------------------------- //

#[test]
fn standard_scaler_updates_and_transforms() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_standard_scaler_new(2, err.ptr(), err.len()) };
    assert!(
        !handle.is_null(),
        "standard_scaler_new failed: {}",
        err.message()
    );

    let s1 = [1.0, 2.0];
    let s2 = [3.0, 4.0];
    let code =
        unsafe { rill_ml_standard_scaler_update(handle, s1.as_ptr(), 2, err.ptr(), err.len()) };
    expect_ok(code, &err);
    let code =
        unsafe { rill_ml_standard_scaler_update(handle, s2.as_ptr(), 2, err.ptr(), err.len()) };
    expect_ok(code, &err);

    let mut count: u64 = 0;
    let code =
        unsafe { rill_ml_standard_scaler_samples_seen(handle, &mut count, err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert_eq!(count, 2);

    let query = [2.0, 3.0];
    let mut out = [0.0; 2];
    let code = unsafe {
        rill_ml_standard_scaler_transform(
            handle,
            query.as_ptr(),
            2,
            out.as_mut_ptr(),
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);
    assert!(out.iter().all(|v| v.is_finite()));

    let code = unsafe { rill_ml_standard_scaler_destroy(handle, err.ptr(), err.len()) };
    expect_ok(code, &err);
}

#[test]
fn standard_scaler_roundtrip_snapshot() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_standard_scaler_new(2, err.ptr(), err.len()) };
    assert!(!handle.is_null());

    let s1 = [1.0, 2.0];
    let code =
        unsafe { rill_ml_standard_scaler_update(handle, s1.as_ptr(), 2, err.ptr(), err.len()) };
    expect_ok(code, &err);

    let mut json = [0 as c_char; 2048];
    let code = unsafe {
        rill_ml_standard_scaler_to_json(handle, json.as_mut_ptr(), json.len(), err.ptr(), err.len())
    };
    expect_ok(code, &err);

    let restored =
        unsafe { rill_ml_standard_scaler_from_json(json.as_ptr(), err.ptr(), err.len()) };
    assert!(
        !restored.is_null(),
        "standard_scaler_from_json failed: {}",
        err.message()
    );

    let mut count: u64 = 0;
    let code =
        unsafe { rill_ml_standard_scaler_samples_seen(restored, &mut count, err.ptr(), err.len()) };
    expect_ok(code, &err);
    assert_eq!(count, 1);

    unsafe {
        rill_ml_standard_scaler_destroy(handle, err.ptr(), err.len());
        rill_ml_standard_scaler_destroy(restored, err.ptr(), err.len());
    }
}

// --------------------------------------------------------------------------- //
// RegressionPipeline
// --------------------------------------------------------------------------- //

#[test]
fn regression_pipeline_learns_and_predicts() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_regression_pipeline_new(2, 0.05, err.ptr(), err.len()) };
    assert!(
        !handle.is_null(),
        "regression_pipeline_new failed: {}",
        err.message()
    );

    let x = [0.1, 0.2];
    for _ in 0..50 {
        let code = unsafe {
            rill_ml_regression_pipeline_learn(handle, x.as_ptr(), 2, 0.5, err.ptr(), err.len())
        };
        expect_ok(code, &err);
    }

    let mut pred = 0.0;
    let code = unsafe {
        rill_ml_regression_pipeline_predict(handle, x.as_ptr(), 2, &mut pred, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert!(pred.is_finite());

    let mut count: u64 = 0;
    let code = unsafe {
        rill_ml_regression_pipeline_samples_seen(handle, &mut count, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert_eq!(count, 50);

    let code = unsafe { rill_ml_regression_pipeline_destroy(handle, err.ptr(), err.len()) };
    expect_ok(code, &err);
}

#[test]
fn regression_pipeline_roundtrip_snapshot() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_regression_pipeline_new(2, 0.05, err.ptr(), err.len()) };
    assert!(!handle.is_null());

    let x = [0.1, 0.2];
    let code = unsafe {
        rill_ml_regression_pipeline_learn(handle, x.as_ptr(), 2, 0.5, err.ptr(), err.len())
    };
    expect_ok(code, &err);

    let mut json = [0 as c_char; 4096];
    let code = unsafe {
        rill_ml_regression_pipeline_to_json(
            handle,
            json.as_mut_ptr(),
            json.len(),
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);

    let restored =
        unsafe { rill_ml_regression_pipeline_from_json(json.as_ptr(), err.ptr(), err.len()) };
    assert!(
        !restored.is_null(),
        "regression_pipeline_from_json failed: {}",
        err.message()
    );

    let mut count: u64 = 0;
    let code = unsafe {
        rill_ml_regression_pipeline_samples_seen(restored, &mut count, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert_eq!(count, 1);

    unsafe {
        rill_ml_regression_pipeline_destroy(handle, err.ptr(), err.len());
        rill_ml_regression_pipeline_destroy(restored, err.ptr(), err.len());
    }
}

// --------------------------------------------------------------------------- //
// ClassificationPipeline
// --------------------------------------------------------------------------- //

#[test]
fn classification_pipeline_learns_and_predicts() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_classification_pipeline_new(2, 0.05, err.ptr(), err.len()) };
    assert!(
        !handle.is_null(),
        "classification_pipeline_new failed: {}",
        err.message()
    );

    let pos = [0.1, 0.2];
    let neg = [-0.1, -0.2];
    for _ in 0..50 {
        let code = unsafe {
            rill_ml_classification_pipeline_learn(handle, pos.as_ptr(), 2, 1, err.ptr(), err.len())
        };
        expect_ok(code, &err);
        let code = unsafe {
            rill_ml_classification_pipeline_learn(handle, neg.as_ptr(), 2, 0, err.ptr(), err.len())
        };
        expect_ok(code, &err);
    }

    let mut proba = 0.0;
    let code = unsafe {
        rill_ml_classification_pipeline_predict_proba(
            handle,
            pos.as_ptr(),
            2,
            &mut proba,
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);
    assert!((0.0..=1.0).contains(&proba));

    let mut count: u64 = 0;
    let code = unsafe {
        rill_ml_classification_pipeline_samples_seen(handle, &mut count, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert_eq!(count, 100);

    let code = unsafe { rill_ml_classification_pipeline_destroy(handle, err.ptr(), err.len()) };
    expect_ok(code, &err);
}

#[test]
fn classification_pipeline_roundtrip_snapshot() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_classification_pipeline_new(2, 0.05, err.ptr(), err.len()) };
    assert!(!handle.is_null());

    let x = [0.1, 0.2];
    let code = unsafe {
        rill_ml_classification_pipeline_learn(handle, x.as_ptr(), 2, 1, err.ptr(), err.len())
    };
    expect_ok(code, &err);

    let mut json = [0 as c_char; 4096];
    let code = unsafe {
        rill_ml_classification_pipeline_to_json(
            handle,
            json.as_mut_ptr(),
            json.len(),
            err.ptr(),
            err.len(),
        )
    };
    expect_ok(code, &err);

    let restored =
        unsafe { rill_ml_classification_pipeline_from_json(json.as_ptr(), err.ptr(), err.len()) };
    assert!(
        !restored.is_null(),
        "classification_pipeline_from_json failed: {}",
        err.message()
    );

    let mut count: u64 = 0;
    let code = unsafe {
        rill_ml_classification_pipeline_samples_seen(restored, &mut count, err.ptr(), err.len())
    };
    expect_ok(code, &err);
    assert_eq!(count, 1);

    unsafe {
        rill_ml_classification_pipeline_destroy(handle, err.ptr(), err.len());
        rill_ml_classification_pipeline_destroy(restored, err.ptr(), err.len());
    }
}

// --------------------------------------------------------------------------- //
// Error-code contract
// --------------------------------------------------------------------------- //

#[test]
fn null_handle_reports_invalid_handle() {
    let mut err = ErrBuf::new();
    let mut out = 0.0;
    let code = unsafe { rill_ml_mean_value(ptr::null_mut(), &mut out, err.ptr(), err.len()) };
    expect_err(code, RILL_ML_ERR_INVALID_HANDLE, &err);

    // destroy(NULL) must also be reported, not crash.
    let code = unsafe { rill_ml_mean_destroy(ptr::null_mut(), err.ptr(), err.len()) };
    expect_err(code, RILL_ML_ERR_INVALID_HANDLE, &err);
}

#[test]
fn undersized_output_buffer_reports_buffer_too_small() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_mean_new(err.ptr(), err.len()) };
    assert!(!handle.is_null());

    let code = unsafe { rill_ml_mean_update(handle, 1.0, err.ptr(), err.len()) };
    expect_ok(code, &err);

    // A 1-byte buffer cannot hold even the smallest snapshot + NUL.
    let mut tiny = [0 as c_char; 1];
    let code = unsafe {
        rill_ml_mean_to_json(handle, tiny.as_mut_ptr(), tiny.len(), err.ptr(), err.len())
    };
    expect_err(code, RILL_ML_ERR_BUFFER_TOO_SMALL, &err);

    // rill_ml_version with a tiny buffer reports BUFFER_TOO_SMALL too.
    let code = unsafe { rill_ml_version(tiny.as_mut_ptr(), tiny.len(), err.ptr(), err.len()) };
    expect_err(code, RILL_ML_ERR_BUFFER_TOO_SMALL, &err);

    unsafe { rill_ml_mean_destroy(handle, err.ptr(), err.len()) };
}

#[test]
fn invalid_binary_target_reports_invalid_argument() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_logistic_regression_new(1, 0.1, err.ptr(), err.len()) };
    assert!(!handle.is_null());

    let x = [1.0];
    let code = unsafe {
        rill_ml_logistic_regression_learn(handle, x.as_ptr(), 1, 2, err.ptr(), err.len())
    };
    expect_err(code, RILL_ML_ERR_INVALID_ARGUMENT, &err);

    unsafe { rill_ml_logistic_regression_destroy(handle, err.ptr(), err.len()) };
}

#[test]
fn malformed_json_is_rejected() {
    let mut err = ErrBuf::new();
    let json = b"not json\0";
    let restored =
        unsafe { rill_ml_mean_from_json(json.as_ptr() as *const c_char, err.ptr(), err.len()) };
    assert!(
        restored.is_null(),
        "malformed JSON must not restore a handle"
    );
    assert!(
        !err.message().is_empty(),
        "malformed JSON must write an error message"
    );
}

#[test]
fn feature_dimension_mismatch_is_rejected() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_linear_regression_new(2, 0.1, err.ptr(), err.len()) };
    assert!(!handle.is_null());

    // A 1-feature vector against a 2-feature model must fail (not panic).
    let short = [1.0];
    let mut out = 0.0;
    let code = unsafe {
        rill_ml_linear_regression_predict(handle, short.as_ptr(), 1, &mut out, err.ptr(), err.len())
    };
    assert_ne!(code, RILL_ML_OK, "dimension mismatch must fail");
    assert!(!err.message().is_empty());

    unsafe { rill_ml_linear_regression_destroy(handle, err.ptr(), err.len()) };
}

#[test]
fn create_with_zero_features_is_rejected() {
    let mut err = ErrBuf::new();
    let handle = unsafe { rill_ml_linear_regression_new(0, 0.1, err.ptr(), err.len()) };
    assert!(
        handle.is_null(),
        "feature_count 0 must fail to create a model"
    );
    assert!(!err.message().is_empty());
}

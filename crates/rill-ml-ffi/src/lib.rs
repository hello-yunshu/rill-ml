//! # rill-ml-ffi
//!
//! A C ABI-oriented FFI layer for [RillML](https://crates.io/crates/rill-ml),
//! the adaptive intelligence runtime for native and edge applications. This
//! crate is the bridge between the Stable Rust core (`rill-ml`) and native
//! consumers written in C, C++, and languages that bind to C — notably Android
//! (via JNI) and iOS (via Swift).
//!
//! **Status: Preview (0.x).** `rill-ml-ffi` is not part of the Stable 1.x ABI
//! freeze. The opaque-handle ABI is designed for future ABI stability, but the
//! exact symbol set, error codes, and header layout may still change within
//! 0.x. Only the Stable crates (`rill-ml`, `rill-handler-api`,
//! `rill-runtime-protocol`, `rill-runtime`) carry a 1.x compatibility promise.
//!
//! The ABI is deliberately small. Every model is exposed through an
//! **opaque handle** (`void *`), never through a `#[repr(C)]` struct, so Rust
//! internal layouts are free to evolve without breaking bindings. Model state
//! is persisted as a versioned JSON snapshot (`Snapshot`) through the
//! `to_json` / `from_json` entry points, which validates the format version
//! and model invariants on restore.
//!
//! # ABI contract
//!
//! ## Opaque handles and ownership
//!
//! - `rill_ml_<Type>_new(...)` returns an opaque handle (`void *`) owned by
//!   the caller. On failure it returns `NULL` and populates the error buffer.
//! - `rill_ml_<Type>_destroy(handle)` frees the handle. Every handle must be
//!   destroyed exactly once; using a handle after destroy is undefined
//!   behaviour. `destroy(NULL)` returns `RILL_ML_ERR_INVALID_HANDLE`.
//! - All memory management is internal to the library (the default Rust
//!   allocator). The caller only ever supplies output buffers with explicit
//!   lengths; the library never returns memory the caller must free.
//!
//! ## Error codes and messages
//!
//! Every fallible function returns an `int` error code and writes a
//! NUL-terminated error message into a caller-provided
//! `char *error_buf, size_t error_buf_len` pair (a zero-length buffer disables
//! messages). See the error-code constants in this module and
//! `include/rill_ml.h`.
//!
//! ## Thread safety
//!
//! Handles are **not** thread-safe. A single handle must not be shared
//! concurrently between threads. Distinct handles may be used from distinct
//! threads without synchronization.
//!
//! ## Panic policy
//!
//! Panics never cross the FFI boundary. Every entry point is wrapped in
//! `std::panic::catch_unwind`; a caught panic is reported as
//! `RILL_ML_ERR_PANIC` with a message. The library's public API returns
//! `Result` instead of panicking, so this is a last-resort guard.
//!
//! ## `unsafe` entry points
//!
//! Every `rill_ml_*` entry point is `unsafe extern "C"` because it
//! dereferences caller-provided raw pointers (handles, feature arrays,
//! buffers). The caller's obligations are exactly the ABI contract above:
//! valid handles with correct lifetime, writable output buffers of the stated
//! length, and non-NULL input pointers. Marking them `unsafe` makes Rust
//! consumers acknowledge that contract explicitly; C / JNI / Swift callers
//! are governed by `include/rill_ml.h`. The `# Safety` obligations are
//! documented here once, at module level, rather than repeated on all
//! ~50 entry points.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_double, c_int, c_void};
use std::panic::{UnwindSafe, catch_unwind};

use rill_ml::RillError;
use rill_ml::models::{
    LinearRegression, LinearRegressionConfig, LogisticRegression, LogisticRegressionConfig,
};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml::persistence::{SNAPSHOT_FORMAT_VERSION, Snapshot, ValidateState};
use rill_ml::pipeline::{ClassificationPipeline, RegressionPipeline};
use rill_ml::preprocessing::StandardScaler;
use rill_ml::stats::Mean;
use rill_ml::traits::{OnlineBinaryClassifier, OnlineRegressor, OnlineStatistic, Transformer};

// --------------------------------------------------------------------------- //
// Error codes
// --------------------------------------------------------------------------- //

/// Operation completed successfully.
pub const RILL_ML_OK: i32 = 0;
/// A caller-provided argument was invalid (bad dimension, non-finite value, ...).
pub const RILL_ML_ERR_INVALID_ARGUMENT: i32 = -1;
/// The model is in a state that cannot perform the requested operation.
pub const RILL_ML_ERR_INVALID_STATE: i32 = -2;
/// A panic was caught at the FFI boundary; state may be undefined.
pub const RILL_ML_ERR_PANIC: i32 = -3;
/// An internal I/O or serialization failure occurred.
pub const RILL_ML_ERR_IO: i32 = -4;
/// The supplied opaque handle was NULL / invalid.
pub const RILL_ML_ERR_INVALID_HANDLE: i32 = -5;
/// A caller-provided output buffer was too small.
pub const RILL_ML_ERR_BUFFER_TOO_SMALL: i32 = -6;

// --------------------------------------------------------------------------- //
// Internal helpers
// --------------------------------------------------------------------------- //

/// A fallible FFI operation result: an error code plus a human-readable message.
#[derive(Debug)]
struct FfiError {
    code: i32,
    message: String,
}

impl FfiError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Maps a [`RillError`] to an FFI error code, keeping the message for the
/// caller-facing error buffer.
fn map_rill_error(error: RillError) -> FfiError {
    let code = match &error {
        RillError::InvalidState(_)
        | RillError::InsufficientData
        | RillError::IncompatibleStateVersion { .. } => RILL_ML_ERR_INVALID_STATE,
        _ => RILL_ML_ERR_INVALID_ARGUMENT,
    };
    FfiError::new(code, error.to_string())
}

/// Wraps `value` into a boxed opaque handle (`*mut c_void`).
fn into_handle<T>(value: T) -> *mut c_void {
    Box::into_raw(Box::new(value)) as *mut c_void
}

/// Mutably borrows the pointee of an opaque handle.
///
/// The returned reference's lifetime is bound to the call site (inferred), not
/// fabricated as `'static`. Callers must not leak it beyond the enclosing FFI
/// export; in practice each export borrows, uses, and drops the reference
/// within a single closure, and the borrow checker ties the inferred lifetime
/// to that scope.
///
/// # Safety
/// `handle` must be either NULL or a pointer previously returned by
/// `into_handle::<T>()` that has not yet been destroyed. Passing a dangling
/// pointer, a pointer to the wrong type, or using the handle after the
/// matching destroy is undefined behaviour.
unsafe fn borrow<'a, T>(handle: *mut c_void) -> Result<&'a mut T, FfiError> {
    if handle.is_null() {
        Err(FfiError::new(RILL_ML_ERR_INVALID_HANDLE, "handle is NULL"))
    } else {
        // Safety: the caller's obligation (see the docs above) guarantees
        // `handle` points at a live `T` that has not yet been destroyed.
        Ok(unsafe { &mut *(handle as *mut T) })
    }
}

/// Writes a NUL-terminated UTF-8 message into a caller-provided error buffer.
///
/// A NULL buffer or a zero length disables the message. Returns `code` for
/// convenient chaining.
///
/// # Safety
/// `error_buf` must be NULL or point to a writable region of at least
/// `error_buf_len` bytes.
unsafe fn write_error(
    error_buf: *mut c_char,
    error_buf_len: usize,
    code: i32,
    message: &str,
) -> i32 {
    if !error_buf.is_null() && error_buf_len > 0 {
        let bytes = message.as_bytes();
        let n = bytes.len().min(error_buf_len - 1);
        // Safety: `error_buf` is a writable region of at least `error_buf_len`
        // bytes (caller obligation), and `n < error_buf_len` is guaranteed by
        // the `min` above.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, error_buf, n);
            *error_buf.add(n) = 0;
        }
    }
    code
}

/// Copies a NUL-terminated UTF-8 string into a caller-provided output buffer.
///
/// # Safety
/// `buf` must be NULL or point to a writable region of at least `buf_len`
/// bytes.
unsafe fn copy_string_into(buf: *mut c_char, buf_len: usize, value: &str) -> Result<(), FfiError> {
    if buf.is_null() || buf_len == 0 {
        return Err(FfiError::new(
            RILL_ML_ERR_INVALID_ARGUMENT,
            "output buffer must be non-NULL with a positive length",
        ));
    }
    let bytes = value.as_bytes();
    if bytes.len() >= buf_len {
        return Err(FfiError::new(
            RILL_ML_ERR_BUFFER_TOO_SMALL,
            format!(
                "output buffer too small: need {} bytes (including NUL), have {buf_len}",
                bytes.len() + 1
            ),
        ));
    }
    // Safety: `buf` is a writable region of at least `buf_len` bytes (caller
    // obligation), and `bytes.len() < buf_len` was checked above.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, bytes.len());
        *buf.add(bytes.len()) = 0;
    }
    Ok(())
}

/// Reads a NUL-terminated C string into a Rust `String`.
///
/// # Safety
/// `ptr` must be NULL or point to a valid NUL-terminated UTF-8 string.
unsafe fn read_c_string(ptr: *const c_char) -> Result<String, FfiError> {
    if ptr.is_null() {
        return Err(FfiError::new(
            RILL_ML_ERR_INVALID_ARGUMENT,
            "input string must not be NULL",
        ));
    }
    let mut len = 0usize;
    // Safety: `ptr` points at a valid NUL-terminated string (caller obligation).
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
    }
    // Safety: the NUL scan above bounded `len`; the bytes are readable.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        FfiError::new(
            RILL_ML_ERR_INVALID_ARGUMENT,
            "input string is not valid UTF-8",
        )
    })
}

/// Reads a contiguous array of doubles into a `Vec`.
///
/// # Safety
/// `ptr` must be NULL or point to at least `len` contiguous `double` values.
unsafe fn read_features(ptr: *const c_double, len: usize) -> Result<Vec<f64>, FfiError> {
    if ptr.is_null() {
        return Err(FfiError::new(
            RILL_ML_ERR_INVALID_ARGUMENT,
            "features pointer must not be NULL",
        ));
    }
    // Safety: `ptr` points at at least `len` contiguous doubles (caller
    // obligation).
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec())
}

/// Copies `weights` into a caller buffer, or queries the required element
/// count when `out` is NULL.
///
/// # Safety
/// `out` must be NULL or point to a writable region of at least `*out_len`
/// doubles; `out_len` must be non-NULL.
unsafe fn copy_weights(
    weights: &[f64],
    out: *mut c_double,
    out_len: *mut usize,
) -> Result<(), FfiError> {
    if out_len.is_null() {
        return Err(FfiError::new(
            RILL_ML_ERR_INVALID_ARGUMENT,
            "out_len must not be NULL",
        ));
    }
    let n = weights.len();
    if out.is_null() {
        // Query mode: report how many elements are needed.
        // Safety: `out_len` is non-NULL (checked above).
        unsafe { *out_len = n };
        return Ok(());
    }
    // Safety: `out_len` is non-NULL (checked above).
    let capacity = unsafe { *out_len };
    if capacity < n {
        return Err(FfiError::new(
            RILL_ML_ERR_BUFFER_TOO_SMALL,
            format!("weights buffer too small: need {n} elements, have {capacity}"),
        ));
    }
    // Safety: `out` points at a writable region of at least `capacity`
    // doubles and `n <= capacity` was checked above; `out_len` is non-NULL.
    unsafe {
        std::ptr::copy_nonoverlapping(weights.as_ptr(), out, n);
        *out_len = n;
    }
    Ok(())
}

/// Runs a fallible operation, converting panics into `RILL_ML_ERR_PANIC`.
fn run<F>(op: F, error_buf: *mut c_char, error_buf_len: usize) -> i32
where
    F: FnOnce() -> Result<i32, FfiError> + UnwindSafe,
{
    match catch_unwind(op) {
        Ok(Ok(code)) => code,
        Ok(Err(error)) => unsafe {
            write_error(error_buf, error_buf_len, error.code, &error.message)
        },
        Err(_) => unsafe {
            write_error(
                error_buf,
                error_buf_len,
                RILL_ML_ERR_PANIC,
                "panic unwound across the rill-ml-ffi boundary",
            )
        },
    }
}

/// Runs a fallible constructor, converting panics into a NULL handle plus
/// `RILL_ML_ERR_PANIC`.
fn run_ptr<F>(op: F, error_buf: *mut c_char, error_buf_len: usize) -> *mut c_void
where
    F: FnOnce() -> Result<*mut c_void, FfiError> + UnwindSafe,
{
    match catch_unwind(op) {
        Ok(Ok(handle)) => handle,
        Ok(Err(error)) => {
            unsafe { write_error(error_buf, error_buf_len, error.code, &error.message) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                write_error(
                    error_buf,
                    error_buf_len,
                    RILL_ML_ERR_PANIC,
                    "panic unwound across the rill-ml-ffi boundary",
                )
            };
            std::ptr::null_mut()
        }
    }
}

/// Serializes `value` inside a versioned [`Snapshot`] envelope into a caller
/// buffer.
///
/// # Safety
/// See [`copy_string_into`].
unsafe fn snapshot_to_buffer<T>(value: &T, buf: *mut c_char, buf_len: usize) -> Result<(), FfiError>
where
    T: Clone + serde::Serialize,
{
    let snapshot = Snapshot::new(value.clone());
    let json = serde_json::to_string(&snapshot).map_err(|error| {
        FfiError::new(
            RILL_ML_ERR_IO,
            format!("failed to serialize model state: {error}"),
        )
    })?;
    // Safety: see `copy_string_into`; `buf`/`buf_len` are caller-provided.
    unsafe { copy_string_into(buf, buf_len, &json) }
}

/// Restores a model from a validated JSON snapshot.
fn snapshot_from_str<T>(json: &str) -> Result<T, FfiError>
where
    T: serde::de::DeserializeOwned + ValidateState,
{
    Snapshot::<T>::from_json_validated(json).map_err(map_rill_error)
}

// --------------------------------------------------------------------------- //
// Version / format helpers
// --------------------------------------------------------------------------- //

/// Writes the `rill-ml-ffi` crate version string (e.g. `0.15.0`) into `buf`.
///
/// Returns `RILL_ML_OK` on success or `RILL_ML_ERR_BUFFER_TOO_SMALL` when
/// `buf_len` cannot hold the version string including its NUL terminator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_version(
    buf: *mut c_char,
    buf_len: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            unsafe { copy_string_into(buf, buf_len, env!("CARGO_PKG_VERSION")) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Returns the snapshot format version used by the `to_json`/`from_json`
/// entry points. Infallible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_snapshot_format_version() -> i32 {
    SNAPSHOT_FORMAT_VERSION as i32
}

// --------------------------------------------------------------------------- //
// Mean
// --------------------------------------------------------------------------- //

/// Creates a new online [`Mean`] accumulator.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_mean_new(
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(|| Ok(into_handle(Mean::new())), error_buf, error_buf_len)
}

/// Updates the [`Mean`] with one observation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_mean_update(
    handle: *mut c_void,
    value: f64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let mean: &mut Mean = unsafe { borrow(handle)? };
            mean.update(value).map_err(map_rill_error)?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the current [`Mean::value`] into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_mean_value(
    handle: *mut c_void,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let mean: &Mean = unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = mean.value() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the number of observations seen so far into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_mean_count(
    handle: *mut c_void,
    out: *mut u64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let mean: &Mean = unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = mean.count() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Serializes the [`Mean`] as a versioned JSON snapshot into `buf`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_mean_to_json(
    handle: *mut c_void,
    buf: *mut c_char,
    buf_len: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let mean: &Mean = unsafe { borrow(handle)? };
            unsafe { snapshot_to_buffer(mean, buf, buf_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Restores a [`Mean`] from a validated JSON snapshot.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_mean_from_json(
    json: *const c_char,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let json_str = unsafe { read_c_string(json)? };
            let model = snapshot_from_str::<Mean>(&json_str)?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Destroys a [`Mean`] handle. The handle must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_mean_destroy(
    handle: *mut c_void,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            if handle.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_HANDLE,
                    "handle is NULL; nothing to destroy",
                ));
            }
            drop(unsafe { Box::from_raw(handle as *mut Mean) });
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

// --------------------------------------------------------------------------- //
// Shared linear-model helpers
// --------------------------------------------------------------------------- //

/// Builds an SGD optimizer for `feature_count` features with the given
/// learning rate and no L2 regularization.
fn sgd_optimizer(feature_count: usize, learning_rate: f64) -> Result<Optimizer, RillError> {
    let mut config = SgdConfig::default();
    config.learning_rate = learning_rate;
    config.l2 = 0.0;
    Optimizer::sgd(feature_count, config)
}

/// Reads the target byte used by the [`OnlineBinaryClassifier`] learn path.
fn parse_binary_target(target: i32) -> Result<bool, FfiError> {
    match target {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(FfiError::new(
            RILL_ML_ERR_INVALID_ARGUMENT,
            "binary target must be 0 or 1",
        )),
    }
}

/// Reads a features slice and validates it is non-empty (dimension checks
/// against the model happen inside the core library).
fn read_features_arg(
    features: *const c_double,
    feature_count: usize,
) -> Result<Vec<f64>, FfiError> {
    unsafe { read_features(features, feature_count) }
}

// --------------------------------------------------------------------------- //
// LinearRegression
// --------------------------------------------------------------------------- //

/// Creates a new [`LinearRegression`] with SGD and the given learning rate.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure
/// (e.g. `feature_count == 0` or a non-positive learning rate).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_new(
    feature_count: usize,
    learning_rate: f64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let optimizer = sgd_optimizer(feature_count, learning_rate).map_err(map_rill_error)?;
            let mut config = LinearRegressionConfig::default();
            config.optimizer = optimizer;
            let model = LinearRegression::new(feature_count, config).map_err(map_rill_error)?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Predicts `y` for `feature_count` doubles in `features` and writes it to
/// `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_predict(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LinearRegression = unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            let prediction = model.predict(&features).map_err(map_rill_error)?;
            unsafe { *out = prediction };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Learns one labeled sample `(features, target)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_learn(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    target: f64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &mut LinearRegression = unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            model.learn(&features, target).map_err(map_rill_error)?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Copies the learned weights into `out` (or queries the required count when
/// `out` is NULL, writing it to `*out_len`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_weights(
    handle: *mut c_void,
    out: *mut c_double,
    out_len: *mut usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LinearRegression = unsafe { borrow(handle)? };
            unsafe { copy_weights(model.weights(), out, out_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the learned intercept into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_intercept(
    handle: *mut c_void,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LinearRegression = unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = model.intercept() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the number of learned samples into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_samples_seen(
    handle: *mut c_void,
    out: *mut u64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LinearRegression = unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = model.samples_seen() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Serializes the [`LinearRegression`] as a versioned JSON snapshot into `buf`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_to_json(
    handle: *mut c_void,
    buf: *mut c_char,
    buf_len: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LinearRegression = unsafe { borrow(handle)? };
            unsafe { snapshot_to_buffer(model, buf, buf_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Restores a [`LinearRegression`] from a validated JSON snapshot.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_from_json(
    json: *const c_char,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let json_str = unsafe { read_c_string(json)? };
            let model = snapshot_from_str::<LinearRegression>(&json_str)?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Destroys a [`LinearRegression`] handle. The handle must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_linear_regression_destroy(
    handle: *mut c_void,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            if handle.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_HANDLE,
                    "handle is NULL; nothing to destroy",
                ));
            }
            drop(unsafe { Box::from_raw(handle as *mut LinearRegression) });
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

// --------------------------------------------------------------------------- //
// LogisticRegression
// --------------------------------------------------------------------------- //

/// Creates a new [`LogisticRegression`] with SGD and the given learning rate.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_new(
    feature_count: usize,
    learning_rate: f64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let optimizer = sgd_optimizer(feature_count, learning_rate).map_err(map_rill_error)?;
            let mut config = LogisticRegressionConfig::default();
            config.optimizer = optimizer;
            let model = LogisticRegression::new(feature_count, config).map_err(map_rill_error)?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Predicts `P(y = 1 | x)` for `feature_count` doubles in `features` and
/// writes it to `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_predict_proba(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LogisticRegression = unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            let probability = model.predict_proba(&features).map_err(map_rill_error)?;
            unsafe { *out = probability };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Predicts the class label (0 or 1, via a 0.5 probability threshold) into
/// `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_predict(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    out: *mut c_int,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LogisticRegression = unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            let class = model.predict(&features).map_err(map_rill_error)?;
            unsafe { *out = if class { 1 } else { 0 } };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Learns one labeled sample `(features, target)` where `target` is 0 or 1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_learn(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    target: i32,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &mut LogisticRegression = unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            let target = parse_binary_target(target)?;
            model.learn(&features, target).map_err(map_rill_error)?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Copies the learned weights into `out` (or queries the required count when
/// `out` is NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_weights(
    handle: *mut c_void,
    out: *mut c_double,
    out_len: *mut usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LogisticRegression = unsafe { borrow(handle)? };
            unsafe { copy_weights(model.weights(), out, out_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the learned intercept into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_intercept(
    handle: *mut c_void,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LogisticRegression = unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = model.intercept() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the number of learned samples into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_samples_seen(
    handle: *mut c_void,
    out: *mut u64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LogisticRegression = unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = model.samples_seen() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Serializes the [`LogisticRegression`] as a versioned JSON snapshot into
/// `buf`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_to_json(
    handle: *mut c_void,
    buf: *mut c_char,
    buf_len: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let model: &LogisticRegression = unsafe { borrow(handle)? };
            unsafe { snapshot_to_buffer(model, buf, buf_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Restores a [`LogisticRegression`] from a validated JSON snapshot.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_from_json(
    json: *const c_char,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let json_str = unsafe { read_c_string(json)? };
            let model = snapshot_from_str::<LogisticRegression>(&json_str)?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Destroys a [`LogisticRegression`] handle. The handle must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_logistic_regression_destroy(
    handle: *mut c_void,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            if handle.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_HANDLE,
                    "handle is NULL; nothing to destroy",
                ));
            }
            drop(unsafe { Box::from_raw(handle as *mut LogisticRegression) });
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

// --------------------------------------------------------------------------- //
// StandardScaler
// --------------------------------------------------------------------------- //

/// Creates a new [`StandardScaler`] for `feature_count` features.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure
/// (e.g. `feature_count == 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_standard_scaler_new(
    feature_count: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let scaler = StandardScaler::new(feature_count).map_err(map_rill_error)?;
            Ok(into_handle(scaler))
        },
        error_buf,
        error_buf_len,
    )
}

/// Updates the scaler's running per-feature statistics with one raw sample.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_standard_scaler_update(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let scaler: &mut StandardScaler = unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            scaler.update(&features).map_err(map_rill_error)?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Standardizes `feature_count` doubles from `features` and writes the result
/// into `out` (which must hold at least `feature_count` doubles).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_standard_scaler_transform(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let scaler: &StandardScaler = unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            let transformed = scaler.transform(&features).map_err(map_rill_error)?;
            // Safety: `out` is non-NULL and the caller must provide space for
            // `feature_count` doubles (the transform output has that length).
            unsafe { std::ptr::copy_nonoverlapping(transformed.as_ptr(), out, transformed.len()) };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the number of samples seen by the scaler into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_standard_scaler_samples_seen(
    handle: *mut c_void,
    out: *mut u64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let scaler: &StandardScaler = unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = scaler.samples_seen() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Serializes the [`StandardScaler`] as a versioned JSON snapshot into `buf`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_standard_scaler_to_json(
    handle: *mut c_void,
    buf: *mut c_char,
    buf_len: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let scaler: &StandardScaler = unsafe { borrow(handle)? };
            unsafe { snapshot_to_buffer(scaler, buf, buf_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Restores a [`StandardScaler`] from a validated JSON snapshot.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_standard_scaler_from_json(
    json: *const c_char,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let json_str = unsafe { read_c_string(json)? };
            let model = snapshot_from_str::<StandardScaler>(&json_str)?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Destroys a [`StandardScaler`] handle. The handle must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_standard_scaler_destroy(
    handle: *mut c_void,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            if handle.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_HANDLE,
                    "handle is NULL; nothing to destroy",
                ));
            }
            drop(unsafe { Box::from_raw(handle as *mut StandardScaler) });
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

// --------------------------------------------------------------------------- //
// RegressionPipeline (StandardScaler + LinearRegression)
// --------------------------------------------------------------------------- //

/// Creates a new regression pipeline: [`StandardScaler`] then
/// [`LinearRegression`], with SGD and the given learning rate.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_regression_pipeline_new(
    feature_count: usize,
    learning_rate: f64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let scaler = StandardScaler::new(feature_count).map_err(map_rill_error)?;
            let optimizer = sgd_optimizer(feature_count, learning_rate).map_err(map_rill_error)?;
            let mut config = LinearRegressionConfig::default();
            config.optimizer = optimizer;
            let model = LinearRegression::new(feature_count, config).map_err(map_rill_error)?;
            let pipeline = RegressionPipeline::new(scaler, model).map_err(map_rill_error)?;
            Ok(into_handle(pipeline))
        },
        error_buf,
        error_buf_len,
    )
}

/// Predicts `y` for `feature_count` doubles in `features` and writes it to
/// `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_regression_pipeline_predict(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &RegressionPipeline<StandardScaler, LinearRegression> =
                unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            let prediction = pipeline.predict(&features).map_err(map_rill_error)?;
            unsafe { *out = prediction };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Learns one labeled sample `(features, target)` through the pipeline.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_regression_pipeline_learn(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    target: f64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &mut RegressionPipeline<StandardScaler, LinearRegression> =
                unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            pipeline.learn(&features, target).map_err(map_rill_error)?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the number of samples seen by the pipeline into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_regression_pipeline_samples_seen(
    handle: *mut c_void,
    out: *mut u64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &RegressionPipeline<StandardScaler, LinearRegression> =
                unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = pipeline.samples_seen() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Serializes the regression pipeline as a versioned JSON snapshot into `buf`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_regression_pipeline_to_json(
    handle: *mut c_void,
    buf: *mut c_char,
    buf_len: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &RegressionPipeline<StandardScaler, LinearRegression> =
                unsafe { borrow(handle)? };
            unsafe { snapshot_to_buffer(pipeline, buf, buf_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Restores a regression pipeline from a validated JSON snapshot.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_regression_pipeline_from_json(
    json: *const c_char,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let json_str = unsafe { read_c_string(json)? };
            let model = snapshot_from_str::<RegressionPipeline<StandardScaler, LinearRegression>>(
                &json_str,
            )?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Destroys a regression pipeline handle. The handle must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_regression_pipeline_destroy(
    handle: *mut c_void,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            if handle.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_HANDLE,
                    "handle is NULL; nothing to destroy",
                ));
            }
            drop(unsafe {
                Box::from_raw(handle as *mut RegressionPipeline<StandardScaler, LinearRegression>)
            });
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

// --------------------------------------------------------------------------- //
// ClassificationPipeline (StandardScaler + LogisticRegression)
// --------------------------------------------------------------------------- //

/// Creates a new classification pipeline: [`StandardScaler`] then
/// [`LogisticRegression`], with SGD and the given learning rate.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_classification_pipeline_new(
    feature_count: usize,
    learning_rate: f64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let scaler = StandardScaler::new(feature_count).map_err(map_rill_error)?;
            let optimizer = sgd_optimizer(feature_count, learning_rate).map_err(map_rill_error)?;
            let mut config = LogisticRegressionConfig::default();
            config.optimizer = optimizer;
            let model = LogisticRegression::new(feature_count, config).map_err(map_rill_error)?;
            let pipeline = ClassificationPipeline::new(scaler, model).map_err(map_rill_error)?;
            Ok(into_handle(pipeline))
        },
        error_buf,
        error_buf_len,
    )
}

/// Predicts `P(y = 1 | x)` for `feature_count` doubles in `features` and
/// writes it to `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_classification_pipeline_predict_proba(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    out: *mut c_double,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &ClassificationPipeline<StandardScaler, LogisticRegression> =
                unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            let probability = pipeline.predict_proba(&features).map_err(map_rill_error)?;
            unsafe { *out = probability };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Learns one labeled sample `(features, target)` through the pipeline, where
/// `target` is 0 or 1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_classification_pipeline_learn(
    handle: *mut c_void,
    features: *const c_double,
    feature_count: usize,
    target: i32,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &mut ClassificationPipeline<StandardScaler, LogisticRegression> =
                unsafe { borrow(handle)? };
            let features = read_features_arg(features, feature_count)?;
            let target = parse_binary_target(target)?;
            pipeline.learn(&features, target).map_err(map_rill_error)?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Writes the number of samples seen by the pipeline into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_classification_pipeline_samples_seen(
    handle: *mut c_void,
    out: *mut u64,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &ClassificationPipeline<StandardScaler, LogisticRegression> =
                unsafe { borrow(handle)? };
            if out.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_ARGUMENT,
                    "out must not be NULL",
                ));
            }
            unsafe { *out = pipeline.samples_seen() };
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Serializes the classification pipeline as a versioned JSON snapshot into
/// `buf`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_classification_pipeline_to_json(
    handle: *mut c_void,
    buf: *mut c_char,
    buf_len: usize,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            let pipeline: &ClassificationPipeline<StandardScaler, LogisticRegression> =
                unsafe { borrow(handle)? };
            unsafe { snapshot_to_buffer(pipeline, buf, buf_len) }?;
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

/// Restores a classification pipeline from a validated JSON snapshot.
///
/// Returns an opaque handle owned by the caller, or `NULL` on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_classification_pipeline_from_json(
    json: *const c_char,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> *mut c_void {
    run_ptr(
        || {
            let json_str = unsafe { read_c_string(json)? };
            let model = snapshot_from_str::<
                ClassificationPipeline<StandardScaler, LogisticRegression>,
            >(&json_str)?;
            Ok(into_handle(model))
        },
        error_buf,
        error_buf_len,
    )
}

/// Destroys a classification pipeline handle. The handle must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_ml_classification_pipeline_destroy(
    handle: *mut c_void,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    run(
        || {
            if handle.is_null() {
                return Err(FfiError::new(
                    RILL_ML_ERR_INVALID_HANDLE,
                    "handle is NULL; nothing to destroy",
                ));
            }
            drop(unsafe {
                Box::from_raw(
                    handle as *mut ClassificationPipeline<StandardScaler, LogisticRegression>,
                )
            });
            Ok(RILL_ML_OK)
        },
        error_buf,
        error_buf_len,
    )
}

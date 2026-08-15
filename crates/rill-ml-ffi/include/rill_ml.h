/*
 * rill_ml.h — Preview C ABI for RillML (opaque-handle FFI).
 *
 * RillML is a lightweight, serializable online machine learning library.
 * This header declares the opaque-handle C ABI implemented by the
 * `rill-ml-ffi` crate (see crates/rill-ml-ffi). It is the contract that
 * Android (JNI) and iOS (Swift) bindings compile against.
 *
 * Status: Preview (0.x). rill-ml-ffi is not part of the Stable 1.x ABI
 * freeze. The symbol set, error codes, and header layout may still change
 * within 0.x. Only the Stable crates (rill-ml, rill-handler-api,
 * rill-runtime-protocol, rill-runtime) carry a 1.x compatibility promise.
 *
 * ABI contract
 * ------------
 *
 * 1. Opaque handles. Every model is exposed as a `void *` handle returned by
 *    a `rill_ml_<Type>_new` (or `_from_json`) function. The caller owns the
 *    handle and must release it exactly once with `rill_ml_<Type>_destroy`.
 *    Using a handle after destroy is undefined behaviour.
 *
 * 2. Error codes. Every fallible function returns an `int` error code
 *    (see the RILL_ML_* constants below; `RILL_ML_OK` means success) and
 *    writes a NUL-terminated error message into a caller-provided
 *    `char *error_buf, size_t error_buf_len` pair. Passing a NULL buffer or a
 *    zero length disables the message.
 *
 * 3. Output buffers. The library never allocates memory the caller must
 *    free; all output goes into caller-provided buffers with explicit
 *    lengths. The `_weights` functions support a query mode: pass `out ==
 *    NULL` to receive the required element count in `*out_len`.
 *
 * 4. Thread safety. Handles are NOT thread-safe. A single handle must not
 *    be shared concurrently between threads; distinct handles may be used
 *    from distinct threads without synchronization.
 *
 * 5. Panic policy. Panics never cross the FFI boundary. A caught panic is
 *    reported as `RILL_ML_ERR_PANIC` with a message.
 *
 * 6. Versioning. `rill_ml_version` returns the crate version string and
 *    `rill_ml_snapshot_format_version` returns the snapshot format version
 *    used by the `_to_json` / `_from_json` entry points.
 */

#ifndef RILL_ML_H
#define RILL_ML_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Error codes                                                         */
/* ------------------------------------------------------------------ */

/** Operation completed successfully. */
#define RILL_ML_OK 0
/** A caller-provided argument was invalid (bad dimension, non-finite value, ...). */
#define RILL_ML_ERR_INVALID_ARGUMENT (-1)
/** The model is in a state that cannot perform the requested operation. */
#define RILL_ML_ERR_INVALID_STATE (-2)
/** A panic was caught at the FFI boundary; state may be undefined. */
#define RILL_ML_ERR_PANIC (-3)
/** An internal I/O or serialization failure occurred. */
#define RILL_ML_ERR_IO (-4)
/** The supplied opaque handle was NULL / invalid. */
#define RILL_ML_ERR_INVALID_HANDLE (-5)
/** A caller-provided output buffer was too small. */
#define RILL_ML_ERR_BUFFER_TOO_SMALL (-6)

/* ------------------------------------------------------------------ */
/* Version / format helpers                                            */
/* ------------------------------------------------------------------ */

/**
 * Writes the `rill-ml-ffi` crate version string (e.g. "0.15.0") into `buf`.
 * Returns RILL_ML_OK on success or RILL_ML_ERR_BUFFER_TOO_SMALL when `buf_len`
 * cannot hold the version string including its NUL terminator.
 */
int rill_ml_version(char *buf, size_t buf_len, char *error_buf, size_t error_buf_len);

/**
 * Returns the snapshot format version used by the `_to_json` / `_from_json`
 * entry points. Infallible.
 */
int rill_ml_snapshot_format_version(void);

/* ------------------------------------------------------------------ */
/* Mean (online mean accumulator)                                      */
/* ------------------------------------------------------------------ */

/** Creates a new online mean accumulator. Returns an owned handle or NULL. */
void *rill_ml_mean_new(char *error_buf, size_t error_buf_len);

/** Updates the mean with one observation `value`. */
int rill_ml_mean_update(void *handle, double value, char *error_buf, size_t error_buf_len);

/** Writes the current mean value into `*out`. */
int rill_ml_mean_value(void *handle, double *out, char *error_buf, size_t error_buf_len);

/** Writes the number of observations seen into `*out`. */
int rill_ml_mean_count(void *handle, uint64_t *out, char *error_buf, size_t error_buf_len);

/** Serializes the mean as a versioned JSON snapshot into `buf`. */
int rill_ml_mean_to_json(void *handle, char *buf, size_t buf_len, char *error_buf, size_t error_buf_len);

/** Restores a mean from a validated JSON snapshot. Returns an owned handle or NULL. */
void *rill_ml_mean_from_json(const char *json, char *error_buf, size_t error_buf_len);

/** Destroys a mean handle. The handle must not be used afterwards. */
int rill_ml_mean_destroy(void *handle, char *error_buf, size_t error_buf_len);

/* ------------------------------------------------------------------ */
/* LinearRegression (online linear regression)                         */
/* ------------------------------------------------------------------ */

/**
 * Creates a new linear regression with SGD and the given learning rate.
 * Returns an owned handle or NULL on failure (e.g. `feature_count == 0` or
 * a non-positive learning rate).
 */
void *rill_ml_linear_regression_new(size_t feature_count, double learning_rate,
                                    char *error_buf, size_t error_buf_len);

/**
 * Predicts `y` for `feature_count` doubles in `features` and writes it to
 * `*out`.
 */
int rill_ml_linear_regression_predict(void *handle, const double *features, size_t feature_count,
                                      double *out, char *error_buf, size_t error_buf_len);

/** Learns one labeled sample `(features, target)`. */
int rill_ml_linear_regression_learn(void *handle, const double *features, size_t feature_count,
                                    double target, char *error_buf, size_t error_buf_len);

/**
 * Copies the learned weights into `out`. When `out` is NULL, writes the
 * required element count to `*out_len` (query mode). Returns
 * RILL_ML_ERR_BUFFER_TOO_SMALL if `*out_len` is too small.
 */
int rill_ml_linear_regression_weights(void *handle, double *out, size_t *out_len,
                                      char *error_buf, size_t error_buf_len);

/** Writes the learned intercept into `*out`. */
int rill_ml_linear_regression_intercept(void *handle, double *out,
                                        char *error_buf, size_t error_buf_len);

/** Writes the number of learned samples into `*out`. */
int rill_ml_linear_regression_samples_seen(void *handle, uint64_t *out,
                                           char *error_buf, size_t error_buf_len);

/** Serializes the model as a versioned JSON snapshot into `buf`. */
int rill_ml_linear_regression_to_json(void *handle, char *buf, size_t buf_len,
                                      char *error_buf, size_t error_buf_len);

/** Restores a model from a validated JSON snapshot. Returns an owned handle or NULL. */
void *rill_ml_linear_regression_from_json(const char *json, char *error_buf, size_t error_buf_len);

/** Destroys a model handle. The handle must not be used afterwards. */
int rill_ml_linear_regression_destroy(void *handle, char *error_buf, size_t error_buf_len);

/* ------------------------------------------------------------------ */
/* LogisticRegression (online binary logistic regression)              */
/* ------------------------------------------------------------------ */

/** Creates a new logistic regression with SGD and the given learning rate. Returns an owned handle or NULL. */
void *rill_ml_logistic_regression_new(size_t feature_count, double learning_rate,
                                      char *error_buf, size_t error_buf_len);

/** Predicts `P(y = 1 | x)` for `feature_count` doubles in `features` and writes it to `*out`. */
int rill_ml_logistic_regression_predict_proba(void *handle, const double *features,
                                              size_t feature_count, double *out,
                                              char *error_buf, size_t error_buf_len);

/** Predicts the class label (0 or 1, via a 0.5 probability threshold) into `*out`. */
int rill_ml_logistic_regression_predict(void *handle, const double *features, size_t feature_count,
                                        int *out, char *error_buf, size_t error_buf_len);

/** Learns one labeled sample `(features, target)` where `target` is 0 or 1. */
int rill_ml_logistic_regression_learn(void *handle, const double *features, size_t feature_count,
                                      int target, char *error_buf, size_t error_buf_len);

/** Copies the learned weights into `out` (or queries the count when `out` is NULL). */
int rill_ml_logistic_regression_weights(void *handle, double *out, size_t *out_len,
                                        char *error_buf, size_t error_buf_len);

/** Writes the learned intercept into `*out`. */
int rill_ml_logistic_regression_intercept(void *handle, double *out,
                                          char *error_buf, size_t error_buf_len);

/** Writes the number of learned samples into `*out`. */
int rill_ml_logistic_regression_samples_seen(void *handle, uint64_t *out,
                                             char *error_buf, size_t error_buf_len);

/** Serializes the model as a versioned JSON snapshot into `buf`. */
int rill_ml_logistic_regression_to_json(void *handle, char *buf, size_t buf_len,
                                        char *error_buf, size_t error_buf_len);

/** Restores a model from a validated JSON snapshot. Returns an owned handle or NULL. */
void *rill_ml_logistic_regression_from_json(const char *json, char *error_buf, size_t error_buf_len);

/** Destroys a model handle. The handle must not be used afterwards. */
int rill_ml_logistic_regression_destroy(void *handle, char *error_buf, size_t error_buf_len);

/* ------------------------------------------------------------------ */
/* StandardScaler (online per-feature standardization)                 */
/* ------------------------------------------------------------------ */

/**
 * Creates a new standard scaler for `feature_count` features. Returns an
 * owned handle or NULL on failure (e.g. `feature_count == 0`).
 */
void *rill_ml_standard_scaler_new(size_t feature_count, char *error_buf, size_t error_buf_len);

/** Updates the scaler's running per-feature statistics with one raw sample. */
int rill_ml_standard_scaler_update(void *handle, const double *features, size_t feature_count,
                                   char *error_buf, size_t error_buf_len);

/**
 * Standardizes `feature_count` doubles from `features` and writes the result
 * into `out` (which must hold at least `feature_count` doubles).
 */
int rill_ml_standard_scaler_transform(void *handle, const double *features, size_t feature_count,
                                      double *out, char *error_buf, size_t error_buf_len);

/** Writes the number of samples seen by the scaler into `*out`. */
int rill_ml_standard_scaler_samples_seen(void *handle, uint64_t *out,
                                         char *error_buf, size_t error_buf_len);

/** Serializes the scaler as a versioned JSON snapshot into `buf`. */
int rill_ml_standard_scaler_to_json(void *handle, char *buf, size_t buf_len,
                                    char *error_buf, size_t error_buf_len);

/** Restores a scaler from a validated JSON snapshot. Returns an owned handle or NULL. */
void *rill_ml_standard_scaler_from_json(const char *json, char *error_buf, size_t error_buf_len);

/** Destroys a scaler handle. The handle must not be used afterwards. */
int rill_ml_standard_scaler_destroy(void *handle, char *error_buf, size_t error_buf_len);

/* ------------------------------------------------------------------ */
/* RegressionPipeline (StandardScaler + LinearRegression)              */
/* ------------------------------------------------------------------ */

/** Creates a new regression pipeline (StandardScaler then LinearRegression). Returns an owned handle or NULL. */
void *rill_ml_regression_pipeline_new(size_t feature_count, double learning_rate,
                                      char *error_buf, size_t error_buf_len);

/** Predicts `y` for `feature_count` doubles in `features` and writes it to `*out`. */
int rill_ml_regression_pipeline_predict(void *handle, const double *features, size_t feature_count,
                                        double *out, char *error_buf, size_t error_buf_len);

/** Learns one labeled sample `(features, target)` through the pipeline. */
int rill_ml_regression_pipeline_learn(void *handle, const double *features, size_t feature_count,
                                      double target, char *error_buf, size_t error_buf_len);

/** Writes the number of samples seen by the pipeline into `*out`. */
int rill_ml_regression_pipeline_samples_seen(void *handle, uint64_t *out,
                                             char *error_buf, size_t error_buf_len);

/** Serializes the pipeline as a versioned JSON snapshot into `buf`. */
int rill_ml_regression_pipeline_to_json(void *handle, char *buf, size_t buf_len,
                                        char *error_buf, size_t error_buf_len);

/** Restores a pipeline from a validated JSON snapshot. Returns an owned handle or NULL. */
void *rill_ml_regression_pipeline_from_json(const char *json, char *error_buf, size_t error_buf_len);

/** Destroys a pipeline handle. The handle must not be used afterwards. */
int rill_ml_regression_pipeline_destroy(void *handle, char *error_buf, size_t error_buf_len);

/* ------------------------------------------------------------------ */
/* ClassificationPipeline (StandardScaler + LogisticRegression)        */
/* ------------------------------------------------------------------ */

/** Creates a new classification pipeline (StandardScaler then LogisticRegression). Returns an owned handle or NULL. */
void *rill_ml_classification_pipeline_new(size_t feature_count, double learning_rate,
                                          char *error_buf, size_t error_buf_len);

/** Predicts `P(y = 1 | x)` for `feature_count` doubles in `features` and writes it to `*out`. */
int rill_ml_classification_pipeline_predict_proba(void *handle, const double *features,
                                                  size_t feature_count, double *out,
                                                  char *error_buf, size_t error_buf_len);

/** Learns one labeled sample `(features, target)` through the pipeline, where `target` is 0 or 1. */
int rill_ml_classification_pipeline_learn(void *handle, const double *features,
                                          size_t feature_count, int target,
                                          char *error_buf, size_t error_buf_len);

/** Writes the number of samples seen by the pipeline into `*out`. */
int rill_ml_classification_pipeline_samples_seen(void *handle, uint64_t *out,
                                                 char *error_buf, size_t error_buf_len);

/** Serializes the pipeline as a versioned JSON snapshot into `buf`. */
int rill_ml_classification_pipeline_to_json(void *handle, char *buf, size_t buf_len,
                                            char *error_buf, size_t error_buf_len);

/** Restores a pipeline from a validated JSON snapshot. Returns an owned handle or NULL. */
void *rill_ml_classification_pipeline_from_json(const char *json, char *error_buf,
                                                size_t error_buf_len);

/** Destroys a pipeline handle. The handle must not be used afterwards. */
int rill_ml_classification_pipeline_destroy(void *handle, char *error_buf, size_t error_buf_len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* RILL_ML_H */

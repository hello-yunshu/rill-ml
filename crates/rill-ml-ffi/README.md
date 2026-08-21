# rill-ml-ffi

Stable C FFI for [RillML](https://github.com/hello-yunshu/rill-ml) — online
machine learning that can be embedded natively. This crate is the bridge
between the Stable Rust core (`rill-ml`) and consumers written in C, C++, and
languages that bind to C, notably:

- **Android** via JNI (link the `staticlib` and call through `JNIEnv`)
- **iOS** via Swift (link the `staticlib` / `cdylib` and call through an
  imported C module)

The ABI is deliberately small and stable. Every model is exposed through an
**opaque handle** (`void *`), never a `#[repr(C)]` struct, so Rust internal
layouts are free to evolve without breaking bindings.

## ABI contract

See [`include/rill_ml.h`](include/rill_ml.h) for the authoritative header.
The raw-pointer trust boundary, caller obligations, and test evidence are
documented in [`docs/FFI_UNSAFE_TRUST_DOMAIN.md`](../../docs/FFI_UNSAFE_TRUST_DOMAIN.md).

### Opaque handles and ownership

- `rill_ml_<Type>_new(...)` / `rill_ml_<Type>_from_json(...)` return an opaque
  handle owned by the caller, or `NULL` on failure.
- `rill_ml_<Type>_destroy(handle)` releases the handle. Every handle must be
  destroyed exactly once; using a handle after destroy is undefined behaviour.
  `destroy(NULL)` reports `RILL_ML_ERR_INVALID_HANDLE`.
- The library never returns memory the caller must free. All output goes into
  caller-provided buffers with explicit lengths.

### Error codes

Every fallible function returns an `int` error code and writes a
NUL-terminated message into a caller-provided
`char *error_buf, size_t error_buf_len` pair (a zero-length buffer disables
messages).

| Constant | Value | Meaning |
| --- | --- | --- |
| `RILL_ML_OK` | `0` | Success |
| `RILL_ML_ERR_INVALID_ARGUMENT` | `-1` | Bad dimension, non-finite value, NULL pointer, ... |
| `RILL_ML_ERR_INVALID_STATE` | `-2` | Model state cannot perform the operation |
| `RILL_ML_ERR_PANIC` | `-3` | A panic was caught at the FFI boundary |
| `RILL_ML_ERR_IO` | `-4` | Internal I/O or serialization failure |
| `RILL_ML_ERR_INVALID_HANDLE` | `-5` | NULL / invalid opaque handle |
| `RILL_ML_ERR_BUFFER_TOO_SMALL` | `-6` | Caller-provided output buffer too small |

### Thread safety

Handles are **not** thread-safe. A single handle must not be shared
concurrently between threads. Distinct handles may be used from distinct
threads without synchronization.

### Panic policy

Panics never cross the FFI boundary. Every entry point is wrapped in
`std::panic::catch_unwind`; a caught panic is reported as `RILL_ML_ERR_PANIC`.

### Versioning

`rill_ml_version(buf, buf_len, ...)` returns the crate version string and
`rill_ml_snapshot_format_version()` returns the snapshot format version used
by the `_to_json` / `_from_json` entry points. Snapshots are versioned JSON
envelopes; `_from_json` validates the format version and model invariants
before restoring.

## Quick start (C)

```c
#include <stdio.h>
#include <rill_ml.h>

int main(void) {
    char err[256];
    void *m = rill_ml_mean_new(err, sizeof(err));
    if (!m) { fprintf(stderr, "mean_new: %s\n", err); return 1; }

    rill_ml_mean_update(m, 1.0, err, sizeof(err));
    rill_ml_mean_update(m, 2.0, err, sizeof(err));
    rill_ml_mean_update(m, 3.0, err, sizeof(err));

    double value = 0.0;
    rill_ml_mean_value(m, &value, err, sizeof(err));
    printf("mean = %.1f\n", value); /* 2.0 */

    rill_ml_mean_destroy(m, err, sizeof(err));
    return 0;
}
```

## Models

| Family | Constructor | Learn / predict | Snapshot |
| --- | --- | --- | --- |
| `Mean` | `rill_ml_mean_new` | `update` / `value` / `count` | `to_json` / `from_json` |
| `LinearRegression` | `rill_ml_linear_regression_new` | `learn` / `predict` / `weights` / `intercept` / `samples_seen` | `to_json` / `from_json` |
| `LogisticRegression` | `rill_ml_logistic_regression_new` | `learn` / `predict` / `predict_proba` / `weights` / `intercept` / `samples_seen` | `to_json` / `from_json` |
| `StandardScaler` | `rill_ml_standard_scaler_new` | `update` / `transform` / `samples_seen` | `to_json` / `from_json` |
| `RegressionPipeline` | `rill_ml_regression_pipeline_new` | `learn` / `predict` / `samples_seen` | `to_json` / `from_json` |
| `ClassificationPipeline` | `rill_ml_classification_pipeline_new` | `learn` / `predict_proba` / `samples_seen` | `to_json` / `from_json` |

Weights functions support a query mode: pass `out == NULL` and the required
element count is written to `*out_len`; then allocate and call again to copy.

## Snapshot size limit

`_from_json` rejects any JSON string larger than **64 MiB**
(`MAX_SNAPSHOT_JSON_BYTES` in the core). The limit is enforced on the raw byte
length before parsing. Invalid JSON, an incompatible snapshot format version,
or invalid model state are also rejected.

## Building

```sh
# Check and test the Rust side (rlib enables the integration tests)
cargo check --locked -p rill-ml-ffi
cargo test  --locked -p rill-ml-ffi

# Build the native library (staticlib + cdylib) and run the C smoke test
cargo build --locked --release -p rill-ml-ffi
./scripts/ffi-c-smoke.sh
```

Artifacts land in `target/release/`:
- `librill_ml_ffi.a` — `staticlib` (embed in Android / iOS apps)
- `librill_ml_ffi.dylib` / `librill_ml_ffi.so` — `cdylib` (dynamic linking)

## Notes for binding authors

- Compile against `include/rill_ml.h`; the header is the single source of
  truth for the ABI.
- Keep handles on the thread that created them.
- Always provide an error buffer (even if just a small one) to diagnose
  failures; every error path writes a human-readable message.
- For Android (JNI): `JNIEXPORT` wrappers call the C functions and store the
  opaque handle in a `long` field (do not pass raw pointers across JNI).
- For iOS (Swift): wrap the C functions in a Swift module; the snapshot JSON
  round-trips cleanly for model persistence across launches.

## License

MIT

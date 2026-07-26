//! Integration tests for the sandboxed WASM InvokeHandler.
//!
//! These tests require a pre-built echo-handler WASM component. Set the
//! `ECHO_HANDLER_WASM` environment variable to the component path, or place
//! the component at `handlers/echo-handler/target/wasm32-unknown-unknown/release/echo-handler.wasm`
//! relative to the workspace root.
//!
//! In CI, the `wasm-handler` job builds the component before running these
//! tests. Local developers can build it manually:
//!
//! ```bash
//! cd handlers/echo-handler
//! cargo build --release --target wasm32-unknown-unknown
//! wasm-tools component new target/wasm32-unknown-unknown/release/echo-handler.wasm \
//!   -o echo-handler.wasm
//! export ECHO_HANDLER_WASM="$PWD/echo-handler.wasm"
//! ```
//!
//! The sandbox attack tests (R-020) also require the malicious test handler
//! component, built from `handlers/test-malicious-handler/`:
//!
//! ```bash
//! cd handlers/test-malicious-handler
//! cargo build --release --target wasm32-unknown-unknown
//! wasm-tools component new target/wasm32-unknown-unknown/release/test-malicious-handler.wasm \
//!   -o ../../target/test-malicious-handler.wasm
//! ```

#![cfg(feature = "wasm")]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use ed25519_dalek::VerifyingKey;
use rill_runtime::{
    InvokeErrorKind, InvokeHandler, LoadedHandlerPack, TrustStore, WasmInvokeHandler,
    build_signed_handler_pack, load_handler_pack,
};
use rill_runtime_protocol::{
    HANDLER_API_VERSION, HANDLER_PACKAGE_FORMAT_VERSION, HandlerPackManifest,
};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Test serialisation.
//
// The `EpochTicker` RAII guard inside `WasmInvokeHandler` starts a
// background thread. The direct ticker-lifecycle observability tests
// (which read a test-only `active_epoch_ticker_count` counter) now live
// in the library's internal `#[cfg(test)] mod tests` in
// `src/handler/wasm.rs`, where the counter and its accessor can be
// `#[cfg(test)]`-private instead of leaking into the public API. This
// file still serialises its tests via `WASM_TEST_LOCK` to keep WASM
// component loading deterministic and avoid resource contention on CI
// runners; the file-wide lock was originally introduced for the ticker
// counter but is retained as a conservative serialisation guard.
// ---------------------------------------------------------------------------

/// Serialises ALL tests in this file to keep WASM component loading
/// deterministic. The guard is recovered from poison so that a panic in
/// one test does not cascade to all subsequent tests via `PoisonError`.
static WASM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquires the global WASM test serialisation lock. Every `#[test]` in
/// this file calls this at entry to ensure tests do not run in parallel.
fn wasm_test_guard() -> std::sync::MutexGuard<'static, ()> {
    WASM_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Returns the echo handler WASM component path, or `None` if not available.
fn echo_handler_component() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ECHO_HANDLER_WASM") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    // Try default paths relative to CARGO_MANIFEST_DIR (crates/rill-runtime).
    // 1. Component built by CI / `wasm-tools component new` at workspace target.
    // 2. Component built locally next to the echo-handler Cargo.toml.
    let workspace_target =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/echo-handler.wasm");
    if workspace_target.exists() {
        return Some(workspace_target);
    }
    let local_component = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../handlers/echo-handler/echo-handler.wasm");
    if local_component.exists() {
        return Some(local_component);
    }
    None
}

/// Builds a signed `.rillhandler` pack from the echo handler component.
fn build_echo_pack(module: &[u8], signing: &SigningKey) -> Vec<u8> {
    let manifest = HandlerPackManifest {
        format_version: HANDLER_PACKAGE_FORMAT_VERSION,
        id: "rillml.echo.handler".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: env!("CARGO_PKG_VERSION").into(),
        publisher_key_id: "wasm-test-key".into(),
        capabilities: vec!["rillml.linearRegression.predict".into()],
        module_sha256: hex::encode(Sha256::digest(module)),
        module_size: module.len() as u64,
    };
    build_signed_handler_pack(&manifest, module, signing).unwrap()
}

fn load_echo_pack(
    pack_bytes: &[u8],
    verifying: &VerifyingKey,
) -> (LoadedHandlerPack, rill_runtime::HandlerPackInspection) {
    let trust = TrustStore(BTreeMap::from([("wasm-test-key".into(), *verifying)]));
    load_handler_pack(std::io::Cursor::new(pack_bytes), &trust).unwrap()
}

#[test]
fn echo_handler_invoke_returns_input() {
    let _guard = wasm_test_guard();
    let component = match echo_handler_component() {
        Some(path) => fs::read(&path).unwrap(),
        None => {
            eprintln!("skipping: echo handler component not built (set ECHO_HANDLER_WASM)");
            return;
        }
    };

    let signing = SigningKey::from_bytes(&[7; 32]);
    let pack_bytes = build_echo_pack(&component, &signing);
    let (loaded, inspection) = load_echo_pack(&pack_bytes, &signing.verifying_key());
    assert_eq!(inspection.id, "rillml.echo.handler");
    assert!(inspection.signature_verified);

    let model = serde_json::json!({"kind": "linearRegression", "weights": [0.5], "intercept": 0.0});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let input = serde_json::json!({"features": [1.0, 2.0]});
    let output = handler
        .invoke("rillml.linearRegression.predict", &input)
        .unwrap();
    // Echo handler returns the input as output.
    assert_eq!(output, input);
}

#[test]
fn echo_handler_rejects_unsupported_capability() {
    let _guard = wasm_test_guard();
    let component = match echo_handler_component() {
        Some(path) => fs::read(&path).unwrap(),
        None => {
            eprintln!("skipping: echo handler component not built (set ECHO_HANDLER_WASM)");
            return;
        }
    };

    let signing = SigningKey::from_bytes(&[7; 32]);
    let pack_bytes = build_echo_pack(&component, &signing);
    let (loaded, _) = load_echo_pack(&pack_bytes, &signing.verifying_key());

    let model = serde_json::json!({"kind": "linearRegression"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let result = handler.invoke("rillml.unknown.predict", &serde_json::json!({}));
    assert!(result.is_err());
    let error = result.unwrap_err();
    // Unsupported capability is reported by the guest via the WIT
    // `handler-error` `unsupported-capability` variant. The host maps
    // this 1:1 to `InvokeErrorKind::UnsupportedCapability` (audit 5.1)
    // while keeping the guest detail host-side only. The stable IPC
    // code stays `handlerInternalError` for v1/v2 backwards compat.
    assert!(
        matches!(error.kind(), InvokeErrorKind::UnsupportedCapability),
        "expected UnsupportedCapability, got: {:?}",
        error.kind()
    );
    assert_eq!(error.stable_code(), "handlerInternalError");
    // The fixed public message must not contain guest-supplied content.
    assert!(!error.public_message().contains("UnsupportedCapability"));
    assert!(!error.public_message().contains("unsupported"));
}

#[test]
fn echo_handler_metadata_mismatch_rejected() {
    let _guard = wasm_test_guard();
    let component = match echo_handler_component() {
        Some(path) => fs::read(&path).unwrap(),
        None => {
            eprintln!("skipping: echo handler component not built (set ECHO_HANDLER_WASM)");
            return;
        }
    };

    // Build a pack with a different handler id than what the guest reports.
    let signing = SigningKey::from_bytes(&[7; 32]);
    let manifest = HandlerPackManifest {
        format_version: HANDLER_PACKAGE_FORMAT_VERSION,
        id: "wrong.handler.id".into(), // mismatched
        version: env!("CARGO_PKG_VERSION").into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: env!("CARGO_PKG_VERSION").into(),
        publisher_key_id: "wasm-test-key".into(),
        capabilities: vec!["rillml.linearRegression.predict".into()],
        module_sha256: hex::encode(Sha256::digest(&component)),
        module_size: component.len() as u64,
    };
    let pack_bytes = build_signed_handler_pack(&manifest, &component, &signing).unwrap();
    let trust = TrustStore(BTreeMap::from([(
        "wasm-test-key".into(),
        signing.verifying_key(),
    )]));
    let (loaded, _) = load_handler_pack(std::io::Cursor::new(&pack_bytes), &trust).unwrap();

    let model = serde_json::json!({});
    let result = WasmInvokeHandler::new(&loaded, &model);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        matches!(error, rill_runtime::HandlerLoadError::MetadataMismatch(_)),
        "expected MetadataMismatch, got: {error:?}"
    );
}

/// Verifies the oversized output protection enforced by the WasmInvokeHandler.
///
/// Building a malicious WASM component that produces oversized output is
/// complex, so this test instead documents and verifies the limit constant.
/// The actual check lives in `handler/wasm.rs` around the `invoke`
/// implementation: any `output_bytes` exceeding `MAX_IO_BYTES` produces a
/// `handlerOutputTooLarge` error. `MAX_IO_BYTES` bounds both input and output
/// JSON payloads (the IPC limit is shared).
#[test]
fn wasm_handler_rejects_oversized_output() {
    let _guard = wasm_test_guard();
    use rill_runtime::handler::wasm::MAX_IO_BYTES;
    // 1 MiB, matching the IPC limit per HANDLER-RFC §5.
    assert_eq!(MAX_IO_BYTES, 1024 * 1024);
}

/// Verifies all WASM sandbox limits match HANDLER-RFC §5.
///
/// This test documents the expected limits and catches accidental changes that
/// could weaken the sandbox. The limits are enforced by `WasmInvokeHandler`
/// via Wasmtime config (fuel, epoch interruption) and a `ResourceLimiter`
/// (memory/table growth).
#[test]
fn wasm_handler_sandbox_limits_verified() {
    let _guard = wasm_test_guard();
    use rill_runtime::handler::wasm::{
        CONFIGURE_FUEL, EPOCH_DEADLINE, EPOCH_TICK_INTERVAL, INVOKE_FUEL, MAX_IO_BYTES,
        MAX_MEMORY_BYTES, MAX_TABLE_ELEMENTS,
    };
    use std::time::Duration;

    // Fuel budgets per call.
    assert_eq!(CONFIGURE_FUEL, 10_000_000);
    assert_eq!(INVOKE_FUEL, 100_000_000);
    // Memory and table caps per instance.
    assert_eq!(MAX_MEMORY_BYTES, 64 * 1024 * 1024);
    assert_eq!(MAX_TABLE_ELEMENTS, 10_000);
    // Input/output JSON payload cap (1 MiB, matches IPC limit).
    assert_eq!(MAX_IO_BYTES, 1024 * 1024);
    // Epoch interruption: 1-second tick, 5-tick deadline (5s wall-clock).
    assert_eq!(EPOCH_TICK_INTERVAL, Duration::from_secs(1));
    assert_eq!(EPOCH_DEADLINE, 5);
}

// ----- R-020: WASM sandbox attack tests -----
//
// These tests use the malicious test handler (`handlers/test-malicious-handler/`)
// which accepts a `"mode"` field in the model JSON to control its behavior.
// Each test loads the handler with a specific mode and verifies that the
// sandbox correctly rejects the malicious behavior with the expected IPC
// error code.

/// Returns the malicious handler WASM component path, or `None` if not available.
fn malicious_handler_component() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("MALICIOUS_HANDLER_WASM") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let workspace_target =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-malicious-handler.wasm");
    if workspace_target.exists() {
        return Some(workspace_target);
    }
    None
}

/// Build a signed `.rillhandler` pack from the malicious handler component.
fn build_malicious_handler_pack(module: &[u8], signing: &SigningKey) -> Vec<u8> {
    let manifest = HandlerPackManifest {
        format_version: HANDLER_PACKAGE_FORMAT_VERSION,
        id: "rillml.test.malicious".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: env!("CARGO_PKG_VERSION").into(),
        publisher_key_id: "wasm-test-key".into(),
        capabilities: vec!["rillml.linearRegression.predict".into()],
        module_sha256: hex::encode(Sha256::digest(module)),
        module_size: module.len() as u64,
    };
    build_signed_handler_pack(&manifest, module, signing).unwrap()
}

fn load_malicious_handler_pack(pack_bytes: &[u8], verifying: &VerifyingKey) -> LoadedHandlerPack {
    let trust = TrustStore(BTreeMap::from([("wasm-test-key".into(), *verifying)]));
    let (loaded, _) = load_handler_pack(std::io::Cursor::new(pack_bytes), &trust).unwrap();
    loaded
}

/// Helper to read the malicious handler component, build a pack, and load it.
/// Returns `(loaded, signing_key)` or skips the test if the component is not
/// available.
fn prepare_malicious_handler() -> Option<(LoadedHandlerPack, SigningKey)> {
    let component = match malicious_handler_component() {
        Some(path) => fs::read(&path).unwrap(),
        None => {
            eprintln!(
                "skipping: malicious handler component not built (set MALICIOUS_HANDLER_WASM)"
            );
            return None;
        }
    };
    let signing = SigningKey::from_bytes(&[8; 32]);
    let pack_bytes = build_malicious_handler_pack(&component, &signing);
    let loaded = load_malicious_handler_pack(&pack_bytes, &signing.verifying_key());
    Some((loaded, signing))
}

#[test]
fn wasm_handler_trap_returns_handler_trap_error() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    // Configure the handler to execute `unreachable` on invoke.
    let model = serde_json::json!({"mode": "trap"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::Trap),
        "expected Trap, got: {:?}",
        error.kind()
    );
    assert_eq!(error.stable_code(), "handlerTrap");
    assert_eq!(error.public_message(), "handler trapped");
    // The trap detail (which may include a wasmtime backtrace) must not
    // appear in the public message or stable code.
    assert!(!error.public_message().contains("unreachable"));
    assert!(!error.stable_code().contains("unreachable"));
}

#[test]
fn wasm_handler_oversized_output_returns_output_too_large() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    // Configure the handler to return >1 MiB JSON output.
    let model = serde_json::json!({"mode": "oversized-output"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::OutputTooLarge),
        "expected OutputTooLarge, got: {:?}",
        error.kind()
    );
    assert_eq!(error.stable_code(), "handlerOutputTooLarge");
}

#[test]
fn wasm_handler_invalid_json_output_returns_invalid_output() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    // Configure the handler to return invalid JSON bytes.
    let model = serde_json::json!({"mode": "invalid-json"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::InvalidOutput),
        "expected InvalidOutput, got: {:?}",
        error.kind()
    );
    assert_eq!(error.stable_code(), "handlerInvalidOutput");
}

#[test]
fn wasm_handler_infinite_loop_returns_timeout() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    // Configure the handler to loop forever. The epoch interruption (5s
    // deadline) must terminate the call and return handlerTimeout.
    let model = serde_json::json!({"mode": "infinite-loop"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let start = std::time::Instant::now();
    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    let elapsed = start.elapsed();

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::Timeout),
        "expected Timeout, got: {:?}",
        error.kind()
    );
    assert_eq!(error.stable_code(), "handlerTimeout");
    assert!(error.retryable());
    // The epoch deadline is 5 seconds; the call must be interrupted within
    // a reasonable window after that (allow 10s for CI overhead).
    assert!(
        elapsed.as_secs() < 15,
        "infinite loop took too long to interrupt: {elapsed:?}"
    );
}

#[test]
fn wasm_handler_echo_mode_works_as_baseline() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    // Verify the malicious handler in "echo" mode behaves correctly.
    // This confirms the test fixture itself is valid before testing attacks.
    let model = serde_json::json!({"mode": "echo"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let input = serde_json::json!({"features": [1.0, 2.0]});
    let output = handler
        .invoke("rillml.linearRegression.predict", &input)
        .unwrap();
    assert_eq!(output, input);
}

/// Verifies the WASM store remains usable after a non-trap error.
///
/// After `OutputTooLarge` (host-side size check on the returned bytes), the
/// underlying Wasmtime `Store` is in a clean state and the same handler
/// instance can be invoked again. The malicious handler's `oversized-output`
/// mode caches the oversized buffer in a `thread_local` and consumes it on
/// the first `invoke` call, so the second call returns `ExecutionFailed`
/// (because the cache is empty). The important assertion is that the second
/// call returns a typed error rather than panicking, hanging, or returning
/// a permanent `Trap`.
#[test]
fn wasm_handler_remains_usable_after_output_too_large() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    let model = serde_json::json!({"mode": "oversized-output"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    // First invoke: oversized output → OutputTooLarge.
    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(matches!(error.kind(), InvokeErrorKind::OutputTooLarge));

    // Second invoke: the thread_local cache is now empty, so the guest
    // returns `ExecutionFailed`. The store is still usable — the call
    // completes and returns a typed error rather than hanging or trapping.
    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::ExecutionFailed),
        "expected ExecutionFailed on second call, got: {:?}",
        error.kind()
    );
}

/// Verifies that one handler instance's failure does not affect another.
///
/// Each `WasmInvokeHandler` owns its own `Engine`, `Store`, and epoch
/// ticker thread, so a malicious handler that traps or loops forever
/// cannot poison the runtime for other handlers. This test creates a
/// trap-mode handler, confirms it returns `Trap`, then creates a fresh
/// echo-mode handler and confirms it works.
#[test]
fn wasm_handler_failure_does_not_affect_other_instances() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    // First handler: traps on every invoke.
    let trap_model = serde_json::json!({"mode": "trap"});
    let trap_handler = WasmInvokeHandler::new(&loaded, &trap_model).unwrap();
    let result = trap_handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    assert!(matches!(result.unwrap_err().kind(), InvokeErrorKind::Trap));

    // Second handler: echo mode. Must work despite the first handler's trap.
    let echo_model = serde_json::json!({"mode": "echo"});
    let echo_handler = WasmInvokeHandler::new(&loaded, &echo_model).unwrap();
    let input = serde_json::json!({"features": [3.0, 4.0]});
    let output = echo_handler
        .invoke("rillml.linearRegression.predict", &input)
        .unwrap();
    assert_eq!(output, input);
}

/// Verifies that an infinite loop in `configure()` is bounded by the
/// epoch deadline and returns a load error (instead of hanging forever).
///
/// The malicious handler's `configure-infinite-loop` mode calls
/// `burn_forever()` inside `configure()`. The host sets an independent
/// fuel budget and epoch deadline on the `configure()` stage (see
/// `handler/wasm.rs` stage 3), so the call must be interrupted within a
/// reasonable window of the 5-second deadline. Because `configure()`
/// runs inside `WasmInvokeHandler::new`, the failure surfaces as a
/// `HandlerLoadError::Init` rather than an `InvokeError`.
#[test]
fn wasm_handler_configure_infinite_loop_returns_timeout() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    let model = serde_json::json!({"mode": "configure-infinite-loop"});
    let start = std::time::Instant::now();
    let result = WasmInvokeHandler::new(&loaded, &model);
    let elapsed = start.elapsed();

    assert!(
        result.is_err(),
        "configure-infinite-loop must fail handler load"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, rill_runtime::HandlerLoadError::Init(ref msg)
            if msg.contains("configure trap")),
        "expected HandlerLoadError::Init mentioning configure trap, got: {err:?}"
    );
    // The epoch deadline is 5 seconds; the load must fail within a
    // reasonable window of that (allow 15s for CI overhead).
    assert!(
        elapsed.as_secs() < 15,
        "configure infinite loop took too long to interrupt: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Metadata infinite-loop fixture (audit 5.3).
//
// The mode-driven malicious handler cannot test a `metadata()` infinite
// loop because `metadata()` is called before `configure()` and cannot
// read the model JSON. A dedicated test-only handler
// (`handlers/test-metadata-loop-handler/`) always loops forever in
// `metadata()`. These tests verify the host's epoch deadline bounds the
// metadata stage, the ticker thread is cleaned up, and subsequent
// handlers still work.
// ---------------------------------------------------------------------------

/// Returns the metadata-loop handler WASM component path, or `None` if
/// not available.
fn metadata_loop_handler_component() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("METADATA_LOOP_HANDLER_WASM") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let workspace_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-metadata-loop-handler.wasm");
    if workspace_target.exists() {
        return Some(workspace_target);
    }
    None
}

/// Build a signed `.rillhandler` pack from the metadata-loop handler
/// component. The manifest id matches the guest's `metadata()` return
/// value — but since `metadata()` loops forever, the host never reaches
/// the metadata-mismatch check. The manifest is still needed to build a
/// valid signed pack.
fn build_metadata_loop_handler_pack(module: &[u8], signing: &SigningKey) -> Vec<u8> {
    let manifest = HandlerPackManifest {
        format_version: HANDLER_PACKAGE_FORMAT_VERSION,
        id: "rillml.test.metadata-loop".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: env!("CARGO_PKG_VERSION").into(),
        publisher_key_id: "wasm-test-key".into(),
        capabilities: vec!["rillml.linearRegression.predict".into()],
        module_sha256: hex::encode(Sha256::digest(module)),
        module_size: module.len() as u64,
    };
    build_signed_handler_pack(&manifest, module, signing).unwrap()
}

fn load_metadata_loop_handler_pack(
    pack_bytes: &[u8],
    verifying: &VerifyingKey,
) -> LoadedHandlerPack {
    let trust = TrustStore(BTreeMap::from([("wasm-test-key".into(), *verifying)]));
    let (loaded, _) = load_handler_pack(std::io::Cursor::new(pack_bytes), &trust).unwrap();
    loaded
}

/// Helper to read the metadata-loop handler component, build a pack, and
/// load it. Returns `None` (skip) if the component is not available.
fn prepare_metadata_loop_handler() -> Option<LoadedHandlerPack> {
    let component = match metadata_loop_handler_component() {
        Some(path) => fs::read(&path).unwrap(),
        None => {
            eprintln!(
                "skipping: metadata-loop handler component not built (set METADATA_LOOP_HANDLER_WASM)"
            );
            return None;
        }
    };
    let signing = SigningKey::from_bytes(&[9; 32]);
    let pack_bytes = build_metadata_loop_handler_pack(&component, &signing);
    Some(load_metadata_loop_handler_pack(
        &pack_bytes,
        &signing.verifying_key(),
    ))
}

/// Verifies that an infinite loop in `metadata()` is bounded by the
/// epoch deadline and returns a load error (instead of hanging forever).
///
/// The metadata-loop handler always calls `burn_forever()` inside
/// `metadata()`. The host sets an independent fuel budget and epoch
/// deadline on the `metadata()` stage (see `handler/wasm.rs` stage 2),
/// so the call must be interrupted within a reasonable window of the
/// 5-second deadline. Because `metadata()` runs inside
/// `WasmInvokeHandler::new`, the failure surfaces as a
/// `HandlerLoadError::Init` rather than an `InvokeError`.
#[test]
fn wasm_handler_metadata_infinite_loop_returns_load_error() {
    let _guard = wasm_test_guard();
    let loaded = match prepare_metadata_loop_handler() {
        Some(v) => v,
        None => return,
    };

    let model = serde_json::json!({});
    let start = std::time::Instant::now();
    let result = WasmInvokeHandler::new(&loaded, &model);
    let elapsed = start.elapsed();

    assert!(
        result.is_err(),
        "metadata-infinite-loop must fail handler load"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, rill_runtime::HandlerLoadError::Init(ref msg)
            if msg.contains("metadata trap")),
        "expected HandlerLoadError::Init mentioning metadata trap, got: {err:?}"
    );
    // The epoch deadline is 5 seconds; the load must fail within a
    // reasonable window of that (allow 15s for CI overhead).
    assert!(
        elapsed.as_secs() < 15,
        "metadata infinite loop took too long to interrupt: {elapsed:?}"
    );
}

/// Verifies that after a metadata-loop handler fails to load, the epoch
/// ticker thread is cleaned up and a subsequent normal handler still
/// works. This catches resource leaks (e.g. a leaked ticker thread that
/// keeps incrementing the engine epoch and interferes with later
/// handlers).
#[test]
fn metadata_loop_handler_failure_does_not_leak_ticker_or_block_later_handlers() {
    let _guard = wasm_test_guard();
    let metadata_loaded = match prepare_metadata_loop_handler() {
        Some(v) => v,
        None => return,
    };

    // Step 1: the metadata-loop handler must fail to load. The
    // `WasmInvokeHandler::new` call returns `Err`, and the
    // `EpochTicker` guard inside the failed constructor is dropped,
    // which signals the background thread to stop and joins it.
    let start = std::time::Instant::now();
    let result = WasmInvokeHandler::new(&metadata_loaded, &serde_json::json!({}));
    let elapsed = start.elapsed();
    assert!(result.is_err(), "metadata-loop handler must fail to load");
    assert!(
        elapsed.as_secs() < 15,
        "metadata-loop load must be bounded by epoch deadline: {elapsed:?}"
    );
    // The failed handler is dropped here (it was never constructed
    // successfully, so only temporary state is dropped). The ticker
    // thread must have been joined by the `EpochTicker` Drop impl.

    // Step 2: a fresh normal (echo) handler must still load and invoke
    // correctly, proving the metadata-loop failure did not leak a
    // thread or leave the runtime in a bad state.
    let echo_component = match echo_handler_component() {
        Some(path) => fs::read(&path).unwrap(),
        None => {
            eprintln!("skipping: echo handler component not built (set ECHO_HANDLER_WASM)");
            return;
        }
    };
    let signing = SigningKey::from_bytes(&[7; 32]);
    let pack_bytes = build_echo_pack(&echo_component, &signing);
    let (echo_loaded, _) = load_echo_pack(&pack_bytes, &signing.verifying_key());

    let model = serde_json::json!({"kind": "linearRegression", "weights": [0.5], "intercept": 0.0});
    let echo_handler = WasmInvokeHandler::new(&echo_loaded, &model)
        .expect("echo handler must load after metadata-loop failure");

    let input = serde_json::json!({"features": [1.0, 2.0]});
    let output = echo_handler
        .invoke("rillml.linearRegression.predict", &input)
        .expect("echo handler must still invoke correctly");
    assert_eq!(
        output, input,
        "echo handler must return the input unchanged"
    );
}

/// Verifies that a guest-supplied oversized error detail string is
/// truncated by the host before being stored on `InvokeError`.
///
/// The malicious handler's `long-error-string` mode returns
/// `HandlerError::ExecutionFailed` with a 16 KiB detail payload. The
/// host's `InvokeError::with_detail` truncates the detail to
/// `MAX_DETAIL_BYTES` (4 KiB) on a UTF-8 char boundary, bounding host
/// memory and stderr noise. The stored detail must not exceed the limit.
#[test]
fn wasm_handler_long_error_string_is_truncated() {
    let _guard = wasm_test_guard();
    use rill_runtime::InvokeError;
    use rill_runtime::MAX_DETAIL_BYTES;

    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    let model = serde_json::json!({"mode": "long-error-string"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    assert!(result.is_err());
    let error: InvokeError = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::ExecutionFailed),
        "expected ExecutionFailed, got: {:?}",
        error.kind()
    );
    assert_eq!(error.stable_code(), "handlerInternalError");
    // The guest attempted to exfiltrate 16 KiB; the host must have
    // truncated the stored detail to <= MAX_DETAIL_BYTES.
    let detail = error
        .detail()
        .expect("ExecutionFailed with detail must store host-only detail");
    assert!(
        detail.len() <= MAX_DETAIL_BYTES,
        "detail length {} must not exceed MAX_DETAIL_BYTES {}",
        detail.len(),
        MAX_DETAIL_BYTES
    );
    // After audit 5.1, the host extracts the inner guest string directly
    // (not the `Debug` representation), so the stored detail is the raw
    // 'X' payload without the `HandlerError::ExecutionFailed("...")`
    // prefix. The truncation must land on a UTF-8 char boundary
    // (all-'X' is ASCII, so any byte offset is a valid char boundary).
    assert!(
        detail.chars().all(|c| c == 'X'),
        "detail must be the raw guest 'X' payload, got: {detail:?}"
    );
    let x_count = detail.chars().filter(|c| *c == 'X').count();
    assert!(
        x_count >= MAX_DETAIL_BYTES - 64,
        "expected at least {} 'X' characters from the guest payload, got {x_count}",
        MAX_DETAIL_BYTES - 64
    );
    // The public message must be the fixed constant, not the payload.
    assert_eq!(error.public_message(), "handler execution failed");
    assert!(!error.public_message().contains("X"));
}

/// Verifies that a handler performing dense floating-point work that
/// exhausts the invoke fuel budget is interrupted and returns a
/// `Timeout` error.
///
/// The malicious handler's `fuel-exhaustion` mode performs real
/// arithmetic in a tight loop (rather than an empty `wrapping_add`
/// loop), exercising fuel accounting for numeric instructions. The
/// epoch deadline remains the authoritative wall-clock guard, but fuel
/// exhaustion alone (without an explicit `loop {}`) must also terminate
/// the call.
#[test]
fn wasm_handler_fuel_exhaustion_returns_timeout() {
    let _guard = wasm_test_guard();
    let (loaded, _) = match prepare_malicious_handler() {
        Some(v) => v,
        None => return,
    };

    let model = serde_json::json!({"mode": "fuel-exhaustion"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let start = std::time::Instant::now();
    let result = handler.invoke("rillml.linearRegression.predict", &serde_json::json!({}));
    let elapsed = start.elapsed();

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::Timeout),
        "expected Timeout for fuel exhaustion, got: {:?}",
        error.kind()
    );
    assert_eq!(error.stable_code(), "handlerTimeout");
    assert!(error.retryable());
    assert!(
        elapsed.as_secs() < 15,
        "fuel exhaustion took too long to interrupt: {elapsed:?}"
    );
}

// The direct ticker-lifecycle observability tests
// (`metadata_loop_failure_restores_active_ticker_count`,
// `normal_handler_drop_restores_active_ticker_count`) previously lived
// here and reached the counter through a `#[doc(hidden)] pub` accessor.
// They have been moved to the library's internal `#[cfg(test)] mod
// tests` in `src/handler/wasm.rs`, where the counter and its accessor
// can be `#[cfg(test)]`-private instead of leaking into the public API.
// The indirect lifecycle test
// (`metadata_loop_handler_failure_does_not_leak_ticker_or_block_later_handlers`)
// is retained below.

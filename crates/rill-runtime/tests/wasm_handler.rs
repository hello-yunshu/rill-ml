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
    InvokeHandler, LoadedHandlerPack, TrustStore, WasmInvokeHandler, build_signed_handler_pack,
    load_handler_pack,
};
use rill_runtime_protocol::{
    HANDLER_API_VERSION, HANDLER_PACKAGE_FORMAT_VERSION, HandlerPackManifest,
};
use sha2::{Digest, Sha256};

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
    assert!(
        error.contains("handlerExecutionFailed") || error.contains("UnsupportedCapability"),
        "unexpected error: {error}"
    );
}

#[test]
fn echo_handler_metadata_mismatch_rejected() {
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
        error.starts_with("handlerTrap"),
        "expected handlerTrap, got: {error}"
    );
}

#[test]
fn wasm_handler_oversized_output_returns_output_too_large() {
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
        error.starts_with("handlerOutputTooLarge"),
        "expected handlerOutputTooLarge, got: {error}"
    );
}

#[test]
fn wasm_handler_invalid_json_output_returns_invalid_output() {
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
        error.starts_with("handlerInvalidOutput"),
        "expected handlerInvalidOutput, got: {error}"
    );
}

#[test]
fn wasm_handler_infinite_loop_returns_timeout() {
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
        error.starts_with("handlerTimeout"),
        "expected handlerTimeout, got: {error}"
    );
    // The epoch deadline is 5 seconds; the call must be interrupted within
    // a reasonable window after that (allow 10s for CI overhead).
    assert!(
        elapsed.as_secs() < 15,
        "infinite loop took too long to interrupt: {elapsed:?}"
    );
}

#[test]
fn wasm_handler_echo_mode_works_as_baseline() {
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

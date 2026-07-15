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
        version: "0.7.1".into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: "0.7.1".into(),
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
        version: "0.7.1".into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: "0.7.1".into(),
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

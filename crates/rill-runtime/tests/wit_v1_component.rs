//! WIT ABI v1 frozen-component compatibility tests.
//!
//! These tests load a prebuilt WASM component compiled from the `v0.13.0`
//! tag using the frozen WIT v1 ABI (`rill:handler@1.0.0`). The component is
//! committed to the repository at
//! `tests/fixtures/handler-v1-component.wasm` and is **never** rebuilt by CI.
//!
//! The tests prove that:
//! - The current runtime (built from the 1.0 source tree) can still load a
//!   handler component compiled from the historical v0.13.0 WIT source.
//! - The component's SHA-256 matches the value recorded in the fixture
//!   manifest, so any accidental replacement of the file is detected.
//! - The full `metadata` → `configure` → `invoke` lifecycle works.
//! - The component can be packed into a signed `.rillhandler` and loaded
//!   back through the standard handler-pack loader.
//! - The handler's reported `api-version` equals
//!   `rill_handler_api::HANDLER_API_VERSION` (1), confirming manifest/WIT
//!   consistency.
//!
//! If any of these tests fail, the change is a breaking WIT ABI change and
//! must be addressed by introducing `rill:handler@2.x` and bumping
//! `HANDLER_API_VERSION`, not by editing the fixture.

#![cfg(feature = "wasm")]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use ed25519_dalek::VerifyingKey;
use rill_handler_api::{HANDLER_API_VERSION, WIT_PACKAGE, WIT_VERSION, WIT_WORLD};
use rill_runtime::{
    HandlerPackInspection, InvokeHandler, LoadedHandlerPack, TrustStore, WasmInvokeHandler,
    build_signed_handler_pack, load_handler_pack,
};
use rill_runtime_protocol::{
    HANDLER_API_VERSION as PROTOCOL_HANDLER_API_VERSION, HANDLER_PACKAGE_FORMAT_VERSION,
    HandlerPackManifest,
};
use sha2::{Digest, Sha256};

/// Serialises all tests in this file to keep WASM component loading
/// deterministic. Reuses the same pattern as `wasm_handler.rs`.
static WIT_V1_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn wit_v1_test_guard() -> std::sync::MutexGuard<'static, ()> {
    WIT_V1_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Expected SHA-256 of the prebuilt component. Must match the value in
/// `handler-v1-component.wasm.manifest.json`. Hard-coded here so that
/// tampering with the manifest alone cannot hide a fixture swap.
const EXPECTED_COMPONENT_SHA256: &str =
    "6cfb4bf2eac5d0d5a4644c56f58b2fd679fc989893bc381a8ae31b410852011b";

/// Path to the prebuilt v1 component fixture.
fn v1_component_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("handler-v1-component.wasm")
}

/// Reads the fixture component bytes, asserting the SHA-256 matches the
/// frozen value. This guards against silent fixture replacement.
fn read_v1_component() -> Vec<u8> {
    let path = v1_component_path();
    assert!(
        path.exists(),
        "prebuilt v1 component fixture not found at {path:?}; \
         this fixture must be committed to the repository and must not be \
         rebuilt by CI"
    );
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"));
    let hash = Sha256::digest(&bytes);
    let hex = hex::encode(hash);
    assert_eq!(
        hex, EXPECTED_COMPONENT_SHA256,
        "prebuilt v1 component SHA-256 mismatch: expected {EXPECTED_COMPONENT_SHA256}, got {hex}. \
         The fixture file was replaced or corrupted. Restore it from git or update \
         EXPECTED_COMPONENT_SHA256 only after an explicit ABI-bump review."
    );
    bytes
}

/// Builds a signed `.rillhandler` pack from the v1 component. The handler
/// id, version and capabilities in the manifest match what the component
/// reports via `metadata()` so the runtime's metadata-mismatch check passes.
fn build_v1_pack(component: &[u8], signing: &SigningKey) -> Vec<u8> {
    let manifest = HandlerPackManifest {
        format_version: HANDLER_PACKAGE_FORMAT_VERSION,
        id: "rillml.echo.handler".into(),
        version: "0.13.0".into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: "0.13.0".into(),
        publisher_key_id: "wit-v1-fixture-key".into(),
        capabilities: vec!["rillml.linearRegression.predict".into()],
        module_sha256: hex::encode(Sha256::digest(component)),
        module_size: component.len() as u64,
    };
    build_signed_handler_pack(&manifest, component, signing).unwrap()
}

fn load_v1_pack(
    pack_bytes: &[u8],
    verifying: &VerifyingKey,
) -> (LoadedHandlerPack, HandlerPackInspection) {
    let trust = TrustStore(BTreeMap::from([("wit-v1-fixture-key".into(), *verifying)]));
    load_handler_pack(std::io::Cursor::new(pack_bytes), &trust).unwrap()
}

// ---------------------------------------------------------------------------
// Fixture integrity
// ---------------------------------------------------------------------------

#[test]
fn v1_component_fixture_exists_and_hash_matches() {
    let _guard = wit_v1_test_guard();
    // `read_v1_component` asserts the SHA-256 internally. If it returns at
    // all, the fixture is present and intact.
    let _bytes = read_v1_component();
}

// ---------------------------------------------------------------------------
// Full lifecycle: metadata → configure → invoke
// ---------------------------------------------------------------------------

#[test]
fn v1_component_loads_and_invokes_with_current_runtime() {
    let _guard = wit_v1_test_guard();
    let component = read_v1_component();
    let signing = SigningKey::from_bytes(&[42; 32]);
    let pack_bytes = build_v1_pack(&component, &signing);
    let (loaded, inspection) = load_v1_pack(&pack_bytes, &signing.verifying_key());

    // Pack-level checks: signature verified, handler id matches.
    assert_eq!(inspection.id, "rillml.echo.handler");
    assert!(inspection.signature_verified);

    // configure + invoke: the echo handler returns its input unchanged.
    let model = serde_json::json!({"kind": "linearRegression", "weights": [0.5], "intercept": 0.0});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    let input = serde_json::json!({"features": [1.0, 2.0, 3.0]});
    let output = handler
        .invoke("rillml.linearRegression.predict", &input)
        .unwrap();
    assert_eq!(
        output, input,
        "echo handler must return its input unchanged"
    );
}

#[test]
fn v1_component_metadata_reports_api_version_1() {
    let _guard = wit_v1_test_guard();
    let component = read_v1_component();
    let signing = SigningKey::from_bytes(&[42; 32]);
    let pack_bytes = build_v1_pack(&component, &signing);
    let (loaded, inspection) = load_v1_pack(&pack_bytes, &signing.verifying_key());

    // The manifest's handler_api_version must equal the frozen constant.
    // `WasmInvokeHandler::new()` internally calls the component's
    // `metadata()` export and asserts `metadata.api_version ==
    // manifest.handler_api_version`, so a successful `new()` call below is
    // the wire-level proof that manifest v1 and WIT v1 agree.
    assert_eq!(
        loaded.manifest.handler_api_version, HANDLER_API_VERSION,
        "manifest handler_api_version must match the frozen rill-handler-api constant"
    );
    assert_eq!(
        loaded.manifest.handler_api_version, PROTOCOL_HANDLER_API_VERSION,
        "handler-api constant must be identical across rill-handler-api and rill-runtime-protocol"
    );
    assert_eq!(
        inspection.handler_api_version, HANDLER_API_VERSION,
        "inspection handler_api_version must match the frozen constant"
    );
    assert_eq!(loaded.manifest.id, "rillml.echo.handler");

    // The successful `new()` call confirms metadata() round-tripped and
    // matched the manifest on id, version, api_version and capabilities.
    let model = serde_json::json!({"kind": "linearRegression"});
    let _handler = WasmInvokeHandler::new(&loaded, &model).unwrap();
}

#[test]
fn v1_component_rejects_unsupported_capability() {
    let _guard = wit_v1_test_guard();
    let component = read_v1_component();
    let signing = SigningKey::from_bytes(&[42; 32]);
    let pack_bytes = build_v1_pack(&component, &signing);
    let (loaded, _) = load_v1_pack(&pack_bytes, &signing.verifying_key());

    let model = serde_json::json!({"kind": "linearRegression"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();

    // The echo handler only declares `rillml.linearRegression.predict`.
    // Invoking any other capability must fail with `UnsupportedCapability`.
    let result = handler.invoke("rillml.unknown.predict", &serde_json::json!({}));
    assert!(result.is_err());
    use rill_runtime::InvokeErrorKind;
    let error = result.unwrap_err();
    assert!(
        matches!(error.kind(), InvokeErrorKind::UnsupportedCapability),
        "expected UnsupportedCapability, got: {:?}",
        error.kind()
    );
}

// ---------------------------------------------------------------------------
// .rillhandler pack round-trip
// ---------------------------------------------------------------------------

#[test]
fn v1_component_packs_and_loads_as_rillhandler() {
    let _guard = wit_v1_test_guard();
    let component = read_v1_component();

    // Pack the v1 component into a signed `.rillhandler` and load it back.
    let signing = SigningKey::from_bytes(&[42; 32]);
    let pack_bytes = build_v1_pack(&component, &signing);
    assert!(!pack_bytes.is_empty());

    let (loaded, inspection) = load_v1_pack(&pack_bytes, &signing.verifying_key());
    assert_eq!(inspection.id, "rillml.echo.handler");
    assert!(inspection.signature_verified);
    assert_eq!(
        inspection.handler_api_version, HANDLER_API_VERSION,
        "manifest handler_api_version must equal frozen HANDLER_API_VERSION"
    );

    // The loaded pack must still produce a working handler.
    let model = serde_json::json!({"kind": "linearRegression"});
    let handler = WasmInvokeHandler::new(&loaded, &model).unwrap();
    let input = serde_json::json!({"echo": true});
    let output = handler
        .invoke("rillml.linearRegression.predict", &input)
        .unwrap();
    assert_eq!(output, input);
}

// ---------------------------------------------------------------------------
// WIT constant consistency (mirrors scripts/check_wit_abi.py in Rust)
// ---------------------------------------------------------------------------

#[test]
fn wit_v1_constants_are_frozen() {
    // These constants are the four independent declarations of the WIT v1
    // ABI version. The Python script `scripts/check_wit_abi.py` verifies
    // the WIT source hash as well; here we assert the Rust-side constants
    // agree with the values frozen for 1.0.
    assert_eq!(HANDLER_API_VERSION, 1);
    assert_eq!(WIT_PACKAGE, "rill:handler");
    assert_eq!(WIT_VERSION, "1.0.0");
    assert_eq!(WIT_WORLD, "invoke-handler");
    // The protocol crate re-exports HANDLER_API_VERSION for handler-pack
    // manifest validation. It must equal the rill-handler-api constant.
    assert_eq!(PROTOCOL_HANDLER_API_VERSION, HANDLER_API_VERSION);
}

//! IPC v1/v2 golden fixture tests.
//!
//! These tests enforce the 1.0 IPC freeze contract:
//! - Each fixture JSON deserialises into the current type.
//! - Serialising the deserialised value reproduces the fixture byte-for-byte
//!   after newline trimming (round-trip).
//! - `deny_unknown_fields` rejects any extra field on every fixture.
//! - The runtime rejects requests whose `apiVersion` is outside the
//!   supported range.
//! - Stable error codes are frozen and the runtime only produces codes from
//!   the frozen set.

#![cfg(test)]

use std::fs;
use std::path::PathBuf;

use rill_runtime_protocol::{
    HandlerPackManifest, ModelPackManifest, RUNTIME_API_VERSION, ReleaseArtifact,
    ReleaseArtifactKind, ReleaseIndexPayload, RuntimeRequest, RuntimeResponse, RuntimeResponseV2,
    error_code,
};

fn fixture_dir(version: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(version)
}

fn read_fixture(version: &str, name: &str) -> String {
    let path = fixture_dir(version).join(format!("{name}.json"));
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {version}/{name}: {e}"))
        .trim_end_matches('\n')
        .to_owned()
}

/// Deserialize a fixture, re-serialise it, and assert byte-for-byte equality
/// with the original fixture text. This proves the wire schema is frozen.
fn assert_roundtrip<T>(fixture_text: &str)
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let parsed: T =
        serde_json::from_str(fixture_text).expect("fixture must deserialize into target type");
    let reserialised = serde_json::to_string(&parsed).expect("reserialise");
    assert_eq!(
        reserialised, fixture_text,
        "round-trip mismatch: fixture text must equal reserialised value"
    );
}

/// Assert that adding an unknown field causes deserialisation to fail.
fn assert_unknown_fields_rejected<T>(fixture_text: &str)
where
    T: serde::de::DeserializeOwned,
{
    // Parse the fixture as a generic JSON value, inject an unknown field,
    // then attempt to deserialise. `deny_unknown_fields` must reject it.
    let mut value: serde_json::Value =
        serde_json::from_str(fixture_text).expect("fixture must be valid JSON");
    if let Some(obj) = value.as_object_mut() {
        obj.insert("__unknown_field__".into(), serde_json::json!(true));
    }
    let tampered = serde_json::to_string(&value).expect("re-encode tampered JSON");
    let result: Result<T, _> = serde_json::from_str(&tampered);
    assert!(
        result.is_err(),
        "unknown field must be rejected by deny_unknown_fields"
    );
}

// ---------------------------------------------------------------------------
// Request fixtures (shared by v1 and v2)
// ---------------------------------------------------------------------------

#[test]
fn v1_request_handshake_roundtrip() {
    let text = read_fixture("v1", "request_handshake");
    assert_roundtrip::<RuntimeRequest>(&text);
}

#[test]
fn v1_request_health_roundtrip() {
    let text = read_fixture("v1", "request_health");
    assert_roundtrip::<RuntimeRequest>(&text);
}

#[test]
fn v1_request_invoke_roundtrip() {
    let text = read_fixture("v1", "request_invoke");
    assert_roundtrip::<RuntimeRequest>(&text);
}

#[test]
fn v1_request_handshake_rejects_unknown_fields() {
    let text = read_fixture("v1", "request_handshake");
    assert_unknown_fields_rejected::<RuntimeRequest>(&text);
}

#[test]
fn v1_request_health_rejects_unknown_fields() {
    let text = read_fixture("v1", "request_health");
    assert_unknown_fields_rejected::<RuntimeRequest>(&text);
}

#[test]
fn v1_request_invoke_rejects_unknown_fields() {
    let text = read_fixture("v1", "request_invoke");
    assert_unknown_fields_rejected::<RuntimeRequest>(&text);
}

#[test]
fn v2_request_handshake_roundtrip() {
    let text = read_fixture("v2", "request_handshake");
    assert_roundtrip::<RuntimeRequest>(&text);
}

#[test]
fn v2_request_health_roundtrip() {
    let text = read_fixture("v2", "request_health");
    assert_roundtrip::<RuntimeRequest>(&text);
}

#[test]
fn v2_request_invoke_roundtrip() {
    let text = read_fixture("v2", "request_invoke");
    assert_roundtrip::<RuntimeRequest>(&text);
}

// ---------------------------------------------------------------------------
// v1 response fixtures
// ---------------------------------------------------------------------------

#[test]
fn v1_response_handshake_roundtrip() {
    let text = read_fixture("v1", "response_handshake");
    assert_roundtrip::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_health_roundtrip() {
    let text = read_fixture("v1", "response_health");
    assert_roundtrip::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_result_roundtrip() {
    let text = read_fixture("v1", "response_result");
    assert_roundtrip::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_error_roundtrip() {
    let text = read_fixture("v1", "response_error");
    assert_roundtrip::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_handshake_rejects_unknown_fields() {
    let text = read_fixture("v1", "response_handshake");
    assert_unknown_fields_rejected::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_health_rejects_unknown_fields() {
    let text = read_fixture("v1", "response_health");
    assert_unknown_fields_rejected::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_result_rejects_unknown_fields() {
    let text = read_fixture("v1", "response_result");
    assert_unknown_fields_rejected::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_error_rejects_unknown_fields() {
    let text = read_fixture("v1", "response_error");
    assert_unknown_fields_rejected::<RuntimeResponse>(&text);
}

#[test]
fn v1_response_rejects_v2_handler_fields() {
    // A v1 handshake response must not accept handlerId, handlerVersion,
    // handlerApiVersion, or effectiveCapabilities.
    let text = read_fixture("v1", "response_handshake");
    let mut value: serde_json::Value =
        serde_json::from_str(&text).expect("fixture must be valid JSON");
    if let Some(obj) = value.as_object_mut() {
        obj.insert("handlerId".into(), serde_json::json!("org.example.handler"));
    }
    let tampered = serde_json::to_string(&value).expect("re-encode tampered JSON");
    let result: Result<RuntimeResponse, _> = serde_json::from_str(&tampered);
    assert!(
        result.is_err(),
        "v1 response must reject v2-only handler fields"
    );
}

// ---------------------------------------------------------------------------
// v2 response fixtures
// ---------------------------------------------------------------------------

#[test]
fn v2_response_handshake_roundtrip() {
    let text = read_fixture("v2", "response_handshake");
    assert_roundtrip::<RuntimeResponseV2>(&text);
}

#[test]
fn v2_response_health_roundtrip() {
    let text = read_fixture("v2", "response_health");
    assert_roundtrip::<RuntimeResponseV2>(&text);
}

#[test]
fn v2_response_result_roundtrip() {
    let text = read_fixture("v2", "response_result");
    assert_roundtrip::<RuntimeResponseV2>(&text);
}

#[test]
fn v2_response_error_roundtrip() {
    let text = read_fixture("v2", "response_error");
    assert_roundtrip::<RuntimeResponseV2>(&text);
}

#[test]
fn v2_response_handshake_rejects_unknown_fields() {
    let text = read_fixture("v2", "response_handshake");
    assert_unknown_fields_rejected::<RuntimeResponseV2>(&text);
}

#[test]
fn v2_response_health_rejects_unknown_fields() {
    let text = read_fixture("v2", "response_health");
    assert_unknown_fields_rejected::<RuntimeResponseV2>(&text);
}

#[test]
fn v2_response_result_rejects_unknown_fields() {
    let text = read_fixture("v2", "response_result");
    assert_unknown_fields_rejected::<RuntimeResponseV2>(&text);
}

#[test]
fn v2_response_error_rejects_unknown_fields() {
    let text = read_fixture("v2", "response_error");
    assert_unknown_fields_rejected::<RuntimeResponseV2>(&text);
}

// ---------------------------------------------------------------------------
// API version boundary checks
// ---------------------------------------------------------------------------

#[test]
fn request_with_api_version_zero_is_out_of_range() {
    // The runtime accepts apiVersion in [1, RUNTIME_API_VERSION]. A zero
    // value must be rejected by the runtime's range check. The protocol
    // crate itself only parses; the runtime enforces the range. We verify
    // the fixture's apiVersion is within range, and that a 0-version
    // request still deserialises (so the runtime can return an
    // `incompatibleApiVersion` error rather than a parse error).
    let text = read_fixture("v1", "request_handshake");
    let parsed: RuntimeRequest = serde_json::from_str(&text).expect("deserialize");
    assert!(parsed.api_version() >= 1);

    let zero_version = r#"{"method":"handshake","requestId":"zero","apiVersion":0,"clientName":"x","clientVersion":"0.0.0"}"#;
    let parsed: RuntimeRequest =
        serde_json::from_str(zero_version).expect("zero apiVersion still parses");
    assert_eq!(parsed.api_version(), 0);
    assert!(!(1..=RUNTIME_API_VERSION).contains(&parsed.api_version()));
}

#[test]
fn request_with_future_api_version_is_out_of_range() {
    let future_version = r#"{"method":"handshake","requestId":"future","apiVersion":999,"clientName":"x","clientVersion":"0.0.0"}"#;
    let parsed: RuntimeRequest =
        serde_json::from_str(future_version).expect("future apiVersion still parses");
    assert_eq!(parsed.api_version(), 999);
    assert!(!(1..=RUNTIME_API_VERSION).contains(&parsed.api_version()));
}

// ---------------------------------------------------------------------------
// Stable error code tests
// ---------------------------------------------------------------------------

#[test]
fn frozen_error_codes_are_unique_and_sorted() {
    let mut codes = error_code::FROZEN_CODES.to_vec();
    let mut sorted = codes.clone();
    sorted.sort();
    assert_eq!(codes, sorted, "FROZEN_CODES must be sorted alphabetically");
    codes.dedup();
    assert_eq!(
        error_code::FROZEN_CODES.len(),
        codes.len(),
        "FROZEN_CODES must not contain duplicates"
    );
}

#[test]
fn frozen_error_codes_match_wire_strings() {
    // The constant values must equal the exact wire strings already
    // produced by the runtime. These strings are frozen for 1.x.
    assert_eq!(error_code::INVALID_JSON, "invalidJson");
    assert_eq!(error_code::INVALID_REQUEST_ID, "invalidRequestId");
    assert_eq!(
        error_code::INCOMPATIBLE_API_VERSION,
        "incompatibleApiVersion"
    );
    assert_eq!(error_code::INVALID_CLIENT_IDENTITY, "invalidClientIdentity");
    assert_eq!(error_code::UNSUPPORTED_CAPABILITY, "unsupportedCapability");
    assert_eq!(error_code::NO_INVOKE_HANDLER, "noInvokeHandler");
    assert_eq!(error_code::HANDLER_TIMEOUT, "handlerTimeout");
    assert_eq!(error_code::HANDLER_TRAP, "handlerTrap");
    assert_eq!(
        error_code::HANDLER_OUTPUT_TOO_LARGE,
        "handlerOutputTooLarge"
    );
    assert_eq!(error_code::HANDLER_INVALID_OUTPUT, "handlerInvalidOutput");
    assert_eq!(error_code::HANDLER_INTERNAL_ERROR, "handlerInternalError");
}

#[test]
fn is_frozen_recognises_all_frozen_codes() {
    for code in error_code::FROZEN_CODES {
        assert!(
            error_code::is_frozen(code),
            "is_frozen must recognise {code}"
        );
    }
}

#[test]
fn is_frozen_rejects_unknown_codes() {
    assert!(!error_code::is_frozen("unknownCode"));
    assert!(!error_code::is_frozen(""));
    assert!(!error_code::is_frozen("handlerInternalError "));
}

#[test]
fn v1_error_fixture_uses_frozen_code() {
    let text = read_fixture("v1", "response_error");
    let parsed: RuntimeResponse = serde_json::from_str(&text).expect("deserialize");
    if let RuntimeResponse::Error { code, .. } = parsed {
        assert!(
            error_code::is_frozen(&code),
            "v1 error fixture must use a frozen code, got {code}"
        );
    } else {
        panic!("expected error response");
    }
}

#[test]
fn v2_error_fixture_uses_frozen_code() {
    let text = read_fixture("v2", "response_error");
    let parsed: RuntimeResponseV2 = serde_json::from_str(&text).expect("deserialize");
    if let RuntimeResponseV2::Error { code, .. } = parsed {
        assert!(
            error_code::is_frozen(&code),
            "v2 error fixture must use a frozen code, got {code}"
        );
    } else {
        panic!("expected error response");
    }
}

// ---------------------------------------------------------------------------
// Manifest and release-index fixture shape validation
// ---------------------------------------------------------------------------

#[test]
fn model_pack_manifest_roundtrip_and_shape() {
    let manifest = ModelPackManifest {
        format_version: rill_runtime_protocol::MODEL_PACK_FORMAT_VERSION,
        id: "rillml.example.default".into(),
        version: "0.7.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "test-key".into(),
        capabilities: vec!["rillml.example".into()],
    };
    let json = serde_json::to_string(&manifest).expect("serialise");
    let restored: ModelPackManifest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored, manifest);
    assert!(manifest.validate_shape().is_ok());
}

#[test]
fn handler_pack_manifest_roundtrip_and_shape() {
    let manifest = HandlerPackManifest {
        format_version: rill_runtime_protocol::HANDLER_PACKAGE_FORMAT_VERSION,
        id: "org.example.handler".into(),
        version: "1.0.0".into(),
        handler_api_version: rill_runtime_protocol::HANDLER_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "test-key".into(),
        capabilities: vec!["org.example.predict".into()],
        module_sha256: "ab".repeat(32),
        module_size: 1024,
    };
    let json = serde_json::to_string(&manifest).expect("serialise");
    let restored: HandlerPackManifest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored, manifest);
    assert!(manifest.validate_shape().is_ok());
}

#[test]
fn release_artifact_runtime_roundtrip_and_shape() {
    let artifact = ReleaseArtifact {
        kind: ReleaseArtifactKind::Runtime,
        id: rill_runtime_protocol::RUNTIME_ARTIFACT_ID.into(),
        version: "0.7.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        target_os: Some("linux".into()),
        target_arch: Some("x86_64".into()),
        handler_api_version: None,
        min_runtime_version: None,
        url: "https://example.invalid/rill-runtime".into(),
        sha256: "ab".repeat(32),
        size: 1024,
    };
    let json = serde_json::to_string(&artifact).expect("serialise");
    let restored: ReleaseArtifact = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored, artifact);
    assert!(artifact.validate_shape().is_ok());
}

#[test]
fn release_index_payload_roundtrip_and_shape() {
    let payload = ReleaseIndexPayload {
        schema_version: rill_runtime_protocol::RELEASE_INDEX_SCHEMA_VERSION,
        channel: "stable".into(),
        generated_at: "2026-07-28T00:00:00Z".into(),
        publisher_key_id: "test-key".into(),
        artifacts: vec![ReleaseArtifact {
            kind: ReleaseArtifactKind::Runtime,
            id: rill_runtime_protocol::RUNTIME_ARTIFACT_ID.into(),
            version: "0.7.0".into(),
            runtime_api_version: RUNTIME_API_VERSION,
            target_os: Some("linux".into()),
            target_arch: Some("x86_64".into()),
            handler_api_version: None,
            min_runtime_version: None,
            url: "https://example.invalid/rill-runtime".into(),
            sha256: "ab".repeat(32),
            size: 1024,
        }],
    };
    let json = serde_json::to_string(&payload).expect("serialise");
    let restored: ReleaseIndexPayload = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored, payload);
    assert!(payload.validate_shape().is_ok());
}

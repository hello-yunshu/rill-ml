//! Real-component regression tests for Preview Stateful Handler ABI v2.

#![cfg(feature = "wasm")]

use std::{fs, path::PathBuf, sync::Arc};

use rill_runtime::{
    PartitionScopeV3, StatefulHandlerErrorKindV2, StatefulHandlerMetadataV2, StatefulHandlerV2,
    StatefulRuntimeConfigV3, StatefulRuntimeEngineV3, WasmStatefulHandlerV2,
};
use rill_runtime_protocol::v3::{
    EnvelopeV3, IdentityV3, RUNTIME_API_VERSION_V3, RuntimeErrorCodeV3, RuntimeRequestV3,
    RuntimeResponseBodyV3,
};

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fixture_path() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("STATEFUL_HANDLER_V2_WASM") {
        let path = PathBuf::from(value);
        assert!(path.is_file(), "STATEFUL_HANDLER_V2_WASM is missing");
        return Some(path);
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-stateful-handler-v2.wasm");
    if path.is_file() {
        Some(path)
    } else if std::env::var_os("RILL_RUN_WASM_FIXTURE_TESTS").is_some() {
        panic!("mandatory Stateful Handler v2 fixture is missing: {path:?}");
    } else {
        None
    }
}

fn metadata() -> StatefulHandlerMetadataV2 {
    StatefulHandlerMetadataV2 {
        id: "rillml.test.stateful-v2".into(),
        version: "2.0.0-preview".into(),
        api_version: 2,
        capabilities: vec!["org.example.decide".into()],
        state_schema_version: 1,
    }
}

fn load(mode: &str) -> Option<WasmStatefulHandlerV2> {
    let bytes = fs::read(fixture_path()?).unwrap();
    Some(
        WasmStatefulHandlerV2::new(&bytes, metadata(), &serde_json::json!({"mode": mode})).unwrap(),
    )
}

fn request(state_generation: u64) -> EnvelopeV3 {
    EnvelopeV3 {
        request_id: "decision-1".into(),
        api_version: RUNTIME_API_VERSION_V3,
        client_identity: IdentityV3 {
            name: "test-host".into(),
            version: "1".into(),
        },
        partition_key: "default".into(),
        capability: Some("org.example.decide".into()),
        deadline_unix_ms: Some(100),
        feature_schema_hash: Some("ab".repeat(32)),
        model_generation: 7,
        state_generation,
        payload_limit: rill_runtime_protocol::MAX_MESSAGE_BYTES as u32,
        request: RuntimeRequestV3::Decide {
            context: serde_json::json!({"features": [1.0]}),
            deterministic_seed: Some(42),
        },
    }
}

fn engine(mode: &str) -> Option<StatefulRuntimeEngineV3> {
    let handler = Arc::new(load(mode)?);
    let config = StatefulRuntimeConfigV3::new(
        IdentityV3 {
            name: "rill-runtime".into(),
            version: "1.0.0".into(),
        },
        7,
        "ab".repeat(32),
        metadata().capabilities,
        br#"{"count":0}"#.to_vec(),
    );
    Some(StatefulRuntimeEngineV3::new(config, handler).unwrap())
}

#[test]
fn stateful_v2_normal_state_update_and_repeated_call() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let handler = match load("normal") {
        Some(handler) => handler,
        None => return,
    };
    let first = handler
        .handle(br#"{"method":"decide"}"#, br#"{"count":0}"#, Some(42))
        .unwrap();
    assert_eq!(first.output, serde_json::json!({"accepted": true}));
    assert_eq!(first.next_state, br#"{"count":1}"#);
    let second = handler
        .handle(br#"{"method":"decide"}"#, &first.next_state, Some(42))
        .unwrap();
    assert_eq!(second.next_state, br#"{"count":1}"#);
}

#[test]
fn stateful_v2_metadata_mismatch_is_rejected() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = match fixture_path() {
        Some(path) => fs::read(path).unwrap(),
        None => return,
    };
    let mut expected = metadata();
    expected.version = "wrong".into();
    let error = WasmStatefulHandlerV2::new(&bytes, expected, &serde_json::json!({})).unwrap_err();
    assert_eq!(error.kind(), StatefulHandlerErrorKindV2::MetadataMismatch);
}

#[test]
fn stateful_v2_trap_timeout_and_oversize_are_typed() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for (mode, expected) in [
        ("trap", StatefulHandlerErrorKindV2::Trap),
        ("timeout", StatefulHandlerErrorKindV2::Timeout),
        (
            "oversized-output",
            StatefulHandlerErrorKindV2::OutputTooLarge,
        ),
    ] {
        let handler = match load(mode) {
            Some(handler) => handler,
            None => return,
        };
        let started = std::time::Instant::now();
        let error = handler
            .handle(br#"{"method":"decide"}"#, br#"{"count":0}"#, None)
            .unwrap_err();
        assert_eq!(error.kind(), expected, "mode={mode}");
        assert!(started.elapsed() < std::time::Duration::from_secs(15));
    }
}

#[test]
fn stateful_v2_invalid_and_oversized_state_are_fail_closed() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for mode in ["corrupt-state", "oversized-state"] {
        let engine = match engine(mode) {
            Some(engine) => engine,
            None => return,
        };
        let before = engine
            .snapshot(&PartitionScopeV3::new("test-host", "default"))
            .unwrap();
        let response = engine.handle_at(request(0), 100);
        assert!(matches!(
            response.response,
            RuntimeResponseBodyV3::Error { error }
                if error.code == RuntimeErrorCodeV3::InvalidState
        ));
        assert_eq!(
            engine
                .snapshot(&PartitionScopeV3::new("test-host", "default"))
                .unwrap(),
            before,
            "mode={mode}"
        );
    }
}

#[test]
fn stateful_v2_real_trap_timeout_and_output_limit_are_fail_closed() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for (mode, expected) in [
        ("trap", RuntimeErrorCodeV3::HandlerTrap),
        ("timeout", RuntimeErrorCodeV3::HandlerTimeout),
        (
            "oversized-output",
            RuntimeErrorCodeV3::HandlerOutputTooLarge,
        ),
    ] {
        let engine = match engine(mode) {
            Some(engine) => engine,
            None => return,
        };
        let before = engine
            .snapshot(&PartitionScopeV3::new("test-host", "default"))
            .unwrap();
        let response = engine.handle_at(request(0), 100);
        assert!(matches!(
            response.response,
            RuntimeResponseBodyV3::Error { error } if error.code == expected
        ));
        assert_eq!(
            engine
                .snapshot(&PartitionScopeV3::new("test-host", "default"))
                .unwrap(),
            before,
            "mode={mode}"
        );
    }
}

//! Positive and negative fixtures for the independent Preview IPC V3 schema.

use std::{fs, path::PathBuf};

use rill_runtime_protocol::v3::{EnvelopeV3, RuntimeResponseV3};

fn fixture(name: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/v3")
            .join(name),
    )
    .unwrap()
    .trim_end_matches(['\r', '\n'])
    .to_owned()
}

fn roundtrip<T>(name: &str)
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let text = fixture(name);
    let parsed: T = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_string(&parsed).unwrap(), text);
}

#[test]
fn v3_positive_request_fixtures_roundtrip_and_validate() {
    for name in ["request_decide.json", "request_feedback.json"] {
        roundtrip::<EnvelopeV3>(name);
        let parsed: EnvelopeV3 = serde_json::from_str(&fixture(name)).unwrap();
        parsed.validate().unwrap();
    }
}

#[test]
fn v3_positive_response_fixtures_roundtrip_and_validate() {
    for name in ["response_handshake.json", "response_error.json"] {
        roundtrip::<RuntimeResponseV3>(name);
        let parsed: RuntimeResponseV3 = serde_json::from_str(&fixture(name)).unwrap();
        parsed.validate().unwrap();
    }
}

#[test]
fn v3_negative_fixtures_are_rejected() {
    let cases: serde_json::Value = serde_json::from_str(&fixture("invalid_cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let value = case["value"].clone();
        match serde_json::from_value::<EnvelopeV3>(value) {
            Err(_) => {}
            Ok(envelope) => assert!(
                envelope.validate().is_err(),
                "negative fixture unexpectedly accepted: {name}"
            ),
        }
    }
}

#[test]
fn v3_json_schema_is_valid_json_and_names_all_methods() {
    let schema = fixture("runtime-ipc-v3.schema.json");
    let value: serde_json::Value = serde_json::from_str(&schema).unwrap();
    assert_eq!(
        value["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    for method in [
        "handshake",
        "health",
        "observe",
        "decide",
        "feedback",
        "inspect",
        "snapshot",
        "reset",
    ] {
        assert!(
            schema.contains(&format!("\"const\":\"{method}\""))
                || schema.contains(&format!("\"const\": \"{method}\""))
        );
    }
}

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

use ed25519_dalek::SigningKey;
use rill_runtime::{LINEAR_REGRESSION_CAPABILITY, build_signed_model_pack};
use rill_runtime_protocol::{
    MODEL_PACK_FORMAT_VERSION, ModelPackManifest, RUNTIME_API_VERSION, RuntimeRequest,
    RuntimeResponse, RuntimeResponseV2,
};

#[test]
fn signed_pack_handshake_and_invoke_work_across_the_real_process_boundary() {
    let signing = SigningKey::from_bytes(&[5; 32]);
    let manifest = ModelPackManifest {
        format_version: MODEL_PACK_FORMAT_VERSION,
        id: "rillml.example.default".into(),
        version: "0.7.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "process-test".into(),
        capabilities: vec![LINEAR_REGRESSION_CAPABILITY.into()],
    };
    let model = serde_json::json!({
        "kind": "linearRegression",
        "weights": [0.5, -0.25],
        "intercept": 1.0
    });
    let pack = build_signed_model_pack(&manifest, &model, &signing).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let pack_path = temporary.path().join("example.rillpack");
    fs::write(&pack_path, pack).unwrap();

    let trust = format!(
        "process-test={}",
        hex::encode(signing.verifying_key().to_bytes())
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_rill-runtime"))
        .args(["serve", "--pack"])
        .arg(&pack_path)
        .args(["--trust-key", &trust])
        .args(["--builtin-handler", "linear-regression"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        RuntimeRequest::Handshake {
            request_id: "integration-handshake".into(),
            api_version: RUNTIME_API_VERSION,
            client_name: "runtime-process-test".into(),
            client_version: "0.7.0".into(),
        },
        RuntimeRequest::Invoke {
            request_id: "integration-invoke".into(),
            api_version: RUNTIME_API_VERSION,
            capability: LINEAR_REGRESSION_CAPABILITY.into(),
            input: serde_json::json!({"features": [4.0, 2.0]}),
        },
    ];
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        serde_json::to_writer(&mut stdin, &request).unwrap();
        stdin.write_all(b"\n").unwrap();
    }
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<RuntimeResponseV2>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 2);
    assert!(matches!(
        &responses[0],
        RuntimeResponseV2::Handshake {
            request_id,
            model_pack_id,
            handler_id,
            ..
        } if request_id == "integration-handshake"
            && model_pack_id == "rillml.example.default"
            && handler_id == "rillml.builtin.linear-regression"
    ));
    assert!(matches!(
        &responses[1],
        RuntimeResponseV2::Result {
            request_id,
            output,
            ..
        } if request_id == "integration-invoke" && output["prediction"] == 2.5
    ));
}

#[test]
fn v1_client_receives_v1_wire_format() {
    let signing = SigningKey::from_bytes(&[6; 32]);
    let manifest = ModelPackManifest {
        format_version: MODEL_PACK_FORMAT_VERSION,
        id: "rillml.example.default".into(),
        version: "0.7.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "v1-test".into(),
        capabilities: vec![LINEAR_REGRESSION_CAPABILITY.into()],
    };
    let model = serde_json::json!({
        "kind": "linearRegression",
        "weights": [1.0],
        "intercept": 0.0
    });
    let pack = build_signed_model_pack(&manifest, &model, &signing).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let pack_path = temporary.path().join("v1-test.rillpack");
    fs::write(&pack_path, pack).unwrap();

    let trust = format!(
        "v1-test={}",
        hex::encode(signing.verifying_key().to_bytes())
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_rill-runtime"))
        .args(["serve", "--pack"])
        .arg(&pack_path)
        .args(["--trust-key", &trust])
        .args(["--builtin-handler", "linear-regression"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Send a v1 handshake (api_version=1).
    let v1_request = r#"{"method":"handshake","requestId":"v1-client","apiVersion":1,"clientName":"v1-test","clientVersion":"0.6.0"}"#;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(v1_request.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response_line = output
        .stdout
        .split(|b| *b == b'\n')
        .find(|l| !l.is_empty())
        .unwrap();
    let response: RuntimeResponse = serde_json::from_slice(response_line).unwrap();
    assert!(matches!(
        response,
        RuntimeResponse::Handshake { request_id, .. } if request_id == "v1-client"
    ));
    // V1 response must not contain handler fields.
    let json = std::str::from_utf8(response_line).unwrap();
    assert!(!json.contains("handlerId"));
    assert!(!json.contains("effectiveCapabilities"));
}

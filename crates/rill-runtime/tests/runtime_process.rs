use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

use ed25519_dalek::SigningKey;
use rill_runtime::build_signed_model_pack;
use rill_runtime_protocol::{
    MODEL_PACK_FORMAT_VERSION, ModelPackManifest, RUNTIME_API_VERSION, RuntimeRequest,
    RuntimeResponse,
};

#[test]
fn signed_pack_handshake_and_invoke_work_across_the_real_process_boundary() {
    let signing = SigningKey::from_bytes(&[5; 32]);
    let manifest = ModelPackManifest {
        format_version: MODEL_PACK_FORMAT_VERSION,
        id: "rillml.example.default".into(),
        version: "0.5.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        min_runtime_version: "0.5.0".into(),
        publisher_key_id: "process-test".into(),
        capabilities: vec!["rillml.example".into()],
    };
    let pack = build_signed_model_pack(&manifest, &serde_json::json!({}), &signing).unwrap();
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
            client_version: "0.5.0".into(),
        },
        RuntimeRequest::Invoke {
            request_id: "integration-invoke".into(),
            api_version: RUNTIME_API_VERSION,
            capability: "rillml.example".into(),
            input: serde_json::json!({}),
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
        .map(|line| serde_json::from_slice::<RuntimeResponse>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 2);
    assert!(matches!(
        &responses[0],
        RuntimeResponse::Handshake {
            request_id,
            model_pack_id,
            ..
        } if request_id == "integration-handshake" && model_pack_id == "rillml.example.default"
    ));
    assert!(matches!(
        &responses[1],
        RuntimeResponse::Error {
            request_id,
            code,
            ..
        } if request_id == "integration-invoke" && code == "noInvokeHandler"
    ));
}

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

use ed25519_dalek::SigningKey;
use rill_runtime::{LINEAR_REGRESSION_CAPABILITY, build_signed_model_pack};
use rill_runtime_protocol::v3::{EnvelopeV3, IdentityV3, RUNTIME_API_VERSION_V3, RuntimeRequestV3};
use rill_runtime_protocol::{
    MODEL_PACK_FORMAT_VERSION, ModelPackManifest, RUNTIME_API_VERSION, RuntimeRequest,
    RuntimeResponse, RuntimeResponseV2,
};

// Imports only needed by the `wasm`-gated cross-process WASM handler test.
#[cfg(feature = "wasm")]
use rill_runtime::build_signed_handler_pack;
#[cfg(feature = "wasm")]
use rill_runtime_protocol::{
    HANDLER_API_VERSION, HANDLER_PACKAGE_FORMAT_VERSION, HandlerPackManifest,
};
#[cfg(feature = "wasm")]
use sha2::{Digest, Sha256};
#[cfg(feature = "wasm")]
use std::path::PathBuf;

/// Build a `Command` that runs the `rill-runtime` CLI binary.
///
/// Under the Actions-first cross-architecture gates the crate test suite runs
/// inside QEMU user-mode. QEMU does NOT intercept an emulated process's
/// `execve` of a *foreign* ELF: it lets the host kernel execute the new image,
/// which fails with `ENOEXEC` unless binfmt_misc is registered (unavailable on
/// the CI host). The child therefore dies immediately, and because Rust's
/// `Command::spawn` goes through glibc `posix_spawn` -- whose error channel is
/// broken under QEMU's `CLONE_VFORK`-as-`fork` emulation -- the parent sees a
/// `Broken pipe` when it writes to the child's stdin.
///
/// The gate exports `RILL_RUNTIME_EXEC_PREFIX` = the host-NATIVE QEMU
/// interpreter for the target (e.g. `qemu-riscv64 -cpu rv64 -L
/// /usr/riscv64-linux-gnu`). A guest exec'ing a host-native binary passes
/// through to the host kernel and runs it natively (QEMU's documented
/// behaviour), so the child reliably starts under QEMU with the same
/// sysroot/-cpu config and the spawned `rill-runtime` is emulated correctly.
/// On native hosts the variable is unset and the binary runs directly --
/// identical to the previous behaviour.
fn runtime_command() -> Command {
    let bin = env!("CARGO_BIN_EXE_rill-runtime");
    match std::env::var_os("RILL_RUNTIME_EXEC_PREFIX") {
        Some(prefix) if !prefix.is_empty() => {
            let prefix = prefix.to_string_lossy();
            let mut parts = prefix.split_whitespace().map(str::to_owned);
            let mut cmd = Command::new(parts.next().expect("RILL_RUNTIME_EXEC_PREFIX is empty"));
            for part in parts {
                cmd.arg(part);
            }
            cmd.arg(bin);
            cmd
        }
        _ => Command::new(bin),
    }
}

fn preview_envelope(
    request_id: &str,
    capability: Option<&str>,
    state_generation: u64,
    request: RuntimeRequestV3,
) -> EnvelopeV3 {
    EnvelopeV3 {
        request_id: request_id.into(),
        api_version: RUNTIME_API_VERSION_V3,
        client_identity: IdentityV3 {
            name: "process-test-host".into(),
            version: "1".into(),
        },
        capability: capability.map(str::to_owned),
        deadline_unix_ms: None,
        feature_schema_hash: capability
            .map(|_| "99c44934c1bfca8fdffb93122d29418d7fe7eb0d81d80f8b2bf4fbdd153151ab".into()),
        model_generation: 0,
        state_generation,
        payload_limit: rill_runtime_protocol::MAX_MESSAGE_BYTES as u32,
        request,
    }
}

#[test]
fn preview_v3_real_subprocess_restart_and_feedback_ledger() {
    let temporary = tempfile::tempdir().unwrap();
    let state_path = temporary.path().join("preview-state.json");
    let handshake = preview_envelope("hello", None, 0, RuntimeRequestV3::Handshake {});
    let decide = preview_envelope(
        "decision-1",
        Some("org.rill.preview.decide"),
        0,
        RuntimeRequestV3::Decide {
            context: serde_json::json!({"actions": [{"id":"route-a","features":[1.0]}]}),
            deterministic_seed: Some(7),
        },
    );
    let mut first = runtime_command()
        .args(["preview-serve", "--state"])
        .arg(&state_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = first.stdin.as_mut().unwrap();
        for request in [&handshake, &decide] {
            serde_json::to_writer(&mut *stdin, request).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
    }
    let first_output = first.wait_with_output().unwrap();
    assert!(
        first_output.status.success(),
        "{}",
        String::from_utf8_lossy(&first_output.stderr)
    );
    let first_lines = first_output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(first_lines.len(), 2);
    assert_eq!(first_lines[0]["response"]["channel"], "preview");
    assert_eq!(first_lines[1]["response"]["decisionId"], "decision-1");
    assert!(state_path.is_file());

    let feedback = preview_envelope(
        "feedback-1",
        Some("org.rill.preview.feedback"),
        1,
        RuntimeRequestV3::Feedback {
            decision_id: "decision-1".into(),
            selected_action_id: "route-a".into(),
            reward: 1.0,
            outcome_time_ms: 2,
            generation: 0,
        },
    );
    let duplicate = preview_envelope(
        "feedback-2",
        Some("org.rill.preview.feedback"),
        2,
        RuntimeRequestV3::Feedback {
            decision_id: "decision-1".into(),
            selected_action_id: "route-a".into(),
            reward: 1.0,
            outcome_time_ms: 2,
            generation: 0,
        },
    );
    let mut second = runtime_command()
        .args(["preview-serve", "--state"])
        .arg(&state_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = second.stdin.as_mut().unwrap();
        for request in [&feedback, &duplicate] {
            serde_json::to_writer(&mut *stdin, request).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
    }
    let second_output = second.wait_with_output().unwrap();
    assert!(
        second_output.status.success(),
        "{}",
        String::from_utf8_lossy(&second_output.stderr)
    );
    let second_lines = second_output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(second_lines[0]["response"]["kind"], "result");
    assert_eq!(
        second_lines[1]["response"]["error"]["code"],
        "duplicateFeedback"
    );
}

#[test]
fn preview_v3_uses_opaque_action_ids_and_context_features() {
    let temporary = tempfile::tempdir().unwrap();
    let state_path = temporary.path().join("contextual-state.json");
    let decide = preview_envelope(
        "contextual-decision",
        Some("org.rill.preview.decide"),
        0,
        RuntimeRequestV3::Decide {
            context: serde_json::json!({
                "actions": [
                    {"id": "opaque-a", "features": [1.0, 0.0]},
                    {"id": "opaque-b", "features": [0.0, 1.0]}
                ]
            }),
            deterministic_seed: Some(11),
        },
    );
    let mut process = runtime_command()
        .args(["preview-serve", "--state"])
        .arg(&state_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = process.stdin.as_mut().unwrap();
        serde_json::to_writer(&mut *stdin, &decide).unwrap();
        stdin.write_all(b"\n").unwrap();
        let feedback = preview_envelope(
            "contextual-feedback",
            Some("org.rill.preview.feedback"),
            1,
            RuntimeRequestV3::Feedback {
                decision_id: "contextual-decision".into(),
                selected_action_id: "opaque-a".into(),
                reward: 1.0,
                outcome_time_ms: 2,
                generation: 0,
            },
        );
        serde_json::to_writer(&mut *stdin, &feedback).unwrap();
        stdin.write_all(b"\n").unwrap();
    }
    let output = process.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        responses[0]["response"]["output"]["selectedActionId"],
        "opaque-a"
    );
    assert_eq!(
        responses[0]["response"]["output"]["scores"][0]["id"],
        "opaque-a"
    );
    assert_eq!(responses[1]["response"]["kind"], "result");
    let persisted =
        serde_json::from_slice::<serde_json::Value>(&fs::read(&state_path).unwrap()).unwrap();
    let state = &persisted["handlerSnapshot"]["state"];
    assert!(state.is_array());
}

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
    let mut child = runtime_command()
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
    let mut child = runtime_command()
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

// ----- R-021: compatibility tests -----

/// Returns the echo handler WASM component path, or `None` if not available.
#[cfg(feature = "wasm")]
fn echo_handler_component() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ECHO_HANDLER_WASM") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let workspace_target =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/echo-handler.wasm");
    if workspace_target.exists() {
        return Some(workspace_target);
    }
    None
}

#[test]
fn builtin_handler_deprecation_notice_printed() {
    // Starting the runtime with --builtin-handler linear-regression must print
    // a deprecation notice on stderr, guiding users toward --handler.
    let signing = SigningKey::from_bytes(&[51; 32]);
    let manifest = ModelPackManifest {
        format_version: MODEL_PACK_FORMAT_VERSION,
        id: "rillml.example.default".into(),
        version: "0.7.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "deprecate-test".into(),
        capabilities: vec![LINEAR_REGRESSION_CAPABILITY.into()],
    };
    let model = serde_json::json!({
        "kind": "linearRegression",
        "weights": [1.0],
        "intercept": 0.0
    });
    let pack = build_signed_model_pack(&manifest, &model, &signing).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let pack_path = temporary.path().join("deprecate.rillpack");
    fs::write(&pack_path, pack).unwrap();

    let trust = format!(
        "deprecate-test={}",
        hex::encode(signing.verifying_key().to_bytes())
    );
    let mut child = runtime_command()
        .args(["serve", "--pack"])
        .arg(&pack_path)
        .args(["--trust-key", &trust])
        .args(["--builtin-handler", "linear-regression"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Send a handshake so the process can exit cleanly.
    let request = RuntimeRequest::Handshake {
        request_id: "deprecate-notice-test".into(),
        api_version: RUNTIME_API_VERSION,
        client_name: "deprecate-test".into(),
        client_version: "0.7.0".into(),
    };
    let mut stdin = child.stdin.take().unwrap();
    serde_json::to_writer(&mut stdin, &request).unwrap();
    stdin.write_all(b"\n").unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("deprecated"),
        "expected deprecation notice on stderr, got: {stderr}"
    );
}

#[test]
#[cfg(feature = "wasm")]
fn wasm_handler_handshake_across_process_boundary() {
    // End-to-end test: start the runtime with a signed .rillhandler (WASM
    // echo handler), send a v2 handshake + invoke, and verify the response.
    // This covers the cross-process WASM handler path that is distinct from
    // the built-in handler path tested above.
    let component = match echo_handler_component() {
        Some(path) => fs::read(&path).unwrap(),
        None => {
            eprintln!("skipping: echo handler component not built (set ECHO_HANDLER_WASM)");
            return;
        }
    };

    // Build a signed model pack.
    let model_signing = SigningKey::from_bytes(&[52; 32]);
    let model_manifest = ModelPackManifest {
        format_version: MODEL_PACK_FORMAT_VERSION,
        id: "rillml.example.default".into(),
        version: "0.7.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "wasm-process-model".into(),
        capabilities: vec!["rillml.linearRegression.predict".into()],
    };
    let model = serde_json::json!({
        "kind": "linearRegression",
        "weights": [0.5],
        "intercept": 0.0
    });
    let model_pack = build_signed_model_pack(&model_manifest, &model, &model_signing).unwrap();

    // Build a signed handler pack from the echo handler component.
    let handler_signing = SigningKey::from_bytes(&[53; 32]);
    let handler_manifest = HandlerPackManifest {
        format_version: HANDLER_PACKAGE_FORMAT_VERSION,
        id: "rillml.echo.handler".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        handler_api_version: HANDLER_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "wasm-process-handler".into(),
        capabilities: vec!["rillml.linearRegression.predict".into()],
        module_sha256: hex::encode(Sha256::digest(&component)),
        module_size: component.len() as u64,
    };
    let handler_pack =
        build_signed_handler_pack(&handler_manifest, &component, &handler_signing).unwrap();

    let temporary = tempfile::tempdir().unwrap();
    let model_path = temporary.path().join("model.rillpack");
    let handler_path = temporary.path().join("echo.rillhandler");
    fs::write(&model_path, model_pack).unwrap();
    fs::write(&handler_path, handler_pack).unwrap();

    let model_trust = format!(
        "wasm-process-model={}",
        hex::encode(model_signing.verifying_key().to_bytes())
    );
    let handler_trust = format!(
        "wasm-process-handler={}",
        hex::encode(handler_signing.verifying_key().to_bytes())
    );

    let mut child = runtime_command()
        .args(["serve", "--pack"])
        .arg(&model_path)
        .args(["--trust-key", &model_trust])
        .args(["--handler"])
        .arg(&handler_path)
        .args(["--handler-trust-key", &handler_trust])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let requests = [
        RuntimeRequest::Handshake {
            request_id: "wasm-process-handshake".into(),
            api_version: RUNTIME_API_VERSION,
            client_name: "wasm-process-test".into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
        },
        RuntimeRequest::Invoke {
            request_id: "wasm-process-invoke".into(),
            api_version: RUNTIME_API_VERSION,
            capability: "rillml.linearRegression.predict".into(),
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
    // Handshake must report the WASM handler id (not the built-in id).
    assert!(matches!(
        &responses[0],
        RuntimeResponseV2::Handshake {
            request_id,
            handler_id,
            ..
        } if request_id == "wasm-process-handshake"
            && handler_id == "rillml.echo.handler"
    ));
    // Echo handler returns the input as output.
    assert!(matches!(
        &responses[1],
        RuntimeResponseV2::Result {
            request_id,
            output,
            ..
        } if request_id == "wasm-process-invoke"
            && output["features"] == serde_json::json!([4.0, 2.0])
    ));
}

#[test]
fn missing_handler_option_returns_error() {
    // 1.0 contract: when neither --handler nor --builtin-handler is passed,
    // the runtime must refuse to start. The previous behaviour silently fell
    // back to the deprecated built-in linear-regression handler, which
    // contradicted the 1.0 deprecation policy. This test pins the new
    // contract so a future regression cannot reintroduce the implicit
    // fallback.
    let signing = SigningKey::from_bytes(&[77; 32]);
    let manifest = ModelPackManifest {
        format_version: MODEL_PACK_FORMAT_VERSION,
        id: "rillml.example.default".into(),
        version: "0.7.0".into(),
        runtime_api_version: RUNTIME_API_VERSION,
        min_runtime_version: "0.7.0".into(),
        publisher_key_id: "missing-handler-test".into(),
        capabilities: vec![LINEAR_REGRESSION_CAPABILITY.into()],
    };
    let model = serde_json::json!({
        "kind": "linearRegression",
        "weights": [1.0],
        "intercept": 0.0
    });
    let pack = build_signed_model_pack(&manifest, &model, &signing).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let pack_path = temporary.path().join("missing-handler.rillpack");
    fs::write(&pack_path, pack).unwrap();

    let trust = format!(
        "missing-handler-test={}",
        hex::encode(signing.verifying_key().to_bytes())
    );

    // Start the runtime with --pack and --model-trust-key but NO --handler
    // or --builtin-handler. Do not wire stdin so the process exits
    // immediately after the CLI parser rejects the missing handler option.
    let output = runtime_command()
        .args(["serve", "--pack"])
        .arg(&pack_path)
        .args(["--model-trust-key", &trust])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when no handler option is supplied; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no --handler or --builtin-handler specified"),
        "expected MissingHandlerOption diagnostic on stderr, got: {stderr}"
    );

    // Also verify the primary parameter name --model-trust-key is accepted
    // (not just the deprecated --trust-key alias) by checking the runtime
    // gets past trust-store parsing and fails specifically on the handler
    // option. A trust-store parse error would emit a different diagnostic.
    assert!(
        !stderr.contains("invalid trusted key"),
        "expected --model-trust-key to be accepted as the primary name; stderr={stderr}"
    );
}

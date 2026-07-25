use std::sync::Arc;

use rill_runtime_protocol::{
    MIN_RUNTIME_API_VERSION, RUNTIME_API_VERSION, RuntimeRequest, RuntimeResponse,
    RuntimeResponseV2,
};
use serde_json::Value;

use crate::handler::HandlerIdentity;
use crate::package::LoadedModelPack;

/// Typed invoke error.
///
/// Replaces the previous `Result<Value, String>` contract. The `kind`
/// selects a stable IPC error code and a fixed public message; `detail`
/// carries host-only diagnostic text (e.g. for stderr logs) and is **never**
/// forwarded to IPC clients, so guests cannot exfiltrate arbitrary content
/// through the error path.
#[derive(Debug, Clone)]
pub struct InvokeError {
    kind: InvokeErrorKind,
    detail: Option<String>,
}

/// Maximum byte length of the host-only `detail` string. Guests can fully
/// control this payload via the WIT `handler-error` variant, so the host
/// truncates it to bound memory and stderr noise. The limit is enforced
/// on a UTF-8 char boundary so the stored string stays valid.
pub const MAX_DETAIL_BYTES: usize = 4 * 1024;

/// Stable categorisation of invoke failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvokeErrorKind {
    /// Host-side input serialisation or size check failed.
    Internal,
    /// Fuel budget or epoch deadline was hit. Retryable.
    Timeout,
    /// Wasmtime trap (unreachable, OOB, stack overflow, …).
    Trap,
    /// Handler output exceeded [`MAX_IO_BYTES`](crate::handler::wasm::MAX_IO_BYTES).
    OutputTooLarge,
    /// Handler output failed JSON deserialisation on the host side.
    InvalidOutput,
    /// Guest reported `invalid-model` / `invalid-input` /
    /// `unsupported-capability` / `execution-failed` via the WIT
    /// `handler-error` variant. The variant detail is stored in
    /// [`InvokeError::detail`] for host logs only.
    ExecutionFailed,
}

impl InvokeError {
    /// Create a new typed error with no host detail.
    pub const fn new(kind: InvokeErrorKind) -> Self {
        Self { kind, detail: None }
    }

    /// Create a new typed error carrying host-only diagnostic text.
    ///
    /// `detail` is intended for `eprintln!` logs and **must not** be sent
    /// to IPC clients. Guests can fully control this string via the WIT
    /// `handler-error` payload, so it cannot be trusted for security
    /// decisions. It is truncated to [`MAX_DETAIL_BYTES`] on a UTF-8 char
    /// boundary so a malicious guest cannot grow host memory unboundedly
    /// through the error path.
    pub fn with_detail(kind: InvokeErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: Some(truncate_to_bytes(detail.into(), MAX_DETAIL_BYTES)),
        }
    }

    /// Error category.
    pub const fn kind(&self) -> InvokeErrorKind {
        self.kind
    }

    /// Host-only diagnostic text. Never sent to IPC clients.
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Stable IPC error code. Backwards-compatible with the v1/v2 wire
    /// format produced by the previous `map_invoke_error` string matching.
    pub const fn stable_code(&self) -> &'static str {
        match self.kind {
            InvokeErrorKind::Internal => "handlerInternalError",
            InvokeErrorKind::Timeout => "handlerTimeout",
            InvokeErrorKind::Trap => "handlerTrap",
            InvokeErrorKind::OutputTooLarge => "handlerOutputTooLarge",
            InvokeErrorKind::InvalidOutput => "handlerInvalidOutput",
            // Guest-reported WIT `handler-error` variants all collapse to
            // `handlerInternalError` on the wire, matching the previous
            // `map_invoke_error` behaviour that mapped
            // `handlerExecutionFailed: ...` to `handlerInternalError`.
            InvokeErrorKind::ExecutionFailed => "handlerInternalError",
        }
    }

    /// Fixed public message. Never contains guest-supplied content.
    pub const fn public_message(&self) -> &'static str {
        match self.kind {
            InvokeErrorKind::Internal => "internal runtime error",
            InvokeErrorKind::Timeout => "handler exceeded the wall-clock deadline",
            InvokeErrorKind::Trap => "handler trapped",
            InvokeErrorKind::OutputTooLarge => "handler output exceeded the size limit",
            InvokeErrorKind::InvalidOutput => "handler output was not valid JSON",
            InvokeErrorKind::ExecutionFailed => "handler execution failed",
        }
    }

    /// Whether the caller may retry the same request on a fresh handler.
    pub const fn retryable(&self) -> bool {
        matches!(self.kind, InvokeErrorKind::Timeout)
    }
}

impl std::fmt::Display for InvokeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.detail {
            Some(detail) => write!(f, "{}: {}", self.stable_code(), detail),
            None => f.write_str(self.stable_code()),
        }
    }
}

impl std::error::Error for InvokeError {}

/// Truncate `s` to at most `max_bytes` on a UTF-8 char boundary.
///
/// `String::truncate` panics on a non-char boundary, so we walk backwards
/// from `max_bytes` until `is_char_boundary` succeeds. The result is always
/// valid UTF-8 and never longer than `max_bytes`.
fn truncate_to_bytes(s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = s;
    truncated.truncate(end);
    truncated
}

/// Consumers can implement this trait to add business-specific invocation logic.
pub trait InvokeHandler: Send + Sync + std::fmt::Debug {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, InvokeError>;
}

/// Internal response type produced by [`RuntimeEngine`]. The IPC layer converts
/// this to a v1 [`RuntimeResponse`] or v2 [`RuntimeResponseV2`] based on the
/// request's `api_version`.
#[derive(Debug, Clone)]
pub enum EngineResponse {
    Handshake {
        request_id: String,
        runtime_version: String,
        model_pack_id: String,
        model_pack_version: String,
        capabilities: Vec<String>,
        handler: Option<HandlerIdentity>,
    },
    Health {
        request_id: String,
        healthy: bool,
        model_pack_id: String,
        model_pack_version: String,
    },
    Result {
        request_id: String,
        output: Value,
    },
    Error {
        request_id: String,
        code: String,
        message: String,
        retryable: bool,
    },
}

impl EngineResponse {
    /// Convert to a v1 wire response. Handler identity fields are dropped.
    pub fn to_v1(&self, api_version: u32) -> RuntimeResponse {
        match self {
            Self::Handshake {
                request_id,
                runtime_version,
                model_pack_id,
                model_pack_version,
                capabilities,
                ..
            } => RuntimeResponse::Handshake {
                request_id: request_id.clone(),
                api_version,
                runtime_version: runtime_version.clone(),
                model_pack_id: model_pack_id.clone(),
                model_pack_version: model_pack_version.clone(),
                capabilities: capabilities.clone(),
            },
            Self::Health {
                request_id,
                healthy,
                model_pack_id,
                model_pack_version,
            } => RuntimeResponse::Health {
                request_id: request_id.clone(),
                api_version,
                healthy: *healthy,
                model_pack_id: model_pack_id.clone(),
                model_pack_version: model_pack_version.clone(),
            },
            Self::Result { request_id, output } => RuntimeResponse::Result {
                request_id: request_id.clone(),
                api_version,
                output: output.clone(),
            },
            Self::Error {
                request_id,
                code,
                message,
                retryable,
            } => RuntimeResponse::Error {
                request_id: request_id.clone(),
                api_version,
                code: code.clone(),
                message: message.clone(),
                retryable: *retryable,
            },
        }
    }

    /// Convert to a v2 wire response. If no handler is loaded, handler fields
    /// are filled with empty/zero values and effective_capabilities equals the
    /// model capabilities.
    pub fn to_v2(&self, api_version: u32) -> RuntimeResponseV2 {
        match self {
            Self::Handshake {
                request_id,
                runtime_version,
                model_pack_id,
                model_pack_version,
                capabilities,
                handler,
            } => {
                let (handler_id, handler_version, handler_api_version, effective) = match handler {
                    Some(h) => (
                        h.handler_id.clone(),
                        h.handler_version.clone(),
                        h.handler_api_version,
                        h.effective_capabilities.clone(),
                    ),
                    None => (String::new(), String::new(), 0, capabilities.clone()),
                };
                RuntimeResponseV2::Handshake {
                    request_id: request_id.clone(),
                    api_version,
                    runtime_version: runtime_version.clone(),
                    model_pack_id: model_pack_id.clone(),
                    model_pack_version: model_pack_version.clone(),
                    capabilities: capabilities.clone(),
                    handler_id,
                    handler_version,
                    handler_api_version,
                    effective_capabilities: effective,
                }
            }
            Self::Health {
                request_id,
                healthy,
                model_pack_id,
                model_pack_version,
            } => RuntimeResponseV2::Health {
                request_id: request_id.clone(),
                api_version,
                healthy: *healthy,
                model_pack_id: model_pack_id.clone(),
                model_pack_version: model_pack_version.clone(),
            },
            Self::Result { request_id, output } => RuntimeResponseV2::Result {
                request_id: request_id.clone(),
                api_version,
                output: output.clone(),
            },
            Self::Error {
                request_id,
                code,
                message,
                retryable,
            } => RuntimeResponseV2::Error {
                request_id: request_id.clone(),
                api_version,
                code: code.clone(),
                message: message.clone(),
                retryable: *retryable,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeEngine {
    pack: LoadedModelPack,
    invoke_handler: Option<Arc<dyn InvokeHandler>>,
    handler_identity: Option<HandlerIdentity>,
    effective_capabilities: Vec<String>,
}

impl RuntimeEngine {
    pub fn new(pack: LoadedModelPack) -> Self {
        Self {
            pack,
            invoke_handler: None,
            handler_identity: None,
            effective_capabilities: Vec::new(),
        }
    }

    pub fn with_invoke_handler(mut self, handler: Arc<dyn InvokeHandler>) -> Self {
        self.invoke_handler = Some(handler);
        self
    }

    /// Attach handler identity and effective capabilities for IPC v2 handshake.
    pub fn with_handler_identity(mut self, identity: HandlerIdentity) -> Self {
        self.effective_capabilities = identity.effective_capabilities.clone();
        self.handler_identity = Some(identity);
        self
    }

    /// Effective capability set (intersection of model and handler). Empty when
    /// no handler is loaded.
    pub fn effective_capabilities(&self) -> &[String] {
        &self.effective_capabilities
    }

    /// Handler identity if a handler was loaded.
    pub fn handler_identity(&self) -> Option<&HandlerIdentity> {
        self.handler_identity.as_ref()
    }

    pub fn handle(&self, request: RuntimeRequest) -> EngineResponse {
        let request_id = request.request_id().to_string();
        if request_id.is_empty() || request_id.len() > 128 {
            return self.error(request_id, "invalidRequestId", "invalid request id", false);
        }
        let api_version = request.api_version();
        if !(MIN_RUNTIME_API_VERSION..=RUNTIME_API_VERSION).contains(&api_version) {
            return self.error(
                request_id,
                "incompatibleApiVersion",
                "runtime API version is not supported",
                false,
            );
        }

        match request {
            RuntimeRequest::Handshake {
                request_id,
                client_name,
                client_version,
                ..
            } => {
                if client_name.is_empty()
                    || client_name.len() > 96
                    || client_version.is_empty()
                    || client_version.len() > 48
                {
                    return self.error(
                        request_id,
                        "invalidClientIdentity",
                        "invalid client identity",
                        false,
                    );
                }
                EngineResponse::Handshake {
                    request_id,
                    runtime_version: env!("CARGO_PKG_VERSION").into(),
                    model_pack_id: self.pack.manifest.id.clone(),
                    model_pack_version: self.pack.manifest.version.clone(),
                    capabilities: self.pack.manifest.capabilities.clone(),
                    handler: self.handler_identity.clone(),
                }
            }
            RuntimeRequest::Health { request_id, .. } => EngineResponse::Health {
                request_id,
                healthy: true,
                model_pack_id: self.pack.manifest.id.clone(),
                model_pack_version: self.pack.manifest.version.clone(),
            },
            RuntimeRequest::Invoke {
                request_id,
                capability,
                input,
                ..
            } => {
                if !self.is_capability_allowed(&capability) {
                    return self.error(
                        request_id,
                        "unsupportedCapability",
                        "capability is not in the effective set",
                        false,
                    );
                }
                let Some(handler) = &self.invoke_handler else {
                    return self.error(
                        request_id,
                        "noInvokeHandler",
                        "no invoke handler registered",
                        false,
                    );
                };
                match handler.invoke(&capability, &input) {
                    Ok(output) => EngineResponse::Result { request_id, output },
                    Err(invoke_err) => {
                        // Log host-side detail (if any) for debugging; the
                        // IPC message is always the fixed public string so
                        // guests cannot exfiltrate content via the error
                        // payload.
                        if let Some(detail) = invoke_err.detail() {
                            eprintln!(
                                "rill-runtime: invoke {} -> {} (detail: {})",
                                capability,
                                invoke_err.stable_code(),
                                detail
                            );
                        }
                        self.error(
                            request_id,
                            invoke_err.stable_code(),
                            invoke_err.public_message(),
                            invoke_err.retryable(),
                        )
                    }
                }
            }
        }
    }

    /// Checks the capability against the effective set when a handler is loaded,
    /// or against the model pack's declared capabilities when no handler is
    /// loaded (for backwards compatibility with built-in handlers selected by
    /// the binary).
    fn is_capability_allowed(&self, capability: &str) -> bool {
        if !self.effective_capabilities.is_empty() {
            self.effective_capabilities.iter().any(|c| c == capability)
        } else {
            self.pack
                .manifest
                .capabilities
                .iter()
                .any(|c| c == capability)
        }
    }

    fn error(
        &self,
        request_id: String,
        code: &str,
        message: &str,
        retryable: bool,
    ) -> EngineResponse {
        EngineResponse::Error {
            request_id,
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

#[cfg(test)]
mod tests {
    use rill_runtime_protocol::{MODEL_PACK_FORMAT_VERSION, ModelPackManifest};

    use super::*;
    use crate::handler::builtin::LINEAR_REGRESSION_CAPABILITY;

    fn engine() -> RuntimeEngine {
        RuntimeEngine::new(LoadedModelPack {
            manifest: ModelPackManifest {
                format_version: MODEL_PACK_FORMAT_VERSION,
                id: "rillml.example.default".into(),
                version: "0.7.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                min_runtime_version: "0.7.0".into(),
                publisher_key_id: "test".into(),
                capabilities: vec!["rillml.example".into()],
            },
            model: serde_json::json!({}),
        })
    }

    #[test]
    fn handshake_reports_loaded_pack() {
        let response = engine().handle(RuntimeRequest::Handshake {
            request_id: "hello".into(),
            api_version: RUNTIME_API_VERSION,
            client_name: "example-host".into(),
            client_version: "0.9.0".into(),
        });
        assert!(matches!(
            response,
            EngineResponse::Handshake { model_pack_id, .. }
                if model_pack_id == "rillml.example.default"
        ));
    }

    #[test]
    fn incompatible_api_is_a_typed_error() {
        let response = engine().handle(RuntimeRequest::Health {
            request_id: "health".into(),
            api_version: RUNTIME_API_VERSION + 1,
        });
        assert!(matches!(
            response,
            EngineResponse::Error { code, .. } if code == "incompatibleApiVersion"
        ));
    }

    #[test]
    fn invoke_without_handler_returns_no_invoke_handler_error() {
        let response = engine().handle(RuntimeRequest::Invoke {
            request_id: "invoke-1".into(),
            api_version: RUNTIME_API_VERSION,
            capability: "rillml.example".into(),
            input: serde_json::json!({}),
        });
        assert!(matches!(
            response,
            EngineResponse::Error { code, .. } if code == "noInvokeHandler"
        ));
    }

    #[test]
    fn invoke_rejects_capability_not_declared_by_signed_manifest() {
        let response = engine().handle(RuntimeRequest::Invoke {
            request_id: "invoke-undeclared".into(),
            api_version: RUNTIME_API_VERSION,
            capability: "undeclared.capability".into(),
            input: serde_json::json!({}),
        });
        assert!(matches!(
            response,
            EngineResponse::Error { code, .. } if code == "unsupportedCapability"
        ));
    }

    #[test]
    fn v1_handshake_omits_handler_fields() {
        let identity = HandlerIdentity {
            handler_id: "org.example.handler".into(),
            handler_version: "1.0.0".into(),
            handler_api_version: 1,
            effective_capabilities: vec!["rillml.example".into()],
        };
        let engine = engine().with_handler_identity(identity);
        let response = engine.handle(RuntimeRequest::Handshake {
            request_id: "v1-test".into(),
            api_version: 1,
            client_name: "v1-host".into(),
            client_version: "0.6.0".into(),
        });
        let v1 = response.to_v1(1);
        let json = serde_json::to_string(&v1).unwrap();
        assert!(!json.contains("handlerId"));
        assert!(!json.contains("effectiveCapabilities"));
    }

    #[test]
    fn v2_handshake_includes_handler_fields() {
        let identity = HandlerIdentity {
            handler_id: "org.example.handler".into(),
            handler_version: "1.0.0".into(),
            handler_api_version: 1,
            effective_capabilities: vec!["rillml.example".into()],
        };
        let engine = engine().with_handler_identity(identity);
        let response = engine.handle(RuntimeRequest::Handshake {
            request_id: "v2-test".into(),
            api_version: 2,
            client_name: "v2-host".into(),
            client_version: "0.7.0".into(),
        });
        let v2 = response.to_v2(2);
        let json = serde_json::to_string(&v2).unwrap();
        assert!(json.contains("\"handlerId\":\"org.example.handler\""));
        assert!(json.contains("\"handlerApiVersion\":1"));
        assert!(json.contains("\"effectiveCapabilities\":[\"rillml.example\"]"));
    }

    #[test]
    fn v2_handshake_without_handler_has_empty_fields() {
        let response = engine().handle(RuntimeRequest::Handshake {
            request_id: "v2-no-handler".into(),
            api_version: 2,
            client_name: "v2-host".into(),
            client_version: "0.7.0".into(),
        });
        let v2 = response.to_v2(2);
        match v2 {
            RuntimeResponseV2::Handshake {
                handler_id,
                handler_version,
                handler_api_version,
                effective_capabilities,
                ..
            } => {
                assert!(handler_id.is_empty());
                assert!(handler_version.is_empty());
                assert_eq!(handler_api_version, 0);
                assert_eq!(effective_capabilities, vec!["rillml.example"]);
            }
            _ => panic!("expected handshake"),
        }
    }

    #[test]
    fn linear_regression_handler_validates_and_predicts() {
        use crate::handler::builtin::LinearRegressionInvokeHandler;

        let pack = LoadedModelPack {
            manifest: ModelPackManifest {
                format_version: MODEL_PACK_FORMAT_VERSION,
                id: "rillml.example.default".into(),
                version: "0.7.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                min_runtime_version: "0.7.0".into(),
                publisher_key_id: "test".into(),
                capabilities: vec![LINEAR_REGRESSION_CAPABILITY.into()],
            },
            model: serde_json::json!({
                "kind": "linearRegression",
                "weights": [0.5, -0.25],
                "intercept": 1.0
            }),
        };
        let handler = LinearRegressionInvokeHandler::from_pack(&pack).unwrap();
        let engine = RuntimeEngine::new(pack).with_invoke_handler(Arc::new(handler));
        let response = engine.handle(RuntimeRequest::Invoke {
            request_id: "invoke-linear".into(),
            api_version: RUNTIME_API_VERSION,
            capability: LINEAR_REGRESSION_CAPABILITY.into(),
            input: serde_json::json!({"features": [4.0, 2.0]}),
        });
        assert!(matches!(
            response,
            EngineResponse::Result { output, .. } if output["prediction"] == 2.5
        ));
    }

    #[test]
    fn invoke_error_stable_codes_match_wire_format() {
        // Every kind must map to the exact IPC code expected by v1/v2
        // clients, preserving backwards compatibility with the previous
        // `map_invoke_error` string matching.
        assert_eq!(
            InvokeError::new(InvokeErrorKind::Trap).stable_code(),
            "handlerTrap"
        );
        assert_eq!(
            InvokeError::new(InvokeErrorKind::Timeout).stable_code(),
            "handlerTimeout"
        );
        assert_eq!(
            InvokeError::new(InvokeErrorKind::OutputTooLarge).stable_code(),
            "handlerOutputTooLarge"
        );
        assert_eq!(
            InvokeError::new(InvokeErrorKind::InvalidOutput).stable_code(),
            "handlerInvalidOutput"
        );
        assert_eq!(
            InvokeError::new(InvokeErrorKind::Internal).stable_code(),
            "handlerInternalError"
        );
        // Guest-reported WIT handler-error variants collapse to
        // handlerInternalError on the wire.
        assert_eq!(
            InvokeError::new(InvokeErrorKind::ExecutionFailed).stable_code(),
            "handlerInternalError"
        );
    }

    #[test]
    fn invoke_error_retryable_only_for_timeout() {
        assert!(InvokeError::new(InvokeErrorKind::Timeout).retryable());
        assert!(!InvokeError::new(InvokeErrorKind::Trap).retryable());
        assert!(!InvokeError::new(InvokeErrorKind::OutputTooLarge).retryable());
        assert!(!InvokeError::new(InvokeErrorKind::InvalidOutput).retryable());
        assert!(!InvokeError::new(InvokeErrorKind::Internal).retryable());
        assert!(!InvokeError::new(InvokeErrorKind::ExecutionFailed).retryable());
    }

    #[test]
    fn invoke_error_public_message_never_contains_detail() {
        // Guest can fully control the detail string; the public message
        // must always be the fixed constant.
        let err = InvokeError::with_detail(
            InvokeErrorKind::ExecutionFailed,
            "SECRET-TOKEN-LEAK-ATTEMPT guest-controlled-payload",
        );
        assert_eq!(err.public_message(), "handler execution failed");
        assert_eq!(err.stable_code(), "handlerInternalError");
        assert_eq!(
            err.detail(),
            Some("SECRET-TOKEN-LEAK-ATTEMPT guest-controlled-payload")
        );
        // The Display impl is for host logs only; the IPC layer must
        // never send `err.to_string()` to clients.
        assert!(err.to_string().contains("SECRET-TOKEN-LEAK-ATTEMPT"));
        // The public_message is what the IPC layer actually sends.
        assert!(!err.public_message().contains("SECRET"));
    }

    #[test]
    fn invoke_error_without_detail_has_no_detail() {
        let err = InvokeError::new(InvokeErrorKind::Trap);
        assert_eq!(err.kind(), InvokeErrorKind::Trap);
        assert_eq!(err.detail(), None);
        assert_eq!(err.stable_code(), "handlerTrap");
        assert_eq!(err.to_string(), "handlerTrap");
    }

    #[test]
    fn invoke_error_detail_is_truncated_to_4kib_on_char_boundary() {
        // A malicious guest tries to grow host memory via an oversized
        // error payload. The host must truncate to MAX_DETAIL_BYTES on a
        // UTF-8 char boundary.
        let huge = "A".repeat(MAX_DETAIL_BYTES * 4);
        let err = InvokeError::with_detail(InvokeErrorKind::ExecutionFailed, huge);
        let detail = err.detail().expect("detail must be stored");
        assert!(
            detail.len() <= MAX_DETAIL_BYTES,
            "detail length {} must not exceed {}",
            detail.len(),
            MAX_DETAIL_BYTES
        );
        // Truncation must land on a char boundary (the string is valid UTF-8
        // by construction, but the test guards against a future unsafe path).
        assert!(detail.chars().all(|c| c == 'A'));
    }

    #[test]
    fn invoke_error_detail_truncation_respects_multibyte_chars() {
        // Multi-byte UTF-8 must not be split mid-codepoint. Use 3-byte
        // CJK characters so the MAX_DETAIL_BYTES boundary lands inside a
        // character; the result must back up to the previous char boundary.
        let emoji = "🌟".repeat(MAX_DETAIL_BYTES); // each '🌟' is 4 bytes
        let err = InvokeError::with_detail(InvokeErrorKind::ExecutionFailed, emoji);
        let detail = err.detail().expect("detail must be stored");
        assert!(detail.len() <= MAX_DETAIL_BYTES);
        // Every stored character must be a complete '🌟'.
        for c in detail.chars() {
            assert_eq!(c, '🌟');
        }
    }

    /// Minimal handler that always returns the supplied error, for
    /// exercising the engine's invoke error path without a WASM sandbox.
    #[derive(Debug)]
    struct FailingHandler {
        err: InvokeError,
    }

    impl InvokeHandler for FailingHandler {
        fn invoke(&self, _capability: &str, _input: &Value) -> Result<Value, InvokeError> {
            Err(self.err.clone())
        }
    }

    #[test]
    fn engine_invoke_error_does_not_leak_guest_detail_in_message() {
        // A malicious guest tries to exfiltrate a token via the WIT
        // handler-error payload. The IPC Error.message field must be
        // the fixed public string, not the guest-supplied detail.
        let err = InvokeError::with_detail(
            InvokeErrorKind::ExecutionFailed,
            "leak-attempt:SECRET-TOKEN",
        );
        let pack = LoadedModelPack {
            manifest: ModelPackManifest {
                format_version: MODEL_PACK_FORMAT_VERSION,
                id: "rillml.example.default".into(),
                version: "0.7.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                min_runtime_version: "0.7.0".into(),
                publisher_key_id: "test".into(),
                capabilities: vec!["rillml.example".into()],
            },
            model: serde_json::json!({}),
        };
        let engine = RuntimeEngine::new(pack).with_invoke_handler(Arc::new(FailingHandler { err }));
        let response = engine.handle(RuntimeRequest::Invoke {
            request_id: "leak-test".into(),
            api_version: RUNTIME_API_VERSION,
            capability: "rillml.example".into(),
            input: serde_json::json!({}),
        });
        match response {
            EngineResponse::Error {
                code,
                message,
                retryable,
                ..
            } => {
                assert_eq!(code, "handlerInternalError");
                assert_eq!(message, "handler execution failed");
                assert!(!retryable);
                // The guest-supplied detail must NOT appear anywhere in
                // the IPC response fields.
                assert!(!message.contains("SECRET"));
                assert!(!message.contains("leak-attempt"));
            }
            _ => panic!("expected EngineResponse::Error"),
        }
    }
}

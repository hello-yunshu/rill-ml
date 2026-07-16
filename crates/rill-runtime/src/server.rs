use std::sync::Arc;

use rill_runtime_protocol::{
    MIN_RUNTIME_API_VERSION, RUNTIME_API_VERSION, RuntimeRequest, RuntimeResponse,
    RuntimeResponseV2,
};
use serde_json::Value;

use crate::handler::HandlerIdentity;
use crate::package::LoadedModelPack;

/// Consumers can implement this trait to add business-specific invocation logic.
pub trait InvokeHandler: Send + Sync + std::fmt::Debug {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, String>;
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
                    Err(message) => {
                        let (code, retryable) = map_invoke_error(&message);
                        self.error(request_id, code, &message, retryable)
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

/// Maps a handler error message to a stable error code. Recognised codes are
/// extracted from the message prefix; unknown errors map to `handlerInternalError`.
fn map_invoke_error(message: &str) -> (&'static str, bool) {
    if message.starts_with("handlerTrap") {
        ("handlerTrap", false)
    } else if message.starts_with("handlerTimeout") {
        ("handlerTimeout", true)
    } else if message.starts_with("handlerOutputTooLarge") {
        ("handlerOutputTooLarge", false)
    } else if message.starts_with("handlerInvalidOutput") {
        ("handlerInvalidOutput", false)
    } else if message.starts_with("handlerInternalError") {
        ("handlerInternalError", false)
    } else if message.starts_with("handlerExecutionFailed") {
        // Handler returned an error via the WIT result type.
        ("handlerInternalError", false)
    } else {
        ("handlerInternalError", false)
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
    fn map_invoke_error_recognizes_handler_trap() {
        let (code, retryable) = map_invoke_error("handlerTrap: unreachable");
        assert_eq!(code, "handlerTrap");
        assert!(!retryable);
    }

    #[test]
    fn map_invoke_error_recognizes_handler_timeout() {
        let (code, retryable) = map_invoke_error("handlerTimeout: epoch deadline exceeded");
        assert_eq!(code, "handlerTimeout");
        assert!(retryable);
    }

    #[test]
    fn map_invoke_error_recognizes_handler_output_too_large() {
        let (code, retryable) = map_invoke_error("handlerOutputTooLarge: output exceeds 1 MiB");
        assert_eq!(code, "handlerOutputTooLarge");
        assert!(!retryable);
    }

    #[test]
    fn map_invoke_error_recognizes_handler_invalid_output() {
        let (code, retryable) =
            map_invoke_error("handlerInvalidOutput: expected value at line 1 column 1");
        assert_eq!(code, "handlerInvalidOutput");
        assert!(!retryable);
    }

    #[test]
    fn map_invoke_error_recognizes_handler_internal_error() {
        let (code, retryable) = map_invoke_error("handlerInternalError: lock poisoned");
        assert_eq!(code, "handlerInternalError");
        assert!(!retryable);
    }

    #[test]
    fn map_invoke_error_recognizes_handler_execution_failed() {
        let (code, retryable) = map_invoke_error("handlerExecutionFailed: guest returned error");
        assert_eq!(code, "handlerInternalError");
        assert!(!retryable);
    }

    #[test]
    fn map_invoke_error_maps_capability_mismatch_to_internal() {
        // handlerCapabilityMismatch is a load-phase error (RFC §6.2) and is
        // never produced by invoke(); it falls through to handlerInternalError.
        let (code, retryable) = map_invoke_error("handlerCapabilityMismatch: cap not declared");
        assert_eq!(code, "handlerInternalError");
        assert!(!retryable);
    }

    #[test]
    fn map_invoke_error_maps_unknown_to_internal_error() {
        let (code, retryable) = map_invoke_error("unknown error");
        assert_eq!(code, "handlerInternalError");
        assert!(!retryable);
    }
}

use std::sync::Arc;

use rill_runtime_protocol::{RUNTIME_API_VERSION, RuntimeRequest, RuntimeResponse};
use serde_json::Value;

use crate::package::LoadedModelPack;

/// 消费方实现此 trait 处理 Invoke 请求。
/// RillML runtime 不提供默认实现。
pub trait InvokeHandler: Send + Sync + std::fmt::Debug {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, String>;
}

#[derive(Debug, Clone)]
pub struct RuntimeEngine {
    pack: LoadedModelPack,
    invoke_handler: Option<Arc<dyn InvokeHandler>>,
}

impl RuntimeEngine {
    pub fn new(pack: LoadedModelPack) -> Self {
        Self {
            pack,
            invoke_handler: None,
        }
    }

    pub fn with_invoke_handler(mut self, handler: Arc<dyn InvokeHandler>) -> Self {
        self.invoke_handler = Some(handler);
        self
    }

    pub fn handle(&self, request: RuntimeRequest) -> RuntimeResponse {
        let request_id = request.request_id().to_string();
        if request_id.is_empty() || request_id.len() > 128 {
            return self.error(request_id, "invalidRequestId", "invalid request id", false);
        }
        if request.api_version() != RUNTIME_API_VERSION {
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
                RuntimeResponse::Handshake {
                    request_id,
                    api_version: RUNTIME_API_VERSION,
                    runtime_version: env!("CARGO_PKG_VERSION").into(),
                    model_pack_id: self.pack.manifest.id.clone(),
                    model_pack_version: self.pack.manifest.version.clone(),
                    capabilities: self.pack.manifest.capabilities.clone(),
                }
            }
            RuntimeRequest::Health { request_id, .. } => RuntimeResponse::Health {
                request_id,
                api_version: RUNTIME_API_VERSION,
                healthy: true,
                model_pack_id: self.pack.manifest.id.clone(),
                model_pack_version: self.pack.manifest.version.clone(),
            },
            RuntimeRequest::Invoke {
                request_id,
                capability,
                input,
                ..
            } => match &self.invoke_handler {
                Some(handler) => match handler.invoke(&capability, &input) {
                    Ok(output) => RuntimeResponse::Result {
                        request_id,
                        api_version: RUNTIME_API_VERSION,
                        output,
                    },
                    Err(message) => self.error(request_id, "invokeFailed", &message, false),
                },
                None => self.error(
                    request_id,
                    "noInvokeHandler",
                    "no invoke handler registered",
                    false,
                ),
            },
        }
    }

    fn error(
        &self,
        request_id: String,
        code: &str,
        message: &str,
        retryable: bool,
    ) -> RuntimeResponse {
        RuntimeResponse::Error {
            request_id,
            api_version: RUNTIME_API_VERSION,
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

    fn engine() -> RuntimeEngine {
        RuntimeEngine::new(LoadedModelPack {
            manifest: ModelPackManifest {
                format_version: MODEL_PACK_FORMAT_VERSION,
                id: "rillml.example.default".into(),
                version: "0.5.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                min_runtime_version: "0.5.0".into(),
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
            RuntimeResponse::Handshake { model_pack_id, .. }
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
            RuntimeResponse::Error { code, .. } if code == "incompatibleApiVersion"
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
            RuntimeResponse::Error { code, .. } if code == "noInvokeHandler"
        ));
    }
}

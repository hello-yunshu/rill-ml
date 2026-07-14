use std::sync::Arc;

use rill_runtime_protocol::{RUNTIME_API_VERSION, RuntimeRequest, RuntimeResponse};
use serde::Deserialize;
use serde_json::Value;

use crate::package::LoadedModelPack;

pub const LINEAR_REGRESSION_CAPABILITY: &str = "rillml.linearRegression.predict";

/// Consumers can implement this trait to add business-specific invocation logic.
pub trait InvokeHandler: Send + Sync + std::fmt::Debug {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, String>;
}

/// Business-neutral linear-regression handler used by the distributed runtime binary.
#[derive(Debug, Clone)]
pub struct LinearRegressionInvokeHandler {
    weights: Vec<f64>,
    intercept: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LinearRegressionModel {
    kind: String,
    weights: Vec<f64>,
    intercept: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LinearRegressionInput {
    features: Vec<f64>,
}

impl LinearRegressionInvokeHandler {
    pub fn from_pack(pack: &LoadedModelPack) -> Result<Self, String> {
        if pack.manifest.capabilities.as_slice() != [LINEAR_REGRESSION_CAPABILITY] {
            return Err(format!(
                "standalone runtime requires exactly the {LINEAR_REGRESSION_CAPABILITY} capability"
            ));
        }
        let model: LinearRegressionModel = serde_json::from_value(pack.model.clone())
            .map_err(|error| format!("invalid linear-regression model: {error}"))?;
        if model.kind != "linearRegression" {
            return Err("unsupported built-in model kind".into());
        }
        if model.weights.is_empty() || model.weights.len() > 65_536 {
            return Err("linear-regression weights must contain 1..=65536 values".into());
        }
        if !model.intercept.is_finite() || model.weights.iter().any(|value| !value.is_finite()) {
            return Err("linear-regression model values must be finite".into());
        }
        Ok(Self {
            weights: model.weights,
            intercept: model.intercept,
        })
    }
}

impl InvokeHandler for LinearRegressionInvokeHandler {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, String> {
        if capability != LINEAR_REGRESSION_CAPABILITY {
            return Err("unsupported capability".into());
        }
        let input: LinearRegressionInput = serde_json::from_value(input.clone())
            .map_err(|error| format!("invalid linear-regression input: {error}"))?;
        if input.features.len() != self.weights.len() {
            return Err(format!(
                "expected {} features, received {}",
                self.weights.len(),
                input.features.len()
            ));
        }
        if input.features.iter().any(|value| !value.is_finite()) {
            return Err("linear-regression input values must be finite".into());
        }
        let prediction = self
            .weights
            .iter()
            .zip(&input.features)
            .try_fold(self.intercept, |sum, (weight, feature)| {
                let next = sum + weight * feature;
                next.is_finite().then_some(next)
            })
            .ok_or_else(|| "linear-regression prediction overflowed".to_string())?;
        Ok(serde_json::json!({ "prediction": prediction }))
    }
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
            } => {
                if !self
                    .pack
                    .manifest
                    .capabilities
                    .iter()
                    .any(|declared| declared == &capability)
                {
                    return self.error(
                        request_id,
                        "unsupportedCapability",
                        "capability is not declared by the loaded model pack",
                        false,
                    );
                }
                match &self.invoke_handler {
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
                }
            }
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
            RuntimeResponse::Error { code, .. } if code == "unsupportedCapability"
        ));
    }

    #[test]
    fn linear_regression_handler_validates_and_predicts() {
        let pack = LoadedModelPack {
            manifest: ModelPackManifest {
                format_version: MODEL_PACK_FORMAT_VERSION,
                id: "rillml.example.default".into(),
                version: "0.5.1".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                min_runtime_version: "0.5.1".into(),
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
            RuntimeResponse::Result { output, .. } if output["prediction"] == 2.5
        ));
    }

    #[test]
    fn linear_regression_handler_rejects_invalid_model() {
        let mut pack = LoadedModelPack {
            manifest: ModelPackManifest {
                format_version: MODEL_PACK_FORMAT_VERSION,
                id: "rillml.example.default".into(),
                version: "0.5.1".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                min_runtime_version: "0.5.1".into(),
                publisher_key_id: "test".into(),
                capabilities: vec![LINEAR_REGRESSION_CAPABILITY.into()],
            },
            model: serde_json::json!({
                "kind": "linearRegression",
                "weights": [],
                "intercept": 0.0
            }),
        };
        assert!(LinearRegressionInvokeHandler::from_pack(&pack).is_err());

        pack.model = serde_json::json!({
            "kind": "unknown",
            "weights": [1.0],
            "intercept": 0.0
        });
        assert!(LinearRegressionInvokeHandler::from_pack(&pack).is_err());
    }
}

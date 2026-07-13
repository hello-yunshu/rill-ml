use rill_runtime_protocol::{RUNTIME_API_VERSION, RuntimeRequest, RuntimeResponse};

use crate::{battery, package::LoadedModelPack};

#[derive(Debug, Clone)]
pub struct RuntimeEngine {
    pack: LoadedModelPack,
}

impl RuntimeEngine {
    pub fn new(pack: LoadedModelPack) -> Self {
        Self { pack }
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
            RuntimeRequest::BatteryPredict {
                request_id, input, ..
            } => match battery::predict(&input, &self.pack.battery) {
                Ok(output) => RuntimeResponse::BatteryPrediction {
                    request_id,
                    api_version: RUNTIME_API_VERSION,
                    output,
                },
                Err(error) => {
                    self.error(request_id, "invalidBatteryInput", &error.to_string(), false)
                }
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
    use rill_runtime_protocol::{BatteryModelConfig, MODEL_PACK_FORMAT_VERSION, ModelPackManifest};

    use super::*;

    fn engine() -> RuntimeEngine {
        RuntimeEngine::new(LoadedModelPack {
            manifest: ModelPackManifest {
                format_version: MODEL_PACK_FORMAT_VERSION,
                id: "mira.battery.default".into(),
                version: "0.5.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                min_runtime_version: "0.5.0".into(),
                publisher_key_id: "test".into(),
                capabilities: vec!["batteryUsage".into()],
            },
            battery: BatteryModelConfig::default(),
        })
    }

    #[test]
    fn handshake_reports_loaded_pack() {
        let response = engine().handle(RuntimeRequest::Handshake {
            request_id: "hello".into(),
            api_version: RUNTIME_API_VERSION,
            client_name: "mira".into(),
            client_version: "0.9.0".into(),
        });
        assert!(matches!(
            response,
            RuntimeResponse::Handshake { model_pack_id, .. }
                if model_pack_id == "mira.battery.default"
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
}

//! Handler adapters for the runtime engine.
//!
//! The runtime delegates capability execution to an [`InvokeHandler`]. Built-in
//! handlers live in [`builtin`]; sandboxed WASM handlers live in [`wasm`] behind
//! the `wasm` feature flag.

pub mod builtin;
#[cfg(feature = "wasm")]
pub mod wasm;

use serde::Serialize;

/// Identity reported by the runtime in IPC v2 handshake responses.
///
/// `effective_capabilities` is the intersection of the model pack and handler
/// pack capability lists. Only these capabilities can be invoked.
#[derive(Debug, Clone, Serialize)]
pub struct HandlerIdentity {
    pub handler_id: String,
    pub handler_version: String,
    pub handler_api_version: u32,
    pub effective_capabilities: Vec<String>,
}

/// Errors that arise while loading or preparing a handler for execution.
#[derive(Debug, thiserror::Error)]
pub enum HandlerLoadError {
    #[error("handler pack error: {0}")]
    Pack(#[from] crate::handler_package::HandlerPackError),
    #[error("handler does not cover all model capabilities: missing {missing:?}")]
    CapabilityMissing { missing: Vec<String> },
    #[error("handler failed to initialize: {0}")]
    Init(String),
    #[error("guest metadata does not match signed manifest: {0}")]
    MetadataMismatch(String),
}

/// Computes the effective capability set: the intersection of model and handler
/// capabilities. Returns an error if the handler does not cover every model
/// capability (first version rejects silent capability loss).
pub fn effective_capabilities(
    model: &[String],
    handler: &[String],
) -> Result<Vec<String>, HandlerLoadError> {
    let mut model_sorted = model.to_vec();
    model_sorted.sort();
    let mut handler_sorted = handler.to_vec();
    handler_sorted.sort();

    let effective: Vec<String> = model_sorted
        .iter()
        .filter(|capability| handler_sorted.binary_search(capability).is_ok())
        .cloned()
        .collect();

    if effective.len() != model.len() {
        let missing: Vec<String> = model_sorted
            .iter()
            .filter(|capability| handler_sorted.binary_search(capability).is_err())
            .cloned()
            .collect();
        return Err(HandlerLoadError::CapabilityMissing { missing });
    }
    Ok(effective)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_capabilities_intersect() {
        let model = vec!["a".into(), "b".into()];
        let handler = vec!["a".into(), "b".into(), "c".into()];
        let result = effective_capabilities(&model, &handler).unwrap();
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn effective_capabilities_reject_missing() {
        let model = vec!["a".into(), "b".into()];
        let handler = vec!["a".into()];
        let error = effective_capabilities(&model, &handler).unwrap_err();
        assert!(
            matches!(error, HandlerLoadError::CapabilityMissing { missing } if missing == vec!["b"])
        );
    }
}

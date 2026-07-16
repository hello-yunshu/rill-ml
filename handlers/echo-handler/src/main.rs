//! Echo handler — a business-neutral WASM component for the Rill runtime.
//!
//! This handler implements the `invoke-handler` WIT world defined in
//! `crates/rill-handler-api/wit/rill-handler.wit`. It accepts any model JSON
//! in `configure()` and echoes the invoke input back as output. It is the
//! official example handler used in CI and release assets to demonstrate that
//! the same `rill-runtime` binary can serve different handlers without
//! recompilation.

wit_bindgen::generate!({
    path: "../../crates/rill-handler-api/wit/rill-handler.wit",
    world: "invoke-handler",
});

/// Echo handler implementation.
struct EchoHandler;

impl Guest for EchoHandler {
    fn metadata() -> HandlerMetadata {
        HandlerMetadata {
            id: "rillml.echo.handler".into(),
            version: "0.7.2".into(),
            api_version: 1,
            capabilities: vec!["rillml.linearRegression.predict".into()],
        }
    }

    fn configure(_model_json: Vec<u8>) -> Result<(), HandlerError> {
        // Echo handler does not parse the model; it accepts any JSON.
        Ok(())
    }

    fn invoke(capability: String, input_json: Vec<u8>) -> Result<Vec<u8>, HandlerError> {
        match capability.as_str() {
            "rillml.linearRegression.predict" => Ok(input_json),
            other => Err(HandlerError::UnsupportedCapability(other.to_string())),
        }
    }
}

export!(EchoHandler);

fn main() {}

//! Test-only handler that loops forever in `metadata()`.
//!
//! This handler exists to verify that the host's epoch deadline and fuel
//! budget bound the `metadata()` stage of `WasmInvokeHandler::new()`.
//! Unlike the malicious handler's mode-driven `configure()` loop, this
//! handler always loops in `metadata()` — there is no way to control it
//! via model JSON because `metadata()` is called before `configure()`.
//!
//! The host sets an independent fuel budget (`CONFIGURE_FUEL`) and epoch
//! deadline (`EPOCH_DEADLINE`) on the `metadata()` stage (see
//! `handler/wasm.rs` stage 2), so the call must be interrupted within a
//! reasonable window of the 5-second wall-clock deadline.
//!
//! This handler is a test fixture only and is never published. It is
//! excluded from the workspace and built separately by CI before running
//! sandbox tests.

wit_bindgen::generate!({
    path: "../../crates/rill-handler-api/wit/rill-handler.wit",
    world: "invoke-handler",
});

/// Burn fuel/epoch until interrupted. `black_box` prevents the compiler
/// from eliminating the loop. This is the same pattern used by the
/// malicious handler's `burn_forever()`.
fn burn_forever() -> ! {
    let mut i = 0u64;
    loop {
        i = i.wrapping_add(1);
        std::hint::black_box(i);
    }
}

struct MetadataLoopHandler;

impl Guest for MetadataLoopHandler {
    fn metadata() -> HandlerMetadata {
        // Loop forever in metadata() to verify the host enforces an
        // independent wall-clock deadline on this stage. The host must
        // interrupt this loop via epoch deadline + fuel budget and return
        // a `HandlerLoadError::Init` mentioning "metadata trap".
        burn_forever()
    }

    fn configure(_model_json: Vec<u8>) -> Result<(), HandlerError> {
        // Unreachable: metadata() loops forever, so configure() is never
        // called. Return Ok defensively in case the host somehow reaches
        // this point.
        Ok(())
    }

    fn invoke(_capability: String, input_json: Vec<u8>) -> Result<Vec<u8>, HandlerError> {
        // Unreachable: metadata() loops forever, so invoke() is never
        // called. Echo the input defensively.
        Ok(input_json)
    }
}

export!(MetadataLoopHandler);

fn main() {}

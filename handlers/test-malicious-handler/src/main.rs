//! Malicious test handler for WASM sandbox attack tests.
//!
//! This handler implements the `invoke-handler` WIT world and accepts a
//! `"mode"` field in the model JSON passed to `configure()`. The mode
//! controls the handler's behavior during `configure()` and `invoke()`:
//!
//! - `"echo"` (default): echo the input (baseline)
//! - `"infinite-loop"`: loop forever in `invoke()` (tests epoch exhaustion → handlerTimeout)
//! - `"configure-infinite-loop"`: loop forever in `configure()` (tests configure epoch deadline)
//! - `"trap"`: execute `unreachable` (tests trap handling → handlerTrap)
//! - `"oversized-output"`: return >1 MiB JSON (tests output size limit → handlerOutputTooLarge)
//! - `"invalid-json"`: return invalid JSON bytes (tests output parsing → handlerInvalidOutput)
//! - `"long-error-string"`: return `HandlerError::ExecutionFailed` with a >16 KiB
//!   detail payload (tests host-side `MAX_DETAIL_BYTES` truncation)
//! - `"fuel-exhaustion"`: perform dense floating-point work that burns through
//!   the invoke fuel budget without an explicit infinite loop
//!
//! `metadata()` is always called before `configure()` and cannot read the
//! model JSON, so a metadata-infinite-loop fixture cannot be triggered via
//! the mode field. The host still protects `metadata()` with an independent
//! fuel budget and epoch deadline (see `handler/wasm.rs` stage 2).
//!
//! This handler is a test fixture only and is never published. It is excluded
//! from the workspace and built separately by CI before running sandbox tests.

wit_bindgen::generate!({
    path: "../../crates/rill-handler-api/wit/rill-handler.wit",
    world: "invoke-handler",
});

use std::cell::RefCell;

thread_local! {
    static MODE: RefCell<&'static str> = RefCell::new("echo");
    static OVERSIZED_OUTPUT: RefCell<Option<Vec<u8>>> = RefCell::new(None);
}

fn set_mode(mode: &'static str) {
    MODE.with(|m| *m.borrow_mut() = mode);
}

fn get_mode() -> &'static str {
    MODE.with(|m| *m.borrow())
}

/// Extract the mode from model JSON bytes via simple byte-pattern matching.
/// Avoids a serde dependency to keep the handler build fast.
fn parse_mode(model_json: &[u8]) -> &'static str {
    const MODES: &[(&[u8], &str)] = &[
        (b"\"mode\":\"infinite-loop\"", "infinite-loop"),
        (b"\"mode\":\"configure-infinite-loop\"", "configure-infinite-loop"),
        (b"\"mode\":\"trap\"", "trap"),
        (b"\"mode\":\"oversized-output\"", "oversized-output"),
        (b"\"mode\":\"invalid-json\"", "invalid-json"),
        (b"\"mode\":\"long-error-string\"", "long-error-string"),
        (b"\"mode\":\"fuel-exhaustion\"", "fuel-exhaustion"),
        (b"\"mode\":\"echo\"", "echo"),
    ];
    for (pattern, name) in MODES {
        if model_json
            .windows(pattern.len())
            .any(|window| window == *pattern)
        {
            return name;
        }
    }
    "echo"
}

/// Burn fuel/epoch until interrupted. `black_box` prevents the compiler
/// from eliminating the loop.
fn burn_forever() -> ! {
    let mut i = 0u64;
    loop {
        i = i.wrapping_add(1);
        std::hint::black_box(i);
    }
}

struct MaliciousHandler;

impl Guest for MaliciousHandler {
    fn metadata() -> HandlerMetadata {
        HandlerMetadata {
            id: "rillml.test.malicious".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            api_version: 1,
            capabilities: vec!["rillml.linearRegression.predict".into()],
        }
    }

    fn configure(model_json: Vec<u8>) -> Result<(), HandlerError> {
        let mode = parse_mode(&model_json);
        set_mode(mode);

        // `configure-infinite-loop`: burn fuel/epoch in configure() to verify
        // the host enforces an independent wall-clock deadline on this stage.
        if mode == "configure-infinite-loop" {
            burn_forever();
        }

        // Pre-compute the oversized output during configure (10M fuel budget)
        // because invoke's 1M fuel budget is too small to construct >1 MiB of
        // data dynamically — even with Vec::resize (which compiles to the
        // memory.fill bulk-memory instruction), wasmtime charges fuel
        // proportional to the fill size for bulk-memory libcalls.
        if mode == "oversized-output" {
            let header = b"{\"data\":\"";
            let footer = b"\"}";
            let filler_len = 1024 * 1024 + 100;
            let mut output = Vec::with_capacity(header.len() + filler_len + footer.len());
            output.extend_from_slice(header);
            output.resize(output.len() + filler_len, b'x');
            output.extend_from_slice(footer);
            OVERSIZED_OUTPUT.with(|cell| *cell.borrow_mut() = Some(output));
        }
        Ok(())
    }

    fn invoke(capability: String, input_json: Vec<u8>) -> Result<Vec<u8>, HandlerError> {
        if capability != "rillml.linearRegression.predict" {
            return Err(HandlerError::UnsupportedCapability(capability));
        }
        match get_mode() {
            "echo" => Ok(input_json),
            "infinite-loop" => burn_forever(),
            "configure-infinite-loop" => {
                // configure() already burned through its deadline; an invoke
                // call should not be reachable for this mode because handler
                // construction fails first. Return ExecutionFailed defensively.
                Err(HandlerError::ExecutionFailed(
                    "configure-infinite-loop reached invoke".into(),
                ))
            }
            "trap" => {
                // Triggers a WASM trap for testing.
                core::arch::wasm32::unreachable()
            }
            "oversized-output" => {
                // Return the output pre-computed during configure(). Returning
                // a pre-built Vec consumes minimal fuel (just a move).
                OVERSIZED_OUTPUT
                    .with(|cell| cell.borrow_mut().take())
                    .ok_or_else(|| {
                        HandlerError::ExecutionFailed("oversized output not pre-computed".into())
                    })
            }
            "invalid-json" => {
                // Return bytes that are not valid JSON/UTF-8 to trigger the
                // handlerInvalidOutput error path.
                Ok(b"\xff\xfe\x00\x01 not valid json \x00".to_vec())
            }
            "long-error-string" => {
                // Return an ExecutionFailed whose detail exceeds the host's
                // MAX_DETAIL_BYTES (4 KiB). The host must truncate the detail
                // before storing it so a malicious guest cannot grow host
                // memory unboundedly through the error path. The detail
                // payload is intentionally 16 KiB so truncation is exercised.
                let detail = "X".repeat(16 * 1024);
                Err(HandlerError::ExecutionFailed(detail))
            }
            "fuel-exhaustion" => {
                // Perform dense but legitimate floating-point work that burns
                // through the invoke fuel budget quickly. Unlike
                // `infinite-loop`, this path performs real arithmetic rather
                // than an empty `wrapping_add` loop, exercising the fuel
                // accounting for numeric instructions.
                let mut acc = 1.0f64;
                let mut i = 0u64;
                loop {
                    i = i.wrapping_add(1);
                    acc = acc * 1.0000001 + i as f64;
                    std::hint::black_box(acc);
                }
            }
            _ => Ok(input_json),
        }
    }
}

export!(MaliciousHandler);

fn main() {}

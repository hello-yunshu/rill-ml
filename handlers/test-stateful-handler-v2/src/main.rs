//! Test-only Stateful Handler ABI v2 component.

#![allow(unsafe_op_in_unsafe_fn)]

wit_bindgen::generate!({
    path: "../../crates/rill-handler-api/wit-v2/rill-handler.wit",
    world: "stateful-handler",
});

use std::cell::RefCell;

thread_local! {
    static MODE: RefCell<&'static str> = RefCell::new("normal");
    static LARGE_OUTPUT: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

fn contains(bytes: &[u8], pattern: &[u8]) -> bool {
    bytes.windows(pattern.len()).any(|window| window == pattern)
}

fn burn_forever() -> ! {
    let mut value = 0_u64;
    loop {
        value = value.wrapping_add(1);
        std::hint::black_box(value);
    }
}

struct Fixture;

impl Guest for Fixture {
    fn metadata() -> HandlerMetadataV2 {
        HandlerMetadataV2 {
            id: "rillml.test.stateful-v2".into(),
            version: "2.0.0-preview".into(),
            api_version: 2,
            capabilities: vec!["org.example.decide".into()],
            state_schema_version: 1,
        }
    }

    fn configure(model_json: Vec<u8>) -> Result<(), HandlerErrorV2> {
        let mode = if contains(&model_json, b"\"mode\":\"timeout\"") {
            "timeout"
        } else if contains(&model_json, b"\"mode\":\"trap\"") {
            "trap"
        } else if contains(&model_json, b"\"mode\":\"corrupt-state\"") {
            "corrupt-state"
        } else if contains(&model_json, b"\"mode\":\"oversized-state\"") {
            "oversized-state"
        } else if contains(&model_json, b"\"mode\":\"oversized-output\"") {
            let mut output = b"{\"data\":\"".to_vec();
            output.resize(1024 * 1024 + 100, b'x');
            output.extend_from_slice(b"\"}");
            LARGE_OUTPUT.with(|slot| *slot.borrow_mut() = Some(output));
            "oversized-output"
        } else {
            "normal"
        };
        MODE.with(|value| *value.borrow_mut() = mode);
        Ok(())
    }

    fn handle(
        _event_json: Vec<u8>,
        current_state: Vec<u8>,
        _deterministic_seed: Option<u64>,
    ) -> Result<HandlerResultV2, HandlerErrorV2> {
        MODE.with(|mode| match *mode.borrow() {
            "timeout" => burn_forever(),
            "trap" => core::arch::wasm32::unreachable(),
            "corrupt-state" => Ok(HandlerResultV2 {
                output_json: br#"{"accepted":true}"#.to_vec(),
                next_state: b"not-json".to_vec(),
            }),
            "oversized-state" => Ok(HandlerResultV2 {
                output_json: br#"{"accepted":true}"#.to_vec(),
                next_state: vec![b'x'; 256 * 1024 + 1],
            }),
            "oversized-output" => Ok(HandlerResultV2 {
                output_json: LARGE_OUTPUT.with(|slot| slot.borrow_mut().take().unwrap()),
                next_state: current_state,
            }),
            _ => Ok(HandlerResultV2 {
                output_json: br#"{"accepted":true}"#.to_vec(),
                next_state: br#"{"count":1}"#.to_vec(),
            }),
        })
    }
}

export!(Fixture);

fn main() {}

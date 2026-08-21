#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_runtime::{TrustStore, load_handler_pack};
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let _ = load_handler_pack(Cursor::new(data), &TrustStore::default());
});

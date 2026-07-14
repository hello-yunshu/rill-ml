//! Host-side compile check for the WASM crate's rlib target.
//!
//! Full `wasm-bindgen-test` runs under `wasm32-unknown-unknown` via
//! `wasm-pack test --node` (see `tests/wasm_api.rs`). This file only ensures
//! the rlib target compiles on the host so that `cargo check --workspace`
//! and `cargo clippy --workspace` keep working without a wasm toolchain
//! installed locally.

#[test]
fn crate_compiles_on_host() {
    // If this test compiles, the rlib target's API surface is valid.
    // Functional verification happens on wasm32 via `wasm-bindgen-test`.
}

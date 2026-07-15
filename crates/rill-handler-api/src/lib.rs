//! Versioned WIT ABI and guest SDK for Rill runtime handlers.
//!
//! This crate ships the canonical WIT world definition
//! ([`wit/rill-handler.wit`](https://github.com/hello-yunshu/rill-ml/tree/main/crates/rill-handler-api/wit/rill-handler.wit))
//! that every Rill handler must export.
//!
//! Handler authors compile a Rust crate to `wasm32-wasip1` using
//! [`wit-bindgen`](https://crates.io/crates/wit-bindgen) to generate guest
//! bindings from the WIT file, then wrap the core module into a Component
//! Model component with `wasm-tools component new`.
//!
//! This crate intentionally has **no dependencies** so that it can be consumed
//! as a build-time WIT source without pulling in any runtime code. The
//! constants below mirror the WIT declarations and are used by both the host
//! and guest for compile-time version checks.

/// Handler ABI version. Independent from the host IPC API version and the model
/// pack format version. Increment only on a breaking WIT change.
pub const HANDLER_API_VERSION: u32 = 1;

/// WIT package name as declared in `rill-handler.wit`.
pub const WIT_PACKAGE: &str = "rill:handler";

/// WIT package version as declared in `rill-handler.wit`.
pub const WIT_VERSION: &str = "1.0.0";

/// WIT world name as declared in `rill-handler.wit`.
pub const WIT_WORLD: &str = "invoke-handler";

/// Maximum number of capabilities a handler may declare.
pub const MAX_CAPABILITIES: usize = 32;

/// Maximum length of a capability string.
pub const MAX_CAPABILITY_LEN: usize = 96;

/// Maximum length of a handler id.
pub const MAX_HANDLER_ID_LEN: usize = 96;

/// Maximum length of a handler version string.
pub const MAX_HANDLER_VERSION_LEN: usize = 48;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_stable() {
        assert_eq!(HANDLER_API_VERSION, 1);
        assert_eq!(WIT_PACKAGE, "rill:handler");
        assert_eq!(WIT_VERSION, "1.0.0");
        assert_eq!(WIT_WORLD, "invoke-handler");
    }
}

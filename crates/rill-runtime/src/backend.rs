//! Runtime backend policy shared by the library and the CLI diagnostics.
//!
//! Keep the target decision in one place: diagnostics must describe the same
//! Wasmtime configuration that the handler adapters actually use.

#![allow(dead_code)]

/// A CI-only compile-time switch used to execute the OHOS Pulley path on an
/// executable ARM64 Linux runner. Release builds never set this variable.
const TEST_FORCE_PULLEY64: bool = option_env!("RILL_TEST_PULLEY64").is_some();

/// The Rill release identity for the current compilation target.
pub const fn platform_identity() -> &'static str {
    if cfg!(all(
        target_os = "linux",
        target_env = "ohos",
        target_arch = "aarch64",
        target_pointer_width = "64",
        target_endian = "little"
    )) {
        "ohos"
    } else {
        std::env::consts::OS
    }
}

/// The Rust target environment, additive to the legacy `os` diagnostic field.
pub const fn target_environment() -> &'static str {
    if cfg!(target_env = "ohos") {
        "ohos"
    } else if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else if cfg!(target_env = "msvc") {
        "msvc"
    } else {
        "unknown"
    }
}

/// The Wasmtime backend selected by the runtime policy.
pub const fn runtime_backend() -> &'static str {
    if cfg!(all(
        target_os = "linux",
        target_env = "ohos",
        target_arch = "aarch64",
        target_pointer_width = "64",
        target_endian = "little"
    )) || TEST_FORCE_PULLEY64
    {
        "pulley64"
    } else if cfg!(any(
        target_arch = "arm",
        target_arch = "x86",
        target_arch = "mips"
    )) {
        if cfg!(all(target_arch = "mips", target_endian = "big")) {
            "pulley32be"
        } else {
            "pulley32"
        }
    } else {
        "cranelift"
    }
}

/// Apply the target-specific Wasmtime configuration used by all WASM handler
/// adapters. The public Rill API remains unchanged.
#[cfg(feature = "wasm")]
pub fn configure_wasmtime(config: &mut wasmtime::Config) -> Result<(), wasmtime::Error> {
    if runtime_backend() == "pulley64" {
        config.target("pulley64")?;
    }
    Ok(())
}

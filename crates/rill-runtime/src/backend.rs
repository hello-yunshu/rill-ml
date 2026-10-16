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

#[cfg(all(test, feature = "wasm"))]
mod security_tests {
    use super::configure_wasmtime;
    use wasmtime::component::{Component, Linker};
    use wasmtime::{Config, Engine, Store};

    fn engine() -> Engine {
        let mut config = Config::new();
        configure_wasmtime(&mut config).unwrap();
        Engine::new(&config).unwrap()
    }

    #[test]
    fn async_components_are_rejected_by_the_production_engine() {
        // wasm-tools 1.254.0 encodings of (component (type (future string)))
        // and (component (type (func async))). No guest code is executed.
        for bytes in [
            &[0, 97, 115, 109, 13, 0, 1, 0, 7, 4, 1, 101, 1, 115][..],
            &[0, 97, 115, 109, 13, 0, 1, 0, 7, 5, 1, 67, 0, 1, 0][..],
        ] {
            let error = Component::new(&engine(), bytes)
                .err()
                .expect("async must be rejected");
            assert!(
                format!("{error:#}").contains("async"),
                "unexpected rejection: {error:#}"
            );
        }
    }

    #[test]
    fn filesystem_host_import_cannot_be_linked() {
        // (component (import "wasi:filesystem/types@0.2.0" (instance (export "open" (func)))))
        let bytes = [
            0, 97, 115, 109, 13, 0, 1, 0, 7, 17, 1, 66, 2, 1, 64, 0, 1, 0, 4, 0, 4, 111, 112, 101,
            110, 1, 0, 10, 32, 1, 0, 27, 119, 97, 115, 105, 58, 102, 105, 108, 101, 115, 121, 115,
            116, 101, 109, 47, 116, 121, 112, 101, 115, 64, 48, 46, 50, 46, 48, 5, 0,
        ];
        let engine = engine();
        let component = Component::new(&engine, bytes).unwrap();
        // The production adapters also use an empty linker: no WASI or host commands.
        let linker = Linker::<()>::new(&engine);
        let mut store = Store::new(&engine, ());
        let error = linker
            .instantiate(&mut store, &component)
            .err()
            .expect("host import must fail");
        assert!(format!("{error:#}").contains("wasi:filesystem/types@0.2.0"));
    }
}

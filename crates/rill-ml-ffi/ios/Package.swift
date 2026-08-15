// swift-tools-version: 5.9
//
// RillMl — Swift wrapper over the Stable C FFI (crates/rill-ml-ffi).
//
// The package exposes one library product `RillMl`. The C ABI is surfaced
// through the `CRillMl` C target (module map + forwarding header), and the
// `RillMl` Swift target provides the ergonomic, ownership-managed wrapper
// (Mean / LinearRegression) used by iOS apps.
//
// Linking the implementation:
//   The math lives in the Rust staticlib `librill_ml_ffi.a` (built by
//   scripts/build-ios-framework.sh, or by cargo for the host). This package
//   only declares the ABI; the final link must supply the symbols.
//
//   * Host `swift test`: build a host staticlib and drop it into
//     `.build-libs/librill_ml_ffi.a` (see README). This is the default
//     `RILL_ML_RUST_LIB_DIR`.
//
//   * iOS cross builds: scripts/build-ios-framework.sh sets
//     `RILL_ML_RUST_LIB_DIR` to the per-target cargo release directory, so
//     the framework binary is assembled with the matching Rust staticlib.

import PackageDescription
import Foundation

let rustLibDir = ProcessInfo.processInfo.environment["RILL_ML_RUST_LIB_DIR"]
    ?? (FileManager.default.currentDirectoryPath + "/.build-libs")

let package = Package(
    name: "RillMl",
    products: [
        .library(name: "RillMl", targets: ["RillMl"])
    ],
    targets: [
        // C target exposing the Stable C ABI as a SwiftPM module. The
        // forwarding header pulls in ../../../include/rill_ml.h (the single
        // source of truth for the C ABI).
        .target(
            name: "CRillMl",
            publicHeadersPath: "include"
        ),
        .target(
            name: "RillMl",
            dependencies: ["CRillMl"],
            linkerSettings: [
                .linkedLibrary("rill_ml_ffi"),
                .unsafeFlags(["-L", rustLibDir])
            ]
        ),
        .testTarget(
            name: "RillMlTests",
            dependencies: ["RillMl"]
        )
    ]
)

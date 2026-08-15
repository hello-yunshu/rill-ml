# rill-ml-ffi · iOS (Swift) binding

A Swift package wrapping the RillML Stable C FFI for iOS:

```
iOS App → RillMl (Swift) → CRillMl (module map) → rill-ml-ffi → rill-ml Core
```

Only the opaque-handle C ABI declared in
[`../include/rill_ml.h`](../include/rill_ml.h) is used — there is no second
implementation of the math.

## Layout

```
ios/
├── Package.swift                          SwiftPM manifest (product `RillMl`)
├── Sources/
│   ├── CRillMl/                           C target exposing the C ABI module
│   │   ├── include/module.modulemap
│   │   ├── include/rill_ml.h              forwarding header → ../include/rill_ml.h
│   │   └── dummy.c                        (SwiftPM requires ≥1 C source)
│   └── RillMl/                            Swift wrapper
│       ├── RillMl.swift                   RillMl facade + RillMlError
│       ├── Mean.swift                     online mean accumulator
│       └── LinearRegression.swift         online linear regression (SGD)
├── Tests/RillMlTests/RillMlTests.swift    end-to-end wrapper tests
└── README.md
```

## Building the native library

The math lives in the Rust staticlib `librill_ml_ffi.a`. It is produced per
platform by:

- iOS device + simulator slices: `scripts/build-ios-framework.sh` (builds the
  Rust staticlib for `aarch64-apple-ios` and `aarch64-apple-ios-sim`, assembles
  a static `RillMl.framework` per slice, and combines them into
  `dist-ios/RillMl.xcframework`).
- macOS host (for `swift test`): 

  ```sh
  cargo build --release -p rill-ml-ffi --target aarch64-apple-darwin
  mkdir -p .build-libs
  cp ../../../../target/aarch64-apple-darwin/release/librill_ml_ffi.a .build-libs/
  ```

  Then `swift test` from this directory runs the real wrapper against the real
  Rust core. `.build-libs/` is gitignored.

`RILL_ML_RUST_LIB_DIR` overrides where the package looks for the staticlib
(the build script sets it to each cargo target's release directory).

## Ownership and error semantics

- **Handles.** `init`/`fromJSON` create an owned opaque handle. Each wrapper
  releases its handle exactly once — explicitly via `close()` or implicitly
  in `deinit`. Using an instance after `close()` throws
  `RillMlError.invalidHandle`.
- **Thread safety.** A single instance must not be shared concurrently between
  threads; distinct instances may be used from distinct threads.
- **Errors.** Every fallible call validates the C error code (see the
  `RILL_ML_*` constants in `rill_ml.h`) and throws the matching
  `RillMlError` case carrying the FFI message.
- **Snapshots.** `toJSON()`/`fromJSON()` use versioned JSON envelopes; the
  wrapper grows its output buffer on `RILL_ML_ERR_BUFFER_TOO_SMALL` up to the
  core 64 MiB snapshot limit.

## Example

```swift
import RillMl

let mean = try Mean()
try mean.update(1.0)
try mean.update(2.0)
try mean.update(3.0)
print(try mean.value()) // 2.0

let json = try mean.toJSON()
let restored = try Mean.fromJSON(json)
print(try restored.count()) // 3

let lr = try LinearRegression(featureCount: 1, learningRate: 0.05)
for _ in 0..<100 { try lr.learn(features: [2.0], target: 10.0) }
print(try lr.predict([2.0])) // ≈ 10.0
```

## Versioning

`RillMlInfo.version()` returns the `rill-ml-ffi` crate version (`0.15.0`);
`RillMlInfo.snapshotFormatVersion` returns `1`.

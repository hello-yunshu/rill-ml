# Third-Party Notices

This file lists third-party software, ideas, or code that have influenced or
been incorporated into RillML, along with the applicable license terms.

## RillML license

RillML is licensed under the MIT License (`LICENSE-MIT`).

## Inspirations and prior art

### River (online machine learning for Python)

RillML is **inspired by the online-learning workflow popularized by River**
(<https://riverml.xyz/>), including the predict → metric.update → learn
evaluation order and the idea of bounded-memory online models.

Important clarifications:

- RillML's code is **independently implemented** in Rust. It is not a port,
  translation, or derivative work of River's Python source code.
- No River source code is included in this repository.
- RillML is **not affiliated with, endorsed by, or sponsored by** River, its
  maintainers, or any related organization.
- The name "RillML" is used solely to evoke the idea of a small, flowing
  stream of data; it does not imply any connection to River or to Rill Data.

River is distributed under the BSD-3-Clause License. We acknowledge the River
team for demonstrating that online machine learning can be both practical and
ergonomic.

### Linfa, SmartCore, Burn

The broader Rust ML ecosystem has influenced RillML's API conventions:

- **Linfa** (<https://github.com/rust-ml/linfa>): Rust ML toolkit, MIT OR
  Apache-2.0. RillML differs in being single-pass/online rather than batch.
- **SmartCore** (<https://github.com/smartcorelib/smartcore>): Rust ML
  library, Apache-2.0. RillML differs in scope and in the online-first
  contract.
- **Burn** (<https://github.com/Tracel-AI/burn>): Deep-learning framework in
  Rust, MIT OR Apache-2.0. RillML is not a deep-learning framework and does
  not share code with Burn.

None of these projects' source code is included in RillML. The mentions here
are for attribution of ideas only.

## Algorithms and references

The numerical algorithms used in RillML are standard, long-published results.
Specific references are cited in the module-level rustdoc where relevant:

- **Welford's algorithm** for online variance: Welford, B. P. (1962). "Note
  on a method for calculating corrected sums of squares and products."
  *Technometrics* 4(3): 419–420.
- **Exponentially weighted moving mean**: standard recurrence used in
  time-series analysis and control charts.
- **SGD with L2 regularization**: standard online gradient descent.
- **AdaGrad**: Duchi, J., Hazan, E., Singer, Y. (2011). "Adaptive Subgradient
  Methods for Online Learning and Stochastic Optimization." *JMLR* 12:
  2121–2159.
- **Huber loss**: Huber, P. J. (1964). "Robust Estimation of a Location
  Parameter." *Annals of Statistics* 53(1): 73–101.
- **Sigmoid and log loss**: standard logistic-regression material; the
  implementation uses a numerically stable form to avoid overflow.

These algorithms are not copyrightable in themselves; any implementation in
RillML is original Rust code written for this project.

## Dependencies

RillML is published as multiple crates. Each crate section below lists only the
dependencies that crate links into its own release artefact. Workspace-internal
crates (`rill-ml`, `rill-handler-api`, `rill-runtime-protocol`,
`rill-runtime`) are not repeated as third-party dependencies.

### `rill-ml` runtime dependencies

| Crate           | Version range | License        | Required by          |
| --------------- | ------------- | -------------- | -------------------- |
| `thiserror`     | ^1            | MIT OR Apache-2.0 | error types        |
| `rand`          | ^0.8          | MIT OR Apache-2.0 | bandit exploration |
| `serde`         | ^1            | MIT OR Apache-2.0 | optional, behind `serde` feature |
| `serde_json`    | ^1            | MIT OR Apache-2.0 | only in dev-deps and examples |

Dev-dependencies (used only for tests, examples, and benchmarks; **not**
linked into releases of `rill-ml`):

| Crate          | License          | Purpose                          |
| -------------- | ---------------- | -------------------------------- |
| `approx`       | MIT OR Apache-2.0 | float-equality assertions       |
| `proptest`     | MIT OR Apache-2.0 | property-based testing          |
| `rand_chacha`  | MIT OR Apache-2.0 | deterministic RNG for tests     |
| `serde_json`   | MIT OR Apache-2.0 | JSON (de)serialization in tests |
| `criterion`    | MIT OR Apache-2.0 | benchmarks                      |

### `rill-handler-api` runtime dependencies

`rill-handler-api` has no third-party runtime dependencies. It only re-exports
the WIT handler ABI constant (`HANDLER_API_VERSION`) and the canonical WIT
world path. Handler authors compile their guest components against the WIT file
directly using `wit-bindgen`; the host uses `wasmtime::component::bindgen!` to
generate bindings from the same file.

### `rill-runtime-protocol` runtime dependencies

| Crate           | Version range | License        | Required by          |
| --------------- | ------------- | -------------- | -------------------- |
| `serde`         | ^1            | MIT OR Apache-2.0 | IPC and manifest (de)serialization |
| `serde_json`    | ^1            | MIT OR Apache-2.0 | JSON IPC wire format |

### `rill-runtime` runtime dependencies (default features)

| Crate              | Version range | License        | Required by          |
| ------------------ | ------------- | -------------- | -------------------- |
| `clap`             | ^4.5          | MIT OR Apache-2.0 | `rill-runtime` and `rill-pack` CLI argument parsing |
| `ed25519-dalek`    | 3.0.0         | BSD-3-Clause     | Ed25519 signature verification for model and handler packs |
| `hex`              | ^0.4          | MIT OR Apache-2.0 | hex encoding/decoding of public keys and digests |
| `semver`           | ^1.0          | MIT OR Apache-2.0 | manifest version parsing and compatibility checks |
| `serde`            | ^1            | MIT OR Apache-2.0 | manifest and archive (de)serialization |
| `serde_json`       | ^1            | MIT OR Apache-2.0 | canonical JSON for manifest and checksums |
| `sha2`             | ^0.10         | MIT OR Apache-2.0 | SHA-256 digests for pack contents |
| `thiserror`        | ^1            | MIT OR Apache-2.0 | error enums for pack and runtime |
| `zip`              | ^2.2          | MIT              | read/write `.rillpack` and `.rillhandler` ZIP archives |

Dev-dependencies (used only for tests; **not** linked into releases of
`rill-runtime`):

| Crate          | License          | Purpose                          |
| -------------- | ---------------- | -------------------------------- |
| `tempfile`     | MIT OR Apache-2.0 | temporary pack files in tests    |

### `rill-runtime` optional dependencies (`wasm` feature)

The `wasm` feature is opt-in. Default builds of `rill-runtime` do not pull in
Wasmtime and cannot load `.rillhandler` packs. When the feature is enabled:

| Crate              | Version range | License        | Required by          |
| ------------------ | ------------- | -------------- | -------------------- |
| `wasmtime`         | ^46           | Apache-2.0 WITH LLVM-exception | sandboxed WASM component execution |

Wasmtime's `Apache-2.0 WITH LLVM-exception` license is compatible with
RillML's MIT license for both distribution and static/dynamic linking. The
`LLVM-exception` is an additional permission that further relaxes the Apache
default, so it does not impose additional obligations on RillML consumers
beyond what Apache-2.0 already requires. Wasmtime is configured with the
minimal feature set (`cranelift`, `component-model`, `runtime`,
`parallel-compilation`); the default feature set is explicitly disabled to
avoid pulling in WASI and other unused subsystems.

### License compatibility notes

- `ed25519-dalek` 3.x is distributed under the BSD-3-Clause License. BSD-3-Clause
  is permissive and compatible with RillML's MIT license; downstream users may
  redistribute combined works under MIT while honouring the BSD-3-Clause
  attribution and disclaimer clauses.
- `wasmtime` is distributed under the Apache-2.0 License with the LLVM
  exception. The LLVM exception explicitly permits static and dynamic linking
  with code under other licenses (including permissive licenses such as MIT)
  without copyleft-style obligations. RillML's use of Wasmtime as an optional
  dependency behind the `wasm` feature does not change the licensing of the
  core `rill-ml`, `rill-handler-api`, or `rill-runtime-protocol` crates.
- `zip` is distributed under the MIT License.

All listed crates are compatible with RillML's MIT license. If a future
contribution introduces a dependency under a different license, it must be
vetted for compatibility and listed here.

## Bundled assets

RillML bundles no binary assets, no vendored third-party source code, and no
generated protocol files. The repository contains only original Rust source,
documentation, configuration, and example data generated by the examples
themselves.

## Updates to this file

Whenever a new dependency is added, an algorithm is adapted from a specific
source, or a license changes, this file must be updated in the same pull
request. Failing to keep `THIRD_PARTY_NOTICES.md` accurate is a blocking
review issue.

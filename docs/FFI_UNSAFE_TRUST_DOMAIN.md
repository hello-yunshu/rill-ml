# C FFI unsafe trust domain

`rill-ml-ffi` is Preview and exposes an opaque-handle C ABI. Rust internal
layouts are not part of the ABI: consumers may only use symbols and types from
[`include/rill_ml.h`](../crates/rill-ml-ffi/include/rill_ml.h).

The `unsafe extern "C"` boundary is a caller-owned trust domain. Before every
call, a binding must guarantee:

- the handle is the correct live opaque handle and is destroyed exactly once;
- pointer arguments are non-null when required and point to readable or
  writable memory for the declared element/byte count;
- feature counts, output lengths, and C strings are valid for the operation;
- one handle is not accessed concurrently from multiple threads;
- snapshots are treated as untrusted input and restored only through
  `*_from_json`, which applies the size, envelope-version, and model-state
  validators.

The library catches Rust panics at the ABI boundary and returns an error code,
but this does not make dangling pointers, wrong-type handles, data races, or
invalid buffer ownership safe. Those remain undefined behavior in the caller.
All raw-pointer dereferences are isolated in documented helpers and every
export uses the same error-buffer contract. The Rust ABI tests exercise the
real exported symbols for lifecycle, null-handle, invalid-argument, buffer,
snapshot, and error-message behavior; the C smoke test covers an actual native
consumer path.

FFI is not a substitute for a stable wire protocol. Consumers needing process
isolation should use `rill-runtime-protocol` and the signed Runtime packages.

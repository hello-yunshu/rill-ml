//! Independently distributable runtime for RillML.
//!
//! Implementation modules (`archive`, `handler`, `handler_package`, `package`,
//! `server`) are `pub(crate)` so that downstream code — including the
//! `rill-runtime` and `rill-pack` binaries, integration tests, and external
//! crates — depends only on the top-level re-exports declared in this file.
//! This avoids the dual-module-path problem where both
//! `rill_runtime::handler::wasm::WasmInvokeHandler` and
//! `rill_runtime::WasmInvokeHandler` would otherwise be part of the 1.x
//! public API surface. Only the re-export path is stable.

pub(crate) mod archive;
pub(crate) mod handler;
pub(crate) mod handler_package;
pub(crate) mod package;
pub(crate) mod server;

pub use archive::{
    ArchiveError, ReleaseIndexError, TrustStore, canonical_json, sign_release_index,
    verify_release_index,
};
pub use handler::builtin::{LINEAR_REGRESSION_CAPABILITY, LinearRegressionInvokeHandler};
#[cfg(feature = "wasm")]
pub use handler::wasm::WasmInvokeHandler;
#[cfg(feature = "wasm")]
pub use handler::wasm::{
    CONFIGURE_FUEL, EPOCH_DEADLINE, EPOCH_TICK_INTERVAL, INVOKE_FUEL, MAX_IO_BYTES,
    MAX_MEMORY_BYTES, MAX_TABLE_ELEMENTS,
};
pub use handler::{HandlerIdentity, HandlerLoadError, effective_capabilities};
pub use handler_package::{
    HandlerPackError, HandlerPackInspection, LoadedHandlerPack, build_signed_handler_pack,
    load_handler_pack,
};
pub use package::{
    LoadedModelPack, ModelPackError, ModelPackInspection, build_signed_model_pack, load_model_pack,
};
pub use server::{
    EngineResponse, HostLogSink, InvokeError, InvokeErrorKind, InvokeHandler, MAX_DETAIL_BYTES,
    RuntimeEngine, StderrLogSink,
};

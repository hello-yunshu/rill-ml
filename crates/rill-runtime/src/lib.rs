//! Independently distributable runtime for RillML.

pub mod archive;
pub mod handler;
pub mod handler_package;
pub mod package;
pub mod server;

pub use archive::{
    ReleaseIndexError, TrustStore, canonical_json, sign_release_index, verify_release_index,
};
pub use handler::builtin::{LINEAR_REGRESSION_CAPABILITY, LinearRegressionInvokeHandler};
#[cfg(feature = "wasm")]
pub use handler::wasm::WasmInvokeHandler;
pub use handler::{HandlerIdentity, HandlerLoadError, effective_capabilities};
pub use handler_package::{
    HandlerPackError, HandlerPackInspection, LoadedHandlerPack, build_signed_handler_pack,
    load_handler_pack,
};
pub use package::{
    LoadedModelPack, ModelPackError, ModelPackInspection, build_signed_model_pack, load_model_pack,
};
pub use server::{EngineResponse, InvokeHandler, RuntimeEngine};

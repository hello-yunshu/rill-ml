//! Sandboxed WASM handler adapter.
//!
//! Loads a signed `.rillhandler` module, instantiates it inside a Wasmtime
//! component sandbox with strict resource limits, and adapts it to the
//! [`InvokeHandler`] trait.
//!
//! ## Sandbox guarantees
//!
//! - No WASI imports (no filesystem, network, environment, stdio, process).
//! - Fuel budget per `configure`/`invoke` call.
//! - Epoch interruption for wall-clock timeout.
//! - Memory and table growth capped by [`HostLimits`].
//! - Input and output JSON bounded by [`MAX_IO_BYTES`].

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use serde_json::Value;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};

use crate::handler::HandlerLoadError;
use crate::handler_package::LoadedHandlerPack;
use crate::server::{InvokeError, InvokeErrorKind, InvokeHandler as InvokeHandlerTrait};

// Generate host bindings from the canonical WIT world. The macro emits an
// `invoke_handler` module containing the `InvokeHandler` instance struct.
mod invoke_handler {
    wasmtime::component::bindgen!({
        path: "../rill-handler-api/wit/rill-handler.wit",
        world: "invoke-handler",
    });
}

/// Fuel budget for a single `configure` call.
pub const CONFIGURE_FUEL: u64 = 10_000_000;
/// Fuel budget for a single `invoke` call.
///
/// Handler input and output are allowed to reach 1 MiB. The previous one-million
/// unit budget could be exhausted by ordinary JSON decoding before a handler's
/// algorithm ran (Mira's battery handler reproduced this with roughly 100
/// samples). The epoch deadline remains the authoritative five-second wall-clock
/// guard, while this larger deterministic budget lets valid bounded payloads run.
pub const INVOKE_FUEL: u64 = 100_000_000;
/// Maximum linear memory size per instance (64 MiB).
pub const MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
/// Maximum table entries per instance.
pub const MAX_TABLE_ELEMENTS: u32 = 10_000;
/// Maximum input/output JSON payload size (1 MiB, matches IPC limit).
pub const MAX_IO_BYTES: usize = 1024 * 1024;
/// Epoch tick interval (1 second).
pub const EPOCH_TICK_INTERVAL: Duration = Duration::from_secs(1);
/// Number of epoch ticks before interruption (5 seconds).
pub const EPOCH_DEADLINE: u64 = 5;

/// Per-instance resource limiter enforcing memory and table caps.
struct HostState;

impl ResourceLimiter for HostState {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _max: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(desired <= MAX_MEMORY_BYTES)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _max: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(desired <= MAX_TABLE_ELEMENTS as usize)
    }
}

struct WasmState {
    store: Store<HostState>,
    bindings: invoke_handler::InvokeHandler,
}

/// RAII guard for the background epoch-ticker thread.
///
/// The ticker must be running before any guest code is invoked so that
/// `metadata()`, `configure()` and `invoke()` are all bounded by the
/// epoch-deadline wall-clock timeout. Dropping the guard signals the thread
/// to stop and joins it; if init fails, dropping this guard ensures no
/// background thread is leaked.
struct EpochTicker {
    stop_flag: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl EpochTicker {
    /// Start the ticker. The thread sleeps for [`EPOCH_TICK_INTERVAL`] then
    /// calls `engine.increment_epoch()` until [`Self::stop`] is called.
    fn start(engine: Engine) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let engine_for_thread = engine;
        let stop_for_thread = Arc::clone(&stop_flag);
        let handle = std::thread::spawn(move || {
            while !stop_for_thread.load(Ordering::Relaxed) {
                std::thread::sleep(EPOCH_TICK_INTERVAL);
                engine_for_thread.increment_epoch();
            }
        });
        Self {
            stop_flag,
            handle: Some(handle),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // The thread sleeps for at most one tick before observing the
            // stop flag, so join waits at most ~1 second.
            let _ = handle.join();
        }
    }
}

/// Sandboxed WASM handler that implements [`InvokeHandler`].
///
/// The handler holds a Wasmtime [`Engine`], a background epoch-ticker thread,
/// and a [`Mutex`] protecting the [`Store`] and component instance. Calls are
/// serialised by the mutex; the first version does not support parallel
/// invocation.
pub struct WasmInvokeHandler {
    engine: Engine,
    _ticker: EpochTicker,
    state: Mutex<WasmState>,
}

impl WasmInvokeHandler {
    /// Load and instantiate a signed handler pack.
    ///
    /// Verifies that guest `metadata()` matches the signed manifest, then calls
    /// `configure()` with the canonical model JSON. Returns an error if any
    /// step fails; no partial state is retained.
    ///
    /// The epoch ticker is started before component instantiation so that
    /// `metadata()`, `configure()` and every later `invoke()` all run under
    /// the same wall-clock deadline. Fuel is reset before each call so the
    /// budgets do not pool across stages.
    pub fn new(pack: &LoadedHandlerPack, model_json: &Value) -> Result<Self, HandlerLoadError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.max_wasm_stack(1024 * 1024);

        let engine = Engine::new(&config)
            .map_err(|e| HandlerLoadError::Init(format!("engine creation failed: {e}")))?;
        // Start the epoch ticker before any guest code runs. If a later
        // step fails, the `EpochTicker` guard dropped at the end of this
        // function (or by `?` propagation) stops the thread.
        let ticker = EpochTicker::start(engine.clone());

        let component = Component::new(&engine, &pack.module)
            .map_err(|e| HandlerLoadError::Init(format!("component compilation failed: {e}")))?;

        let linker: Linker<HostState> = Linker::new(&engine);
        let mut store = Store::new(&engine, HostState);
        store.limiter(|state| state as &mut dyn ResourceLimiter);

        // Stage 1: component instantiation. Fresh fuel + deadline.
        store
            .set_fuel(CONFIGURE_FUEL)
            .map_err(|e| HandlerLoadError::Init(format!("failed to set instantiate fuel: {e}")))?;
        store.set_epoch_deadline(EPOCH_DEADLINE);
        let bindings = invoke_handler::InvokeHandler::instantiate(&mut store, &component, &linker)
            .map_err(|e| HandlerLoadError::Init(format!("instantiation failed: {e}")))?;

        // Stage 2: metadata(). Fresh fuel + deadline so it cannot inherit
        // leftover fuel from instantiation.
        store
            .set_fuel(CONFIGURE_FUEL)
            .map_err(|e| HandlerLoadError::Init(format!("failed to set metadata fuel: {e}")))?;
        store.set_epoch_deadline(EPOCH_DEADLINE);
        let metadata = bindings
            .call_metadata(&mut store)
            .map_err(|e| HandlerLoadError::Init(format!("metadata trap: {e}")))?;
        if metadata.id != pack.manifest.id {
            return Err(HandlerLoadError::MetadataMismatch(format!(
                "guest id '{}' != manifest id '{}'",
                metadata.id, pack.manifest.id
            )));
        }
        if metadata.version != pack.manifest.version {
            return Err(HandlerLoadError::MetadataMismatch(format!(
                "guest version '{}' != manifest version '{}'",
                metadata.version, pack.manifest.version
            )));
        }
        if metadata.api_version != pack.manifest.handler_api_version {
            return Err(HandlerLoadError::MetadataMismatch(format!(
                "guest api version {} != manifest api version {}",
                metadata.api_version, pack.manifest.handler_api_version
            )));
        }
        let mut manifest_caps = pack.manifest.capabilities.clone();
        manifest_caps.sort();
        let mut metadata_caps = metadata.capabilities.clone();
        metadata_caps.sort();
        if manifest_caps != metadata_caps {
            return Err(HandlerLoadError::MetadataMismatch(
                "guest capabilities != manifest capabilities".into(),
            ));
        }

        // Stage 3: configure(). Fresh fuel + deadline.
        let model_bytes = serde_json::to_vec(model_json)
            .map_err(|e| HandlerLoadError::Init(format!("model serialization failed: {e}")))?;
        if model_bytes.len() > MAX_IO_BYTES {
            return Err(HandlerLoadError::Init("model JSON exceeds limit".into()));
        }
        store
            .set_fuel(CONFIGURE_FUEL)
            .map_err(|e| HandlerLoadError::Init(format!("failed to set configure fuel: {e}")))?;
        store.set_epoch_deadline(EPOCH_DEADLINE);
        let configure_result = bindings
            .call_configure(&mut store, &model_bytes)
            .map_err(|e| HandlerLoadError::Init(format!("configure trap: {e}")))?;
        if let Err(handler_error) = configure_result {
            return Err(HandlerLoadError::Init(format!(
                "configure rejected model: {handler_error:?}"
            )));
        }

        Ok(Self {
            engine,
            _ticker: ticker,
            state: Mutex::new(WasmState { store, bindings }),
        })
    }

    /// Returns the engine reference (needed for external epoch control if any).
    #[allow(dead_code)]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

impl std::fmt::Debug for WasmInvokeHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmInvokeHandler")
            .field(
                "epoch_ticker_running",
                &!self._ticker.stop_flag.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl InvokeHandlerTrait for WasmInvokeHandler {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, InvokeError> {
        let input_bytes = serde_json::to_vec(input).map_err(|e| {
            InvokeError::with_detail(
                InvokeErrorKind::Internal,
                format!("input serialization failed: {e}"),
            )
        })?;
        if input_bytes.len() > MAX_IO_BYTES {
            return Err(InvokeError::new(InvokeErrorKind::Internal));
        }

        let mut state = self.state.lock().map_err(|_| {
            // A poisoned mutex indicates a panic in a previous call; the
            // handler is no longer usable. Surface this as an internal
            // error rather than crashing the runtime.
            InvokeError::with_detail(InvokeErrorKind::Internal, "handler state mutex poisoned")
        })?;

        state.store.set_fuel(INVOKE_FUEL).map_err(|e| {
            InvokeError::with_detail(
                InvokeErrorKind::Internal,
                format!("failed to set invoke fuel: {e}"),
            )
        })?;
        state.store.set_epoch_deadline(EPOCH_DEADLINE);

        // Destructure to avoid simultaneous immutable borrow of `bindings` and
        // mutable borrow of `store` through the same `MutexGuard`.
        let WasmState { store, bindings } = &mut *state;
        let result = bindings
            .call_invoke(store, capability, &input_bytes)
            .map_err(|e| {
                // Map fuel exhaustion and epoch interruption to handlerTimeout.
                // Wasmtime 46's Error Display wraps the trap in a WasmBacktrace
                // context, so string matching on the Display is unreliable;
                // downcast to the concrete Trap variant instead.
                if let Some(trap) = e.downcast_ref::<Trap>()
                    && matches!(trap, Trap::OutOfFuel | Trap::Interrupt)
                {
                    return InvokeError::new(InvokeErrorKind::Timeout);
                }
                // Avoid leaking the wasmtime Display (which may include a
                // WASM backtrace) to IPC clients; log it host-side instead.
                // The Display string is captured as host-only detail.
                let detail = format!("{e}");
                eprintln!("rill-runtime: handler trap: {detail}");
                InvokeError::with_detail(InvokeErrorKind::Trap, detail)
            })?;

        match result {
            Ok(output_bytes) => {
                if output_bytes.len() > MAX_IO_BYTES {
                    return Err(InvokeError::new(InvokeErrorKind::OutputTooLarge));
                }
                serde_json::from_slice(&output_bytes).map_err(|e| {
                    InvokeError::with_detail(
                        InvokeErrorKind::InvalidOutput,
                        format!("host-side JSON deserialisation failed: {e}"),
                    )
                })
            }
            Err(handler_error) => {
                // Guest reported a typed `handler-error` variant
                // (`invalid-model` / `invalid-input` /
                // `unsupported-capability` / `execution-failed`). The
                // detail string is fully guest-controlled, so it must
                // never reach IPC clients. We capture it as host-only
                // detail for `eprintln!` diagnostics and collapse the
                // variant to `handlerInternalError` on the wire (matching
                // the previous `map_invoke_error` behaviour).
                let detail = format!("{handler_error:?}");
                eprintln!("rill-runtime: guest handler-error for {capability}: {detail}");
                Err(InvokeError::with_detail(
                    InvokeErrorKind::ExecutionFailed,
                    detail,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `ResourceLimiter` implementation on `HostState`.
    //!
    //! The sandbox caps linear memory and table growth (see `MAX_MEMORY_BYTES`
    //! and `MAX_TABLE_ELEMENTS`). These tests verify the limiter directly so
    //! that a future refactor that weakens the caps is caught without
    //! requiring a malicious WASM component that tries to grow memory/table
    //! past the limits (which would be hard to author portably).

    use super::*;

    #[test]
    fn memory_limiter_accepts_growth_within_max() {
        let mut state = HostState;
        // Growth to exactly the cap is allowed.
        assert!(
            state
                .memory_growing(0, MAX_MEMORY_BYTES, None)
                .expect("memory_growing must not error")
        );
        // Growth below the cap is allowed.
        assert!(
            state
                .memory_growing(0, MAX_MEMORY_BYTES - 1, None)
                .expect("memory_growing must not error")
        );
    }

    #[test]
    fn memory_limiter_rejects_growth_exceeding_max() {
        let mut state = HostState;
        // Growth one byte beyond the cap is rejected.
        assert!(
            !state
                .memory_growing(MAX_MEMORY_BYTES - 1, MAX_MEMORY_BYTES + 1, None)
                .expect("memory_growing must not error")
        );
        // Growth far beyond the cap is rejected.
        assert!(
            !state
                .memory_growing(0, MAX_MEMORY_BYTES * 2, None)
                .expect("memory_growing must not error")
        );
    }

    #[test]
    fn table_limiter_accepts_growth_within_max() {
        let mut state = HostState;
        // Growth to exactly the cap is allowed.
        assert!(
            state
                .table_growing(0, MAX_TABLE_ELEMENTS as usize, None)
                .expect("table_growing must not error")
        );
        // Growth below the cap is allowed.
        assert!(
            state
                .table_growing(0, (MAX_TABLE_ELEMENTS - 1) as usize, None)
                .expect("table_growing must not error")
        );
    }

    #[test]
    fn table_limiter_rejects_growth_exceeding_max() {
        let mut state = HostState;
        // Growth one element beyond the cap is rejected.
        assert!(
            !state
                .table_growing(
                    (MAX_TABLE_ELEMENTS - 1) as usize,
                    (MAX_TABLE_ELEMENTS + 1) as usize,
                    None
                )
                .expect("table_growing must not error")
        );
        // Growth far beyond the cap is rejected.
        assert!(
            !state
                .table_growing(0, (MAX_TABLE_ELEMENTS * 2) as usize, None)
                .expect("table_growing must not error")
        );
    }
}

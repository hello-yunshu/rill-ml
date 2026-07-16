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
use crate::server::InvokeHandler as InvokeHandlerTrait;

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
pub const INVOKE_FUEL: u64 = 1_000_000;
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

/// Sandboxed WASM handler that implements [`InvokeHandler`].
///
/// The handler holds a Wasmtime [`Engine`], a background epoch-ticker thread,
/// and a [`Mutex`] protecting the [`Store`] and component instance. Calls are
/// serialised by the mutex; the first version does not support parallel
/// invocation.
pub struct WasmInvokeHandler {
    engine: Engine,
    stop_flag: Arc<AtomicBool>,
    epoch_thread: Option<std::thread::JoinHandle<()>>,
    state: Mutex<WasmState>,
}

impl WasmInvokeHandler {
    /// Load and instantiate a signed handler pack.
    ///
    /// Verifies that guest `metadata()` matches the signed manifest, then calls
    /// `configure()` with the canonical model JSON. Returns an error if any
    /// step fails; no partial state is retained.
    pub fn new(pack: &LoadedHandlerPack, model_json: &Value) -> Result<Self, HandlerLoadError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.max_wasm_stack(1024 * 1024);

        let engine = Engine::new(&config)
            .map_err(|e| HandlerLoadError::Init(format!("engine creation failed: {e}")))?;
        let component = Component::new(&engine, &pack.module)
            .map_err(|e| HandlerLoadError::Init(format!("component compilation failed: {e}")))?;

        let linker: Linker<HostState> = Linker::new(&engine);
        let mut store = Store::new(&engine, HostState);
        store.limiter(|state| state as &mut dyn ResourceLimiter);

        store
            .set_fuel(CONFIGURE_FUEL)
            .map_err(|e| HandlerLoadError::Init(format!("failed to set configure fuel: {e}")))?;
        store.set_epoch_deadline(EPOCH_DEADLINE);

        let bindings = invoke_handler::InvokeHandler::instantiate(&mut store, &component, &linker)
            .map_err(|e| HandlerLoadError::Init(format!("instantiation failed: {e}")))?;

        // Verify guest metadata matches the signed manifest.
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

        // Call configure with canonical model JSON.
        let model_bytes = serde_json::to_vec(model_json)
            .map_err(|e| HandlerLoadError::Init(format!("model serialization failed: {e}")))?;
        if model_bytes.len() > MAX_IO_BYTES {
            return Err(HandlerLoadError::Init("model JSON exceeds limit".into()));
        }
        let configure_result = bindings
            .call_configure(&mut store, &model_bytes)
            .map_err(|e| HandlerLoadError::Init(format!("configure trap: {e}")))?;
        if let Err(handler_error) = configure_result {
            return Err(HandlerLoadError::Init(format!(
                "configure rejected model: {handler_error:?}"
            )));
        }

        // Start epoch ticker thread.
        let stop_flag = Arc::new(AtomicBool::new(false));
        let engine_for_thread = engine.clone();
        let stop_for_thread = Arc::clone(&stop_flag);
        let epoch_thread = std::thread::spawn(move || {
            while !stop_for_thread.load(Ordering::Relaxed) {
                std::thread::sleep(EPOCH_TICK_INTERVAL);
                engine_for_thread.increment_epoch();
            }
        });

        Ok(Self {
            engine,
            stop_flag,
            epoch_thread: Some(epoch_thread),
            state: Mutex::new(WasmState { store, bindings }),
        })
    }

    /// Returns the engine reference (needed for external epoch control if any).
    #[allow(dead_code)]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

impl Drop for WasmInvokeHandler {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.epoch_thread.take() {
            let _ = thread.join();
        }
    }
}

impl std::fmt::Debug for WasmInvokeHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmInvokeHandler")
            .field(
                "epoch_thread_running",
                &!self.stop_flag.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl InvokeHandlerTrait for WasmInvokeHandler {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, String> {
        let input_bytes = serde_json::to_vec(input)
            .map_err(|e| format!("handlerInternalError: input serialization: {e}"))?;
        if input_bytes.len() > MAX_IO_BYTES {
            return Err("handlerInternalError: input exceeds limit".into());
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| "handlerInternalError: lock poisoned".to_string())?;

        state
            .store
            .set_fuel(INVOKE_FUEL)
            .map_err(|e| format!("handlerInternalError: failed to set invoke fuel: {e}"))?;
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
                    return "handlerTimeout".to_string();
                }
                // Avoid leaking the wasmtime Display (which may include a
                // WASM backtrace) to IPC clients; log it host-side instead.
                eprintln!("rill-runtime: handler trap: {e}");
                "handlerTrap: wasm trap occurred".to_string()
            })?;

        match result {
            Ok(output_bytes) => {
                if output_bytes.len() > MAX_IO_BYTES {
                    return Err("handlerOutputTooLarge".into());
                }
                serde_json::from_slice(&output_bytes)
                    .map_err(|e| format!("handlerInvalidOutput: {e}"))
            }
            Err(handler_error) => {
                let msg = format!("{handler_error:?}");
                Err(format!("handlerExecutionFailed: {msg}"))
            }
        }
    }
}

//! Wasmtime host adapter for the Preview Stateful Handler ABI v2.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use rill_handler_api::v2::{MAX_EVENT_BYTES, MAX_OUTPUT_BYTES, MAX_STATE_BYTES};
use serde_json::Value;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};

use crate::{
    StatefulHandlerErrorKindV2, StatefulHandlerErrorV2, StatefulHandlerMetadataV2,
    StatefulHandlerResultV2, StatefulHandlerV2,
};

mod bindings {
    wasmtime::component::bindgen!({
        path: "wit-v2/rill-handler.wit",
        world: "stateful-handler",
    });
}

pub const STATEFUL_CONFIGURE_FUEL: u64 = 10_000_000;
pub const STATEFUL_HANDLE_FUEL: u64 = 100_000_000;
pub const STATEFUL_EPOCH_TICK_INTERVAL: Duration = Duration::from_secs(1);
pub const STATEFUL_EPOCH_DEADLINE: u64 = 5;

struct HostStateV2;

impl ResourceLimiter for HostStateV2 {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(desired <= crate::MAX_MEMORY_BYTES)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(desired <= crate::MAX_TABLE_ELEMENTS as usize)
    }
}

struct WasmStateV2 {
    store: Store<HostStateV2>,
    bindings: bindings::StatefulHandler,
}

struct EpochTickerV2 {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl EpochTickerV2 {
    fn start(engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                std::thread::sleep(STATEFUL_EPOCH_TICK_INTERVAL);
                engine.increment_epoch();
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for EpochTickerV2 {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Sandboxed ABI v2 handler. The linker provides no WASI interfaces, so the
/// guest has no filesystem, network, environment, process, stdio, clock or
/// random imports.
pub struct WasmStatefulHandlerV2 {
    metadata: StatefulHandlerMetadataV2,
    _engine: Engine,
    _ticker: EpochTickerV2,
    state: Mutex<WasmStateV2>,
}

impl WasmStatefulHandlerV2 {
    /// Compile and configure a component whose bytes were already authenticated
    /// by the caller. Guest metadata must exactly match `expected_metadata`.
    pub fn new(
        component_bytes: &[u8],
        expected_metadata: StatefulHandlerMetadataV2,
        model_json: &Value,
    ) -> Result<Self, StatefulHandlerErrorV2> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.max_wasm_stack(1024 * 1024);
        let engine = Engine::new(&config).map_err(|error| {
            StatefulHandlerErrorV2::with_detail(
                StatefulHandlerErrorKindV2::Internal,
                error.to_string(),
            )
        })?;
        let ticker = EpochTickerV2::start(engine.clone());
        let component = Component::new(&engine, component_bytes).map_err(|error| {
            StatefulHandlerErrorV2::with_detail(
                StatefulHandlerErrorKindV2::InvalidModel,
                error.to_string(),
            )
        })?;
        // Deliberately empty: no WASI or other ambient-authority imports.
        let linker: Linker<HostStateV2> = Linker::new(&engine);
        let mut store = Store::new(&engine, HostStateV2);
        store.limiter(|state| state as &mut dyn ResourceLimiter);
        set_budget(&mut store, STATEFUL_CONFIGURE_FUEL)?;
        let bindings = bindings::StatefulHandler::instantiate(&mut store, &component, &linker)
            .map_err(map_load_trap)?;

        set_budget(&mut store, STATEFUL_CONFIGURE_FUEL)?;
        let guest = bindings.call_metadata(&mut store).map_err(map_load_trap)?;
        let actual = StatefulHandlerMetadataV2 {
            id: guest.id,
            version: guest.version,
            api_version: guest.api_version,
            capabilities: guest.capabilities,
            state_schema_version: guest.state_schema_version,
        };
        if actual != expected_metadata {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::MetadataMismatch,
            ));
        }

        let model_bytes = serde_json::to_vec(model_json).map_err(|error| {
            StatefulHandlerErrorV2::with_detail(
                StatefulHandlerErrorKindV2::InvalidModel,
                error.to_string(),
            )
        })?;
        if model_bytes.len() > MAX_EVENT_BYTES {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidModel,
            ));
        }
        set_budget(&mut store, STATEFUL_CONFIGURE_FUEL)?;
        let configured = bindings
            .call_configure(&mut store, &model_bytes)
            .map_err(map_load_trap)?;
        if let Err(error) = configured {
            return Err(map_guest_error(error));
        }

        Ok(Self {
            metadata: actual,
            _engine: engine,
            _ticker: ticker,
            state: Mutex::new(WasmStateV2 { store, bindings }),
        })
    }
}

impl std::fmt::Debug for WasmStatefulHandlerV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmStatefulHandlerV2")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl StatefulHandlerV2 for WasmStatefulHandlerV2 {
    fn metadata(&self) -> &StatefulHandlerMetadataV2 {
        &self.metadata
    }

    fn handle(
        &self,
        event_json: &[u8],
        current_state: &[u8],
        deterministic_seed: Option<u64>,
    ) -> Result<StatefulHandlerResultV2, StatefulHandlerErrorV2> {
        if event_json.len() > MAX_EVENT_BYTES || current_state.len() > MAX_STATE_BYTES {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidEvent,
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        set_budget(&mut state.store, STATEFUL_HANDLE_FUEL)?;
        let WasmStateV2 { store, bindings } = &mut *state;
        let result = bindings
            .call_handle(store, event_json, current_state, deterministic_seed)
            .map_err(map_call_trap)?;
        let result = result.map_err(map_guest_error)?;
        if result.output_json.len() > MAX_OUTPUT_BYTES {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::OutputTooLarge,
            ));
        }
        if result.next_state.len() > MAX_STATE_BYTES {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidState,
            ));
        }
        let output = serde_json::from_slice(&result.output_json).map_err(|error| {
            StatefulHandlerErrorV2::with_detail(
                StatefulHandlerErrorKindV2::InvalidOutput,
                error.to_string(),
            )
        })?;
        Ok(StatefulHandlerResultV2 {
            output,
            next_state: result.next_state,
        })
    }
}

fn set_budget(store: &mut Store<HostStateV2>, fuel: u64) -> Result<(), StatefulHandlerErrorV2> {
    store.set_fuel(fuel).map_err(|error| {
        StatefulHandlerErrorV2::with_detail(StatefulHandlerErrorKindV2::Internal, error.to_string())
    })?;
    store.set_epoch_deadline(STATEFUL_EPOCH_DEADLINE);
    Ok(())
}

fn map_load_trap(error: wasmtime::Error) -> StatefulHandlerErrorV2 {
    if let Some(trap) = error.downcast_ref::<Trap>()
        && matches!(trap, Trap::OutOfFuel | Trap::Interrupt)
    {
        return StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Timeout);
    }
    StatefulHandlerErrorV2::with_detail(StatefulHandlerErrorKindV2::Trap, error.to_string())
}

fn map_call_trap(error: wasmtime::Error) -> StatefulHandlerErrorV2 {
    map_load_trap(error)
}

fn map_guest_error(error: bindings::HandlerErrorV2) -> StatefulHandlerErrorV2 {
    let (kind, detail) = match error {
        bindings::HandlerErrorV2::InvalidModel(detail) => {
            (StatefulHandlerErrorKindV2::InvalidModel, detail)
        }
        bindings::HandlerErrorV2::InvalidEvent(detail) => {
            (StatefulHandlerErrorKindV2::InvalidEvent, detail)
        }
        bindings::HandlerErrorV2::InvalidState(detail) => {
            (StatefulHandlerErrorKindV2::InvalidState, detail)
        }
        bindings::HandlerErrorV2::IncompatibleVersion(detail) => {
            (StatefulHandlerErrorKindV2::IncompatibleVersion, detail)
        }
        bindings::HandlerErrorV2::ExecutionFailed(detail) => {
            (StatefulHandlerErrorKindV2::Internal, detail)
        }
    };
    StatefulHandlerErrorV2::with_detail(kind, detail)
}

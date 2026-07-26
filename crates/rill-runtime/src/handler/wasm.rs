//! Sandboxed WASM handler adapter.
//!
//! Loads a signed `.rillhandler` module, instantiates it inside a Wasmtime
//! component sandbox with strict resource limits, and adapts it to the
//! [`InvokeHandler`](crate::server::InvokeHandler) trait.
//!
//! ## Sandbox guarantees
//!
//! - No WASI imports (no filesystem, network, environment, stdio, process).
//! - Fuel budget per `configure`/`invoke` call.
//! - Epoch interruption for wall-clock timeout.
//! - Memory and table growth capped by `HostState` (implements [`ResourceLimiter`]).
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

// Test-only counter of live epoch-ticker threads. Incremented at thread
// entry and decremented before thread exit, so a non-zero value means a
// ticker thread is still running. Used by the internal ticker-lifecycle
// unit tests in `mod tests` below to directly observe that failed handler
// loads and handler drops join the background thread.
//
// This counter, the matching `fetch_add`/`fetch_sub` ops in
// `EpochTicker::start`, and the `active_epoch_ticker_count` accessor in
// `mod tests` are all `#[cfg(test)]`-only: they do not exist in release
// builds, in CI builds that link the library as a dependency (integration
// tests), or in the published crate. The production ticker hot path
// performs zero atomic operations for instrumentation. The previous
// design exposed a `#[doc(hidden)] pub fn active_epoch_ticker_count()`
// so that integration tests (a separate crate) could reach the counter;
// that leaked a test probe into the public API. The current design moves
// the lifecycle tests into this module's internal `#[cfg(test)] mod
// tests`, which can reach private items directly through `super::*`.
#[cfg(test)]
static ACTIVE_EPOCH_TICKERS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

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
            // Increment the test-only active-ticker counter at thread entry
            // so the count reflects threads that have actually started. The
            // matching decrement runs just before the thread exits (after
            // the stop flag is observed), so a non-zero count means a
            // ticker is still running. `Drop` joins the handle, which
            // guarantees the decrement has happened by the time drop
            // returns.
            //
            // The counter and these atomic ops are `#[cfg(test)]`-gated, so
            // the production ticker hot path performs zero atomic
            // operations for instrumentation. The whole counter does not
            // exist in release builds, in CI builds that link the library
            // as a dependency (integration tests), or in the published
            // crate. The internal `mod tests` below reaches the counter
            // directly through `super::*`, so no `pub` accessor is needed.
            #[cfg(test)]
            ACTIVE_EPOCH_TICKERS.fetch_add(1, Ordering::SeqCst);
            while !stop_for_thread.load(Ordering::Relaxed) {
                std::thread::sleep(EPOCH_TICK_INTERVAL);
                engine_for_thread.increment_epoch();
            }
            #[cfg(test)]
            ACTIVE_EPOCH_TICKERS.fetch_sub(1, Ordering::SeqCst);
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

/// Sandboxed WASM handler that implements [`crate::server::InvokeHandler`].
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
            // Map each WIT variant to extract the guest-supplied detail
            // string. The variant name is included in the load error for
            // host-side diagnostics; the guest detail is truncated by
            // the caller's formatting. This avoids leaking the Rust type
            // name (`HandlerError::VariantName`) that the previous
            // `{handler_error:?}` Debug format exposed.
            let (variant, detail) = match handler_error {
                invoke_handler::HandlerError::InvalidModel(s) => ("invalid-model", s),
                invoke_handler::HandlerError::InvalidInput(s) => ("invalid-input", s),
                invoke_handler::HandlerError::UnsupportedCapability(s) => {
                    ("unsupported-capability", s)
                }
                invoke_handler::HandlerError::ExecutionFailed(s) => ("execution-failed", s),
            };
            return Err(HandlerLoadError::Init(format!(
                "configure rejected model ({variant}): {detail}"
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
                // The wasmtime Display string may include a full WASM
                // backtrace (guest-controlled). Construct `InvokeError`
                // first — `with_detail` truncates to `MAX_DETAIL_BYTES` —
                // and do NOT log here. The `RuntimeEngine` layer logs the
                // already-truncated detail exactly once (see audit 5.2).
                InvokeError::with_detail(InvokeErrorKind::Trap, format!("{e}"))
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
                // Guest reported a typed `handler-error` variant. Map
                // each WIT variant to the corresponding `InvokeErrorKind`
                // and extract the inner detail string (which is fully
                // guest-controlled). `InvokeError::with_detail` truncates
                // the detail to `MAX_DETAIL_BYTES` on a UTF-8 char
                // boundary. The adapter does NOT log the error — the
                // `RuntimeEngine` layer logs the already-truncated detail
                // exactly once (see audit 5.1 + 5.2).
                let (kind, detail) = match handler_error {
                    invoke_handler::HandlerError::InvalidModel(s) => {
                        (InvokeErrorKind::InvalidModel, s)
                    }
                    invoke_handler::HandlerError::InvalidInput(s) => {
                        (InvokeErrorKind::InvalidInput, s)
                    }
                    invoke_handler::HandlerError::UnsupportedCapability(s) => {
                        (InvokeErrorKind::UnsupportedCapability, s)
                    }
                    invoke_handler::HandlerError::ExecutionFailed(s) => {
                        (InvokeErrorKind::ExecutionFailed, s)
                    }
                };
                Err(InvokeError::with_detail(kind, detail))
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

    // -----------------------------------------------------------------
    // Ticker lifecycle observability (audit 6.5 / fourth-stage 4-A-01
    // / fifth-stage 5-A-01..5-A-03).
    //
    // The `EpochTicker` RAII guard starts a background thread that
    // periodically calls `engine.increment_epoch()`. The tests below use
    // the test-only `ACTIVE_EPOCH_TICKERS` counter (and the private
    // `active_epoch_ticker_count` accessor defined in this module) to
    // directly observe that the thread is started on handler
    // construction and joined on handler drop or load failure.
    //
    // These tests were previously in `tests/wasm_handler.rs` and reached
    // the counter through a `#[doc(hidden)] pub` accessor, which leaked
    // a test probe into the public API. They have been moved here so the
    // counter and accessor can both be `#[cfg(test)]`-private.
    //
    // Fifth-stage strengthening:
    // - The metadata-loop test now spawns the constructor in a worker
    //   thread so the main thread can observe `baseline + 1` *during*
    //   construction (previously the synchronous flow could pass even if
    //   the ticker never started, because by the time `new()` returned
    //   `Err` the constructor had already joined the thread).
    // - Fixture gating now respects `RILL_RUN_WASM_FIXTURE_TESTS=1`:
    //   the dedicated `wasm-handler` CI job sets this env var so a
    //   regression in fixture production surfaces as a hard CI failure
    //   rather than a silently-skipped test. The regular workspace
    //   `cargo test` job (which does not build the fixtures) still
    //   skips gracefully.
    // - The previously-named `ticker_probe_is_not_in_public_api` test
    //   was renamed to `ticker_probe_is_available_to_internal_tests`
    //   because it does *not* actually assert anything about the public
    //   API. The real public-API leak check is a `cargo doc` + `grep`
    //   step in the dedicated CI job.
    // -----------------------------------------------------------------

    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use ed25519_dalek::{SigningKey, VerifyingKey};
    use rill_runtime_protocol::{
        HANDLER_API_VERSION, HANDLER_PACKAGE_FORMAT_VERSION, HandlerPackManifest,
    };
    use sha2::{Digest, Sha256};

    /// Serialises lifecycle tests in this module so their assertions
    /// about the global `ACTIVE_EPOCH_TICKERS` counter are not perturbed
    /// by parallel ticker creation/drop. The guard is recovered from
    /// poison so a panic in one test does not cascade.
    static LIFECYCLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Test-only accessor for the count of currently-running epoch-ticker
    /// threads. Private to this module — does not exist in non-test
    /// builds, cannot be reached from external crates or integration
    /// tests.
    fn active_epoch_ticker_count() -> usize {
        ACTIVE_EPOCH_TICKERS.load(Ordering::SeqCst)
    }

    /// Acquires the lifecycle test serialisation lock.
    fn lifecycle_guard() -> std::sync::MutexGuard<'static, ()> {
        LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Polls `active_epoch_ticker_count` until it reaches `target` or
    /// `timeout` elapses. Uses 10 ms polling to avoid flaky fixed sleeps
    /// while bounding wait time.
    fn wait_for_active_ticker_count(target: usize, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            if active_epoch_ticker_count() == target {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Resolves a WASM fixture path for the lifecycle tests.
    ///
    /// Resolution order:
    /// 1. The `env_name` environment variable (e.g. `ECHO_HANDLER_WASM`) if
    ///    set. The path must point to an existing file — a missing file
    ///    panics so a misconfigured CI step cannot silently skip the test.
    /// 2. The workspace-relative fallback under `target/`.
    /// 3. If neither exists and `RILL_RUN_WASM_FIXTURE_TESTS` is **not**
    ///    set, returns `None` so the regular workspace `cargo test` job
    ///    (which does not build the WASM fixtures) can skip the test
    ///    without failing.
    /// 4. If neither exists and `RILL_RUN_WASM_FIXTURE_TESTS=1` is set,
    ///    panics — the dedicated CI job must fail loudly instead of
    ///    silently reporting green.
    ///
    /// The dedicated `wasm-handler` CI job sets
    /// `RILL_RUN_WASM_FIXTURE_TESTS=1` after building the fixtures so
    /// that a regression in fixture production surfaces as a hard CI
    /// failure rather than a skipped test.
    fn fixture_path(env_name: &str, fallback_relative: &str) -> Option<PathBuf> {
        if let Ok(value) = std::env::var(env_name) {
            let path = PathBuf::from(value);
            assert!(
                path.is_file(),
                "{env_name} points to missing fixture: {}",
                path.display()
            );
            return Some(path);
        }

        let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fallback_relative);
        if fallback.is_file() {
            return Some(fallback);
        }

        if std::env::var_os("RILL_RUN_WASM_FIXTURE_TESTS").is_some() {
            panic!(
                "{env_name} is not set and fallback fixture does not exist: {} \
                 (RILL_RUN_WASM_FIXTURE_TESTS=1 is set, so missing fixtures must fail)",
                fallback.display()
            );
        }

        None
    }

    /// Returns the echo handler WASM component path, or `None` if not
    /// available and fixture tests are not mandatory. Mirrors
    /// `tests/wasm_handler.rs::echo_handler_component`.
    fn echo_handler_component() -> Option<PathBuf> {
        fixture_path("ECHO_HANDLER_WASM", "../../target/echo-handler.wasm")
    }

    /// Returns the metadata-loop handler WASM component path, or `None`.
    fn metadata_loop_handler_component() -> Option<PathBuf> {
        fixture_path(
            "METADATA_LOOP_HANDLER_WASM",
            "../../target/test-metadata-loop-handler.wasm",
        )
    }

    /// Builds a signed `.rillhandler` pack from the echo handler.
    fn build_echo_pack(module: &[u8], signing: &SigningKey) -> Vec<u8> {
        let manifest = HandlerPackManifest {
            format_version: HANDLER_PACKAGE_FORMAT_VERSION,
            id: "rillml.echo.handler".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            handler_api_version: HANDLER_API_VERSION,
            min_runtime_version: env!("CARGO_PKG_VERSION").into(),
            publisher_key_id: "wasm-test-key".into(),
            capabilities: vec!["rillml.linearRegression.predict".into()],
            module_sha256: hex::encode(Sha256::digest(module)),
            module_size: module.len() as u64,
        };
        crate::build_signed_handler_pack(&manifest, module, signing).unwrap()
    }

    /// Builds a signed `.rillhandler` pack from the metadata-loop
    /// handler. The manifest id matches the guest's `metadata()` return
    /// value — but since `metadata()` loops forever, the host never
    /// reaches the mismatch check.
    fn build_metadata_loop_pack(module: &[u8], signing: &SigningKey) -> Vec<u8> {
        let manifest = HandlerPackManifest {
            format_version: HANDLER_PACKAGE_FORMAT_VERSION,
            id: "rillml.test.metadata-loop".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            handler_api_version: HANDLER_API_VERSION,
            min_runtime_version: env!("CARGO_PKG_VERSION").into(),
            publisher_key_id: "wasm-test-key".into(),
            capabilities: vec!["rillml.linearRegression.predict".into()],
            module_sha256: hex::encode(Sha256::digest(module)),
            module_size: module.len() as u64,
        };
        crate::build_signed_handler_pack(&manifest, module, signing).unwrap()
    }

    /// Loads a signed pack, returning the `LoadedHandlerPack`.
    fn load_pack(pack_bytes: &[u8], verifying: &VerifyingKey) -> crate::LoadedHandlerPack {
        let trust = crate::TrustStore(BTreeMap::from([("wasm-test-key".into(), *verifying)]));
        let (loaded, _) =
            crate::load_handler_pack(std::io::Cursor::new(pack_bytes), &trust).unwrap();
        loaded
    }

    /// Verifies that constructing a normal echo handler increments the
    /// active ticker count, and dropping it restores the count. This
    /// directly proves the ticker thread is started on construction and
    /// joined on drop.
    #[test]
    fn normal_handler_drop_restores_active_ticker_count() {
        let _guard = lifecycle_guard();

        let component = match echo_handler_component() {
            Some(path) => fs::read(&path).unwrap(),
            None => {
                eprintln!(
                    "skipping: echo handler component not built \
                     (set ECHO_HANDLER_WASM or RILL_RUN_WASM_FIXTURE_TESTS=1)"
                );
                return;
            }
        };

        let signing = SigningKey::from_bytes(&[7; 32]);
        let pack_bytes = build_echo_pack(&component, &signing);
        let loaded = load_pack(&pack_bytes, &signing.verifying_key());

        let baseline = active_epoch_ticker_count();

        let model =
            serde_json::json!({"kind": "linearRegression", "weights": [0.5], "intercept": 0.0});
        let handler =
            WasmInvokeHandler::new(&loaded, &model).expect("echo handler must load successfully");

        // The ticker thread increments the count at entry. Wait for the
        // increment to be visible.
        assert!(
            wait_for_active_ticker_count(baseline + 1, std::time::Duration::from_secs(3)),
            "active ticker count did not reach {} after handler construction (got {}, baseline {})",
            baseline + 1,
            active_epoch_ticker_count(),
            baseline
        );

        // Dropping the handler must join the ticker thread and restore
        // the count to baseline.
        drop(handler);

        assert!(
            wait_for_active_ticker_count(baseline, std::time::Duration::from_secs(3)),
            "active ticker count did not return to baseline {} after handler drop (got {})",
            baseline,
            active_epoch_ticker_count()
        );
    }

    /// Verifies that a failed metadata-loop handler load does not leak
    /// the epoch-ticker thread — and crucially, that the ticker was
    /// actually *started* during construction (not just absent
    /// throughout). The previous synchronous test could pass even if the
    /// ticker never started, because by the time `new()` returned `Err`
    /// the constructor had already joined the ticker thread, leaving the
    /// counter at baseline.
    ///
    /// The strengthened flow:
    /// 1. Record `baseline` ticker count.
    /// 2. Spawn a worker thread that calls `WasmInvokeHandler::new`.
    /// 3. From the main test thread, observe `count == baseline + 1`
    ///    while the constructor is still blocked inside `metadata()`
    ///    (which loops forever and is interrupted by the epoch deadline).
    /// 4. Wait for the worker to return — the result must be `Err`
    ///    because `metadata()` cannot complete within the epoch budget.
    /// 5. After the worker returns, observe `count == baseline`,
    ///    proving the `EpochTicker` guard was dropped during error
    ///    propagation and joined its background thread.
    ///
    /// `LoadedHandlerPack` is `Send + Sync` (its fields are
    /// `HandlerPackManifest` of plain scalars/strings and a `Vec<u8>`),
    /// so the worker thread takes an `Arc<LoadedHandlerPack>` rather
    /// than moving the owned value. No production type needs to grow
    /// `Send`/`Sync` bounds for this test.
    #[test]
    fn metadata_loop_failure_restores_active_ticker_count() {
        let _guard = lifecycle_guard();

        let component = match metadata_loop_handler_component() {
            Some(path) => fs::read(&path).unwrap(),
            None => {
                eprintln!(
                    "skipping: metadata-loop handler component not built \
                     (set METADATA_LOOP_HANDLER_WASM or RILL_RUN_WASM_FIXTURE_TESTS=1)"
                );
                return;
            }
        };

        let signing = SigningKey::from_bytes(&[9; 32]);
        let pack_bytes = build_metadata_loop_pack(&component, &signing);
        let loaded = Arc::new(load_pack(&pack_bytes, &signing.verifying_key()));

        let baseline = active_epoch_ticker_count();

        // Spawn the constructor in a worker thread so the main thread
        // can observe the active-ticker counter while the constructor
        // is still blocked inside the metadata-loop guest. The thread
        // takes an `Arc<LoadedHandlerPack>` so the pack is shared, not
        // moved; no production type needs to grow `Send`/`Sync` for
        // this test.
        let worker_loaded = Arc::clone(&loaded);
        let worker = std::thread::spawn(move || {
            WasmInvokeHandler::new(&worker_loaded, &serde_json::json!({}))
        });

        // While the worker is blocked inside metadata() (which loops
        // forever), the EpochTicker thread must have started and
        // incremented the counter. Wait for it to reach baseline + 1
        // — this directly proves the ticker was started, not just
        // absent throughout.
        assert!(
            wait_for_active_ticker_count(baseline + 1, std::time::Duration::from_secs(10)),
            "metadata-loop constructor never started an epoch ticker \
             (count stayed at {}, expected {} during construction)",
            active_epoch_ticker_count(),
            baseline + 1
        );

        // The metadata-loop handler's `metadata()` loops forever; the
        // host's epoch deadline interrupts it, and `WasmInvokeHandler::new`
        // returns Err. The `EpochTicker` guard is dropped during error
        // propagation, which joins the thread.
        let result = worker.join().expect(
            "metadata-loop constructor thread panicked; \
             this may indicate a bug in the ticker thread startup or the \
             epoch interruption path",
        );
        assert!(result.is_err(), "metadata-loop handler must fail to load");

        // After the failed load, the ticker thread must have been joined
        // and the count must return to baseline.
        assert!(
            wait_for_active_ticker_count(baseline, std::time::Duration::from_secs(3)),
            "active ticker count did not return to baseline {} after metadata-loop failure (got {})",
            baseline,
            active_epoch_ticker_count()
        );

        // Drop the Arc<LoadedHandlerPack> explicitly so its refcount
        // goes to zero and any test-only state is released before the
        // next test runs.
        drop(loaded);
    }

    /// Source-level invariant: the test-only `ACTIVE_EPOCH_TICKERS`
    /// static and its private `active_epoch_ticker_count` accessor are
    /// reachable from this internal `#[cfg(test)] mod tests` (via
    /// `use super::*`), so the lifecycle tests above can observe ticker
    /// thread start/stop directly. This test confirms the counter is
    /// *available to internal tests* — it is **not** a public-API
    /// assertion. The actual public-API leak check (probe absent from
    /// `cargo doc --features wasm --no-deps` output) is performed as a
    /// separate `grep` step in the dedicated `wasm-handler` CI job,
    /// which fails the build if `active_epoch_ticker_count` or
    /// `ACTIVE_EPOCH_TICKERS` appears in `target/doc/rill_runtime/`.
    ///
    /// This test was previously named
    /// `ticker_probe_is_not_in_public_api`, which was misleading
    /// because a private `fn` reachable from `super::*` says nothing
    /// about whether a future refactor might re-expose it as `pub`.
    /// The renamed test now accurately describes what it checks.
    #[test]
    fn ticker_probe_is_available_to_internal_tests() {
        let _guard = lifecycle_guard();
        let baseline = active_epoch_ticker_count();
        // The static itself is reachable from `super::*` (via the
        // `use super::*;` at the top of `mod tests`). If the static or
        // the accessor were `pub`, `cargo doc --features wasm --no-deps`
        // would surface them in the public API; the dedicated CI
        // `grep -R "active_epoch_ticker_count|ACTIVE_EPOCH_TICKERS"
        // target/doc/rill_runtime` step must return no matches.
        let _ = ACTIVE_EPOCH_TICKERS.load(Ordering::SeqCst);
        assert_eq!(active_epoch_ticker_count(), baseline);
    }
}

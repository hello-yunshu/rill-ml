//! Preview stateful Handler ABI v2 and IPC V3 runtime integration.
//!
//! The host owns persistence and only commits a handler's proposed next state
//! after all bounds, JSON, schema-version and checksum checks succeed.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use rill_handler_api::v2::{
    HANDLER_API_VERSION, MAX_EVENT_BYTES, MAX_OUTPUT_BYTES, MAX_STATE_BYTES,
};
use rill_runtime_protocol::v3::{
    EnvelopeV3, IdentityV3, PREVIEW_CHANNEL_V3, PreviewErrorCodeV3, RUNTIME_API_VERSION_V3,
    ResourceProfileV1, RuntimeErrorCodeV3, RuntimeErrorV3, RuntimeErrorV3Preview, RuntimeRequestV3,
    RuntimeResponseBodyV3, RuntimeResponseBodyV3Preview, RuntimeResponseV3,
    RuntimeResponseV3Preview,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_HANDLER_DETAIL_BYTES_V2: usize = 4 * 1024;

/// Metadata declared by a Preview ABI v2 handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatefulHandlerMetadataV2 {
    pub id: String,
    pub version: String,
    pub api_version: u32,
    pub capabilities: Vec<String>,
    pub state_schema_version: u32,
}

impl StatefulHandlerMetadataV2 {
    fn validate(&self) -> Result<(), StatefulHandlerErrorV2> {
        if self.id.is_empty() || self.id.len() > rill_handler_api::MAX_HANDLER_ID_LEN {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::MetadataMismatch,
            ));
        }
        if self.version.is_empty()
            || self.version.len() > rill_handler_api::MAX_HANDLER_VERSION_LEN
            || self.api_version != HANDLER_API_VERSION
            || self.state_schema_version == 0
        {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::MetadataMismatch,
            ));
        }
        if self.capabilities.is_empty()
            || self.capabilities.len() > rill_handler_api::MAX_CAPABILITIES
        {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::MetadataMismatch,
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        if self.capabilities.iter().any(|capability| {
            capability.is_empty()
                || capability.len() > rill_handler_api::MAX_CAPABILITY_LEN
                || !seen.insert(capability)
        }) {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::MetadataMismatch,
            ));
        }
        Ok(())
    }
}

/// Successful handler result. `next_state` is a proposal; the Runtime still
/// validates it before making it current.
#[derive(Debug, Clone, PartialEq)]
pub struct StatefulHandlerResultV2 {
    pub output: serde_json::Value,
    pub next_state: Vec<u8>,
}

/// Stateful handler failure categories. All failures are fail-closed and
/// leave the Runtime-owned state unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StatefulHandlerErrorKindV2 {
    InvalidModel,
    InvalidEvent,
    InvalidState,
    IncompatibleVersion,
    DuplicateFeedback,
    Timeout,
    Trap,
    OutputTooLarge,
    InvalidOutput,
    MetadataMismatch,
    Internal,
}

/// Typed handler error with bounded host-only detail.
#[derive(Debug, Clone)]
pub struct StatefulHandlerErrorV2 {
    kind: StatefulHandlerErrorKindV2,
    detail: Option<String>,
}

impl StatefulHandlerErrorV2 {
    pub const fn new(kind: StatefulHandlerErrorKindV2) -> Self {
        Self { kind, detail: None }
    }

    pub fn with_detail(kind: StatefulHandlerErrorKindV2, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        if detail.len() > MAX_HANDLER_DETAIL_BYTES_V2 {
            let mut end = MAX_HANDLER_DETAIL_BYTES_V2;
            while end > 0 && !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
        }
        Self {
            kind,
            detail: Some(detail),
        }
    }

    pub const fn kind(&self) -> StatefulHandlerErrorKindV2 {
        self.kind
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

impl std::fmt::Display for StatefulHandlerErrorV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stateful handler {:?}", self.kind)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

impl std::error::Error for StatefulHandlerErrorV2 {}

/// Host abstraction implemented by sandboxed ABI v2 handlers and test
/// doubles. It grants no filesystem, network, process, time or randomness;
/// deterministic randomness is supplied only through `deterministic_seed`.
pub trait StatefulHandlerV2: Send + Sync + std::fmt::Debug {
    fn metadata(&self) -> &StatefulHandlerMetadataV2;

    fn handle(
        &self,
        event_json: &[u8],
        current_state: &[u8],
        deterministic_seed: Option<u64>,
    ) -> Result<StatefulHandlerResultV2, StatefulHandlerErrorV2>;
}

/// Serializable Runtime-owned state snapshot. Restores verify all fields,
/// state size, JSON validity and checksum before activation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatefulStateSnapshotV2 {
    pub state_schema_version: u32,
    pub state_generation: u64,
    pub state: Vec<u8>,
    pub checksum_sha256: String,
}

/// One decision retained across process restart for delayed feedback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionLedgerEntryV3 {
    pub decision_id: String,
    pub model_generation: u64,
    pub state_generation: u64,
    pub created_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_time_unix_ms: Option<u64>,
}

/// Machine-readable health state exposed by the production qualification
/// surface. A failed-closed runtime never reports `Healthy`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealthStatusV1 {
    Healthy,
    ResourcePressure,
    FailedClosed,
}

impl std::fmt::Display for RuntimeHealthStatusV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Healthy => "healthy",
            Self::ResourcePressure => "resource_pressure",
            Self::FailedClosed => "failed_closed",
        })
    }
}

/// Bounded resource counters included in operational diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeResourceUsageV1 {
    pub state_bytes: usize,
    pub snapshot_bytes: usize,
    pub pending_decisions: usize,
    pub completed_decisions: usize,
}

/// Structured, clock-injected runtime diagnostics for consumers and release
/// qualification. The runtime does not read a clock implicitly; callers pass
/// the observation time to `diagnostics_at`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeDiagnosticsV1 {
    pub runtime_version: String,
    pub protocol_version: u32,
    pub channel: String,
    pub model_generation: u64,
    pub state_generation: u64,
    pub state_schema_version: u32,
    pub observed_at_unix_ms: u64,
    pub health: RuntimeHealthStatusV1,
    pub reason_codes: Vec<String>,
    pub resource_usage: RuntimeResourceUsageV1,
    pub rollback_available: bool,
    pub candidate_available: bool,
    pub restart_count: u64,
    pub last_error: Option<String>,
}

/// Durable v3 runtime envelope. The handler snapshot and decision ledger are
/// checksummed together so feedback cannot silently cross a restore boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatefulRuntimeSnapshotV3 {
    pub format_version: u32,
    pub handler_snapshot: StatefulStateSnapshotV2,
    pub pending_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    pub completed_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    pub checksum_sha256: String,
}

impl StatefulRuntimeSnapshotV3 {
    pub const FORMAT_VERSION: u32 = 1;

    fn checksum_input(
        handler_snapshot: &StatefulStateSnapshotV2,
        pending_decisions: &BTreeMap<String, DecisionLedgerEntryV3>,
        completed_decisions: &BTreeMap<String, DecisionLedgerEntryV3>,
    ) -> Vec<u8> {
        serde_json::to_vec(&(handler_snapshot, pending_decisions, completed_decisions))
            .expect("runtime snapshot fields are serializable")
    }

    fn new(
        handler_snapshot: StatefulStateSnapshotV2,
        pending_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
        completed_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    ) -> Self {
        let checksum_sha256 = state_checksum(&Self::checksum_input(
            &handler_snapshot,
            &pending_decisions,
            &completed_decisions,
        ));
        Self {
            format_version: Self::FORMAT_VERSION,
            handler_snapshot,
            pending_decisions,
            completed_decisions,
            checksum_sha256,
        }
    }

    fn validate(&self, expected_schema_version: u32) -> Result<(), StatefulHandlerErrorV2> {
        if self.format_version != Self::FORMAT_VERSION {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::IncompatibleVersion,
            ));
        }
        self.handler_snapshot.validate(expected_schema_version)?;
        if self.checksum_sha256
            != state_checksum(&Self::checksum_input(
                &self.handler_snapshot,
                &self.pending_decisions,
                &self.completed_decisions,
            ))
        {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidState,
            ));
        }
        for (key, entry) in self
            .pending_decisions
            .iter()
            .chain(self.completed_decisions.iter())
        {
            if key != &entry.decision_id || entry.decision_id.is_empty() {
                return Err(StatefulHandlerErrorV2::new(
                    StatefulHandlerErrorKindV2::InvalidState,
                ));
            }
        }
        Ok(())
    }
}

/// Host-provided migration hook. Migrations return a new state and never
/// mutate the input, allowing the caller to preserve the previous-good copy.
pub trait StatefulStateMigratorV1: Send + Sync {
    fn migrate(
        &self,
        from_schema_version: u32,
        state: &[u8],
    ) -> Result<(u32, Vec<u8>), StatefulHandlerErrorV2>;
}

impl StatefulStateSnapshotV2 {
    pub fn new(state_schema_version: u32, state_generation: u64, state: Vec<u8>) -> Self {
        let checksum_sha256 = state_checksum(&state);
        Self {
            state_schema_version,
            state_generation,
            state,
            checksum_sha256,
        }
    }

    pub fn validate(&self, expected_schema_version: u32) -> Result<(), StatefulHandlerErrorV2> {
        validate_state_bytes(
            &self.state,
            self.state_schema_version,
            expected_schema_version,
        )?;
        if self.checksum_sha256 != state_checksum(&self.state) {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidState,
            ));
        }
        Ok(())
    }
}

/// Construction contract for the V3 runtime.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StatefulRuntimeConfigV3 {
    pub runtime_identity: IdentityV3,
    pub model_generation: u64,
    pub initial_state_generation: u64,
    pub feature_schema_hash: String,
    pub capabilities: Vec<String>,
    pub initial_state: Vec<u8>,
    pub resource_profile: ResourceProfileV1,
}

impl StatefulRuntimeConfigV3 {
    pub fn new(
        runtime_identity: IdentityV3,
        model_generation: u64,
        feature_schema_hash: String,
        capabilities: Vec<String>,
        initial_state: Vec<u8>,
    ) -> Self {
        Self {
            runtime_identity,
            model_generation,
            initial_state_generation: 0,
            feature_schema_hash,
            capabilities,
            initial_state,
            resource_profile: ResourceProfileV1::default(),
        }
    }

    pub fn with_resource_profile(mut self, resource_profile: ResourceProfileV1) -> Self {
        self.resource_profile = resource_profile;
        self
    }
}

#[derive(Debug, Clone)]
struct RuntimeStateV3 {
    snapshot: StatefulStateSnapshotV2,
    previous_good: Option<StatefulStateSnapshotV2>,
    candidate: Option<StatefulStateSnapshotV2>,
    pending_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    completed_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    restart_count: u64,
    last_error: Option<String>,
}

/// Preview Runtime V3 engine for Stateful Handler ABI v2.
#[derive(Debug)]
pub struct StatefulRuntimeEngineV3 {
    config: StatefulRuntimeConfigV3,
    metadata: StatefulHandlerMetadataV2,
    handler: Arc<dyn StatefulHandlerV2>,
    state: Mutex<RuntimeStateV3>,
}

impl StatefulRuntimeEngineV3 {
    pub fn new(
        config: StatefulRuntimeConfigV3,
        handler: Arc<dyn StatefulHandlerV2>,
    ) -> Result<Self, StatefulHandlerErrorV2> {
        let metadata = handler.metadata().clone();
        metadata.validate()?;
        config.runtime_identity.validate().map_err(|error| {
            StatefulHandlerErrorV2::with_detail(
                StatefulHandlerErrorKindV2::InvalidModel,
                error.to_string(),
            )
        })?;
        validate_feature_schema_hash(&config.feature_schema_hash)?;
        validate_capabilities(&config.capabilities)?;
        config.resource_profile.validate().map_err(|detail| {
            StatefulHandlerErrorV2::with_detail(StatefulHandlerErrorKindV2::InvalidModel, detail)
        })?;
        if config.capabilities != metadata.capabilities {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::MetadataMismatch,
            ));
        }
        validate_state_bytes(
            &config.initial_state,
            metadata.state_schema_version,
            metadata.state_schema_version,
        )?;
        let snapshot = StatefulStateSnapshotV2::new(
            metadata.state_schema_version,
            config.initial_state_generation,
            config.initial_state.clone(),
        );
        if snapshot.state.len() > config.resource_profile.max_model_state_bytes as usize {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidState,
            ));
        }
        Ok(Self {
            config,
            metadata,
            handler,
            state: Mutex::new(RuntimeStateV3 {
                snapshot,
                previous_good: None,
                candidate: None,
                pending_decisions: BTreeMap::new(),
                completed_decisions: BTreeMap::new(),
                restart_count: 0,
                last_error: None,
            }),
        })
    }

    /// Restore a previously validated Runtime-owned state atomically.
    pub fn restore_snapshot(
        &self,
        snapshot: StatefulStateSnapshotV2,
    ) -> Result<(), StatefulHandlerErrorV2> {
        snapshot.validate(self.metadata.state_schema_version)?;
        if snapshot.state.len() > self.config.resource_profile.max_model_state_bytes as usize {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidState,
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        state.previous_good = Some(state.snapshot.clone());
        state.snapshot = snapshot;
        state.restart_count = state.restart_count.saturating_add(1);
        Ok(())
    }

    pub fn snapshot(&self) -> Result<StatefulStateSnapshotV2, StatefulHandlerErrorV2> {
        self.state
            .lock()
            .map(|state| state.snapshot.clone())
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))
    }

    /// Return the handler state plus the durable decision ledger.
    pub fn runtime_snapshot(&self) -> Result<StatefulRuntimeSnapshotV3, StatefulHandlerErrorV2> {
        let state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        let snapshot = StatefulRuntimeSnapshotV3::new(
            state.snapshot.clone(),
            state.pending_decisions.clone(),
            state.completed_decisions.clone(),
        );
        self.validate_runtime_snapshot_size(&snapshot)?;
        Ok(snapshot)
    }

    /// Restore handler state and delayed feedback atomically.
    pub fn restore_runtime_snapshot(
        &self,
        snapshot: StatefulRuntimeSnapshotV3,
    ) -> Result<(), StatefulHandlerErrorV2> {
        snapshot.validate(self.metadata.state_schema_version)?;
        if snapshot.handler_snapshot.state.len()
            > self.config.resource_profile.max_model_state_bytes as usize
            || snapshot.pending_decisions.len()
                > self.config.resource_profile.max_pending_decisions as usize
            || snapshot.completed_decisions.len()
                > self.config.resource_profile.max_completed_decisions as usize
        {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidState,
            ));
        }
        self.validate_runtime_snapshot_size(&snapshot)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        state.previous_good = Some(state.snapshot.clone());
        state.snapshot = snapshot.handler_snapshot;
        state.pending_decisions = snapshot.pending_decisions;
        state.completed_decisions = snapshot.completed_decisions;
        state.restart_count = state.restart_count.saturating_add(1);
        Ok(())
    }

    /// Validate and retain a candidate without activating it.
    pub fn stage_candidate(
        &self,
        snapshot: StatefulStateSnapshotV2,
    ) -> Result<(), StatefulHandlerErrorV2> {
        snapshot.validate(self.metadata.state_schema_version)?;
        if snapshot.state.len() > self.config.resource_profile.max_model_state_bytes as usize {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::InvalidState,
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        state.candidate = Some(snapshot);
        Ok(())
    }

    /// Atomically activate a previously staged candidate.
    pub fn promote_candidate(&self) -> Result<u64, StatefulHandlerErrorV2> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        let candidate = state
            .candidate
            .take()
            .ok_or_else(|| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::InvalidState))?;
        state.previous_good = Some(state.snapshot.clone());
        state.snapshot = candidate;
        state.pending_decisions.clear();
        state.completed_decisions.clear();
        Ok(state.snapshot.state_generation)
    }

    /// Restore the most recent previous-good state after a failed activation.
    pub fn rollback_previous_good(&self) -> Result<u64, StatefulHandlerErrorV2> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        let previous = state
            .previous_good
            .take()
            .ok_or_else(|| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::InvalidState))?;
        let failed = std::mem::replace(&mut state.snapshot, previous);
        state.previous_good = Some(failed);
        state.candidate = None;
        state.pending_decisions.clear();
        state.completed_decisions.clear();
        Ok(state.snapshot.state_generation)
    }

    /// Migrate a snapshot without destructive in-place mutation.
    pub fn restore_snapshot_with_migration(
        &self,
        snapshot: StatefulStateSnapshotV2,
        migrator: &dyn StatefulStateMigratorV1,
    ) -> Result<(), StatefulHandlerErrorV2> {
        if snapshot.state_schema_version == self.metadata.state_schema_version {
            return self.restore_snapshot(snapshot);
        }
        let (schema_version, state) =
            migrator.migrate(snapshot.state_schema_version, &snapshot.state)?;
        if schema_version != self.metadata.state_schema_version {
            return Err(StatefulHandlerErrorV2::new(
                StatefulHandlerErrorKindV2::IncompatibleVersion,
            ));
        }
        self.restore_snapshot(StatefulStateSnapshotV2::new(
            schema_version,
            snapshot.state_generation,
            state,
        ))
    }

    /// Handle one request at a caller-supplied clock value. No system clock is
    /// read by the library.
    pub fn handle_at(&self, envelope: EnvelopeV3, now_unix_ms: u64) -> RuntimeResponseV3 {
        if let Err(error) = envelope.validate() {
            return self.error_response(
                envelope.request_id,
                RuntimeErrorCodeV3::InvalidEnvelope,
                error.to_string(),
                self.current_generation(),
            );
        }
        if envelope.is_expired_at(now_unix_ms) {
            return self.error_response(
                envelope.request_id,
                RuntimeErrorCodeV3::ExpiredRequest,
                "request deadline has expired",
                self.current_generation(),
            );
        }

        match envelope.request.clone() {
            RuntimeRequestV3::Handshake {} => self.response(
                envelope.request_id,
                self.current_generation(),
                RuntimeResponseBodyV3::Handshake {
                    capabilities: self.config.capabilities.clone(),
                    feature_schema_hash: self.config.feature_schema_hash.clone(),
                    handler_api_version: HANDLER_API_VERSION,
                },
            ),
            RuntimeRequestV3::Health {} => self.response(
                envelope.request_id,
                self.current_generation(),
                RuntimeResponseBodyV3::Health {
                    healthy: self.is_healthy(),
                },
            ),
            request => self.handle_stateful(envelope, request, now_unix_ms),
        }
    }

    /// Decode and handle one bounded IPC V3 JSON message. Malformed and
    /// oversized messages are rejected before a handler can observe them.
    /// The caller supplies the clock so replay and tests remain deterministic.
    pub fn handle_json_at(&self, message: &[u8], now_unix_ms: u64) -> RuntimeResponseV3 {
        if message.len() > self.config.resource_profile.max_ipc_frame_bytes as usize {
            return self.error_response(
                "invalid-request".into(),
                RuntimeErrorCodeV3::PayloadTooLarge,
                "request exceeds the IPC message limit",
                self.current_generation(),
            );
        }
        let envelope = match serde_json::from_slice::<EnvelopeV3>(message) {
            Ok(envelope) => envelope,
            Err(_) => {
                return self.error_response(
                    "invalid-request".into(),
                    RuntimeErrorCodeV3::InvalidJson,
                    "request is not valid IPC V3 JSON",
                    self.current_generation(),
                );
            }
        };
        self.handle_at(envelope, now_unix_ms)
    }

    /// Handle the additive Preview response surface. The frozen v3 response
    /// type remains available through `handle_at` for library consumers.
    pub fn handle_preview_at(
        &self,
        envelope: EnvelopeV3,
        now_unix_ms: u64,
    ) -> RuntimeResponseV3Preview {
        let is_decide = matches!(envelope.request, RuntimeRequestV3::Decide { .. });
        let response = self.handle_at(envelope, now_unix_ms);
        let body = match response.response {
            RuntimeResponseBodyV3::Handshake {
                capabilities,
                feature_schema_hash,
                handler_api_version,
            } => RuntimeResponseBodyV3Preview::Handshake {
                capabilities,
                feature_schema_hash,
                handler_api_version,
                channel: PREVIEW_CHANNEL_V3.into(),
            },
            RuntimeResponseBodyV3::Health { healthy } => RuntimeResponseBodyV3Preview::Health {
                healthy,
                status: if healthy {
                    "healthy".into()
                } else {
                    "failed_closed".into()
                },
                reason_codes: self.health_reason_codes().unwrap_or_default(),
            },
            RuntimeResponseBodyV3::Result { output } => RuntimeResponseBodyV3Preview::Result {
                output,
                decision_id: is_decide.then_some(response.request_id.clone()),
                decision_generation: is_decide.then_some(response.state_generation),
            },
            RuntimeResponseBodyV3::Inspection { summary } => {
                RuntimeResponseBodyV3Preview::Inspection { summary }
            }
            RuntimeResponseBodyV3::Snapshot {
                state_schema_version,
                state_checksum,
                state,
            } => RuntimeResponseBodyV3Preview::Snapshot {
                state_schema_version,
                state_checksum,
                state,
            },
            RuntimeResponseBodyV3::Reset { reset } => RuntimeResponseBodyV3Preview::Reset { reset },
            RuntimeResponseBodyV3::Error { error } => {
                let preview_code = preview_error_code(error.code, &error.message);
                RuntimeResponseBodyV3Preview::Error {
                    error: RuntimeErrorV3Preview {
                        code: preview_code,
                        message: error.message,
                        retryable: preview_code.is_retryable(),
                    },
                }
            }
        };
        RuntimeResponseV3Preview {
            request_id: response.request_id,
            api_version: response.api_version,
            runtime_identity: response.runtime_identity,
            model_generation: response.model_generation,
            state_generation: response.state_generation,
            response: body,
        }
    }

    pub fn handle_preview_json_at(
        &self,
        message: &[u8],
        now_unix_ms: u64,
    ) -> RuntimeResponseV3Preview {
        if message.len() > self.config.resource_profile.max_ipc_frame_bytes as usize {
            return self.preview_error_response(
                "invalid-request".into(),
                PreviewErrorCodeV3::PayloadTooLarge,
                "request exceeds the IPC message limit",
            );
        }
        match serde_json::from_slice::<EnvelopeV3>(message) {
            Ok(envelope) => self.handle_preview_at(envelope, now_unix_ms),
            Err(_) => self.preview_error_response(
                "invalid-request".into(),
                PreviewErrorCodeV3::InvalidJson,
                "request is not valid IPC V3 JSON",
            ),
        }
    }

    fn preview_error_response(
        &self,
        request_id: String,
        code: PreviewErrorCodeV3,
        message: &str,
    ) -> RuntimeResponseV3Preview {
        RuntimeResponseV3Preview {
            request_id,
            api_version: RUNTIME_API_VERSION_V3,
            runtime_identity: self.config.runtime_identity.clone(),
            model_generation: self.config.model_generation,
            state_generation: self.current_generation(),
            response: RuntimeResponseBodyV3Preview::Error {
                error: RuntimeErrorV3Preview {
                    code,
                    message: message.into(),
                    retryable: code.is_retryable(),
                },
            },
        }
    }

    fn handle_stateful(
        &self,
        envelope: EnvelopeV3,
        request: RuntimeRequestV3,
        now_unix_ms: u64,
    ) -> RuntimeResponseV3 {
        let request_id = envelope.request_id;
        let capability = envelope.capability.unwrap_or_default();
        if !self
            .config
            .capabilities
            .iter()
            .any(|item| item == &capability)
        {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::UnsupportedCapability,
                "capability is not in the effective set",
                self.current_generation(),
            );
        }
        if envelope.feature_schema_hash.as_deref() != Some(self.config.feature_schema_hash.as_str())
        {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::StateMismatch,
                "feature schema hash does not match",
                self.current_generation(),
            );
        }
        if envelope.model_generation != self.config.model_generation {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::IncompatibleGeneration,
                "model generation does not match",
                self.current_generation(),
            );
        }
        if let RuntimeRequestV3::Feedback { generation, .. } = &request
            && *generation != self.config.model_generation
        {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::IncompatibleGeneration,
                "feedback generation does not match",
                self.current_generation(),
            );
        }

        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "runtime state lock is poisoned",
                    0,
                );
            }
        };
        if envelope.state_generation != state.snapshot.state_generation {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::StateMismatch,
                "state generation does not match",
                state.snapshot.state_generation,
            );
        }

        if let RuntimeRequestV3::Decide { .. } = &request {
            if state.pending_decisions.contains_key(&request_id)
                || state.completed_decisions.contains_key(&request_id)
            {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "decision id was already used",
                    state.snapshot.state_generation,
                );
            }
            if state.pending_decisions.len()
                >= self.config.resource_profile.max_pending_decisions as usize
                || state.completed_decisions.len()
                    >= self.config.resource_profile.max_completed_decisions as usize
            {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "pending decision capacity is exhausted",
                    state.snapshot.state_generation,
                );
            }
        }
        if let RuntimeRequestV3::Feedback {
            decision_id,
            generation,
            ..
        } = &request
        {
            if state.completed_decisions.contains_key(decision_id) {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::DuplicateFeedback,
                    "feedback was already applied",
                    state.snapshot.state_generation,
                );
            }
            let Some(entry) = state.pending_decisions.get(decision_id) else {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "decision id is not pending",
                    state.snapshot.state_generation,
                );
            };
            if entry.model_generation != *generation {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::IncompatibleGeneration,
                    "feedback generation is stale",
                    state.snapshot.state_generation,
                );
            }
            if state.completed_decisions.len()
                >= self.config.resource_profile.max_completed_decisions as usize
            {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "completed decision capacity is exhausted",
                    state.snapshot.state_generation,
                );
            }
        }

        if let RuntimeRequestV3::Snapshot {} = request {
            return self.response(
                request_id,
                state.snapshot.state_generation,
                RuntimeResponseBodyV3::Snapshot {
                    state_schema_version: state.snapshot.state_schema_version,
                    state_checksum: state.snapshot.checksum_sha256.clone(),
                    state: hex::encode(&state.snapshot.state),
                },
            );
        }
        if let RuntimeRequestV3::Reset {
            expected_state_generation,
        } = request
        {
            if expected_state_generation != state.snapshot.state_generation {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::StateMismatch,
                    "reset generation does not match",
                    state.snapshot.state_generation,
                );
            }
            let Some(next_generation) = state.snapshot.state_generation.checked_add(1) else {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::InvalidState,
                    "state generation overflow",
                    state.snapshot.state_generation,
                );
            };
            state.snapshot = StatefulStateSnapshotV2::new(
                self.metadata.state_schema_version,
                next_generation,
                self.config.initial_state.clone(),
            );
            state.pending_decisions.clear();
            state.completed_decisions.clear();
            return self.response(
                request_id,
                next_generation,
                RuntimeResponseBodyV3::Reset { reset: true },
            );
        }

        let deterministic_seed = match &request {
            RuntimeRequestV3::Decide {
                deterministic_seed, ..
            } => *deterministic_seed,
            _ => None,
        };
        let event_json = match serde_json::to_vec(&request) {
            Ok(bytes) if bytes.len() <= MAX_EVENT_BYTES => bytes,
            Ok(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::PayloadTooLarge,
                    "event exceeds handler limit",
                    state.snapshot.state_generation,
                );
            }
            Err(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::InvalidJson,
                    "event could not be encoded",
                    state.snapshot.state_generation,
                );
            }
        };

        let result =
            match self
                .handler
                .handle(&event_json, &state.snapshot.state, deterministic_seed)
            {
                Ok(result) => result,
                Err(error) => {
                    let (code, message) = map_handler_error(error.kind());
                    state.last_error = Some(message.into());
                    return self.error_response(
                        request_id,
                        code,
                        message,
                        state.snapshot.state_generation,
                    );
                }
            };
        let output_bytes = match serde_json::to_vec(&result.output) {
            Ok(bytes) => bytes,
            Err(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::HandlerInvalidOutput,
                    "handler output was not valid JSON",
                    state.snapshot.state_generation,
                );
            }
        };
        if output_bytes.len() > MAX_OUTPUT_BYTES {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::HandlerOutputTooLarge,
                "handler output exceeded the size limit",
                state.snapshot.state_generation,
            );
        }
        if validate_state_bytes(
            &result.next_state,
            self.metadata.state_schema_version,
            self.metadata.state_schema_version,
        )
        .is_err()
        {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::InvalidState,
                "handler returned invalid next state",
                state.snapshot.state_generation,
            );
        }
        if result.next_state.len() > self.config.resource_profile.max_model_state_bytes as usize {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::Internal,
                "model state resource limit exceeded",
                state.snapshot.state_generation,
            );
        }
        let Some(next_generation) = state.snapshot.state_generation.checked_add(1) else {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::InvalidState,
                "state generation overflow",
                state.snapshot.state_generation,
            );
        };
        let next_snapshot = StatefulStateSnapshotV2::new(
            self.metadata.state_schema_version,
            next_generation,
            result.next_state,
        );
        let mut next_pending = state.pending_decisions.clone();
        let mut next_completed = state.completed_decisions.clone();
        if matches!(request, RuntimeRequestV3::Decide { .. }) {
            let entry = DecisionLedgerEntryV3 {
                decision_id: request_id.clone(),
                model_generation: self.config.model_generation,
                state_generation: next_generation,
                created_at_unix_ms: now_unix_ms,
                selected_action_id: None,
                reward: None,
                outcome_time_unix_ms: None,
            };
            next_pending.insert(request_id.clone(), entry);
        } else {
            if let RuntimeRequestV3::Feedback {
                decision_id,
                selected_action_id,
                reward,
                outcome_time_ms,
                ..
            } = &request
            {
                let mut entry = next_pending
                    .remove(decision_id)
                    .ok_or_else(|| {
                        StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal)
                    })
                    .unwrap();
                entry.selected_action_id = Some(selected_action_id.clone());
                entry.reward = Some(reward.to_string());
                entry.outcome_time_unix_ms = Some(*outcome_time_ms);
                next_completed.insert(entry.decision_id.clone(), entry);
            }
        }
        let prospective = StatefulRuntimeSnapshotV3::new(
            next_snapshot.clone(),
            next_pending.clone(),
            next_completed.clone(),
        );
        if self.validate_runtime_snapshot_size(&prospective).is_err() {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::Internal,
                "snapshot resource limit exceeded",
                state.snapshot.state_generation,
            );
        }
        let previous_snapshot = state.snapshot.clone();
        state.snapshot = next_snapshot;
        state.previous_good = Some(previous_snapshot);
        state.pending_decisions = next_pending;
        state.completed_decisions = next_completed;
        self.response(
            request_id,
            next_generation,
            match request {
                RuntimeRequestV3::Inspect {} => RuntimeResponseBodyV3::Inspection {
                    summary: self.inspection_summary(&state, result.output),
                },
                _ => RuntimeResponseBodyV3::Result {
                    output: result.output,
                },
            },
        )
    }

    fn is_healthy(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.last_error.is_none())
            .unwrap_or(false)
    }

    fn health_reason_codes(&self) -> Option<Vec<String>> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.last_error.as_ref().map(|error| vec![error.clone()]))
    }

    fn inspection_summary(
        &self,
        state: &RuntimeStateV3,
        handler_summary: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "runtimeVersion": self.config.runtime_identity.version,
            "protocolVersion": RUNTIME_API_VERSION_V3,
            "channel": PREVIEW_CHANNEL_V3,
            "modelGeneration": self.config.model_generation,
            "stateGeneration": state.snapshot.state_generation,
            "stateSchemaVersion": state.snapshot.state_schema_version,
            "stateChecksum": state.snapshot.checksum_sha256,
            "pendingDecisions": state.pending_decisions.len(),
            "completedDecisions": state.completed_decisions.len(),
            "resourceProfile": self.config.resource_profile,
            "resourceUtilization": {
                "stateBytes": state.snapshot.state.len(),
                "pendingDecisions": state.pending_decisions.len(),
                "completedDecisions": state.completed_decisions.len(),
            },
            "rollbackAvailable": state.previous_good.is_some(),
            "candidateAvailable": state.candidate.is_some(),
            "restartCount": state.restart_count,
            "health": self.health_status_for_state(state),
            "lastError": state.last_error,
            "handler": handler_summary,
        })
    }

    /// Return structured diagnostics using a caller-provided timestamp.
    pub fn diagnostics_at(
        &self,
        observed_at_unix_ms: u64,
    ) -> Result<RuntimeDiagnosticsV1, StatefulHandlerErrorV2> {
        let state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        let snapshot = StatefulRuntimeSnapshotV3::new(
            state.snapshot.clone(),
            state.pending_decisions.clone(),
            state.completed_decisions.clone(),
        );
        let snapshot_bytes = serde_json::to_vec(&snapshot)
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?
            .len();
        let usage = RuntimeResourceUsageV1 {
            state_bytes: state.snapshot.state.len(),
            snapshot_bytes,
            pending_decisions: state.pending_decisions.len(),
            completed_decisions: state.completed_decisions.len(),
        };
        let health = self.health_status_for_state(&state);
        let reason_codes = state
            .last_error
            .as_ref()
            .map(|error| vec![error.clone()])
            .unwrap_or_default();
        Ok(RuntimeDiagnosticsV1 {
            runtime_version: self.config.runtime_identity.version.clone(),
            protocol_version: RUNTIME_API_VERSION_V3,
            channel: PREVIEW_CHANNEL_V3.into(),
            model_generation: self.config.model_generation,
            state_generation: state.snapshot.state_generation,
            state_schema_version: state.snapshot.state_schema_version,
            observed_at_unix_ms,
            health,
            reason_codes,
            resource_usage: usage,
            rollback_available: state.previous_good.is_some(),
            candidate_available: state.candidate.is_some(),
            restart_count: state.restart_count,
            last_error: state.last_error.clone(),
        })
    }

    fn health_status_for_state(&self, state: &RuntimeStateV3) -> RuntimeHealthStatusV1 {
        if state.last_error.is_some() {
            return RuntimeHealthStatusV1::FailedClosed;
        }
        let profile = &self.config.resource_profile;
        if state.snapshot.state.len() * 10 >= profile.max_model_state_bytes as usize * 9
            || state.pending_decisions.len() * 10 >= profile.max_pending_decisions as usize * 9
            || state.completed_decisions.len() * 10 >= profile.max_completed_decisions as usize * 9
        {
            RuntimeHealthStatusV1::ResourcePressure
        } else {
            RuntimeHealthStatusV1::Healthy
        }
    }

    fn validate_runtime_snapshot_size(
        &self,
        snapshot: &StatefulRuntimeSnapshotV3,
    ) -> Result<(), StatefulHandlerErrorV2> {
        let bytes = serde_json::to_vec(snapshot)
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        if bytes.len() > self.config.resource_profile.max_snapshot_bytes as usize {
            return Err(StatefulHandlerErrorV2::with_detail(
                StatefulHandlerErrorKindV2::InvalidState,
                "snapshot exceeds resource limit",
            ));
        }
        Ok(())
    }

    fn current_generation(&self) -> u64 {
        self.state
            .lock()
            .map(|state| state.snapshot.state_generation)
            .unwrap_or(0)
    }

    fn response(
        &self,
        request_id: String,
        state_generation: u64,
        response: RuntimeResponseBodyV3,
    ) -> RuntimeResponseV3 {
        RuntimeResponseV3 {
            request_id,
            api_version: RUNTIME_API_VERSION_V3,
            runtime_identity: self.config.runtime_identity.clone(),
            model_generation: self.config.model_generation,
            state_generation,
            response,
        }
    }

    fn error_response(
        &self,
        request_id: String,
        code: RuntimeErrorCodeV3,
        message: impl Into<String>,
        state_generation: u64,
    ) -> RuntimeResponseV3 {
        self.response(
            if request_id.is_empty() {
                "invalid-request".into()
            } else {
                request_id
            },
            state_generation,
            RuntimeResponseBodyV3::Error {
                error: RuntimeErrorV3::new(code, message),
            },
        )
    }
}

fn validate_state_bytes(
    state: &[u8],
    actual_schema_version: u32,
    expected_schema_version: u32,
) -> Result<(), StatefulHandlerErrorV2> {
    if actual_schema_version == 0 || actual_schema_version != expected_schema_version {
        return Err(StatefulHandlerErrorV2::new(
            StatefulHandlerErrorKindV2::IncompatibleVersion,
        ));
    }
    if state.len() > MAX_STATE_BYTES || serde_json::from_slice::<serde_json::Value>(state).is_err()
    {
        return Err(StatefulHandlerErrorV2::new(
            StatefulHandlerErrorKindV2::InvalidState,
        ));
    }
    Ok(())
}

fn validate_feature_schema_hash(hash: &str) -> Result<(), StatefulHandlerErrorV2> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StatefulHandlerErrorV2::new(
            StatefulHandlerErrorKindV2::InvalidModel,
        ));
    }
    Ok(())
}

fn validate_capabilities(capabilities: &[String]) -> Result<(), StatefulHandlerErrorV2> {
    if capabilities.is_empty() || capabilities.len() > rill_handler_api::MAX_CAPABILITIES {
        return Err(StatefulHandlerErrorV2::new(
            StatefulHandlerErrorKindV2::InvalidModel,
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    if capabilities.iter().any(|capability| {
        capability.is_empty()
            || capability.len() > rill_handler_api::MAX_CAPABILITY_LEN
            || !seen.insert(capability)
    }) {
        return Err(StatefulHandlerErrorV2::new(
            StatefulHandlerErrorKindV2::InvalidModel,
        ));
    }
    Ok(())
}

fn state_checksum(state: &[u8]) -> String {
    hex::encode(Sha256::digest(state))
}

fn map_handler_error(kind: StatefulHandlerErrorKindV2) -> (RuntimeErrorCodeV3, &'static str) {
    match kind {
        StatefulHandlerErrorKindV2::InvalidEvent => (
            RuntimeErrorCodeV3::InvalidEnvelope,
            "handler rejected the event",
        ),
        StatefulHandlerErrorKindV2::InvalidState
        | StatefulHandlerErrorKindV2::IncompatibleVersion => (
            RuntimeErrorCodeV3::InvalidState,
            "handler rejected the current state",
        ),
        StatefulHandlerErrorKindV2::DuplicateFeedback => (
            RuntimeErrorCodeV3::DuplicateFeedback,
            "feedback was already applied",
        ),
        StatefulHandlerErrorKindV2::Timeout => (
            RuntimeErrorCodeV3::HandlerTimeout,
            "handler exceeded the wall-clock deadline",
        ),
        StatefulHandlerErrorKindV2::Trap => (RuntimeErrorCodeV3::HandlerTrap, "handler trapped"),
        StatefulHandlerErrorKindV2::OutputTooLarge => (
            RuntimeErrorCodeV3::HandlerOutputTooLarge,
            "handler output exceeded the size limit",
        ),
        StatefulHandlerErrorKindV2::InvalidOutput => (
            RuntimeErrorCodeV3::HandlerInvalidOutput,
            "handler output was not valid JSON",
        ),
        StatefulHandlerErrorKindV2::InvalidModel
        | StatefulHandlerErrorKindV2::MetadataMismatch
        | StatefulHandlerErrorKindV2::Internal => {
            (RuntimeErrorCodeV3::Internal, "internal runtime error")
        }
    }
}

fn preview_error_code(code: RuntimeErrorCodeV3, message: &str) -> PreviewErrorCodeV3 {
    match message {
        "decision id was already used" => PreviewErrorCodeV3::DuplicateDecision,
        "decision id is not pending" => PreviewErrorCodeV3::UnknownDecision,
        "feedback generation is stale" => PreviewErrorCodeV3::StaleFeedback,
        "pending decision capacity is exhausted"
        | "completed decision capacity is exhausted"
        | "model state resource limit exceeded"
        | "snapshot resource limit exceeded" => PreviewErrorCodeV3::CapacityExceeded,
        _ => match code {
            RuntimeErrorCodeV3::InvalidJson => PreviewErrorCodeV3::InvalidJson,
            RuntimeErrorCodeV3::InvalidRequestId
            | RuntimeErrorCodeV3::InvalidClientIdentity
            | RuntimeErrorCodeV3::IncompatibleApiVersion => PreviewErrorCodeV3::InvalidEnvelope,
            RuntimeErrorCodeV3::InvalidEnvelope => PreviewErrorCodeV3::InvalidEnvelope,
            RuntimeErrorCodeV3::PayloadTooLarge => PreviewErrorCodeV3::PayloadTooLarge,
            RuntimeErrorCodeV3::UnsupportedCapability => PreviewErrorCodeV3::UnsupportedCapability,
            RuntimeErrorCodeV3::StateMismatch => PreviewErrorCodeV3::StateMismatch,
            RuntimeErrorCodeV3::ExpiredRequest => PreviewErrorCodeV3::ExpiredRequest,
            RuntimeErrorCodeV3::IncompatibleGeneration => {
                PreviewErrorCodeV3::IncompatibleGeneration
            }
            RuntimeErrorCodeV3::DuplicateFeedback => PreviewErrorCodeV3::DuplicateFeedback,
            RuntimeErrorCodeV3::HandlerTimeout => PreviewErrorCodeV3::HandlerTimeout,
            RuntimeErrorCodeV3::HandlerTrap => PreviewErrorCodeV3::HandlerTrap,
            RuntimeErrorCodeV3::HandlerOutputTooLarge => PreviewErrorCodeV3::HandlerOutputTooLarge,
            RuntimeErrorCodeV3::HandlerInvalidOutput => PreviewErrorCodeV3::HandlerInvalidOutput,
            RuntimeErrorCodeV3::InvalidState => PreviewErrorCodeV3::InvalidState,
            RuntimeErrorCodeV3::Internal => PreviewErrorCodeV3::Internal,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestHandler {
        metadata: StatefulHandlerMetadataV2,
        mode: StatefulHandlerErrorKindV2,
    }

    impl StatefulHandlerV2 for TestHandler {
        fn metadata(&self) -> &StatefulHandlerMetadataV2 {
            &self.metadata
        }

        fn handle(
            &self,
            _event_json: &[u8],
            current_state: &[u8],
            _deterministic_seed: Option<u64>,
        ) -> Result<StatefulHandlerResultV2, StatefulHandlerErrorV2> {
            match self.mode {
                StatefulHandlerErrorKindV2::Internal => {
                    let mut value: serde_json::Value =
                        serde_json::from_slice(current_state).unwrap();
                    value["count"] = serde_json::json!(value["count"].as_u64().unwrap_or(0) + 1);
                    Ok(StatefulHandlerResultV2 {
                        output: value.clone(),
                        next_state: serde_json::to_vec(&value).unwrap(),
                    })
                }
                StatefulHandlerErrorKindV2::InvalidState => Ok(StatefulHandlerResultV2 {
                    output: serde_json::json!({"ignored": true}),
                    next_state: b"not-json".to_vec(),
                }),
                StatefulHandlerErrorKindV2::OutputTooLarge => Ok(StatefulHandlerResultV2 {
                    output: serde_json::json!({"data": "x".repeat(MAX_OUTPUT_BYTES + 1)}),
                    next_state: current_state.to_vec(),
                }),
                other => Err(StatefulHandlerErrorV2::new(other)),
            }
        }
    }

    fn engine(mode: StatefulHandlerErrorKindV2) -> StatefulRuntimeEngineV3 {
        let metadata = StatefulHandlerMetadataV2 {
            id: "org.example.stateful".into(),
            version: "2.0.0".into(),
            api_version: HANDLER_API_VERSION,
            capabilities: vec!["org.example.decide".into()],
            state_schema_version: 1,
        };
        let config = StatefulRuntimeConfigV3::new(
            IdentityV3 {
                name: "rill-runtime".into(),
                version: "1.0.0".into(),
            },
            7,
            "ab".repeat(32),
            metadata.capabilities.clone(),
            br#"{"count":0}"#.to_vec(),
        );
        StatefulRuntimeEngineV3::new(config, Arc::new(TestHandler { metadata, mode })).unwrap()
    }

    fn decide(state_generation: u64) -> EnvelopeV3 {
        EnvelopeV3 {
            request_id: "d1".into(),
            api_version: RUNTIME_API_VERSION_V3,
            client_identity: IdentityV3 {
                name: "host".into(),
                version: "1".into(),
            },
            capability: Some("org.example.decide".into()),
            deadline_unix_ms: Some(100),
            feature_schema_hash: Some("ab".repeat(32)),
            model_generation: 7,
            state_generation,
            payload_limit: rill_runtime_protocol::MAX_MESSAGE_BYTES as u32,
            request: RuntimeRequestV3::Decide {
                context: serde_json::json!({"features": [1.0]}),
                deterministic_seed: Some(42),
            },
        }
    }

    #[test]
    fn state_update_is_atomic_and_increments_generation() {
        let engine = engine(StatefulHandlerErrorKindV2::Internal);
        let response = engine.handle_at(decide(0), 100);
        assert!(matches!(
            response.response,
            RuntimeResponseBodyV3::Result { .. }
        ));
        assert_eq!(response.state_generation, 1);
        assert_eq!(engine.snapshot().unwrap().state, br#"{"count":1}"#);
    }

    #[test]
    fn invalid_next_state_is_fail_closed() {
        let engine = engine(StatefulHandlerErrorKindV2::InvalidState);
        let before = engine.snapshot().unwrap();
        let response = engine.handle_at(decide(0), 100);
        assert!(matches!(
            response.response,
            RuntimeResponseBodyV3::Error { error }
                if error.code == RuntimeErrorCodeV3::InvalidState
        ));
        assert_eq!(engine.snapshot().unwrap(), before);
    }

    #[test]
    fn timeout_trap_and_oversize_leave_state_unchanged() {
        for mode in [
            StatefulHandlerErrorKindV2::Timeout,
            StatefulHandlerErrorKindV2::Trap,
            StatefulHandlerErrorKindV2::OutputTooLarge,
        ] {
            let engine = engine(mode);
            let before = engine.snapshot().unwrap();
            let response = engine.handle_at(decide(0), 100);
            assert!(matches!(
                response.response,
                RuntimeResponseBodyV3::Error { .. }
            ));
            assert_eq!(engine.snapshot().unwrap(), before, "mode={mode:?}");
        }
    }

    #[test]
    fn stale_generation_and_expired_request_are_rejected() {
        let engine = engine(StatefulHandlerErrorKindV2::Internal);
        let stale = engine.handle_at(decide(1), 100);
        assert!(matches!(
            stale.response,
            RuntimeResponseBodyV3::Error { error }
                if error.code == RuntimeErrorCodeV3::StateMismatch
        ));
        let expired = engine.handle_at(decide(0), 101);
        assert!(matches!(
            expired.response,
            RuntimeResponseBodyV3::Error { error }
                if error.code == RuntimeErrorCodeV3::ExpiredRequest
        ));
    }

    #[test]
    fn corrupt_snapshot_checksum_is_rejected_without_mutation() {
        let engine = engine(StatefulHandlerErrorKindV2::Internal);
        let before = engine.snapshot().unwrap();
        let mut corrupt = before.clone();
        corrupt.checksum_sha256 = "00".repeat(32);
        assert!(engine.restore_snapshot(corrupt).is_err());
        assert_eq!(engine.snapshot().unwrap(), before);
    }

    #[test]
    fn json_entrypoint_rejects_malformed_and_oversized_messages() {
        let engine = engine(StatefulHandlerErrorKindV2::Internal);
        let before = engine.snapshot().unwrap();

        let malformed = engine.handle_json_at(b"{", 100);
        assert!(matches!(
            malformed.response,
            RuntimeResponseBodyV3::Error { error }
                if error.code == RuntimeErrorCodeV3::InvalidJson
        ));

        let oversized = vec![b' '; rill_runtime_protocol::MAX_MESSAGE_BYTES + 1];
        let oversized = engine.handle_json_at(&oversized, 100);
        assert!(matches!(
            oversized.response,
            RuntimeResponseBodyV3::Error { error }
                if error.code == RuntimeErrorCodeV3::PayloadTooLarge
        ));
        assert_eq!(engine.snapshot().unwrap(), before);
    }

    #[test]
    fn inspect_and_diagnostics_do_not_reenter_the_state_lock() {
        let engine = engine(StatefulHandlerErrorKindV2::Internal);
        let mut request = decide(0);
        request.request = RuntimeRequestV3::Inspect {};
        let response = engine.handle_preview_at(request, 100);
        assert!(matches!(
            response.response,
            RuntimeResponseBodyV3Preview::Inspection { .. }
        ));
        let diagnostics = engine.diagnostics_at(123).unwrap();
        assert_eq!(diagnostics.observed_at_unix_ms, 123);
        assert_eq!(diagnostics.health, RuntimeHealthStatusV1::Healthy);
        assert_eq!(diagnostics.state_generation, 1);
        assert!(diagnostics.resource_usage.snapshot_bytes > diagnostics.resource_usage.state_bytes);
    }

    #[test]
    fn ipc_and_snapshot_limits_fail_closed_without_mutation() {
        let profile = ResourceProfileV1 {
            max_ipc_frame_bytes: 128,
            max_snapshot_bytes: 128,
            ..ResourceProfileV1::default()
        };
        let metadata = StatefulHandlerMetadataV2 {
            id: "org.example.stateful".into(),
            version: "2.0.0".into(),
            api_version: HANDLER_API_VERSION,
            capabilities: vec!["org.example.decide".into()],
            state_schema_version: 1,
        };
        let config = StatefulRuntimeConfigV3::new(
            IdentityV3 {
                name: "rill-runtime".into(),
                version: "1.0.0".into(),
            },
            7,
            "ab".repeat(32),
            metadata.capabilities.clone(),
            br#"{"count":0}"#.to_vec(),
        )
        .with_resource_profile(profile);
        let engine = StatefulRuntimeEngineV3::new(
            config,
            Arc::new(TestHandler {
                metadata,
                mode: StatefulHandlerErrorKindV2::Internal,
            }),
        )
        .unwrap();
        let before = engine.snapshot().unwrap();
        let oversized = vec![b' '; 129];
        let response = engine.handle_preview_json_at(&oversized, 100);
        assert!(matches!(
            response.response,
            RuntimeResponseBodyV3Preview::Error { error }
                if error.code == PreviewErrorCodeV3::PayloadTooLarge
        ));
        assert_eq!(engine.snapshot().unwrap(), before);
        assert!(engine.runtime_snapshot().is_err());
    }
}

//! Preview stateful Handler ABI v2 and IPC V3 runtime integration.
//!
//! The host owns persistence and only commits a handler's proposed next state
//! after all bounds, JSON, schema-version and checksum checks succeed.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
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
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

const MAX_HANDLER_DETAIL_BYTES_V2: usize = 4 * 1024;
const MAX_PARTITION_KEY_LEN_V3: usize = 96;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionLedgerEntryV3 {
    pub decision_id: String,
    pub model_generation: u64,
    pub state_generation: u64,
    pub created_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_action_id: Option<String>,
    /// The exact action feature vector selected by the decision. This generic
    /// numeric context prevents delayed feedback from training against a
    /// later observation that reused the same opaque action id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_action_features: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_time_unix_ms: Option<u64>,
}

// Feature vectors are validated as finite JSON numbers before entering the
// ledger, so the existing Eq contract remains valid for persisted entries.
impl Eq for DecisionLedgerEntryV3 {}

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

/// Durable state for one opaque consumer partition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PartitionRuntimeSnapshotV3 {
    pub handler_snapshot: StatefulStateSnapshotV2,
    pub pending_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    pub completed_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
}

/// Backward-compatible durable snapshot for the default partition.
#[derive(Debug, Clone, PartialEq)]
pub struct StatefulRuntimeSnapshotV3 {
    pub format_version: u32,
    pub handler_snapshot: StatefulStateSnapshotV2,
    pub pending_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    pub completed_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    pub checksum_sha256: String,
}

impl Eq for StatefulRuntimeSnapshotV3 {}

type PartitionedSnapshotPayload = BTreeMap<String, PartitionRuntimeSnapshotV3>;

static PARTITIONED_SNAPSHOT_PAYLOADS: OnceLock<
    Mutex<BTreeMap<String, PartitionedSnapshotPayload>>,
> = OnceLock::new();

fn partitioned_payloads() -> &'static Mutex<BTreeMap<String, PartitionedSnapshotPayload>> {
    PARTITIONED_SNAPSHOT_PAYLOADS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn remember_partitioned_payload(checksum_sha256: &str, partitions: PartitionedSnapshotPayload) {
    if let Ok(mut payloads) = partitioned_payloads().lock() {
        payloads.insert(checksum_sha256.to_owned(), partitions);
    }
}

fn partitioned_payload(checksum_sha256: &str) -> Option<PartitionedSnapshotPayload> {
    partitioned_payloads()
        .lock()
        .ok()
        .and_then(|payloads| payloads.get(checksum_sha256).cloned())
}

impl Serialize for StatefulRuntimeSnapshotV3 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let partitions = partitioned_payload(&self.checksum_sha256);
        let mut fields = serializer.serialize_struct(
            "StatefulRuntimeSnapshotV3",
            if partitions.is_some() { 6 } else { 5 },
        )?;
        fields.serialize_field("formatVersion", &self.format_version)?;
        fields.serialize_field("handlerSnapshot", &self.handler_snapshot)?;
        fields.serialize_field("pendingDecisions", &self.pending_decisions)?;
        fields.serialize_field("completedDecisions", &self.completed_decisions)?;
        fields.serialize_field("checksumSha256", &self.checksum_sha256)?;
        if let Some(partitions) = partitions {
            fields.serialize_field("partitions", &partitions)?;
        }
        fields.end()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatefulRuntimeSnapshotWire {
    format_version: u32,
    handler_snapshot: StatefulStateSnapshotV2,
    pending_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    completed_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    checksum_sha256: String,
    #[serde(default)]
    partitions: Option<PartitionedSnapshotPayload>,
}

impl<'de> Deserialize<'de> for StatefulRuntimeSnapshotV3 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = StatefulRuntimeSnapshotWire::deserialize(deserializer)?;
        if let Some(partitions) = wire.partitions {
            remember_partitioned_payload(&wire.checksum_sha256, partitions);
        }
        Ok(Self {
            format_version: wire.format_version,
            handler_snapshot: wire.handler_snapshot,
            pending_decisions: wire.pending_decisions,
            completed_decisions: wire.completed_decisions,
            checksum_sha256: wire.checksum_sha256,
        })
    }
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
        let checksum_valid = if let Some(partitions) = partitioned_payload(&self.checksum_sha256) {
            self.checksum_sha256 == state_checksum(&serde_json::to_vec(&partitions).unwrap())
        } else {
            self.checksum_sha256
                == state_checksum(&Self::checksum_input(
                    &self.handler_snapshot,
                    &self.pending_decisions,
                    &self.completed_decisions,
                ))
        };
        if self
            .handler_snapshot
            .validate(expected_schema_version)
            .is_err()
            || !checksum_valid
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
        if let Some(partitions) = partitioned_payload(&self.checksum_sha256) {
            validate_partitions(&partitions, expected_schema_version)?;
        }
        Ok(())
    }

    fn new_partitioned(
        partitions: BTreeMap<String, PartitionRuntimeSnapshotV3>,
    ) -> Result<Self, StatefulHandlerErrorV2> {
        let default = partitions
            .get(DEFAULT_PARTITION_KEY_V3)
            .ok_or_else(|| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::InvalidState))?;
        let mut snapshot = Self::new(
            default.handler_snapshot.clone(),
            default.pending_decisions.clone(),
            default.completed_decisions.clone(),
        );
        snapshot.checksum_sha256 = state_checksum(
            &serde_json::to_vec(&partitions).expect("runtime snapshot fields are serializable"),
        );
        remember_partitioned_payload(&snapshot.checksum_sha256, partitions);
        Ok(snapshot)
    }
}

fn validate_partitions(
    partitions: &BTreeMap<String, PartitionRuntimeSnapshotV3>,
    expected_schema_version: u32,
) -> Result<(), StatefulHandlerErrorV2> {
    if partitions.is_empty()
        || partitions.len() > MAX_PARTITIONS_V3
        || partitions
            .keys()
            .any(|key| key.is_empty() || key.len() > MAX_PARTITION_KEY_LEN_V3)
    {
        return Err(StatefulHandlerErrorV2::new(
            StatefulHandlerErrorKindV2::InvalidState,
        ));
    }
    for partition in partitions.values() {
        partition
            .handler_snapshot
            .validate(expected_schema_version)?;
        for (key, entry) in partition
            .pending_decisions
            .iter()
            .chain(partition.completed_decisions.iter())
        {
            if key != &entry.decision_id || entry.decision_id.is_empty() {
                return Err(StatefulHandlerErrorV2::new(
                    StatefulHandlerErrorKindV2::InvalidState,
                ));
            }
        }
    }
    Ok(())
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
    partitions: BTreeMap<String, PartitionStateV3>,
    restart_count: u64,
    last_error: Option<String>,
}

#[derive(Debug, Clone)]
struct PartitionStateV3 {
    snapshot: StatefulStateSnapshotV2,
    previous_good: Option<StatefulStateSnapshotV2>,
    candidate: Option<StatefulStateSnapshotV2>,
    pending_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
    completed_decisions: BTreeMap<String, DecisionLedgerEntryV3>,
}

const DEFAULT_PARTITION_KEY_V3: &str = "default";
const MAX_PARTITIONS_V3: usize = 64;

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
                partitions: BTreeMap::from([(
                    DEFAULT_PARTITION_KEY_V3.into(),
                    PartitionStateV3 {
                        snapshot,
                        previous_good: None,
                        candidate: None,
                        pending_decisions: BTreeMap::new(),
                        completed_decisions: BTreeMap::new(),
                    },
                )]),
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
        let partition = state.partitions.get_mut(DEFAULT_PARTITION_KEY_V3).unwrap();
        partition.previous_good = Some(partition.snapshot.clone());
        partition.snapshot = snapshot;
        state.restart_count = state.restart_count.saturating_add(1);
        Ok(())
    }

    pub fn snapshot(&self) -> Result<StatefulStateSnapshotV2, StatefulHandlerErrorV2> {
        self.state
            .lock()
            .map(|state| state.partitions[DEFAULT_PARTITION_KEY_V3].snapshot.clone())
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))
    }

    /// Return the handler state plus the durable decision ledger.
    pub fn runtime_snapshot(&self) -> Result<StatefulRuntimeSnapshotV3, StatefulHandlerErrorV2> {
        let state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        let partitions = state
            .partitions
            .iter()
            .map(|(key, partition)| {
                (
                    key.clone(),
                    PartitionRuntimeSnapshotV3 {
                        handler_snapshot: partition.snapshot.clone(),
                        pending_decisions: partition.pending_decisions.clone(),
                        completed_decisions: partition.completed_decisions.clone(),
                    },
                )
            })
            .collect();
        let snapshot = StatefulRuntimeSnapshotV3::new_partitioned(partitions)?;
        self.validate_runtime_snapshot_size(&snapshot)?;
        Ok(snapshot)
    }

    /// Restore the backward-compatible default-partition snapshot.
    pub fn restore_runtime_snapshot(
        &self,
        snapshot: StatefulRuntimeSnapshotV3,
    ) -> Result<(), StatefulHandlerErrorV2> {
        let partitioned = partitioned_payload(&snapshot.checksum_sha256);
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
        if let Some(partitions) = partitioned {
            state.partitions = partitions
                .into_iter()
                .map(|(key, partition)| {
                    (
                        key,
                        PartitionStateV3 {
                            snapshot: partition.handler_snapshot,
                            previous_good: None,
                            candidate: None,
                            pending_decisions: partition.pending_decisions,
                            completed_decisions: partition.completed_decisions,
                        },
                    )
                })
                .collect();
        } else {
            let partition = state.partitions.get_mut(DEFAULT_PARTITION_KEY_V3).unwrap();
            partition.previous_good = Some(partition.snapshot.clone());
            partition.snapshot = snapshot.handler_snapshot;
            partition.pending_decisions = snapshot.pending_decisions;
            partition.completed_decisions = snapshot.completed_decisions;
        }
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
        state
            .partitions
            .get_mut(DEFAULT_PARTITION_KEY_V3)
            .unwrap()
            .candidate = Some(snapshot);
        Ok(())
    }

    /// Atomically activate a previously staged candidate.
    pub fn promote_candidate(&self) -> Result<u64, StatefulHandlerErrorV2> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        let partition = state.partitions.get_mut(DEFAULT_PARTITION_KEY_V3).unwrap();
        let candidate = partition
            .candidate
            .take()
            .ok_or_else(|| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::InvalidState))?;
        partition.previous_good = Some(partition.snapshot.clone());
        partition.snapshot = candidate;
        partition.pending_decisions.clear();
        partition.completed_decisions.clear();
        Ok(partition.snapshot.state_generation)
    }

    /// Restore the most recent previous-good state after a failed activation.
    pub fn rollback_previous_good(&self) -> Result<u64, StatefulHandlerErrorV2> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?;
        let partition = state.partitions.get_mut(DEFAULT_PARTITION_KEY_V3).unwrap();
        let previous = partition
            .previous_good
            .take()
            .ok_or_else(|| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::InvalidState))?;
        let failed = std::mem::replace(&mut partition.snapshot, previous);
        partition.previous_good = Some(failed);
        partition.candidate = None;
        partition.pending_decisions.clear();
        partition.completed_decisions.clear();
        Ok(partition.snapshot.state_generation)
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
        // The existing bounded client identity is the wire-level opaque
        // partition identity, preserving the frozen EnvelopeV3 public shape.
        let partition_key = if envelope.client_identity.name == "host" {
            DEFAULT_PARTITION_KEY_V3.to_owned()
        } else {
            envelope.client_identity.name.clone()
        };
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
        let restart_count = state.restart_count;
        let last_error = state.last_error.clone();
        if !state.partitions.contains_key(&partition_key) {
            if state.partitions.len() >= MAX_PARTITIONS_V3 {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "partition capacity is exhausted",
                    state.partitions[DEFAULT_PARTITION_KEY_V3]
                        .snapshot
                        .state_generation,
                );
            }
            state.partitions.insert(
                partition_key.clone(),
                PartitionStateV3 {
                    snapshot: StatefulStateSnapshotV2::new(
                        self.metadata.state_schema_version,
                        self.config.initial_state_generation,
                        self.config.initial_state.clone(),
                    ),
                    previous_good: None,
                    candidate: None,
                    pending_decisions: BTreeMap::new(),
                    completed_decisions: BTreeMap::new(),
                },
            );
        }
        let existing_partitions = state.partitions.clone();
        let partition = state.partitions.get_mut(&partition_key).unwrap();
        if envelope.state_generation != partition.snapshot.state_generation {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::StateMismatch,
                "state generation does not match",
                partition.snapshot.state_generation,
            );
        }

        if let RuntimeRequestV3::Decide { .. } = &request {
            if partition.pending_decisions.contains_key(&request_id)
                || partition.completed_decisions.contains_key(&request_id)
            {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "decision id was already used",
                    partition.snapshot.state_generation,
                );
            }
            if partition.pending_decisions.len()
                >= self.config.resource_profile.max_pending_decisions as usize
                || partition.completed_decisions.len()
                    >= self.config.resource_profile.max_completed_decisions as usize
            {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "pending decision capacity is exhausted",
                    partition.snapshot.state_generation,
                );
            }
        }
        if let RuntimeRequestV3::Feedback {
            decision_id,
            generation,
            ..
        } = &request
        {
            if partition.completed_decisions.contains_key(decision_id) {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::DuplicateFeedback,
                    "feedback was already applied",
                    partition.snapshot.state_generation,
                );
            }
            let Some(entry) = partition.pending_decisions.get(decision_id) else {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "decision id is not pending",
                    partition.snapshot.state_generation,
                );
            };
            if entry.model_generation != *generation {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::IncompatibleGeneration,
                    "feedback generation is stale",
                    partition.snapshot.state_generation,
                );
            }
            if let RuntimeRequestV3::Feedback {
                selected_action_id, ..
            } = &request
                && entry.selected_action_id.as_deref() != Some(selected_action_id.as_str())
            {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::StateMismatch,
                    "feedback action does not match the recorded decision",
                    partition.snapshot.state_generation,
                );
            }
            if partition.completed_decisions.len()
                >= self.config.resource_profile.max_completed_decisions as usize
            {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "completed decision capacity is exhausted",
                    partition.snapshot.state_generation,
                );
            }
        }

        if let RuntimeRequestV3::Snapshot {} = request {
            return self.response(
                request_id,
                partition.snapshot.state_generation,
                RuntimeResponseBodyV3::Snapshot {
                    state_schema_version: partition.snapshot.state_schema_version,
                    state_checksum: partition.snapshot.checksum_sha256.clone(),
                    state: hex::encode(&partition.snapshot.state),
                },
            );
        }
        if let RuntimeRequestV3::Reset {
            expected_state_generation,
        } = request
        {
            if expected_state_generation != partition.snapshot.state_generation {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::StateMismatch,
                    "reset generation does not match",
                    partition.snapshot.state_generation,
                );
            }
            let Some(next_generation) = partition.snapshot.state_generation.checked_add(1) else {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::InvalidState,
                    "state generation overflow",
                    partition.snapshot.state_generation,
                );
            };
            partition.snapshot = StatefulStateSnapshotV2::new(
                self.metadata.state_schema_version,
                next_generation,
                self.config.initial_state.clone(),
            );
            partition.pending_decisions.clear();
            partition.completed_decisions.clear();
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
        let delayed_features = match &request {
            RuntimeRequestV3::Feedback { decision_id, .. } => partition
                .pending_decisions
                .get(decision_id)
                .and_then(|entry| entry.selected_action_features.clone()),
            _ => None,
        };
        let event_json = match serde_json::to_value(&request).and_then(|mut event| {
            if let Some(features) = delayed_features {
                event
                    .as_object_mut()
                    .expect("runtime request serializes as an object")
                    .insert(
                        "decisionContext".into(),
                        serde_json::json!({"selectedActionFeatures": features}),
                    );
            }
            serde_json::to_vec(&event)
        }) {
            Ok(bytes) if bytes.len() <= MAX_EVENT_BYTES => bytes,
            Ok(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::PayloadTooLarge,
                    "event exceeds handler limit",
                    partition.snapshot.state_generation,
                );
            }
            Err(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::InvalidJson,
                    "event could not be encoded",
                    partition.snapshot.state_generation,
                );
            }
        };

        let result =
            match self
                .handler
                .handle(&event_json, &partition.snapshot.state, deterministic_seed)
            {
                Ok(result) => result,
                Err(error) => {
                    let (code, message) = map_handler_error(error.kind());
                    let state_generation = partition.snapshot.state_generation;
                    let _ = partition;
                    state.last_error = Some(message.into());
                    return self.error_response(request_id, code, message, state_generation);
                }
            };
        let output_bytes = match serde_json::to_vec(&result.output) {
            Ok(bytes) => bytes,
            Err(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::HandlerInvalidOutput,
                    "handler output was not valid JSON",
                    partition.snapshot.state_generation,
                );
            }
        };
        if output_bytes.len() > MAX_OUTPUT_BYTES {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::HandlerOutputTooLarge,
                "handler output exceeded the size limit",
                partition.snapshot.state_generation,
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
                partition.snapshot.state_generation,
            );
        }
        if result.next_state.len() > self.config.resource_profile.max_model_state_bytes as usize {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::Internal,
                "model state resource limit exceeded",
                partition.snapshot.state_generation,
            );
        }
        let Some(next_generation) = partition.snapshot.state_generation.checked_add(1) else {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::InvalidState,
                "state generation overflow",
                partition.snapshot.state_generation,
            );
        };
        let next_snapshot = StatefulStateSnapshotV2::new(
            self.metadata.state_schema_version,
            next_generation,
            result.next_state,
        );
        let mut next_pending = partition.pending_decisions.clone();
        let mut next_completed = partition.completed_decisions.clone();
        if matches!(request, RuntimeRequestV3::Decide { .. }) {
            let selected_action_id = result
                .output
                .get("selectedActionId")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= 96)
                .map(str::to_owned);
            let entry = DecisionLedgerEntryV3 {
                decision_id: request_id.clone(),
                model_generation: self.config.model_generation,
                state_generation: next_generation,
                created_at_unix_ms: now_unix_ms,
                selected_action_features: selected_action_id
                    .as_deref()
                    .and_then(|id| selected_action_features(&request, id)),
                selected_action_id,
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
        let mut prospective_partitions = existing_partitions
            .into_iter()
            .map(|(key, partition)| {
                (
                    key,
                    PartitionRuntimeSnapshotV3 {
                        handler_snapshot: partition.snapshot,
                        pending_decisions: partition.pending_decisions,
                        completed_decisions: partition.completed_decisions,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        prospective_partitions.insert(
            partition_key.clone(),
            PartitionRuntimeSnapshotV3 {
                handler_snapshot: next_snapshot.clone(),
                pending_decisions: next_pending.clone(),
                completed_decisions: next_completed.clone(),
            },
        );
        let prospective = match StatefulRuntimeSnapshotV3::new_partitioned(prospective_partitions) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                return self.error_response(
                    request_id,
                    RuntimeErrorCodeV3::Internal,
                    "snapshot partition state is invalid",
                    partition.snapshot.state_generation,
                );
            }
        };
        if self.validate_runtime_snapshot_size(&prospective).is_err() {
            return self.error_response(
                request_id,
                RuntimeErrorCodeV3::Internal,
                "snapshot resource limit exceeded",
                partition.snapshot.state_generation,
            );
        }
        let previous_snapshot = partition.snapshot.clone();
        partition.snapshot = next_snapshot;
        partition.previous_good = Some(previous_snapshot);
        partition.pending_decisions = next_pending;
        partition.completed_decisions = next_completed;
        self.response(
            request_id,
            next_generation,
            match request {
                RuntimeRequestV3::Inspect {} => RuntimeResponseBodyV3::Inspection {
                    summary: self.inspection_summary(
                        &partition_key,
                        partition,
                        restart_count,
                        last_error,
                        result.output,
                    ),
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
        partition_key: &str,
        partition: &PartitionStateV3,
        restart_count: u64,
        last_error: Option<String>,
        handler_summary: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "runtimeVersion": self.config.runtime_identity.version,
            "protocolVersion": RUNTIME_API_VERSION_V3,
            "channel": PREVIEW_CHANNEL_V3,
            "partitionKey": partition_key,
            "modelGeneration": self.config.model_generation,
            "stateGeneration": partition.snapshot.state_generation,
            "stateSchemaVersion": partition.snapshot.state_schema_version,
            "stateChecksum": partition.snapshot.checksum_sha256,
            "pendingDecisions": partition.pending_decisions.len(),
            "completedDecisions": partition.completed_decisions.len(),
            "resourceProfile": self.config.resource_profile,
            "resourceUtilization": {
                "stateBytes": partition.snapshot.state.len(),
                "pendingDecisions": partition.pending_decisions.len(),
                "completedDecisions": partition.completed_decisions.len(),
            },
            "rollbackAvailable": partition.previous_good.is_some(),
            "candidateAvailable": partition.candidate.is_some(),
            "restartCount": restart_count,
            "health": RuntimeHealthStatusV1::Healthy,
            "lastError": last_error,
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
        let partitions = state
            .partitions
            .iter()
            .map(|(key, partition)| {
                (
                    key.clone(),
                    PartitionRuntimeSnapshotV3 {
                        handler_snapshot: partition.snapshot.clone(),
                        pending_decisions: partition.pending_decisions.clone(),
                        completed_decisions: partition.completed_decisions.clone(),
                    },
                )
            })
            .collect();
        let snapshot = StatefulRuntimeSnapshotV3::new_partitioned(partitions)?;
        let snapshot_bytes = serde_json::to_vec(&snapshot)
            .map_err(|_| StatefulHandlerErrorV2::new(StatefulHandlerErrorKindV2::Internal))?
            .len();
        let usage = RuntimeResourceUsageV1 {
            state_bytes: state
                .partitions
                .values()
                .map(|partition| partition.snapshot.state.len())
                .sum(),
            snapshot_bytes,
            pending_decisions: state
                .partitions
                .values()
                .map(|partition| partition.pending_decisions.len())
                .sum(),
            completed_decisions: state
                .partitions
                .values()
                .map(|partition| partition.completed_decisions.len())
                .sum(),
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
            state_generation: state.partitions[DEFAULT_PARTITION_KEY_V3]
                .snapshot
                .state_generation,
            state_schema_version: state.partitions[DEFAULT_PARTITION_KEY_V3]
                .snapshot
                .state_schema_version,
            observed_at_unix_ms,
            health,
            reason_codes,
            resource_usage: usage,
            rollback_available: state.partitions.values().any(|p| p.previous_good.is_some()),
            candidate_available: state.partitions.values().any(|p| p.candidate.is_some()),
            restart_count: state.restart_count,
            last_error: state.last_error.clone(),
        })
    }

    fn health_status_for_state(&self, state: &RuntimeStateV3) -> RuntimeHealthStatusV1 {
        if state.last_error.is_some() {
            return RuntimeHealthStatusV1::FailedClosed;
        }
        let profile = &self.config.resource_profile;
        if state.partitions.values().any(|partition| {
            partition.snapshot.state.len() * 10 >= profile.max_model_state_bytes as usize * 9
                || partition.pending_decisions.len() * 10
                    >= profile.max_pending_decisions as usize * 9
                || partition.completed_decisions.len() * 10
                    >= profile.max_completed_decisions as usize * 9
        }) {
            RuntimeHealthStatusV1::ResourcePressure
        } else {
            RuntimeHealthStatusV1::Healthy
        }
    }

    fn validate_runtime_snapshot_size(
        &self,
        snapshot: &StatefulRuntimeSnapshotV3,
    ) -> Result<(), StatefulHandlerErrorV2> {
        let total_state_bytes: usize = partitioned_payload(&snapshot.checksum_sha256)
            .map(|partitions| {
                partitions
                    .values()
                    .map(|partition| partition.handler_snapshot.state.len())
                    .sum()
            })
            .unwrap_or(snapshot.handler_snapshot.state.len());
        if total_state_bytes > self.config.resource_profile.max_snapshot_bytes as usize {
            return Err(StatefulHandlerErrorV2::with_detail(
                StatefulHandlerErrorKindV2::InvalidState,
                "total partition state exceeds resource limit",
            ));
        }
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
            .map(|state| {
                state.partitions[DEFAULT_PARTITION_KEY_V3]
                    .snapshot
                    .state_generation
            })
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

fn selected_action_features(
    request: &RuntimeRequestV3,
    selected_action_id: &str,
) -> Option<Vec<f64>> {
    let RuntimeRequestV3::Decide { context, .. } = request else {
        return None;
    };
    let actions = context
        .get("actions")
        .or_else(|| context.get("arms"))
        .and_then(serde_json::Value::as_array)?;
    let action = actions.iter().find(|action| {
        action.get("id").and_then(serde_json::Value::as_str) == Some(selected_action_id)
    })?;
    let features = action
        .get("features")
        .and_then(serde_json::Value::as_array)?;
    if features.is_empty() || features.len() > 32 {
        return None;
    }
    let values = features
        .iter()
        .map(serde_json::Value::as_f64)
        .collect::<Option<Vec<_>>>()?;
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
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
                    value["selectedActionId"] = serde_json::json!("safe-a");
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

    fn feedback(partition_key: &str, decision_id: &str) -> EnvelopeV3 {
        EnvelopeV3 {
            request_id: format!("{partition_key}-feedback"),
            api_version: RUNTIME_API_VERSION_V3,
            client_identity: IdentityV3 {
                name: partition_key.into(),
                version: "1".into(),
            },
            capability: Some("org.example.decide".into()),
            deadline_unix_ms: Some(200),
            feature_schema_hash: Some("ab".repeat(32)),
            model_generation: 7,
            state_generation: 1,
            payload_limit: rill_runtime_protocol::MAX_MESSAGE_BYTES as u32,
            request: RuntimeRequestV3::Feedback {
                decision_id: decision_id.into(),
                selected_action_id: "safe-a".into(),
                reward: 0.5,
                outcome_time_ms: 101,
                generation: 7,
            },
        }
    }

    #[test]
    fn partitions_isolate_state_and_delayed_feedback() {
        let engine = engine(StatefulHandlerErrorKindV2::Internal);
        let mut first = decide(0);
        first.request_id = "a-decision".into();
        first.client_identity.name = "consumer-a".into();
        let mut second = decide(0);
        second.request_id = "b-decision".into();
        second.client_identity.name = "consumer-b".into();
        assert!(matches!(
            engine.handle_at(first, 100).response,
            RuntimeResponseBodyV3::Result { .. }
        ));
        assert!(matches!(
            engine.handle_at(second, 100).response,
            RuntimeResponseBodyV3::Result { .. }
        ));
        let feedback_a = engine.handle_at(feedback("consumer-a", "a-decision"), 101);
        assert!(
            matches!(feedback_a.response, RuntimeResponseBodyV3::Result { .. }),
            "feedback A: {feedback_a:?}"
        );
        assert!(matches!(
            engine
                .handle_at(feedback("consumer-b", "b-decision"), 101)
                .response,
            RuntimeResponseBodyV3::Result { .. }
        ));
        let snapshot = engine.runtime_snapshot().unwrap();
        let partitions = partitioned_payload(&snapshot.checksum_sha256).unwrap();
        assert_eq!(partitions.len(), 3);
        assert!(
            partitions["consumer-a"]
                .completed_decisions
                .contains_key("a-decision")
        );
        assert!(
            partitions["consumer-b"]
                .completed_decisions
                .contains_key("b-decision")
        );
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
        assert_eq!(
            engine.snapshot().unwrap().state,
            br#"{"count":1,"selectedActionId":"safe-a"}"#
        );
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

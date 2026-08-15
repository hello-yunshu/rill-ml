//! `rill-pm-adapter` — the `pm-rill-shadow` protocol v1 decision host.
//!
//! This crate provides a small Unix-domain-socket decision service that
//! OpenWrt Performance Manager (PM) talks to. It is owned and released by the
//! Rill repository. It implements the independent `pm-rill-shadow` wire
//! contract (protocolVersion 1) — **NOT** the Rill Runtime IPC API v1/v2/v3.
//! The two contracts deliberately share no version semantics.
//!
//! # Responsibilities
//!
//! * Unix domain socket listener;
//! * bounded NDJSON framing (one request per newline, fail closed on
//!   oversized frames);
//! * PM contract negotiation (contract name + protocolVersion must match);
//! * `status` / `observe` / `outcome` operations;
//! * advisory-only recommendation — the adapter **never** mutates the router
//!   (no OpenWrt apply / UCI / sysctl / ethtool);
//! * decision ledger binding + strict validated-outcome rejection rules
//!   (unknown decision, duplicate, action mismatch, context mismatch,
//!   generation mismatch, expiry, non-validated, non-finite reward);
//! * context-partitioned + goal-partitioned model;
//! * bounded persistent state and restart recovery;
//! * model health reporting.
//!
//! All actual configuration changes remain owned by the PM Core.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rill_ml::decision::{
    DecisionId, DecisionLedger, DecisionLedgerConfig, DecisionOutcome, PendingDecision,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Protocol constants
// ---------------------------------------------------------------------------

/// Independent PM<->adapter contract name (distinct from Rill Runtime IPC).
pub const CONTRACT: &str = "pm-rill-shadow";
/// Version of the PM<->adapter contract.
pub const PROTOCOL_VERSION: u32 = 1;
/// Default maximum size of one newline-delimited JSON frame.
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 1 << 20;
/// Default per-request processing timeout in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 1_000;
/// Maximum request id length.
pub const MAX_REQUEST_ID_LEN: usize = 128;
/// Maximum length of bounded string fields (ids, keys, goals, ...).
pub const MAX_STRING_LEN: usize = 512;
/// Maximum serialized state file size accepted on load / written on save.
pub const MAX_STATE_FILE_BYTES: u64 = 4 << 20;
/// Maximum number of context partitions retained.
pub const MAX_PARTITIONS: usize = 256;
/// Maximum number of tracked actions per partition.
pub const MAX_ACTIONS_PER_PARTITION: usize = 64;
/// Decision validity window (milliseconds).
pub const DECISION_TTL_MS: u64 = 3_600_000;
/// Persisted state schema version.
pub const STATE_SCHEMA_VERSION: u32 = 1;
/// Default state file name inside the state directory.
pub const STATE_FILE_NAME: &str = "adapter-state.json";

/// Capabilities the adapter actually implements (not merely advertises).
pub const CAPABILITIES: [&str; 5] = [
    "context-partitioned-model",
    "goal-partition",
    "validated-outcome",
    "decision-ledger",
    "model-health",
];

// ---------------------------------------------------------------------------
// Wire protocol types (pm-rill-shadow v1)
// ---------------------------------------------------------------------------

/// A validated request dispatched by `op`.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Status {
        contract: String,
        protocol_version: u32,
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_unix_ms: Option<u64>,
    },
    Observe {
        contract: String,
        protocol_version: u32,
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_unix_ms: Option<u64>,
        device_profile: String,
        capability_hash: String,
        topology_generation: u64,
        path_id: String,
        route_identity: String,
        workload_class: serde_json::Value,
        measurement_class: String,
        #[serde(default)]
        context: Option<serde_json::Value>,
        #[serde(default)]
        integrations: Option<Vec<serde_json::Value>>,
        goal: String,
        integration_fingerprint: String,
        context_key: String,
        available_actions: Vec<AvailableAction>,
    },
    Outcome {
        contract: String,
        protocol_version: u32,
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_unix_ms: Option<u64>,
        decision_id: String,
        context_key: String,
        action_id: String,
        session_id: String,
        goal: String,
        model_generation: u64,
        validated: bool,
        reward: f64,
    },
}

/// One candidate action advertised by PM. The adapter only reads `id` plus a
/// bounded estimate; unknown extra fields are tolerated so PM can grow the
/// payload without breaking the wire contract.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableAction {
    pub id: String,
    #[serde(default)]
    pub risk: Option<String>,
}

/// Recommendation returned by `observe`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recommendation {
    pub action_id: String,
    pub confidence: f64,
    pub advisory: bool,
}

/// Body of a successful `status` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelHealth {
    pub overall: String,
    pub partitions: usize,
    pub total_samples: u64,
    pub stale_partitions: usize,
    pub max_partition_samples: u64,
    pub min_partition_samples: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Body of a successful `observe` response.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObserveBody {
    pub decision_id: String,
    pub recommendation: Recommendation,
}

/// Body of a successful `outcome` response.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeBody {
    pub accepted: bool,
}

/// Uniform response envelope. Only the fields relevant to `op` are populated.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub contract: &'static str,
    pub protocol_version: u32,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rill_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_health: Option<ModelHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<Recommendation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

impl Response {
    fn ok(request_id: &str, body: ResponseBody) -> Self {
        let mut response = Self {
            contract: CONTRACT,
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.to_owned(),
            ok: true,
            adapter_version: None,
            rill_version: None,
            state: None,
            capabilities: None,
            model_health: None,
            decision_id: None,
            recommendation: None,
            accepted: None,
            error: None,
        };
        match body {
            ResponseBody::Status {
                adapter_version,
                rill_version,
                state,
                capabilities,
                model_health,
            } => {
                response.adapter_version = Some(adapter_version);
                response.rill_version = Some(rill_version);
                response.state = Some(state);
                response.capabilities = Some(capabilities);
                response.model_health = Some(model_health);
            }
            ResponseBody::Observe {
                decision_id,
                recommendation,
            } => {
                response.decision_id = Some(decision_id);
                response.recommendation = Some(recommendation);
            }
            ResponseBody::Outcome { accepted } => {
                response.accepted = Some(accepted);
            }
        }
        response
    }

    fn error(request_id: &str, error: AdapterError) -> Self {
        Self {
            contract: CONTRACT,
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.to_owned(),
            ok: false,
            adapter_version: None,
            rill_version: None,
            state: None,
            capabilities: None,
            model_health: None,
            decision_id: None,
            recommendation: None,
            accepted: None,
            error: Some(ErrorObject {
                code: error.code().to_owned(),
                message: error.message().to_owned(),
                retryable: error.retryable(),
            }),
        }
    }
}

/// Response body variants selected by `op`.
enum ResponseBody {
    Status {
        adapter_version: String,
        rill_version: String,
        state: String,
        capabilities: Vec<String>,
        model_health: ModelHealth,
    },
    Observe {
        decision_id: String,
        recommendation: Recommendation,
    },
    Outcome {
        accepted: bool,
    },
}

/// Wire error object (fail-closed, stable code strings).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorObject {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

// ---------------------------------------------------------------------------
// Errors (fail-closed)
// ---------------------------------------------------------------------------

/// Stable fail-closed error codes. Codes are frozen for the lifetime of
/// protocol v1; new codes are additive only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCode {
    InvalidJson,
    WrongContract,
    WrongProtocolVersion,
    UnsupportedOp,
    InvalidRequestId,
    InvalidRequest,
    RequestExpired,
    RequestTimeout,
    FrameTooLarge,
    InvalidDecisionId,
    UnknownDecision,
    DuplicateFeedback,
    ActionMismatch,
    ContextMismatch,
    GenerationMismatch,
    ExpiredDecision,
    InvalidTimestamp,
    NonValidated,
    NonFiniteReward,
    LedgerCapacity,
    PartitionCapacity,
    StateLoad,
    StatePersistence,
    Internal,
}

impl ErrorCode {
    /// Stable camelCase code carried on the wire.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidJson => "invalidJson",
            Self::WrongContract => "wrongContract",
            Self::WrongProtocolVersion => "wrongProtocolVersion",
            Self::UnsupportedOp => "unsupportedOp",
            Self::InvalidRequestId => "invalidRequestId",
            Self::InvalidRequest => "invalidRequest",
            Self::RequestExpired => "requestExpired",
            Self::RequestTimeout => "requestTimeout",
            Self::FrameTooLarge => "frameTooLarge",
            Self::InvalidDecisionId => "invalidDecisionId",
            Self::UnknownDecision => "unknownDecision",
            Self::DuplicateFeedback => "duplicateFeedback",
            Self::ActionMismatch => "actionMismatch",
            Self::ContextMismatch => "contextMismatch",
            Self::GenerationMismatch => "generationMismatch",
            Self::ExpiredDecision => "expiredDecision",
            Self::InvalidTimestamp => "invalidTimestamp",
            Self::NonValidated => "nonValidated",
            Self::NonFiniteReward => "nonFiniteReward",
            Self::LedgerCapacity => "ledgerCapacity",
            Self::PartitionCapacity => "partitionCapacity",
            Self::StateLoad => "stateLoad",
            Self::StatePersistence => "statePersistence",
            Self::Internal => "internal",
        }
    }

    /// Whether retrying the same request is safe and meaningful.
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::RequestTimeout
                | Self::StatePersistence
                | Self::StateLoad
                | Self::LedgerCapacity
                | Self::PartitionCapacity
                | Self::Internal
        )
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// Adapter error carrying a stable code and a bounded message.
#[derive(Debug, Clone, Error)]
#[error("{code}: {message}")]
pub struct AdapterError {
    code: ErrorCode,
    message: String,
}

impl AdapterError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code.code()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn retryable(&self) -> bool {
        self.code.retryable()
    }
}

impl From<rill_ml::decision::DecisionLedgerError> for AdapterError {
    fn from(error: rill_ml::decision::DecisionLedgerError) -> Self {
        use rill_ml::decision::DecisionLedgerError as E;
        let code = match error {
            E::UnknownDecision => ErrorCode::UnknownDecision,
            E::DuplicateFeedback => ErrorCode::DuplicateFeedback,
            E::ActionMismatch => ErrorCode::ActionMismatch,
            E::GenerationMismatch => ErrorCode::GenerationMismatch,
            E::FeedbackExpired => ErrorCode::ExpiredDecision,
            E::FeedbackBeforeDecision => ErrorCode::InvalidTimestamp,
            E::NonFiniteReward => ErrorCode::NonFiniteReward,
            E::PendingCapacityExceeded | E::CompletedCapacityExceeded => ErrorCode::LedgerCapacity,
            _ => ErrorCode::InvalidRequest,
        };
        Self::new(code, error.to_string())
    }
}

// ---------------------------------------------------------------------------
// Partitioned model (context-partitioned + goal-partitioned)
// ---------------------------------------------------------------------------

/// Per-action running estimate within one partition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionEstimate {
    pub pulls: u64,
    pub sum: f64,
    pub mean: f64,
}

impl Default for ActionEstimate {
    fn default() -> Self {
        Self {
            pulls: 0,
            sum: 0.0,
            mean: 0.0,
        }
    }
}

/// One model partition, keyed by context + goal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Partition {
    pub goal: String,
    pub actions: BTreeMap<String, ActionEstimate>,
    pub created_ms: u64,
    pub last_update_ms: u64,
}

impl Partition {
    fn new(goal: String, now_ms: u64) -> Self {
        Self {
            goal,
            actions: BTreeMap::new(),
            created_ms: now_ms,
            last_update_ms: now_ms,
        }
    }

    fn total_samples(&self) -> u64 {
        self.actions.values().map(|estimate| estimate.pulls).sum()
    }
}

/// Build the partition key from the canonical context key and goal.
pub fn partition_key(context_key: &str, goal: &str) -> String {
    format!("{context_key}::goal={goal}")
}

/// Pick the recommended action: highest mean with a fewest-pulls tie-break so
/// unseen actions stay discoverable (advisory-only exploration).
fn recommend(available: &[AvailableAction], partition: &Partition) -> String {
    let mut best: Option<(f64, u64, &String)> = None;
    for action in available {
        let default = ActionEstimate::default();
        let estimate = partition.actions.get(&action.id).unwrap_or(&default);
        let candidate = (estimate.mean, u64::MAX - estimate.pulls, &action.id);
        if best.as_ref().is_none_or(|(mean, pulls, _)| {
            candidate.0 > *mean || (candidate.0 == *mean && candidate.1 > *pulls)
        }) {
            best = Some(candidate);
        }
    }
    best.map(|(_, _, id)| id.clone()).unwrap_or_else(|| {
        available
            .first()
            .map(|action| action.id.clone())
            .unwrap_or_default()
    })
}

fn confidence_of(partition: &Partition, action_id: &str) -> f64 {
    let total = partition.total_samples();
    if total == 0 {
        return 0.0;
    }
    let pulls = partition
        .actions
        .get(action_id)
        .map(|estimate| estimate.pulls)
        .unwrap_or(0);
    pulls as f64 / total as f64
}

// ---------------------------------------------------------------------------
// Adapter state
// ---------------------------------------------------------------------------

/// Runtime configuration derived from CLI flags.
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    pub socket: PathBuf,
    pub state_dir: PathBuf,
    pub max_message: usize,
    pub timeout_ms: u64,
}

/// Persisted state envelope (bounded JSON, restart recovery).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedState {
    schema_version: u32,
    adapter_version: String,
    model_generation: u64,
    next_decision_counter: u64,
    partitions: BTreeMap<String, Partition>,
    ledger: DecisionLedger<String, String>,
    saved_at_ms: u64,
}

/// The adapter decision host.
#[derive(Debug, Clone)]
pub struct AdapterState {
    config: AdapterConfig,
    model_generation: u64,
    next_decision_counter: u64,
    partitions: BTreeMap<String, Partition>,
    ledger: DecisionLedger<String, String>,
    started_ms: u64,
    persistence_failures: u64,
    started_fresh_reason: Option<String>,
}

impl AdapterState {
    /// Create an empty (fresh) adapter state.
    pub fn new(config: AdapterConfig) -> Result<Self, AdapterError> {
        let ledger = DecisionLedger::new(
            DecisionLedgerConfig::new(1_024, 4_096)?
                .with_value_limits(MAX_STRING_LEN, MAX_STRING_LEN)?,
        )?;
        Ok(Self {
            config,
            model_generation: 1,
            next_decision_counter: 0,
            partitions: BTreeMap::new(),
            ledger,
            started_ms: 0,
            persistence_failures: 0,
            started_fresh_reason: None,
        })
    }

    /// Load persisted state from disk, or start fresh when absent. A corrupt
    /// or oversized state file is reported (degraded) rather than silently
    /// reused, and the adapter starts fresh without blocking startup.
    pub fn from_disk(config: AdapterConfig, now_ms: u64) -> Result<Self, AdapterError> {
        let path = state_path(&config);
        if !path.exists() {
            let mut state = Self::new(config)?;
            state.started_ms = now_ms;
            return Ok(state);
        }
        let metadata = std::fs::metadata(&path).map_err(|error| {
            AdapterError::new(ErrorCode::StateLoad, format!("cannot stat state: {error}"))
        })?;
        if metadata.len() > MAX_STATE_FILE_BYTES {
            let mut state = Self::new(config)?;
            state.started_ms = now_ms;
            state.started_fresh_reason = Some("state-file-oversized".to_owned());
            return Ok(state);
        }
        let bytes = std::fs::read(&path).map_err(|error| {
            AdapterError::new(ErrorCode::StateLoad, format!("cannot read state: {error}"))
        })?;
        let persisted: PersistedState = match serde_json::from_slice(&bytes) {
            Ok(persisted) => persisted,
            Err(error) => {
                let mut state = Self::new(config)?;
                state.started_ms = now_ms;
                state.started_fresh_reason =
                    Some(format!("state-corrupt-recovered-fresh: {error}"));
                return Ok(state);
            }
        };
        if persisted.schema_version != STATE_SCHEMA_VERSION {
            let mut state = Self::new(config)?;
            state.started_ms = now_ms;
            state.started_fresh_reason = Some(format!(
                "state-schema-mismatch: {}",
                persisted.schema_version
            ));
            return Ok(state);
        }
        Ok(Self {
            config,
            model_generation: persisted.model_generation.max(1),
            next_decision_counter: persisted.next_decision_counter,
            partitions: persisted.partitions,
            ledger: persisted.ledger,
            started_ms: now_ms,
            persistence_failures: 0,
            started_fresh_reason: None,
        })
    }

    /// Path of the persisted state file.
    pub fn state_file_path(&self) -> PathBuf {
        state_path(&self.config)
    }

    /// Current model generation.
    pub fn model_generation(&self) -> u64 {
        self.model_generation
    }

    /// Current adapter version (crate version).
    pub fn adapter_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// Maximum accepted NDJSON frame size (bounded framing).
    pub fn max_message(&self) -> usize {
        self.config.max_message
    }

    // -- status -------------------------------------------------------------

    /// Compute model health and the advertised adapter `state`.
    pub fn status(&self, now_ms: u64) -> (String, ModelHealth) {
        let health = self.model_health(now_ms);
        let state = if self.persistence_failures > 0
            || self.started_fresh_reason.is_some()
            || health.overall == "degraded"
        {
            "degraded"
        } else if health.total_samples == 0 {
            "collecting"
        } else {
            "learning"
        };
        (state.to_owned(), health)
    }

    fn model_health(&self, _now_ms: u64) -> ModelHealth {
        let mut total = 0u64;
        let mut stale = 0usize;
        let mut max_samples = 0u64;
        let mut min_samples = u64::MAX;
        for partition in self.partitions.values() {
            let samples = partition.total_samples();
            total = total.saturating_add(samples);
            max_samples = max_samples.max(samples);
            min_samples = min_samples.min(samples);
            if samples == 0 {
                stale += 1;
            }
        }
        if self.partitions.is_empty() {
            min_samples = 0;
        }
        let mut note = None;
        if self.persistence_failures > 0 {
            note = Some(format!(
                "persistence-failures={}; state may not survive restart",
                self.persistence_failures
            ));
        } else if let Some(reason) = &self.started_fresh_reason {
            note = Some(format!("started-fresh: {reason}"));
        }
        let overall = if self.persistence_failures > 0 || self.started_fresh_reason.is_some() {
            "degraded"
        } else {
            "healthy"
        };
        ModelHealth {
            overall: overall.to_owned(),
            partitions: self.partitions.len(),
            total_samples: total,
            stale_partitions: stale,
            max_partition_samples: max_samples,
            min_partition_samples: if self.partitions.is_empty() {
                0
            } else {
                min_samples
            },
            note,
        }
    }

    // -- observe ------------------------------------------------------------

    /// Register a decision context, recommend an advisory action, and return a
    /// stable `decisionId` the PM must bind its outcome to.
    pub fn observe(&mut self, request: &Request, now_ms: u64) -> Result<ObserveBody, AdapterError> {
        let Request::Observe {
            context_key,
            goal,
            available_actions,
            ..
        } = request
        else {
            return Err(AdapterError::new(
                ErrorCode::InvalidRequest,
                "op is not observe",
            ));
        };

        let key = partition_key(context_key, goal);
        // Phase 1: partition setup + recommendation. The partition borrow ends
        // here so `self.ledger` / `self.persist` can be mutated without a
        // second simultaneous mutable borrow of `self`.
        let (action_id, confidence) = {
            let partition = if let Some(partition) = self.partitions.get_mut(&key) {
                partition
            } else {
                if self.partitions.len() >= MAX_PARTITIONS {
                    return Err(AdapterError::new(
                        ErrorCode::PartitionCapacity,
                        "partition capacity exhausted",
                    ));
                }
                self.partitions
                    .insert(key.clone(), Partition::new(goal.clone(), now_ms));
                self.partitions
                    .get_mut(&key)
                    .expect("partition inserted above")
            };
            for action in available_actions {
                if !partition.actions.contains_key(&action.id) {
                    if partition.actions.len() >= MAX_ACTIONS_PER_PARTITION {
                        return Err(AdapterError::new(
                            ErrorCode::PartitionCapacity,
                            "per-partition action capacity exhausted",
                        ));
                    }
                    partition
                        .actions
                        .insert(action.id.clone(), ActionEstimate::default());
                }
            }
            partition.last_update_ms = now_ms;
            let action_id = recommend(available_actions, partition);
            let confidence = confidence_of(partition, &action_id);
            (action_id, confidence)
        };

        // Phase 2: decision registration + persistence (no partition borrow held).
        let decision_id = self.next_decision_id(request.request_id(), now_ms);
        let pending = PendingDecision::new(
            decision_id,
            context_key.clone(),
            action_id.clone(),
            now_ms,
            now_ms.saturating_add(DECISION_TTL_MS),
            self.model_generation,
        );
        self.ledger.register(pending)?;
        self.persist(now_ms)?;
        Ok(ObserveBody {
            decision_id: format_decision_id(decision_id),
            recommendation: Recommendation {
                action_id,
                confidence,
                advisory: true,
            },
        })
    }

    // -- outcome ------------------------------------------------------------

    /// Apply a validated, bound outcome to the decision ledger and the
    /// partitioned model. Every mismatch is rejected and leaves state intact.
    pub fn outcome(&mut self, request: &Request, now_ms: u64) -> Result<OutcomeBody, AdapterError> {
        let Request::Outcome {
            decision_id,
            context_key,
            action_id,
            goal,
            model_generation,
            validated,
            reward,
            ..
        } = request
        else {
            return Err(AdapterError::new(
                ErrorCode::InvalidRequest,
                "op is not outcome",
            ));
        };
        if !validated {
            return Err(AdapterError::new(
                ErrorCode::NonValidated,
                "outcome is not validated",
            ));
        }
        if !reward.is_finite() {
            return Err(AdapterError::new(
                ErrorCode::NonFiniteReward,
                "reward must be finite",
            ));
        }
        let id = parse_decision_id(decision_id)?;

        if self.ledger.completed(id).is_some() {
            return Err(AdapterError::new(
                ErrorCode::DuplicateFeedback,
                "feedback already applied",
            ));
        }
        let pending = self
            .ledger
            .pending(id)
            .ok_or_else(|| AdapterError::new(ErrorCode::UnknownDecision, "unknown decision"))?;
        if pending.context != *context_key {
            return Err(AdapterError::new(
                ErrorCode::ContextMismatch,
                "context mismatch",
            ));
        }
        if pending.action != *action_id {
            return Err(AdapterError::new(
                ErrorCode::ActionMismatch,
                "action mismatch",
            ));
        }

        let outcome = DecisionOutcome {
            decision_id: id,
            action: action_id.clone(),
            reward: *reward,
            observed_at: now_ms,
            model_generation: *model_generation,
        };
        self.ledger.apply_feedback(outcome)?;

        let key = partition_key(context_key, goal);
        let partition = self.partitions.get_mut(&key).ok_or_else(|| {
            AdapterError::new(ErrorCode::Internal, "partition missing for decision")
        })?;
        let estimate = partition.actions.entry(action_id.clone()).or_default();
        estimate.pulls = estimate.pulls.saturating_add(1);
        estimate.sum += reward;
        estimate.mean = estimate.sum / estimate.pulls as f64;
        partition.last_update_ms = now_ms;

        self.persist(now_ms)?;
        Ok(OutcomeBody { accepted: true })
    }

    // -- decision ids -------------------------------------------------------

    fn next_decision_id(&mut self, request_id: &str, now_ms: u64) -> DecisionId {
        self.next_decision_counter = self.next_decision_counter.saturating_add(1);
        let mut hasher = Sha256::new();
        hasher.update(now_ms.to_le_bytes());
        hasher.update(self.next_decision_counter.to_le_bytes());
        hasher.update(request_id.as_bytes());
        let digest = hasher.finalize();
        let hi = u64::from_be_bytes(digest[0..8].try_into().expect("8 bytes"));
        let lo = u64::from_be_bytes(digest[8..16].try_into().expect("8 bytes"));
        DecisionId(((hi as u128) << 64) | (lo as u128))
    }

    // -- persistence --------------------------------------------------------

    /// Atomically persist the full adapter state (temp file + rename).
    pub fn persist(&mut self, now_ms: u64) -> Result<(), AdapterError> {
        let persisted = PersistedState {
            schema_version: STATE_SCHEMA_VERSION,
            adapter_version: self.adapter_version().to_owned(),
            model_generation: self.model_generation,
            next_decision_counter: self.next_decision_counter,
            partitions: self.partitions.clone(),
            ledger: self.ledger.clone(),
            saved_at_ms: now_ms,
        };
        let json = serde_json::to_vec(&persisted)
            .map_err(|error| AdapterError::new(ErrorCode::StatePersistence, error.to_string()))?;
        if json.len() as u64 > MAX_STATE_FILE_BYTES {
            return Err(AdapterError::new(
                ErrorCode::StatePersistence,
                "serialized state exceeds bound",
            ));
        }
        let directory = &self.config.state_dir;
        if let Err(error) = std::fs::create_dir_all(directory) {
            self.persistence_failures = self.persistence_failures.saturating_add(1);
            return Err(AdapterError::new(
                ErrorCode::StatePersistence,
                format!("cannot create state dir: {error}"),
            ));
        }
        let final_path = state_path(&self.config);
        let tmp_path = directory.join(format!("{STATE_FILE_NAME}.tmp"));
        if let Err(error) =
            std::fs::write(&tmp_path, &json).and_then(|_| std::fs::rename(&tmp_path, &final_path))
        {
            self.persistence_failures = self.persistence_failures.saturating_add(1);
            return Err(AdapterError::new(
                ErrorCode::StatePersistence,
                format!("cannot persist state: {error}"),
            ));
        }
        self.persistence_failures = 0;
        Ok(())
    }
}

fn state_path(config: &AdapterConfig) -> PathBuf {
    config.state_dir.join(STATE_FILE_NAME)
}

fn format_decision_id(id: DecisionId) -> String {
    format!("{:032x}", id.0)
}

fn parse_decision_id(value: &str) -> Result<DecisionId, AdapterError> {
    if value.is_empty() || value.len() > 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AdapterError::new(
            ErrorCode::InvalidDecisionId,
            "invalid decision id",
        ));
    }
    let parsed = u128::from_str_radix(value, 16)
        .map_err(|_| AdapterError::new(ErrorCode::InvalidDecisionId, "invalid decision id"))?;
    Ok(DecisionId(parsed))
}

// ---------------------------------------------------------------------------
// Request parsing / validation (fail-closed)
// ---------------------------------------------------------------------------

/// Extract the request id from a raw JSON object for error envelopes.
fn raw_request_id(raw: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| {
            value
                .get("requestId")
                .and_then(|id| id.as_str())
                .map(str::to_owned)
        })
        .filter(|id| !id.is_empty() && id.len() <= MAX_REQUEST_ID_LEN)
        .unwrap_or_default()
}

/// Strictly parse and validate one request frame.
pub fn parse_request(raw: &[u8]) -> Result<Request, (String, AdapterError)> {
    let request_id = raw_request_id(raw);
    let request: Request = serde_json::from_slice(raw).map_err(|error| {
        let code = if serde_json::from_slice::<serde_json::Value>(raw).is_err() {
            ErrorCode::InvalidJson
        } else {
            ErrorCode::InvalidRequest
        };
        (
            request_id.clone(),
            AdapterError::new(code, format!("cannot parse request: {error}")),
        )
    })?;
    if let Err(error) = validate_request(&request) {
        return Err((request_id, error));
    }
    Ok(request)
}

fn validate_request(request: &Request) -> Result<(), AdapterError> {
    if request.contract() != CONTRACT {
        return Err(AdapterError::new(
            ErrorCode::WrongContract,
            "unknown contract",
        ));
    }
    if request.protocol_version() != PROTOCOL_VERSION {
        return Err(AdapterError::new(
            ErrorCode::WrongProtocolVersion,
            "unsupported protocol version",
        ));
    }
    let request_id = request.request_id();
    if request_id.is_empty() || request_id.len() > MAX_REQUEST_ID_LEN {
        return Err(AdapterError::new(
            ErrorCode::InvalidRequestId,
            "invalid request id",
        ));
    }
    match request {
        Request::Status { .. } => Ok(()),
        Request::Observe {
            device_profile,
            path_id,
            goal,
            context_key,
            available_actions,
            workload_class,
            context,
            ..
        } => {
            validate_bounded("deviceProfile", device_profile)?;
            validate_bounded("pathId", path_id)?;
            validate_bounded("goal", goal)?;
            validate_bounded("contextKey", context_key)?;
            if available_actions.is_empty() {
                return Err(AdapterError::new(
                    ErrorCode::InvalidRequest,
                    "availableActions must not be empty",
                ));
            }
            if available_actions
                .iter()
                .any(|action| action.id.is_empty() || action.id.len() > MAX_STRING_LEN)
            {
                return Err(AdapterError::new(
                    ErrorCode::InvalidRequest,
                    "invalid action id",
                ));
            }
            let workload_bytes = serde_json::to_vec(workload_class)
                .map_err(|error| AdapterError::new(ErrorCode::InvalidRequest, error.to_string()))?
                .len();
            if workload_bytes > MAX_STRING_LEN * 4 {
                return Err(AdapterError::new(
                    ErrorCode::InvalidRequest,
                    "workloadClass too large",
                ));
            }
            if let Some(context) = context {
                let size = serde_json::to_vec(context)
                    .map_err(|error| {
                        AdapterError::new(ErrorCode::InvalidRequest, error.to_string())
                    })?
                    .len();
                if size > MAX_STRING_LEN * 512 {
                    return Err(AdapterError::new(
                        ErrorCode::InvalidRequest,
                        "context too large",
                    ));
                }
            }
            Ok(())
        }
        Request::Outcome {
            decision_id,
            context_key,
            action_id,
            session_id,
            goal,
            model_generation,
            validated,
            reward,
            ..
        } => {
            validate_bounded("decisionId", decision_id)?;
            validate_bounded("contextKey", context_key)?;
            validate_bounded("actionId", action_id)?;
            validate_bounded("sessionId", session_id)?;
            validate_bounded("goal", goal)?;
            if *model_generation == 0 {
                return Err(AdapterError::new(
                    ErrorCode::InvalidRequest,
                    "invalid modelGeneration",
                ));
            }
            if !validated {
                return Err(AdapterError::new(
                    ErrorCode::NonValidated,
                    "outcome must be validated",
                ));
            }
            if !reward.is_finite() {
                return Err(AdapterError::new(
                    ErrorCode::NonFiniteReward,
                    "reward must be finite",
                ));
            }
            Ok(())
        }
    }
}

fn validate_bounded(field: &str, value: &str) -> Result<(), AdapterError> {
    if value.is_empty() || value.len() > MAX_STRING_LEN {
        return Err(AdapterError::new(
            ErrorCode::InvalidRequest,
            format!("{field} must be non-empty and bounded"),
        ));
    }
    Ok(())
}

impl Request {
    pub fn contract(&self) -> &str {
        match self {
            Self::Status { contract, .. }
            | Self::Observe { contract, .. }
            | Self::Outcome { contract, .. } => contract,
        }
    }

    pub fn protocol_version(&self) -> u32 {
        match self {
            Self::Status {
                protocol_version, ..
            }
            | Self::Observe {
                protocol_version, ..
            }
            | Self::Outcome {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::Status { request_id, .. }
            | Self::Observe { request_id, .. }
            | Self::Outcome { request_id, .. } => request_id,
        }
    }

    /// Whether the request deadline has elapsed at `now_ms`.
    pub fn is_expired_at(&self, now_ms: u64) -> bool {
        let deadline = match self {
            Self::Status {
                deadline_unix_ms, ..
            }
            | Self::Observe {
                deadline_unix_ms, ..
            }
            | Self::Outcome {
                deadline_unix_ms, ..
            } => *deadline_unix_ms,
        };
        deadline.is_some_and(|deadline| now_ms > deadline)
    }
}

// ---------------------------------------------------------------------------
// Request dispatch
// ---------------------------------------------------------------------------

/// Dispatch one validated request against adapter state, returning a response
/// envelope (not including the trailing newline).
pub fn handle_request(state: &mut AdapterState, raw: &[u8], now_ms: u64) -> Vec<u8> {
    let (request_id, parsed) = match parse_request(raw) {
        Ok(request) => (request.request_id().to_owned(), Ok(request)),
        Err((request_id, error)) => (request_id, Err(error)),
    };
    let response = match parsed {
        Err(error) => Response::error(&request_id, error),
        Ok(request) => {
            if request.is_expired_at(now_ms) {
                Response::error(
                    &request_id,
                    AdapterError::new(ErrorCode::RequestExpired, "request deadline elapsed"),
                )
            } else {
                let started = std::time::Instant::now();
                let timeout = std::time::Duration::from_millis(state.config.timeout_ms);
                let body = match &request {
                    Request::Status { .. } => {
                        let (state_name, health) = state.status(now_ms);
                        Ok(ResponseBody::Status {
                            adapter_version: state.adapter_version().to_owned(),
                            rill_version: rill_ml::RILL_VERSION.to_owned(),
                            state: state_name,
                            capabilities: CAPABILITIES.iter().map(|c| (*c).to_owned()).collect(),
                            model_health: health,
                        })
                    }
                    Request::Observe { .. } => {
                        state
                            .observe(&request, now_ms)
                            .map(|body| ResponseBody::Observe {
                                decision_id: body.decision_id,
                                recommendation: body.recommendation,
                            })
                    }
                    Request::Outcome { .. } => {
                        state
                            .outcome(&request, now_ms)
                            .map(|body| ResponseBody::Outcome {
                                accepted: body.accepted,
                            })
                    }
                };
                match body {
                    Ok(body) => Response::ok(&request_id, body),
                    Err(_error) if started.elapsed() >= timeout => Response::error(
                        &request_id,
                        AdapterError::new(ErrorCode::RequestTimeout, "request processing timeout"),
                    ),
                    Err(error) => Response::error(&request_id, error),
                }
            }
        }
    };
    serde_json::to_vec(&response).unwrap_or_else(|_| {
        serde_json::to_vec(&Response::error(
            "",
            AdapterError::new(ErrorCode::Internal, "response serialization failed"),
        ))
        .unwrap_or_default()
    })
}

// ---------------------------------------------------------------------------
// Bounded NDJSON framing over a byte stream
// ---------------------------------------------------------------------------

/// Outcome of reading one newline-delimited frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    /// A complete frame not exceeding `max` bytes (trailing `\n` removed).
    Line(Vec<u8>),
    /// A frame exceeded `max` bytes; the connection must be closed.
    TooLarge,
    /// End of stream before any byte was read.
    Eof,
}

/// Read exactly one newline-delimited frame with a hard `max` bound.
pub fn read_frame<R: std::io::Read>(reader: &mut R, max: usize) -> std::io::Result<Frame> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let count = reader.read(&mut byte)?;
        if count == 0 {
            return Ok(if line.is_empty() {
                Frame::Eof
            } else {
                Frame::Line(line)
            });
        }
        if byte[0] == b'\n' {
            return Ok(Frame::Line(line));
        }
        if byte[0] != b'\r' {
            line.push(byte[0]);
        }
        if line.len() > max {
            return Ok(Frame::TooLarge);
        }
    }
}

/// Write a frame followed by a newline.
pub fn write_frame<W: std::io::Write>(writer: &mut W, body: &[u8]) -> std::io::Result<()> {
    writer.write_all(body)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DIR_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn config() -> AdapterConfig {
        // Each test uses a unique state dir so concurrent `persist()` calls
        // (shared fixed tmp filename) cannot race across tests.
        let n = TEST_DIR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        AdapterConfig {
            socket: PathBuf::from(format!("/tmp/pm-shadow-{}-{n}.sock", std::process::id())),
            state_dir: PathBuf::from(format!("/tmp/pm-shadow-state-{}-{n}", std::process::id())),
            max_message: DEFAULT_MAX_MESSAGE_BYTES,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    fn status_request(request_id: &str) -> serde_json::Value {
        serde_json::json!({
            "contract": CONTRACT,
            "protocolVersion": PROTOCOL_VERSION,
            "requestId": request_id,
            "op": "status"
        })
    }

    fn observe_request(request_id: &str) -> serde_json::Value {
        serde_json::json!({
            "contract": CONTRACT,
            "protocolVersion": PROTOCOL_VERSION,
            "requestId": request_id,
            "op": "observe",
            "deviceProfile": "recommended",
            "capabilityHash": "cap-hash",
            "topologyGeneration": 1,
            "pathId": "path:lan-to-wan",
            "routeIdentity": "unresolved",
            "workloadClass": ["plain_forwarding"],
            "measurementClass": "passive_before_after",
            "goal": "balanced",
            "integrationFingerprint": "integ-fp",
            "contextKey": "ctx-v1:goal=balanced",
            "availableActions": [
                {"id": "fastpath-software", "risk": "benchmark"},
                {"id": "ring-rx-tx", "risk": "benchmark"}
            ]
        })
    }

    fn outcome_json(decision_id: &str) -> serde_json::Value {
        serde_json::json!({
            "contract": CONTRACT,
            "protocolVersion": PROTOCOL_VERSION,
            "requestId": "outcome-test",
            "op": "outcome",
            "decisionId": decision_id,
            "contextKey": "ctx-v1:goal=balanced",
            "actionId": "fastpath-software",
            "sessionId": "session-1",
            "goal": "balanced",
            "modelGeneration": 1,
            "validated": true,
            "reward": 0.7
        })
    }

    fn fresh_state() -> AdapterState {
        AdapterState::new(config()).unwrap()
    }

    #[test]
    fn status_round_trip_reports_capabilities() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&status_request("status-1")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["contract"], CONTRACT);
        assert_eq!(response["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["requestId"], "status-1");
        assert_eq!(response["state"], "collecting");
        let caps: Vec<&str> = response["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        for capability in CAPABILITIES {
            assert!(
                caps.contains(&capability),
                "missing capability {capability}"
            );
        }
    }

    #[test]
    fn observe_then_validated_outcome_round_trip() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-1")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], true);
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let action_id = response["recommendation"]["actionId"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(response["recommendation"]["advisory"], true);
        assert!(!decision_id.is_empty());
        assert!(["fastpath-software", "ring-rx-tx"].contains(&action_id.as_str()));

        let outcome = outcome_json(&decision_id);
        let body = handle_request(&mut state, &serde_json::to_vec(&outcome).unwrap(), 20_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["accepted"], true);

        // Now the partition has a sample: state becomes learning.
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&status_request("status-2")).unwrap(),
            21_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["state"], "learning");
    }

    #[test]
    fn wrong_contract_fails_closed() {
        let mut state = fresh_state();
        let mut value = status_request("status-x");
        value["contract"] = serde_json::json!("other-contract");
        let body = handle_request(&mut state, &serde_json::to_vec(&value).unwrap(), 10_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "wrongContract");
    }

    #[test]
    fn wrong_protocol_version_fails_closed() {
        let mut state = fresh_state();
        let mut value = status_request("status-x");
        value["protocolVersion"] = serde_json::json!(2);
        let body = handle_request(&mut state, &serde_json::to_vec(&value).unwrap(), 10_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "wrongProtocolVersion");
    }

    #[test]
    fn unknown_op_fails_closed() {
        let mut state = fresh_state();
        let value = serde_json::json!({
            "contract": CONTRACT,
            "protocolVersion": PROTOCOL_VERSION,
            "requestId": "status-x",
            "op": "explode"
        });
        let body = handle_request(&mut state, &serde_json::to_vec(&value).unwrap(), 10_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
    }

    #[test]
    fn invalid_json_fails_closed() {
        let mut state = fresh_state();
        let body = handle_request(&mut state, b"{not-json", 10_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "invalidJson");
    }

    #[test]
    fn unknown_decision_outcome_fails_closed() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&outcome_json("deadbeef")).unwrap(),
            20_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "unknownDecision");
    }

    #[test]
    fn duplicate_outcome_fails_closed() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-dup")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let outcome = outcome_json(&decision_id);
        let raw = serde_json::to_vec(&outcome).unwrap();
        let first = handle_request(&mut state, &raw, 20_000);
        let first: serde_json::Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(first["ok"], true);
        let second = handle_request(&mut state, &raw, 21_000);
        let second: serde_json::Value = serde_json::from_slice(&second).unwrap();
        assert_eq!(second["ok"], false);
        assert_eq!(second["error"]["code"], "duplicateFeedback");
    }

    #[test]
    fn action_mismatch_outcome_fails_closed() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-am")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let mut outcome = outcome_json(&decision_id);
        outcome["actionId"] = serde_json::json!("other-action");
        let body = handle_request(&mut state, &serde_json::to_vec(&outcome).unwrap(), 20_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "actionMismatch");
    }

    #[test]
    fn context_mismatch_outcome_fails_closed() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-cm")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let mut outcome = outcome_json(&decision_id);
        outcome["contextKey"] = serde_json::json!("ctx-v1:goal=other");
        let body = handle_request(&mut state, &serde_json::to_vec(&outcome).unwrap(), 20_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "contextMismatch");
    }

    #[test]
    fn generation_mismatch_outcome_fails_closed() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-gm")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let mut outcome = outcome_json(&decision_id);
        outcome["modelGeneration"] = serde_json::json!(99);
        let body = handle_request(&mut state, &serde_json::to_vec(&outcome).unwrap(), 20_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "generationMismatch");
    }

    #[test]
    fn non_validated_outcome_fails_closed() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-nv")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let mut outcome = outcome_json(&decision_id);
        outcome["validated"] = serde_json::json!(false);
        let body = handle_request(&mut state, &serde_json::to_vec(&outcome).unwrap(), 20_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "nonValidated");
    }

    #[test]
    fn non_finite_reward_outcome_fails_closed() {
        // JSON cannot carry NaN, so the non-finite guard is exercised through
        // the typed request path (defense-in-depth for in-memory rewards).
        let mut state = fresh_state();
        let request = Request::Outcome {
            contract: CONTRACT.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            request_id: "outcome-nf".to_owned(),
            deadline_unix_ms: None,
            decision_id: "deadbeef".to_owned(),
            context_key: "ctx-v1:goal=balanced".to_owned(),
            action_id: "fastpath-software".to_owned(),
            session_id: "session-1".to_owned(),
            goal: "balanced".to_owned(),
            model_generation: 1,
            validated: true,
            reward: f64::NAN,
        };
        let error = state.outcome(&request, 20_000).unwrap_err();
        assert_eq!(error.code(), "nonFiniteReward");
    }

    #[test]
    fn expiry_prevents_outcome() {
        let mut state = fresh_state();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-exp")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let outcome = outcome_json(&decision_id);
        let raw = serde_json::to_vec(&outcome).unwrap();
        let body = handle_request(&mut state, &raw, 10_000 + DECISION_TTL_MS + 1);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "expiredDecision");
    }

    #[test]
    fn persistence_round_trip_recovers_state() {
        let dir = std::env::temp_dir().join(format!("pm-shadow-state-{}", std::process::id()));
        let cfg = AdapterConfig {
            state_dir: dir.clone(),
            ..config()
        };
        let mut state = AdapterState::new(cfg.clone()).unwrap();
        let body = handle_request(
            &mut state,
            &serde_json::to_vec(&observe_request("obs-persist")).unwrap(),
            10_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = response["decisionId"].as_str().unwrap().to_owned();
        let outcome = outcome_json(&decision_id);
        let body = handle_request(&mut state, &serde_json::to_vec(&outcome).unwrap(), 20_000);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["ok"], true);

        let mut reloaded = AdapterState::from_disk(cfg.clone(), 30_000).unwrap();
        assert!(reloaded.state_file_path().exists());
        let body = handle_request(
            &mut reloaded,
            &serde_json::to_vec(&status_request("status-reload")).unwrap(),
            31_000,
        );
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["state"], "learning");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decision_id_hex_round_trip() {
        let id = DecisionId(0xdead_beef_u128);
        let hex = format_decision_id(id);
        assert_eq!(parse_decision_id(&hex).unwrap(), id);
        assert!(parse_decision_id("zzz").is_err());
        assert!(parse_decision_id("").is_err());
    }

    #[test]
    fn oversized_frame_detected() {
        let mut cursor = std::io::Cursor::new(b"123456789\n".to_vec());
        assert_eq!(read_frame(&mut cursor, 5).unwrap(), Frame::TooLarge);
        let mut cursor = std::io::Cursor::new(b"12345\n".to_vec());
        assert_eq!(
            read_frame(&mut cursor, 5).unwrap(),
            Frame::Line(b"12345".to_vec())
        );
        let mut cursor = std::io::Cursor::new(Vec::new());
        assert_eq!(read_frame(&mut cursor, 5).unwrap(), Frame::Eof);
    }
}

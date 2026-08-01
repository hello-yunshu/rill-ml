//! Bounded, caller-clocked primitives for decisions with delayed feedback.
//!
//! The ledger intentionally knows nothing about product actions or reward
//! meaning. Callers provide identifiers, timestamps, generations, contexts,
//! and actions. Pending decisions are never evicted implicitly.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::RillError;
#[cfg(feature = "serde")]
use crate::ValidateState;

/// Version of the serialized ledger state introduced with the Preview API.
pub const DECISION_LEDGER_STATE_VERSION: u32 = 1;
/// Default maximum serialized-equivalent context size per decision.
pub const DEFAULT_MAX_CONTEXT_BYTES: usize = 1024 * 1024;
/// Default maximum serialized-equivalent action size per decision.
pub const DEFAULT_MAX_ACTION_BYTES: usize = 64 * 1024;

/// Caller-extensible validation hook that makes per-decision memory bounded.
pub trait BoundedDecisionValue {
    /// Validate content and reject an estimated representation above `max_bytes`.
    fn validate_bounded(&self, max_bytes: usize) -> Result<(), DecisionLedgerError>;
}

impl BoundedDecisionValue for Vec<f64> {
    fn validate_bounded(&self, max_bytes: usize) -> Result<(), DecisionLedgerError> {
        if self
            .len()
            .checked_mul(std::mem::size_of::<f64>())
            .is_none_or(|size| size > max_bytes)
        {
            return Err(DecisionLedgerError::ValueTooLarge);
        }
        if self.iter().any(|value| !value.is_finite()) {
            return Err(DecisionLedgerError::InvalidValue);
        }
        Ok(())
    }
}

impl BoundedDecisionValue for Vec<u8> {
    fn validate_bounded(&self, max_bytes: usize) -> Result<(), DecisionLedgerError> {
        if self.len() > max_bytes {
            return Err(DecisionLedgerError::ValueTooLarge);
        }
        Ok(())
    }
}

impl BoundedDecisionValue for String {
    fn validate_bounded(&self, max_bytes: usize) -> Result<(), DecisionLedgerError> {
        if self.len() > max_bytes {
            return Err(DecisionLedgerError::ValueTooLarge);
        }
        Ok(())
    }
}

macro_rules! impl_fixed_decision_value {
    ($($type:ty),+ $(,)?) => {
        $(impl BoundedDecisionValue for $type {
            fn validate_bounded(&self, max_bytes: usize) -> Result<(), DecisionLedgerError> {
                if std::mem::size_of::<$type>() > max_bytes {
                    return Err(DecisionLedgerError::ValueTooLarge);
                }
                Ok(())
            }
        })+
    };
}

impl_fixed_decision_value!((), bool, usize, u64, u32, u16, u8, i64, i32, i16, i8);

/// Opaque, caller-assigned decision identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DecisionId(pub u128);

/// Capacity limits for a [`DecisionLedger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[non_exhaustive]
pub struct DecisionLedgerConfig {
    /// Maximum number of incomplete decisions. They are never auto-evicted.
    pub max_pending: usize,
    /// Maximum number of completed tombstones. They are never auto-evicted.
    pub max_completed: usize,
    /// Maximum bounded-size estimate for one context.
    pub max_context_bytes: usize,
    /// Maximum bounded-size estimate for one action.
    pub max_action_bytes: usize,
}

impl DecisionLedgerConfig {
    /// Build explicit pending/completed bounds.
    pub fn new(max_pending: usize, max_completed: usize) -> Result<Self, DecisionLedgerError> {
        if max_pending == 0 || max_completed == 0 {
            return Err(DecisionLedgerError::InvalidCapacity);
        }
        Ok(Self {
            max_pending,
            max_completed,
            max_context_bytes: DEFAULT_MAX_CONTEXT_BYTES,
            max_action_bytes: DEFAULT_MAX_ACTION_BYTES,
        })
    }

    /// Override per-entry context/action byte bounds.
    pub fn with_value_limits(
        mut self,
        max_context_bytes: usize,
        max_action_bytes: usize,
    ) -> Result<Self, DecisionLedgerError> {
        if max_context_bytes == 0 || max_action_bytes == 0 {
            return Err(DecisionLedgerError::InvalidCapacity);
        }
        self.max_context_bytes = max_context_bytes;
        self.max_action_bytes = max_action_bytes;
        Ok(self)
    }
}

impl Default for DecisionLedgerConfig {
    fn default() -> Self {
        Self {
            max_pending: 1_024,
            max_completed: 4_096,
            max_context_bytes: DEFAULT_MAX_CONTEXT_BYTES,
            max_action_bytes: DEFAULT_MAX_ACTION_BYTES,
        }
    }
}

/// Immutable facts recorded at decision time.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PendingDecision<C, A> {
    /// Unique identifier.
    pub id: DecisionId,
    /// Context presented to the decision policy.
    pub context: C,
    /// Selected action.
    pub action: A,
    /// Caller-supplied decision timestamp.
    pub created_at: u64,
    /// Last timestamp at which feedback is accepted, inclusively.
    pub expires_at: u64,
    /// Model generation that produced the action.
    pub model_generation: u64,
}

impl<C, A> PendingDecision<C, A> {
    /// Construct immutable decision facts.
    pub const fn new(
        id: DecisionId,
        context: C,
        action: A,
        created_at: u64,
        expires_at: u64,
        model_generation: u64,
    ) -> Self {
        Self {
            id,
            context,
            action,
            created_at,
            expires_at,
            model_generation,
        }
    }
}

/// Delayed outcome submitted by a caller.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct DecisionOutcome<A> {
    /// Decision being completed.
    pub decision_id: DecisionId,
    /// Action the outcome belongs to.
    pub action: A,
    /// Caller-defined finite reward.
    pub reward: f64,
    /// Caller-supplied outcome timestamp.
    pub observed_at: u64,
    /// Generation expected by the outcome producer.
    pub model_generation: u64,
}

/// Completed decision retained as a bounded replay tombstone.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct CompletedDecision<C, A> {
    /// Original immutable decision facts.
    pub decision: PendingDecision<C, A>,
    /// Accepted reward.
    pub reward: f64,
    /// Accepted outcome timestamp.
    pub observed_at: u64,
}

/// Result of registering a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionReceipt {
    /// Registered or replayed identifier.
    pub id: DecisionId,
    /// Whether this call inserted new pending state.
    pub status: RegistrationStatus,
}

/// Registration/idempotency status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationStatus {
    /// Newly inserted pending decision.
    Registered,
    /// Exact replay of an existing pending decision.
    PendingReplay,
    /// Exact replay of a decision already represented by a tombstone.
    CompletedReplay,
}

/// Result of accepted feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackStatus {
    /// Feedback was accepted and the decision moved to completed state.
    Applied,
}

/// Validation and capacity failures. Every failure leaves the ledger intact.
#[derive(Debug, Error, Clone, PartialEq)]
#[non_exhaustive]
pub enum DecisionLedgerError {
    /// A configured capacity was zero.
    #[error("decision ledger capacities must be greater than zero")]
    InvalidCapacity,
    /// Decision expiry preceded creation.
    #[error("decision expiry precedes its creation time")]
    InvalidTimeRange,
    /// A reward was NaN or infinite.
    #[error("decision reward must be finite")]
    NonFiniteReward,
    /// Pending storage is full; live decisions are not evicted implicitly.
    #[error("pending decision capacity is exhausted")]
    PendingCapacityExceeded,
    /// Tombstone storage is full; completed entries require explicit cleanup.
    #[error("completed decision capacity is exhausted")]
    CompletedCapacityExceeded,
    /// A context or action exceeds the configured byte estimate.
    #[error("decision context or action exceeds its configured byte limit")]
    ValueTooLarge,
    /// A context or action violates its type-specific semantic invariants.
    #[error("decision context or action is invalid")]
    InvalidValue,
    /// The identifier exists with different immutable facts.
    #[error("decision id already exists with different facts")]
    DecisionConflict,
    /// Feedback references no pending or completed identifier.
    #[error("feedback references an unknown decision")]
    UnknownDecision,
    /// Feedback was already accepted for this identifier.
    #[error("feedback was already applied")]
    DuplicateFeedback,
    /// Outcome timestamp precedes decision creation.
    #[error("feedback predates the decision")]
    FeedbackBeforeDecision,
    /// Outcome timestamp is after the inclusive expiry boundary.
    #[error("feedback arrived after decision expiry")]
    FeedbackExpired,
    /// Outcome generation differs from the recorded generation.
    #[error("feedback model generation does not match the decision")]
    GenerationMismatch,
    /// Outcome action differs from the selected action.
    #[error("feedback action does not match the decision")]
    ActionMismatch,
}

/// Bounded pending decisions plus bounded completed tombstones.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct DecisionLedger<C, A> {
    state_version: u32,
    config: DecisionLedgerConfig,
    pending: BTreeMap<DecisionId, PendingDecision<C, A>>,
    completed: BTreeMap<DecisionId, CompletedDecision<C, A>>,
}

impl<C, A> DecisionLedger<C, A>
where
    C: PartialEq + BoundedDecisionValue,
    A: PartialEq + BoundedDecisionValue,
{
    /// Create an empty ledger with explicit capacity bounds.
    pub fn new(config: DecisionLedgerConfig) -> Result<Self, DecisionLedgerError> {
        if config.max_pending == 0
            || config.max_completed == 0
            || config.max_context_bytes == 0
            || config.max_action_bytes == 0
        {
            return Err(DecisionLedgerError::InvalidCapacity);
        }
        Ok(Self {
            state_version: DECISION_LEDGER_STATE_VERSION,
            config,
            pending: BTreeMap::new(),
            completed: BTreeMap::new(),
        })
    }

    /// Register immutable decision facts, accepting exact idempotent replay.
    pub fn register(
        &mut self,
        decision: PendingDecision<C, A>,
    ) -> Result<DecisionReceipt, DecisionLedgerError> {
        if decision.expires_at < decision.created_at {
            return Err(DecisionLedgerError::InvalidTimeRange);
        }
        decision
            .context
            .validate_bounded(self.config.max_context_bytes)?;
        decision
            .action
            .validate_bounded(self.config.max_action_bytes)?;
        if let Some(existing) = self.pending.get(&decision.id) {
            return if existing == &decision {
                Ok(DecisionReceipt {
                    id: decision.id,
                    status: RegistrationStatus::PendingReplay,
                })
            } else {
                Err(DecisionLedgerError::DecisionConflict)
            };
        }
        if let Some(existing) = self.completed.get(&decision.id) {
            return if existing.decision == decision {
                Ok(DecisionReceipt {
                    id: decision.id,
                    status: RegistrationStatus::CompletedReplay,
                })
            } else {
                Err(DecisionLedgerError::DecisionConflict)
            };
        }
        if self.pending.len() >= self.config.max_pending {
            return Err(DecisionLedgerError::PendingCapacityExceeded);
        }
        let id = decision.id;
        self.pending.insert(id, decision);
        Ok(DecisionReceipt {
            id,
            status: RegistrationStatus::Registered,
        })
    }

    /// Validate and apply delayed feedback transactionally.
    pub fn apply_feedback(
        &mut self,
        outcome: DecisionOutcome<A>,
    ) -> Result<FeedbackStatus, DecisionLedgerError> {
        if !outcome.reward.is_finite() {
            return Err(DecisionLedgerError::NonFiniteReward);
        }
        outcome
            .action
            .validate_bounded(self.config.max_action_bytes)?;
        if self.completed.contains_key(&outcome.decision_id) {
            return Err(DecisionLedgerError::DuplicateFeedback);
        }
        let pending = self
            .pending
            .get(&outcome.decision_id)
            .ok_or(DecisionLedgerError::UnknownDecision)?;
        if outcome.observed_at < pending.created_at {
            return Err(DecisionLedgerError::FeedbackBeforeDecision);
        }
        if outcome.observed_at > pending.expires_at {
            return Err(DecisionLedgerError::FeedbackExpired);
        }
        if outcome.model_generation != pending.model_generation {
            return Err(DecisionLedgerError::GenerationMismatch);
        }
        if outcome.action != pending.action {
            return Err(DecisionLedgerError::ActionMismatch);
        }
        if self.completed.len() >= self.config.max_completed {
            return Err(DecisionLedgerError::CompletedCapacityExceeded);
        }

        // Every fallible check is above this point. Moving the pending entry is
        // therefore an all-or-nothing state transition.
        let decision = self
            .pending
            .remove(&outcome.decision_id)
            .ok_or(DecisionLedgerError::UnknownDecision)?;
        self.completed.insert(
            outcome.decision_id,
            CompletedDecision {
                decision,
                reward: outcome.reward,
                observed_at: outcome.observed_at,
            },
        );
        Ok(FeedbackStatus::Applied)
    }

    /// Return pending decision facts without updating state.
    pub fn pending(&self, id: DecisionId) -> Option<&PendingDecision<C, A>> {
        self.pending.get(&id)
    }

    /// Return a completed tombstone without updating state.
    pub fn completed(&self, id: DecisionId) -> Option<&CompletedDecision<C, A>> {
        self.completed.get(&id)
    }

    /// Number of incomplete decisions.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Number of retained completed tombstones.
    pub fn completed_len(&self) -> usize {
        self.completed.len()
    }

    /// Validate all feature-independent structural and bounded-value invariants.
    ///
    /// This is available even when the optional `serde` feature is disabled;
    /// [`ValidateState`] delegates to the same checks when serde persistence is
    /// enabled.
    pub fn validate(&self) -> Result<(), RillError> {
        self.validate_structure()
    }

    /// Explicitly clear decisions whose expiry is strictly before `now`.
    pub fn clear_expired(&mut self, now: u64) -> Vec<DecisionId> {
        let ids = self
            .pending
            .iter()
            .filter_map(|(&id, decision)| (decision.expires_at < now).then_some(id))
            .collect::<Vec<_>>();
        for id in &ids {
            self.pending.remove(id);
        }
        ids
    }

    /// Explicitly clear tombstones observed strictly before `before`.
    pub fn clear_completed_before(&mut self, before: u64) -> Vec<DecisionId> {
        let ids = self
            .completed
            .iter()
            .filter_map(|(&id, decision)| (decision.observed_at < before).then_some(id))
            .collect::<Vec<_>>();
        for id in &ids {
            self.completed.remove(id);
        }
        ids
    }

    fn validate_structure(&self) -> Result<(), RillError> {
        if self.state_version != DECISION_LEDGER_STATE_VERSION {
            return Err(RillError::IncompatibleStateVersion {
                expected: DECISION_LEDGER_STATE_VERSION,
                actual: self.state_version,
            });
        }
        if self.config.max_pending == 0
            || self.config.max_completed == 0
            || self.config.max_context_bytes == 0
            || self.config.max_action_bytes == 0
        {
            return Err(RillError::InvalidState(
                "decision ledger capacity must be non-zero".to_owned(),
            ));
        }
        if self.pending.len() > self.config.max_pending
            || self.completed.len() > self.config.max_completed
        {
            return Err(RillError::InvalidState(
                "decision ledger exceeds its configured capacity".to_owned(),
            ));
        }
        for (id, decision) in &self.pending {
            if id != &decision.id || decision.expires_at < decision.created_at {
                return Err(RillError::InvalidState(
                    "pending decision has inconsistent id or timestamps".to_owned(),
                ));
            }
            if self.completed.contains_key(id) {
                return Err(RillError::InvalidState(
                    "decision exists in pending and completed sets".to_owned(),
                ));
            }
            decision
                .context
                .validate_bounded(self.config.max_context_bytes)
                .map_err(|error| RillError::InvalidState(error.to_string()))?;
            decision
                .action
                .validate_bounded(self.config.max_action_bytes)
                .map_err(|error| RillError::InvalidState(error.to_string()))?;
        }
        for (id, completed) in &self.completed {
            let decision = &completed.decision;
            if id != &decision.id
                || decision.expires_at < decision.created_at
                || completed.observed_at < decision.created_at
                || completed.observed_at > decision.expires_at
                || !completed.reward.is_finite()
            {
                return Err(RillError::InvalidState(
                    "completed decision has inconsistent state".to_owned(),
                ));
            }
            decision
                .context
                .validate_bounded(self.config.max_context_bytes)
                .map_err(|error| RillError::InvalidState(error.to_string()))?;
            decision
                .action
                .validate_bounded(self.config.max_action_bytes)
                .map_err(|error| RillError::InvalidState(error.to_string()))?;
        }
        Ok(())
    }

    /// Validate structural invariants and caller-owned context/action state.
    pub fn validate_state_with<FC, FA>(
        &self,
        mut validate_context: FC,
        mut validate_action: FA,
    ) -> Result<(), RillError>
    where
        FC: FnMut(&C) -> Result<(), RillError>,
        FA: FnMut(&A) -> Result<(), RillError>,
    {
        self.validate_structure()?;
        for decision in self.pending.values() {
            validate_context(&decision.context)?;
            validate_action(&decision.action)?;
        }
        for completed in self.completed.values() {
            validate_context(&completed.decision.context)?;
            validate_action(&completed.decision.action)?;
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<C, A> ValidateState for DecisionLedger<C, A>
where
    C: PartialEq + BoundedDecisionValue,
    A: PartialEq + BoundedDecisionValue,
{
    fn validate_state(&self) -> Result<(), RillError> {
        self.validate()
    }
}

/// LinUCB-oriented decision facts; the generic ledger remains algorithm-free.
#[cfg(feature = "bandit")]
pub type DelayedContextualDecision = PendingDecision<Vec<f64>, usize>;

/// Atomically apply a validated outcome to both a ledger and LinUCB model.
///
/// The helper clones both Preview ledger state and the model, so either both
/// transitions succeed or neither changes. Callers requiring a lower-copy
/// path can validate with [`DecisionLedger::pending`] and manage their own
/// transaction boundary.
#[cfg(feature = "bandit")]
pub fn apply_contextual_outcome(
    ledger: &mut DecisionLedger<Vec<f64>, usize>,
    model: &mut crate::bandit::LinUcb,
    outcome: DecisionOutcome<usize>,
) -> Result<FeedbackStatus, RillError> {
    use crate::bandit::ContextualBandit;

    let pending = ledger
        .pending(outcome.decision_id)
        .ok_or_else(|| RillError::InvalidState("unknown delayed decision".to_owned()))?;
    if pending.context.len() != model.feature_count() {
        return Err(RillError::DimensionMismatch {
            expected: model.feature_count(),
            actual: pending.context.len(),
        });
    }
    for &value in &pending.context {
        if !value.is_finite() {
            return Err(RillError::NonFiniteValue {
                field: "decision context",
                value,
            });
        }
    }
    let context = pending.context.clone();
    let action = outcome.action;
    let reward = outcome.reward;
    let mut next_ledger = ledger.clone();
    let mut next_model = model.clone();
    let status = next_ledger
        .apply_feedback(outcome)
        .map_err(|error| RillError::InvalidState(error.to_string()))?;
    next_model.update(action, &context, reward)?;
    *ledger = next_ledger;
    *model = next_model;
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn decision(id: u128) -> PendingDecision<Vec<f64>, usize> {
        PendingDecision::new(DecisionId(id), vec![1.0, 2.0], 1, 10, 20, 7)
    }

    fn outcome(id: u128) -> DecisionOutcome<usize> {
        DecisionOutcome {
            decision_id: DecisionId(id),
            action: 1,
            reward: 0.5,
            observed_at: 15,
            model_generation: 7,
        }
    }

    #[test]
    fn registration_is_idempotent_but_conflicts_fail() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(2, 2).unwrap()).unwrap();
        assert_eq!(
            ledger.register(decision(1)).unwrap().status,
            RegistrationStatus::Registered
        );
        assert_eq!(
            ledger.register(decision(1)).unwrap().status,
            RegistrationStatus::PendingReplay
        );
        let mut conflict = decision(1);
        conflict.action = 0;
        assert_eq!(
            ledger.register(conflict),
            Err(DecisionLedgerError::DecisionConflict)
        );
        assert_eq!(ledger.pending_len(), 1);
    }

    #[test]
    fn feedback_checks_are_atomic_and_tombstoned() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(1, 1).unwrap()).unwrap();
        ledger.register(decision(1)).unwrap();
        let before = ledger.clone();
        let mut bad = outcome(1);
        bad.action = 0;
        assert_eq!(
            ledger.apply_feedback(bad),
            Err(DecisionLedgerError::ActionMismatch)
        );
        assert_eq!(ledger, before);
        assert_eq!(
            ledger.apply_feedback(outcome(1)).unwrap(),
            FeedbackStatus::Applied
        );
        assert_eq!(ledger.pending_len(), 0);
        assert_eq!(ledger.completed_len(), 1);
        assert_eq!(
            ledger.apply_feedback(outcome(1)),
            Err(DecisionLedgerError::DuplicateFeedback)
        );
    }

    #[test]
    fn expiry_boundary_is_inclusive_and_cleanup_is_explicit() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(2, 2).unwrap()).unwrap();
        ledger.register(decision(1)).unwrap();
        let mut at_boundary = outcome(1);
        at_boundary.observed_at = 20;
        assert!(ledger.apply_feedback(at_boundary).is_ok());

        ledger.register(decision(2)).unwrap();
        assert!(ledger.clear_expired(20).is_empty());
        assert_eq!(ledger.clear_expired(21), vec![DecisionId(2)]);
    }

    #[test]
    fn capacity_never_evicts_live_or_completed_state() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(1, 1).unwrap()).unwrap();
        ledger.register(decision(1)).unwrap();
        assert_eq!(
            ledger.register(decision(2)),
            Err(DecisionLedgerError::PendingCapacityExceeded)
        );
        ledger.apply_feedback(outcome(1)).unwrap();
        ledger.register(decision(2)).unwrap();
        assert_eq!(
            ledger.apply_feedback(outcome(2)),
            Err(DecisionLedgerError::CompletedCapacityExceeded)
        );
        assert!(ledger.pending(DecisionId(2)).is_some());
    }

    #[test]
    fn per_decision_context_and_action_memory_is_bounded() {
        let config = DecisionLedgerConfig::new(2, 2)
            .unwrap()
            .with_value_limits(8, std::mem::size_of::<usize>())
            .unwrap();
        let mut ledger = DecisionLedger::new(config).unwrap();
        assert_eq!(
            ledger.register(decision(1)),
            Err(DecisionLedgerError::ValueTooLarge)
        );
        assert_eq!(ledger.pending_len(), 0);
    }

    #[test]
    fn every_feedback_fact_is_checked_without_mutation() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(2, 2).unwrap()).unwrap();
        ledger.register(decision(1)).unwrap();
        let cases = [
            (
                DecisionOutcome {
                    decision_id: DecisionId(9),
                    ..outcome(1)
                },
                DecisionLedgerError::UnknownDecision,
            ),
            (
                DecisionOutcome {
                    observed_at: 9,
                    ..outcome(1)
                },
                DecisionLedgerError::FeedbackBeforeDecision,
            ),
            (
                DecisionOutcome {
                    observed_at: 21,
                    ..outcome(1)
                },
                DecisionLedgerError::FeedbackExpired,
            ),
            (
                DecisionOutcome {
                    model_generation: 8,
                    ..outcome(1)
                },
                DecisionLedgerError::GenerationMismatch,
            ),
            (
                DecisionOutcome {
                    action: 0,
                    ..outcome(1)
                },
                DecisionLedgerError::ActionMismatch,
            ),
        ];
        for (bad, expected) in cases {
            let before = ledger.clone();
            assert_eq!(ledger.apply_feedback(bad), Err(expected));
            assert_eq!(ledger, before);
        }
    }

    #[test]
    fn completed_replay_and_explicit_tombstone_cleanup() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(2, 2).unwrap()).unwrap();
        ledger.register(decision(1)).unwrap();
        ledger.apply_feedback(outcome(1)).unwrap();
        assert_eq!(
            ledger.register(decision(1)).unwrap().status,
            RegistrationStatus::CompletedReplay
        );
        assert!(ledger.clear_completed_before(15).is_empty());
        assert_eq!(ledger.clear_completed_before(16), vec![DecisionId(1)]);
        assert_eq!(ledger.completed_len(), 0);
    }

    #[test]
    fn caller_validator_can_enforce_domain_specific_context_rules() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(2, 2).unwrap()).unwrap();
        let mut invalid = decision(1);
        invalid.context[0] = -1.0;
        ledger.register(invalid).unwrap();
        assert!(
            ledger
                .validate_state_with(
                    |context| {
                        for &value in context {
                            if value < 0.0 {
                                return Err(RillError::InvalidState(
                                    "context values must be non-negative".to_owned(),
                                ));
                            }
                        }
                        Ok(())
                    },
                    |_| Ok(()),
                )
                .is_err()
        );
    }

    #[cfg(feature = "bandit")]
    #[test]
    fn contextual_helper_updates_model_and_ledger_together() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(2, 2).unwrap()).unwrap();
        ledger
            .register(PendingDecision::new(DecisionId(1), vec![1.0], 0, 10, 20, 7))
            .unwrap();
        let config = crate::bandit::LinUcbConfig {
            arm_count: 1,
            feature_count: 1,
            alpha: 1.0,
        };
        let mut model = crate::bandit::LinUcb::new(config).unwrap();
        apply_contextual_outcome(
            &mut ledger,
            &mut model,
            DecisionOutcome {
                decision_id: DecisionId(1),
                action: 0,
                reward: 1.0,
                observed_at: 12,
                model_generation: 7,
            },
        )
        .unwrap();
        assert_eq!(ledger.pending_len(), 0);
        assert_eq!(ledger.completed_len(), 1);
        assert_eq!(crate::bandit::ContextualBandit::samples_seen(&model), 1);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_and_corruption_validation() {
        let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(2, 2).unwrap()).unwrap();
        ledger.register(decision(1)).unwrap();
        let json = serde_json::to_string(&ledger).unwrap();
        let restored: DecisionLedger<Vec<f64>, usize> = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, ledger);
        restored.validate_state().unwrap();

        let corrupt = json.replace("\"state_version\":1", "\"state_version\":9");
        let restored: DecisionLedger<Vec<f64>, usize> = serde_json::from_str(&corrupt).unwrap();
        assert!(restored.validate_state().is_err());

        let corrupt = json.replace("\"max_pending\":2", "\"max_pending\":0");
        let restored: DecisionLedger<Vec<f64>, usize> = serde_json::from_str(&corrupt).unwrap();
        assert!(restored.validate_state().is_err());
    }

    proptest! {
        #[test]
        fn arbitrary_invalid_feedback_never_removes_pending(
            reward in prop_oneof![Just(f64::NAN), Just(f64::INFINITY)],
            observed_at in any::<u64>(),
        ) {
            let mut ledger = DecisionLedger::new(DecisionLedgerConfig::new(1, 1).unwrap()).unwrap();
            ledger.register(decision(1)).unwrap();
            let before = ledger.clone();
            let result = ledger.apply_feedback(DecisionOutcome {
                decision_id: DecisionId(1), action: 1, reward, observed_at, model_generation: 7,
            });
            prop_assert!(result.is_err());
            prop_assert_eq!(ledger, before);
        }
    }
}

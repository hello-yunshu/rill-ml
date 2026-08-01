//! Deterministic delayed-decision replay harness.
//!
//! The harness is a development/verification tool. It does not execute
//! product actions and does not infer reward semantics.

use crate::bandit::{ContextualBandit, LinUcb};
use crate::decision::{
    DecisionId, DecisionLedger, DecisionLedgerConfig, DecisionLedgerError, DecisionOutcome,
    PendingDecision, RegistrationStatus, apply_contextual_outcome,
};
use crate::{RillError, ValidateState};

/// One recorded decision plus its optional delayed outcome.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DecisionReplayRecord {
    pub timestamp: u64,
    pub decision_id: DecisionId,
    pub context: Vec<f64>,
    pub selected_arm: usize,
    pub outcome_time: Option<u64>,
    pub reward: Option<f64>,
    pub generation: u64,
    pub feature_schema_hash: String,
    /// Optional counterfactual baseline reward supplied by the caller.
    pub baseline_reward: Option<f64>,
    /// Optional best-known reward used for an explainable regret estimate.
    pub optimal_reward: Option<f64>,
    /// Caller-recorded drift boundary. The harness resets the replay model
    /// before selecting this decision and records the event.
    pub drift: bool,
}

/// Bounded replay configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct DecisionReplayConfig {
    pub max_records: usize,
    pub max_feedback_delay: u64,
    pub model_generation: u64,
    pub feature_schema_hash: String,
    pub max_pending: usize,
    pub max_completed: usize,
}

impl DecisionReplayConfig {
    pub fn new(
        max_records: usize,
        max_feedback_delay: u64,
        model_generation: u64,
        feature_schema_hash: String,
    ) -> Result<Self, RillError> {
        if max_records == 0 || feature_schema_hash.is_empty() || feature_schema_hash.len() > 128 {
            return Err(RillError::InvalidState(
                "invalid decision replay configuration".into(),
            ));
        }
        Ok(Self {
            max_records,
            max_feedback_delay,
            model_generation,
            feature_schema_hash,
            max_pending: max_records,
            max_completed: max_records,
        })
    }
}

/// Bounded feedback-latency summary.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FeedbackLatencySummary {
    pub count: usize,
    pub min: u64,
    pub max: u64,
    pub mean: f64,
    pub p95: u64,
}

/// Replay result. Regret is only accumulated for records that provide an
/// `optimal_reward`; baseline comparison only uses records that provide a
/// `baseline_reward`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DecisionReplayReport {
    pub cumulative_reward: f64,
    pub approximate_regret: f64,
    pub baseline_cumulative_reward: f64,
    pub reward_over_baseline: f64,
    pub per_arm_pulls: Vec<u64>,
    pub pending: usize,
    pub completed: usize,
    pub expired: usize,
    pub missing_feedback: usize,
    pub duplicate_feedback: usize,
    pub rejected: usize,
    pub generation_transitions: usize,
    pub drift_events: usize,
    pub feedback_latency: Option<FeedbackLatencySummary>,
    /// Stable, non-cryptographic FNV-1a digest of accepted replay facts.
    pub deterministic_replay_digest: String,
}

/// Stateful harness that can be serialized/restored between replay chunks.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DecisionReplayHarness {
    config: DecisionReplayConfig,
    model: LinUcb,
    ledger: DecisionLedger<Vec<f64>, usize>,
    current_generation: u64,
    cumulative_reward: f64,
    approximate_regret: f64,
    baseline_cumulative_reward: f64,
    per_arm_pulls: Vec<u64>,
    latencies: Vec<u64>,
    expired: usize,
    duplicate_feedback: usize,
    rejected: usize,
    generation_transitions: usize,
    drift_events: usize,
    missing_feedback: usize,
    processed_records: usize,
    digest: u64,
}

impl DecisionReplayHarness {
    pub fn new(model: LinUcb, config: DecisionReplayConfig) -> Result<Self, RillError> {
        if model.arm_count() == 0 || model.feature_count() == 0 {
            return Err(RillError::InvalidState("invalid replay model".into()));
        }
        let ledger_config = DecisionLedgerConfig::new(config.max_pending, config.max_completed)
            .map_err(ledger_error)?;
        let arm_count = model.arm_count();
        let current_generation = config.model_generation;
        Ok(Self {
            config,
            model,
            ledger: DecisionLedger::new(ledger_config).map_err(ledger_error)?,
            current_generation,
            cumulative_reward: 0.0,
            approximate_regret: 0.0,
            baseline_cumulative_reward: 0.0,
            per_arm_pulls: vec![0; arm_count],
            latencies: Vec::new(),
            expired: 0,
            duplicate_feedback: 0,
            rejected: 0,
            generation_transitions: 0,
            drift_events: 0,
            missing_feedback: 0,
            processed_records: 0,
            digest: 0xcbf29ce484222325,
        })
    }

    pub fn model(&self) -> &LinUcb {
        &self.model
    }

    pub fn ledger(&self) -> &DecisionLedger<Vec<f64>, usize> {
        &self.ledger
    }

    /// Replay a bounded chunk. Decisions sort before outcomes at the same
    /// timestamp; equal-type ties preserve input order.
    pub fn replay(
        &mut self,
        records: &[DecisionReplayRecord],
    ) -> Result<DecisionReplayReport, RillError> {
        let processed_records = self
            .processed_records
            .checked_add(records.len())
            .ok_or_else(|| RillError::InvalidState("replay record count overflow".into()))?;
        if processed_records > self.config.max_records {
            return Err(RillError::InvalidState(
                "replay record capacity exceeded".into(),
            ));
        }
        for record in records {
            validate_record(record, self.model.feature_count(), self.model.arm_count())?;
        }

        // A replay chunk is transactional: any unexpected model/ledger or
        // arithmetic failure leaves the caller's harness unchanged.
        let mut next = self.clone();
        next.processed_records = processed_records;
        next.replay_in_place(records)?;
        next.validate_state()?;
        let report = next.report();
        *self = next;
        Ok(report)
    }

    fn replay_in_place(&mut self, records: &[DecisionReplayRecord]) -> Result<(), RillError> {
        #[derive(Clone, Copy)]
        enum EventKind {
            Decision,
            Outcome,
        }
        let mut events = Vec::with_capacity(records.len().saturating_mul(2));
        for (index, record) in records.iter().enumerate() {
            events.push((record.timestamp, 0_u8, index, EventKind::Decision));
            if let Some(outcome_time) = record.outcome_time {
                events.push((outcome_time, 1_u8, index, EventKind::Outcome));
            }
        }
        events.sort_by_key(|event| (event.0, event.1, event.2));

        for (_, _, index, kind) in events {
            let record = &records[index];
            match kind {
                EventKind::Decision => self.replay_decision(record)?,
                EventKind::Outcome => self.replay_outcome(record)?,
            }
        }
        Ok(())
    }

    fn replay_decision(&mut self, record: &DecisionReplayRecord) -> Result<(), RillError> {
        if record.feature_schema_hash != self.config.feature_schema_hash {
            checked_increment_usize(&mut self.rejected, "rejected")?;
            return Ok(());
        }
        if record.generation < self.current_generation {
            checked_increment_usize(&mut self.rejected, "rejected")?;
            return Ok(());
        }
        if record.generation > self.current_generation {
            self.model.reset();
            self.current_generation = record.generation;
            checked_increment_usize(&mut self.generation_transitions, "generation transitions")?;
        }
        if record.drift {
            self.model.reset();
            checked_increment_usize(&mut self.drift_events, "drift events")?;
        }

        let samples_before = self.model.samples_seen();
        let selected = self.model.select_deterministic(&record.context)?;
        if self.model.samples_seen() != samples_before {
            return Err(RillError::InvalidState(
                "LinUCB select modified learning state during replay".into(),
            ));
        }
        if selected != record.selected_arm {
            checked_increment_usize(&mut self.rejected, "rejected")?;
            return Ok(());
        }
        let expires_at = record
            .timestamp
            .checked_add(self.config.max_feedback_delay)
            .ok_or_else(|| RillError::InvalidState("decision expiry overflow".into()))?;
        let decision = PendingDecision::new(
            record.decision_id,
            record.context.clone(),
            record.selected_arm,
            record.timestamp,
            expires_at,
            record.generation,
        );
        let registered = match self.ledger.register(decision).map_err(ledger_error)?.status {
            RegistrationStatus::Registered => {
                self.digest_record(record, b'd');
                true
            }
            RegistrationStatus::PendingReplay | RegistrationStatus::CompletedReplay => false,
        };
        if registered && record.outcome_time.is_none() {
            self.missing_feedback = self.missing_feedback.checked_add(1).ok_or_else(|| {
                RillError::InvalidState("missing feedback counter overflow".into())
            })?;
        }
        Ok(())
    }

    fn replay_outcome(&mut self, record: &DecisionReplayRecord) -> Result<(), RillError> {
        let Some(outcome_time) = record.outcome_time else {
            return Ok(());
        };
        let Some(reward) = record.reward else {
            checked_increment_usize(&mut self.rejected, "rejected")?;
            return Ok(());
        };
        if record.generation != self.current_generation {
            checked_increment_usize(&mut self.rejected, "rejected")?;
            return Ok(());
        }
        if self.ledger.completed(record.decision_id).is_some() {
            self.duplicate_feedback = self.duplicate_feedback.checked_add(1).ok_or_else(|| {
                RillError::InvalidState("duplicate feedback counter overflow".into())
            })?;
            return Ok(());
        }
        let Some(pending) = self.ledger.pending(record.decision_id) else {
            self.rejected = self
                .rejected
                .checked_add(1)
                .ok_or_else(|| RillError::InvalidState("rejected counter overflow".into()))?;
            return Ok(());
        };
        if outcome_time > pending.expires_at {
            let removed = self.ledger.clear_expired(outcome_time).len();
            self.expired = self
                .expired
                .checked_add(removed)
                .ok_or_else(|| RillError::InvalidState("expired counter overflow".into()))?;
            return Ok(());
        }
        if outcome_time < pending.created_at
            || record.generation != pending.model_generation
            || record.selected_arm != pending.action
        {
            self.rejected = self
                .rejected
                .checked_add(1)
                .ok_or_else(|| RillError::InvalidState("rejected counter overflow".into()))?;
            return Ok(());
        }
        let outcome = DecisionOutcome {
            decision_id: record.decision_id,
            action: record.selected_arm,
            reward,
            observed_at: outcome_time,
            model_generation: record.generation,
        };
        let next_reward = finite_add(self.cumulative_reward, reward, "cumulative reward")?;
        let next_baseline = if let Some(baseline) = record.baseline_reward {
            finite_add(self.baseline_cumulative_reward, baseline, "baseline reward")?
        } else {
            self.baseline_cumulative_reward
        };
        let next_regret = if let Some(optimal) = record.optimal_reward {
            let regret = (optimal - reward).max(0.0);
            if !regret.is_finite() {
                return Err(RillError::InvalidState("regret overflow".into()));
            }
            finite_add(self.approximate_regret, regret, "approximate regret")?
        } else {
            self.approximate_regret
        };
        let next_pulls = self.per_arm_pulls[record.selected_arm]
            .checked_add(1)
            .ok_or_else(|| RillError::InvalidState("per-arm pull overflow".into()))?;
        if self.latencies.len() >= self.config.max_records {
            return Err(RillError::InvalidState(
                "feedback latency capacity exceeded".into(),
            ));
        }

        apply_contextual_outcome(&mut self.ledger, &mut self.model, outcome)?;
        self.cumulative_reward = next_reward;
        self.baseline_cumulative_reward = next_baseline;
        self.approximate_regret = next_regret;
        self.per_arm_pulls[record.selected_arm] = next_pulls;
        self.latencies.push(outcome_time - record.timestamp);
        self.digest_record(record, b'o');
        Ok(())
    }

    fn report(&self) -> DecisionReplayReport {
        let reward_over_baseline = self.cumulative_reward - self.baseline_cumulative_reward;
        DecisionReplayReport {
            cumulative_reward: self.cumulative_reward,
            approximate_regret: self.approximate_regret,
            baseline_cumulative_reward: self.baseline_cumulative_reward,
            reward_over_baseline,
            per_arm_pulls: self.per_arm_pulls.clone(),
            pending: self.ledger.pending_len(),
            completed: self.ledger.completed_len(),
            expired: self.expired,
            missing_feedback: self.missing_feedback,
            duplicate_feedback: self.duplicate_feedback,
            rejected: self.rejected,
            generation_transitions: self.generation_transitions,
            drift_events: self.drift_events,
            feedback_latency: latency_summary(&self.latencies),
            deterministic_replay_digest: format!("{:016x}", self.digest),
        }
    }

    fn digest_record(&mut self, record: &DecisionReplayRecord, marker: u8) {
        fn mix(digest: &mut u64, bytes: &[u8]) {
            for byte in bytes {
                *digest ^= u64::from(*byte);
                *digest = digest.wrapping_mul(0x100000001b3);
            }
        }
        mix(&mut self.digest, &[marker]);
        mix(&mut self.digest, &record.timestamp.to_le_bytes());
        mix(&mut self.digest, &record.decision_id.0.to_le_bytes());
        mix(
            &mut self.digest,
            &(record.selected_arm as u64).to_le_bytes(),
        );
        mix(&mut self.digest, &record.generation.to_le_bytes());
        for value in &record.context {
            mix(&mut self.digest, &value.to_bits().to_le_bytes());
        }
        if let Some(reward) = record.reward {
            mix(&mut self.digest, &reward.to_bits().to_le_bytes());
        }
    }

    #[cfg(feature = "serde")]
    pub fn checkpoint_json(&self) -> Result<String, RillError> {
        serde_json::to_string(self).map_err(|error| RillError::InvalidState(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn restore_json(json: &str) -> Result<Self, RillError> {
        let restored: Self = serde_json::from_str(json)
            .map_err(|error| RillError::InvalidState(error.to_string()))?;
        restored.validate_state()?;
        Ok(restored)
    }
}

impl ValidateState for DecisionReplayHarness {
    fn validate_state(&self) -> Result<(), RillError> {
        if self.config.max_records == 0
            || self.config.max_pending == 0
            || self.config.max_completed == 0
            || self.config.feature_schema_hash.is_empty()
            || self.config.feature_schema_hash.len() > 128
            || self.current_generation < self.config.model_generation
            || self.processed_records > self.config.max_records
            || self.per_arm_pulls.len() != self.model.arm_count()
            || self.latencies.len() > self.config.max_records
            || !self.cumulative_reward.is_finite()
            || !self.approximate_regret.is_finite()
            || !self.baseline_cumulative_reward.is_finite()
            || !(self.cumulative_reward - self.baseline_cumulative_reward).is_finite()
        {
            return Err(RillError::InvalidState("invalid replay state".into()));
        }
        let total_pulls = self.per_arm_pulls.iter().try_fold(0_u64, |sum, pulls| {
            sum.checked_add(*pulls)
                .ok_or_else(|| RillError::InvalidState("per-arm pull sum overflow".into()))
        })?;
        if total_pulls < self.model.samples_seen()
            || total_pulls as usize != self.ledger.completed_len()
        {
            return Err(RillError::InvalidState(
                "replay pulls, model samples, and completed ledger disagree".into(),
            ));
        }
        self.model.validate()?;
        self.ledger.validate()?;
        Ok(())
    }
}

fn validate_record(
    record: &DecisionReplayRecord,
    feature_count: usize,
    arm_count: usize,
) -> Result<(), RillError> {
    if record.context.len() != feature_count {
        return Err(RillError::DimensionMismatch {
            expected: feature_count,
            actual: record.context.len(),
        });
    }
    if record.context.iter().any(|value| !value.is_finite()) {
        return Err(RillError::InvalidState("non-finite replay context".into()));
    }
    if record.selected_arm >= arm_count {
        return Err(RillError::InvalidArm {
            expected: arm_count,
            actual: record.selected_arm,
        });
    }
    if record.outcome_time.is_some() != record.reward.is_some() {
        return Err(RillError::InvalidState(
            "outcome time and reward must both be present or absent".into(),
        ));
    }
    if let Some(outcome_time) = record.outcome_time
        && outcome_time < record.timestamp
    {
        return Err(RillError::InvalidState("outcome precedes decision".into()));
    }
    for value in [record.reward, record.baseline_reward, record.optimal_reward]
        .into_iter()
        .flatten()
    {
        if !value.is_finite() {
            return Err(RillError::InvalidState("non-finite replay reward".into()));
        }
    }
    Ok(())
}

fn latency_summary(latencies: &[u64]) -> Option<FeedbackLatencySummary> {
    if latencies.is_empty() {
        return None;
    }
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let sum = sorted
        .iter()
        .fold(0_u128, |sum, value| sum + u128::from(*value));
    let index = ((sorted.len() - 1) * 95).div_ceil(100);
    Some(FeedbackLatencySummary {
        count: sorted.len(),
        min: sorted[0],
        max: *sorted.last().unwrap(),
        mean: sum as f64 / sorted.len() as f64,
        p95: sorted[index],
    })
}

fn ledger_error(error: DecisionLedgerError) -> RillError {
    RillError::InvalidState(error.to_string())
}

fn finite_add(current: f64, value: f64, field: &'static str) -> Result<f64, RillError> {
    let next = current + value;
    if next.is_finite() {
        Ok(next)
    } else {
        Err(RillError::InvalidState(format!("{field} overflow")))
    }
}

fn checked_increment_usize(value: &mut usize, field: &'static str) -> Result<(), RillError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| RillError::InvalidState(format!("{field} counter overflow")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bandit::LinUcbConfig;

    fn harness() -> DecisionReplayHarness {
        DecisionReplayHarness::new(
            LinUcb::new(LinUcbConfig::default()).unwrap(),
            DecisionReplayConfig::new(16, 10, 1, "schema-a".into()).unwrap(),
        )
        .unwrap()
    }

    fn record(id: u128, timestamp: u64, outcome: Option<(u64, f64)>) -> DecisionReplayRecord {
        let model = LinUcb::new(LinUcbConfig::default()).unwrap();
        let arm = model.select_deterministic(&[1.0]).unwrap();
        DecisionReplayRecord {
            timestamp,
            decision_id: DecisionId(id),
            context: vec![1.0],
            selected_arm: arm,
            outcome_time: outcome.map(|value| value.0),
            reward: outcome.map(|value| value.1),
            generation: 1,
            feature_schema_hash: "schema-a".into(),
            baseline_reward: Some(0.25),
            optimal_reward: Some(1.0),
            drift: false,
        }
    }

    #[test]
    fn update_waits_for_outcome_and_reports_metrics() {
        let mut harness = harness();
        let report = harness
            .replay(&[record(1, 0, Some((5, 0.75))), record(2, 1, None)])
            .unwrap();
        assert_eq!(harness.model().samples_seen(), 1);
        assert_eq!(report.cumulative_reward, 0.75);
        assert_eq!(report.approximate_regret, 0.25);
        assert_eq!(report.pending, 1);
        assert_eq!(report.completed, 1);
        assert_eq!(report.missing_feedback, 1);
        assert_eq!(report.feedback_latency.unwrap().mean, 5.0);
    }

    #[test]
    fn duplicate_expired_and_schema_mismatch_are_counted() {
        let mut harness = harness();
        let duplicate = record(1, 0, Some((5, 0.5)));
        let expired = record(2, 1, Some((20, 0.5)));
        let mut mismatch = record(3, 2, Some((3, 0.5)));
        mismatch.feature_schema_hash = "other".into();
        let report = harness
            .replay(&[duplicate.clone(), duplicate, expired, mismatch])
            .unwrap();
        assert_eq!(report.duplicate_feedback, 1);
        assert_eq!(report.expired, 1);
        assert!(report.rejected >= 1);
    }

    #[test]
    fn replay_digest_is_deterministic_and_restore_continues() {
        let records = [record(1, 0, Some((2, 0.5)))];
        let mut a = harness();
        let mut b = harness();
        let report_a = a.replay(&records).unwrap();
        let report_b = b.replay(&records).unwrap();
        assert_eq!(
            report_a.deterministic_replay_digest,
            report_b.deterministic_replay_digest
        );

        #[cfg(feature = "serde")]
        {
            let checkpoint = a.checkpoint_json().unwrap();
            let restored = DecisionReplayHarness::restore_json(&checkpoint).unwrap();
            assert_eq!(restored.model().samples_seen(), a.model().samples_seen());
            assert_eq!(restored.ledger().completed_len(), 1);
        }
    }
}

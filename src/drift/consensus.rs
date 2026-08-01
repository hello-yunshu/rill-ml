//! Bounded, explainable consensus over multiple drift detectors.
//!
//! `DriftConsensus` combines caller-supplied detector levels. It never resets
//! a model or performs a business action; it only reports a hysteretic,
//! auditable consensus level.

use std::collections::BTreeSet;

use crate::drift::DriftLevel;
use crate::error::{RillError, checked_increment};
use crate::persistence::ValidateState;

const MAX_DETECTOR_NAME_BYTES: usize = 128;

/// One detector's vote for a consensus window.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[non_exhaustive]
pub struct DriftVote {
    /// Stable caller-defined detector identifier.
    pub detector: String,
    /// Detector level for this window.
    pub level: DriftLevel,
    /// Whether this detector had enough data to cast a meaningful vote.
    pub available: bool,
}

impl DriftVote {
    /// Construct an available vote.
    pub fn new(detector: impl Into<String>, level: DriftLevel) -> Result<Self, RillError> {
        let vote = Self {
            detector: detector.into(),
            level,
            available: true,
        };
        vote.validate()?;
        Ok(vote)
    }

    /// Construct a temporarily unavailable detector vote.
    pub fn unavailable(detector: impl Into<String>) -> Result<Self, RillError> {
        let vote = Self {
            detector: detector.into(),
            level: DriftLevel::None,
            available: false,
        };
        vote.validate()?;
        Ok(vote)
    }

    fn validate(&self) -> Result<(), RillError> {
        if self.detector.is_empty() || self.detector.len() > MAX_DETECTOR_NAME_BYTES {
            return Err(RillError::InvalidState(format!(
                "drift detector name length must be in 1..={MAX_DETECTOR_NAME_BYTES} bytes"
            )));
        }
        if !self.available && self.level != DriftLevel::None {
            return Err(RillError::InvalidState(
                "an unavailable detector cannot cast a warning or drift vote".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Configuration for [`DriftConsensus`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[non_exhaustive]
pub struct DriftConsensusConfig {
    /// Warning-or-drift votes required for a warning candidate.
    pub minimum_warning_votes: usize,
    /// Drift votes required for a confirmed drift candidate.
    pub minimum_drift_votes: usize,
    /// Consecutive candidate windows required to change level.
    pub confirmation_windows: u64,
    /// Consecutive complete, stable windows required to clear a level.
    pub clear_windows: u64,
    /// Windows after an event during which another event is suppressed.
    pub cooldown_windows: u64,
    /// Maximum distance at which a repeated event is merged into the last one.
    pub event_merge_horizon: u64,
    /// Initial complete windows reported as warming.
    pub warming_windows: u64,
    /// Maximum votes and retained detector identifiers.
    pub max_detectors: usize,
}

impl Default for DriftConsensusConfig {
    fn default() -> Self {
        Self {
            minimum_warning_votes: 1,
            minimum_drift_votes: 2,
            confirmation_windows: 2,
            clear_windows: 3,
            cooldown_windows: 5,
            event_merge_horizon: 20,
            warming_windows: 1,
            max_detectors: 16,
        }
    }
}

impl DriftConsensusConfig {
    /// Validate thresholds and capacity bounds.
    pub fn validate(&self) -> Result<(), RillError> {
        if self.max_detectors == 0 || self.max_detectors > 128 {
            return Err(RillError::InvalidCapacity(self.max_detectors));
        }
        if self.minimum_warning_votes == 0
            || self.minimum_warning_votes > self.max_detectors
            || self.minimum_drift_votes == 0
            || self.minimum_drift_votes > self.max_detectors
            || self.minimum_warning_votes > self.minimum_drift_votes
        {
            return Err(RillError::InvalidState(
                "drift consensus vote thresholds are inconsistent".to_owned(),
            ));
        }
        if self.confirmation_windows == 0 || self.clear_windows == 0 {
            return Err(RillError::InvalidState(
                "confirmation_windows and clear_windows must be positive".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Bounded summary of a consensus event.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct DriftEventSummary {
    /// Monotonic event identifier local to this consensus instance.
    pub event_id: u64,
    /// First consensus window included in this event.
    pub first_window: u64,
    /// Most recent merged consensus window.
    pub last_window: u64,
    /// Highest level observed across merged occurrences.
    pub peak_level: DriftLevel,
    /// Sorted, deduplicated detector identifiers involved in the event.
    pub detectors: Vec<String>,
    /// Number of merged occurrences.
    pub occurrences: u64,
    /// Caller-supplied configuration/model generation context.
    pub generation: u64,
}

/// Explainable result for one consensus window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftConsensusResult {
    /// Hysteretic consensus level after this window.
    pub level: DriftLevel,
    /// Available warning-or-drift votes.
    pub warning_votes: usize,
    /// Available drift votes.
    pub drift_votes: usize,
    /// Number of available detector votes.
    pub available_detectors: usize,
    /// Detectors currently voting Warning or Drift.
    pub triggered_detectors: Vec<String>,
    /// Consecutive windows for the current raw candidate.
    pub consecutive_windows: u64,
    /// Current stable-window streak used for clearing.
    pub clear_streak: u64,
    /// Remaining event cooldown windows.
    pub cooldown_remaining: u64,
    /// Whether the consensus is still warming.
    pub warming: bool,
    /// Whether insufficient detector data prevented a state transition.
    pub incomplete: bool,
    /// Whether this call observed a generation change.
    pub generation_changed: bool,
    /// Whether a new (unmerged) event was created.
    pub new_event: bool,
    /// Whether a repeated occurrence was merged into the prior event.
    pub merged_event: bool,
    /// Latest event summary, if any.
    pub event: Option<DriftEventSummary>,
}

/// Stateful drift vote consensus with hysteresis, cooldown and event merging.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DriftConsensus {
    config: DriftConsensusConfig,
    windows_seen: u64,
    generation_windows: u64,
    active_generation: Option<u64>,
    active_level: DriftLevel,
    warning_streak: u64,
    drift_streak: u64,
    clear_streak: u64,
    cooldown_remaining: u64,
    next_event_id: u64,
    last_event: Option<DriftEventSummary>,
}

impl DriftConsensus {
    /// Create an empty consensus state.
    pub fn new(config: DriftConsensusConfig) -> Result<Self, RillError> {
        config.validate()?;
        Ok(Self {
            config,
            windows_seen: 0,
            generation_windows: 0,
            active_generation: None,
            active_level: DriftLevel::None,
            warning_streak: 0,
            drift_streak: 0,
            clear_streak: 0,
            cooldown_remaining: 0,
            next_event_id: 1,
            last_event: None,
        })
    }

    /// Configuration used by this consensus instance.
    pub const fn config(&self) -> &DriftConsensusConfig {
        &self.config
    }

    /// Number of vote windows processed.
    pub const fn windows_seen(&self) -> u64 {
        self.windows_seen
    }

    /// Current hysteretic consensus level.
    pub const fn level(&self) -> DriftLevel {
        self.active_level
    }

    /// Latest bounded event summary.
    pub const fn last_event(&self) -> Option<&DriftEventSummary> {
        self.last_event.as_ref()
    }

    /// Reset all dynamic state while retaining configuration.
    pub fn reset(&mut self) {
        self.windows_seen = 0;
        self.generation_windows = 0;
        self.active_generation = None;
        self.active_level = DriftLevel::None;
        self.warning_streak = 0;
        self.drift_streak = 0;
        self.clear_streak = 0;
        self.cooldown_remaining = 0;
        self.next_event_id = 1;
        self.last_event = None;
    }

    /// Incorporate one complete or partial detector-vote window.
    pub fn update(
        &mut self,
        votes: &[DriftVote],
        generation: u64,
    ) -> Result<DriftConsensusResult, RillError> {
        self.validate_votes(votes)?;
        let mut next = self.clone();
        let result = next.update_inner(votes, generation)?;
        *self = next;
        Ok(result)
    }

    fn update_inner(
        &mut self,
        votes: &[DriftVote],
        generation: u64,
    ) -> Result<DriftConsensusResult, RillError> {
        let next_windows = checked_increment(self.windows_seen, "consensus windows_seen")?;
        let generation_changed = self.active_generation.is_some_and(|old| old != generation);
        if generation_changed {
            self.generation_windows = 0;
            self.active_level = DriftLevel::None;
            self.warning_streak = 0;
            self.drift_streak = 0;
            self.clear_streak = 0;
            self.cooldown_remaining = 0;
        }
        self.active_generation = Some(generation);
        self.windows_seen = next_windows;
        self.generation_windows =
            checked_increment(self.generation_windows, "consensus generation_windows")?;
        let event_allowed = self.cooldown_remaining == 0;
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
        }

        let available_detectors = votes.iter().filter(|vote| vote.available).count();
        let drift_votes = votes
            .iter()
            .filter(|vote| vote.available && vote.level == DriftLevel::Drift)
            .count();
        let warning_votes = votes
            .iter()
            .filter(|vote| vote.available && vote.level.is_change())
            .count();
        let triggered_detectors: Vec<String> = votes
            .iter()
            .filter(|vote| vote.available && vote.level.is_change())
            .map(|vote| vote.detector.clone())
            .collect();
        let warming = self.generation_windows <= self.config.warming_windows;
        let incomplete = available_detectors < self.config.minimum_warning_votes;
        if warming || incomplete {
            self.warning_streak = 0;
            self.drift_streak = 0;
            return Ok(self.result(
                warning_votes,
                drift_votes,
                available_detectors,
                triggered_detectors,
                0,
                warming,
                incomplete,
                generation_changed,
                false,
                false,
            ));
        }

        let raw_level = if drift_votes >= self.config.minimum_drift_votes {
            DriftLevel::Drift
        } else if warning_votes >= self.config.minimum_warning_votes {
            DriftLevel::Warning
        } else {
            DriftLevel::None
        };

        let mut new_event = false;
        let mut merged_event = false;
        let consecutive_windows;
        match raw_level {
            DriftLevel::Drift => {
                self.clear_streak = 0;
                self.warning_streak = 0;
                self.drift_streak = checked_increment(self.drift_streak, "drift streak")?;
                consecutive_windows = self.drift_streak;
                if self.drift_streak >= self.config.confirmation_windows
                    && (self.active_level != DriftLevel::Drift
                        || self
                            .drift_streak
                            .is_multiple_of(self.config.confirmation_windows))
                {
                    self.active_level = DriftLevel::Drift;
                    if event_allowed {
                        (new_event, merged_event) =
                            self.record_event(DriftLevel::Drift, &triggered_detectors, generation)?;
                    }
                }
            }
            DriftLevel::Warning => {
                self.clear_streak = 0;
                self.drift_streak = 0;
                self.warning_streak = checked_increment(self.warning_streak, "warning streak")?;
                consecutive_windows = self.warning_streak;
                if self.active_level == DriftLevel::None
                    && self.warning_streak >= self.config.confirmation_windows
                {
                    self.active_level = DriftLevel::Warning;
                    if event_allowed {
                        (new_event, merged_event) = self.record_event(
                            DriftLevel::Warning,
                            &triggered_detectors,
                            generation,
                        )?;
                    }
                }
            }
            DriftLevel::None => {
                self.warning_streak = 0;
                self.drift_streak = 0;
                self.clear_streak = checked_increment(self.clear_streak, "clear streak")?;
                consecutive_windows = self.clear_streak;
                if self.active_level != DriftLevel::None
                    && self.clear_streak >= self.config.clear_windows
                {
                    self.active_level = DriftLevel::None;
                    self.clear_streak = 0;
                }
            }
        }

        Ok(self.result(
            warning_votes,
            drift_votes,
            available_detectors,
            triggered_detectors,
            consecutive_windows,
            false,
            false,
            generation_changed,
            new_event,
            merged_event,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn result(
        &self,
        warning_votes: usize,
        drift_votes: usize,
        available_detectors: usize,
        triggered_detectors: Vec<String>,
        consecutive_windows: u64,
        warming: bool,
        incomplete: bool,
        generation_changed: bool,
        new_event: bool,
        merged_event: bool,
    ) -> DriftConsensusResult {
        DriftConsensusResult {
            level: self.active_level,
            warning_votes,
            drift_votes,
            available_detectors,
            triggered_detectors,
            consecutive_windows,
            clear_streak: self.clear_streak,
            cooldown_remaining: self.cooldown_remaining,
            warming,
            incomplete,
            generation_changed,
            new_event,
            merged_event,
            event: self.last_event.clone(),
        }
    }

    fn validate_votes(&self, votes: &[DriftVote]) -> Result<(), RillError> {
        if votes.len() > self.config.max_detectors {
            return Err(RillError::InvalidCapacity(votes.len()));
        }
        let mut names = BTreeSet::new();
        for vote in votes {
            vote.validate()?;
            if !names.insert(vote.detector.as_str()) {
                return Err(RillError::InvalidState(format!(
                    "duplicate drift detector vote: {}",
                    vote.detector
                )));
            }
        }
        Ok(())
    }

    fn record_event(
        &mut self,
        level: DriftLevel,
        detectors: &[String],
        generation: u64,
    ) -> Result<(bool, bool), RillError> {
        let can_merge = self.last_event.as_ref().is_some_and(|event| {
            event.generation == generation
                && self.windows_seen.saturating_sub(event.last_window)
                    <= self.config.event_merge_horizon
        });
        if can_merge && let Some(event) = self.last_event.as_mut() {
            event.last_window = self.windows_seen;
            event.occurrences = checked_increment(event.occurrences, "event occurrences")?;
            if level_rank(level) > level_rank(event.peak_level) {
                event.peak_level = level;
            }
            let mut merged: BTreeSet<String> = event.detectors.iter().cloned().collect();
            merged.extend(detectors.iter().cloned());
            event.detectors = merged.into_iter().take(self.config.max_detectors).collect();
            self.cooldown_remaining = self.config.cooldown_windows;
            return Ok((false, true));
        }

        let event_id = self.next_event_id;
        self.next_event_id = checked_increment(self.next_event_id, "next_event_id")?;
        let unique: BTreeSet<String> = detectors.iter().cloned().collect();
        self.last_event = Some(DriftEventSummary {
            event_id,
            first_window: self.windows_seen,
            last_window: self.windows_seen,
            peak_level: level,
            detectors: unique.into_iter().take(self.config.max_detectors).collect(),
            occurrences: 1,
            generation,
        });
        self.cooldown_remaining = self.config.cooldown_windows;
        Ok((true, false))
    }
}

impl ValidateState for DriftConsensus {
    fn validate_state(&self) -> Result<(), RillError> {
        self.config.validate()?;
        if self.generation_windows > self.windows_seen
            || self.warning_streak > self.generation_windows
            || self.drift_streak > self.generation_windows
            || self.clear_streak > self.generation_windows
            || self.cooldown_remaining > self.config.cooldown_windows
            || self.next_event_id == 0
        {
            return Err(RillError::InvalidState(
                "drift consensus counters are inconsistent".to_owned(),
            ));
        }
        if self.active_generation.is_none() && self.generation_windows != 0 {
            return Err(RillError::InvalidState(
                "drift consensus generation counter has no generation".to_owned(),
            ));
        }
        if let Some(event) = &self.last_event {
            if event.event_id >= self.next_event_id
                || event.first_window == 0
                || event.first_window > event.last_window
                || event.last_window > self.windows_seen
                || event.occurrences == 0
                || event.peak_level == DriftLevel::None
                || event.detectors.len() > self.config.max_detectors
            {
                return Err(RillError::InvalidState(
                    "drift consensus event summary is inconsistent".to_owned(),
                ));
            }
            let mut unique = BTreeSet::new();
            for detector in &event.detectors {
                DriftVote::new(detector.clone(), DriftLevel::None)?;
                if !unique.insert(detector) {
                    return Err(RillError::InvalidState(
                        "drift consensus event has duplicate detector names".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn level_rank(level: DriftLevel) -> u8 {
    match level {
        DriftLevel::None => 0,
        DriftLevel::Warning => 1,
        DriftLevel::Drift => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DriftConsensusConfig {
        DriftConsensusConfig {
            minimum_warning_votes: 1,
            minimum_drift_votes: 2,
            confirmation_windows: 2,
            clear_windows: 2,
            cooldown_windows: 2,
            event_merge_horizon: 10,
            warming_windows: 0,
            max_detectors: 3,
        }
    }

    fn votes(levels: [DriftLevel; 3]) -> Vec<DriftVote> {
        levels
            .into_iter()
            .enumerate()
            .map(|(index, level)| DriftVote::new(format!("d{index}"), level).unwrap())
            .collect()
    }

    #[test]
    fn one_two_and_three_of_three_votes_are_explainable() {
        let mut consensus = DriftConsensus::new(config()).unwrap();
        let one = consensus
            .update(
                &votes([DriftLevel::Drift, DriftLevel::None, DriftLevel::None]),
                1,
            )
            .unwrap();
        assert_eq!(one.warning_votes, 1);
        assert_eq!(one.drift_votes, 1);
        assert_eq!(one.level, DriftLevel::None);

        let two_votes = votes([DriftLevel::Drift, DriftLevel::Drift, DriftLevel::None]);
        let first = consensus.update(&two_votes, 1).unwrap();
        assert_eq!(first.level, DriftLevel::None);
        let second = consensus.update(&two_votes, 1).unwrap();
        assert_eq!(second.level, DriftLevel::Drift);
        assert!(second.new_event);

        let three = consensus.update(&votes([DriftLevel::Drift; 3]), 1).unwrap();
        assert_eq!(three.drift_votes, 3);
        assert_eq!(three.level, DriftLevel::Drift);
    }

    #[test]
    fn warning_escalates_to_drift_and_clear_uses_hysteresis() {
        let mut consensus = DriftConsensus::new(config()).unwrap();
        let warning_votes = votes([DriftLevel::Warning, DriftLevel::None, DriftLevel::None]);
        consensus.update(&warning_votes, 1).unwrap();
        assert_eq!(
            consensus.update(&warning_votes, 1).unwrap().level,
            DriftLevel::Warning
        );

        let drift_votes = votes([DriftLevel::Drift, DriftLevel::Drift, DriftLevel::None]);
        consensus.update(&drift_votes, 1).unwrap();
        assert_eq!(
            consensus.update(&drift_votes, 1).unwrap().level,
            DriftLevel::Drift
        );

        let stable = votes([DriftLevel::None; 3]);
        assert_eq!(
            consensus.update(&stable, 1).unwrap().level,
            DriftLevel::Drift
        );
        assert_eq!(
            consensus.update(&stable, 1).unwrap().level,
            DriftLevel::None
        );
    }

    #[test]
    fn interrupted_confirmation_and_incomplete_data_do_not_trigger_or_clear() {
        let mut consensus = DriftConsensus::new(config()).unwrap();
        let drift_votes = votes([DriftLevel::Drift, DriftLevel::Drift, DriftLevel::None]);
        consensus.update(&drift_votes, 1).unwrap();
        consensus.update(&votes([DriftLevel::None; 3]), 1).unwrap();
        assert_eq!(
            consensus.update(&drift_votes, 1).unwrap().level,
            DriftLevel::None
        );

        let unavailable = vec![
            DriftVote::unavailable("d0").unwrap(),
            DriftVote::unavailable("d1").unwrap(),
        ];
        let result = consensus.update(&unavailable, 1).unwrap();
        assert!(result.incomplete);
        assert_eq!(result.level, DriftLevel::None);
    }

    #[test]
    fn cooldown_and_merge_horizon_merge_repeated_events() {
        let mut consensus = DriftConsensus::new(config()).unwrap();
        let drift_votes = votes([DriftLevel::Drift, DriftLevel::Drift, DriftLevel::None]);
        consensus.update(&drift_votes, 1).unwrap();
        let first = consensus.update(&drift_votes, 1).unwrap();
        assert!(first.new_event);
        assert_eq!(first.event.as_ref().unwrap().occurrences, 1);

        // Cooldown suppresses the next confirmation boundary.
        consensus.update(&drift_votes, 1).unwrap();
        let suppressed = consensus.update(&drift_votes, 1).unwrap();
        assert!(!suppressed.new_event && !suppressed.merged_event);

        consensus.update(&drift_votes, 1).unwrap();
        let merged = consensus.update(&drift_votes, 1).unwrap();
        assert!(merged.merged_event);
        assert_eq!(merged.event.unwrap().occurrences, 2);
    }

    #[test]
    fn generation_change_resets_confirmation_context() {
        let mut consensus = DriftConsensus::new(config()).unwrap();
        let drift_votes = votes([DriftLevel::Drift, DriftLevel::Drift, DriftLevel::None]);
        consensus.update(&drift_votes, 1).unwrap();
        let changed = consensus.update(&drift_votes, 2).unwrap();
        assert!(changed.generation_changed);
        assert_eq!(changed.level, DriftLevel::None);
        assert_eq!(
            consensus.update(&drift_votes, 2).unwrap().level,
            DriftLevel::Drift
        );
    }

    #[test]
    fn serde_restore_preserves_continuity_and_validation_rejects_corruption() {
        let mut consensus = DriftConsensus::new(config()).unwrap();
        consensus
            .update(
                &votes([DriftLevel::Warning, DriftLevel::None, DriftLevel::None]),
                7,
            )
            .unwrap();
        #[cfg(feature = "serde")]
        {
            let json = serde_json::to_string(&consensus).unwrap();
            let mut restored: DriftConsensus = serde_json::from_str(&json).unwrap();
            restored.validate_state().unwrap();
            let next = votes([DriftLevel::Warning, DriftLevel::None, DriftLevel::None]);
            assert_eq!(
                consensus.update(&next, 7).unwrap(),
                restored.update(&next, 7).unwrap()
            );
        }

        let mut corrupt = consensus.clone();
        corrupt.cooldown_remaining = corrupt.config.cooldown_windows + 1;
        assert!(corrupt.validate_state().is_err());
    }

    #[test]
    fn duplicate_votes_and_capacity_are_rejected_atomically() {
        let mut consensus = DriftConsensus::new(config()).unwrap();
        let duplicate = vec![
            DriftVote::new("same", DriftLevel::None).unwrap(),
            DriftVote::new("same", DriftLevel::Drift).unwrap(),
        ];
        assert!(consensus.update(&duplicate, 1).is_err());
        assert_eq!(consensus.windows_seen(), 0);

        let too_many = (0..4)
            .map(|i| DriftVote::new(format!("d{i}"), DriftLevel::None).unwrap())
            .collect::<Vec<_>>();
        assert!(consensus.update(&too_many, 1).is_err());
        assert_eq!(consensus.windows_seen(), 0);
    }

    #[test]
    fn counter_overflow_and_corrupt_event_state_are_failure_atomic() {
        let warning_votes = votes([DriftLevel::Warning, DriftLevel::None, DriftLevel::None]);
        let mut consensus = DriftConsensus::new(config()).unwrap();
        consensus.active_generation = Some(1);
        consensus.windows_seen = 10;
        consensus.generation_windows = 10;
        consensus.warning_streak = u64::MAX;
        let before = consensus.clone();
        assert!(consensus.update(&warning_votes, 1).is_err());
        assert_eq!(consensus, before);

        let drift_votes = votes([DriftLevel::Drift, DriftLevel::Drift, DriftLevel::None]);
        let mut corrupt = DriftConsensus::new(config()).unwrap();
        corrupt.active_generation = Some(1);
        corrupt.windows_seen = 1;
        corrupt.generation_windows = 1;
        corrupt.drift_streak = 1;
        corrupt.next_event_id = u64::MAX;
        let before = corrupt.clone();
        assert!(corrupt.update(&drift_votes, 1).is_err());
        assert_eq!(corrupt, before);
    }
}

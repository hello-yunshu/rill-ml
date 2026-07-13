//! Per-arm diagnostic statistics.
//!
//! [`ArmStats`] is a lightweight snapshot of a single arm's state, returned by
//! [`Bandit::arm_stats`](crate::bandit::Bandit::arm_stats). It does not own any
//! data from the bandit — it is a copy for diagnostics.

use crate::error::RillError;

/// A snapshot of a single arm's statistics.
///
/// Produced by [`Bandit::arm_stats`](crate::bandit::Bandit::arm_stats) for
/// diagnostics. All fields are plain values — this struct does not borrow from
/// the bandit.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ArmStats {
    /// How many times this arm has been pulled.
    pub pulls: u64,
    /// The total reward accumulated by this arm.
    pub total_reward: f64,
    /// The mean reward of this arm (`total_reward / pulls`), or `0.0` if the
    /// arm has never been pulled.
    pub mean_reward: f64,
}

impl ArmStats {
    /// Create a new `ArmStats` for an arm that has never been pulled.
    pub const fn empty() -> Self {
        Self {
            pulls: 0,
            total_reward: 0.0,
            mean_reward: 0.0,
        }
    }

    /// Create a new `ArmStats` from raw per-arm data.
    ///
    /// If `pulls` is zero, `mean_reward` is set to `0.0`.
    pub fn new(pulls: u64, total_reward: f64) -> Result<Self, RillError> {
        if !total_reward.is_finite() {
            return Err(RillError::NonFiniteValue {
                field: "total_reward",
                value: total_reward,
            });
        }
        let mean_reward = if pulls > 0 {
            total_reward / pulls as f64
        } else {
            0.0
        };
        Ok(Self {
            pulls,
            total_reward,
            mean_reward,
        })
    }
}

impl Default for ArmStats {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stats() {
        let s = ArmStats::empty();
        assert_eq!(s.pulls, 0);
        assert!((s.total_reward - 0.0).abs() < 1e-12);
        assert!((s.mean_reward - 0.0).abs() < 1e-12);
    }

    #[test]
    fn default_is_empty() {
        assert_eq!(ArmStats::default(), ArmStats::empty());
    }

    #[test]
    fn new_with_pulls() {
        let s = ArmStats::new(10, 5.0).unwrap();
        assert_eq!(s.pulls, 10);
        assert!((s.total_reward - 5.0).abs() < 1e-12);
        assert!((s.mean_reward - 0.5).abs() < 1e-12);
    }

    #[test]
    fn new_with_zero_pulls() {
        let s = ArmStats::new(0, 0.0).unwrap();
        assert_eq!(s.pulls, 0);
        assert!((s.mean_reward - 0.0).abs() < 1e-12);
    }

    #[test]
    fn new_rejects_non_finite() {
        assert!(ArmStats::new(5, f64::NAN).is_err());
        assert!(ArmStats::new(5, f64::INFINITY).is_err());
    }

    #[test]
    fn mean_reward_for_various_pulls() {
        let s = ArmStats::new(3, 9.0).unwrap();
        assert!((s.mean_reward - 3.0).abs() < 1e-12);

        let s = ArmStats::new(100, 250.0).unwrap();
        assert!((s.mean_reward - 2.5).abs() < 1e-12);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let s = ArmStats::new(42, 123.5).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let restored: ArmStats = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pulls, 42);
        assert!((restored.total_reward - 123.5).abs() < 1e-12);
        assert!((restored.mean_reward - (123.5 / 42.0)).abs() < 1e-12);
    }
}

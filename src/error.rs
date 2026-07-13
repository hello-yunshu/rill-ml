//! Error types for RillML.
//!
//! All public APIs that can receive invalid input return [`Result<_, RillError>`]
//! instead of panicking. Only truly unrecoverable internal invariant violations
//! use assertions.

use thiserror::Error;

/// The unified error type returned by all RillML public APIs.
#[derive(Debug, Error)]
pub enum RillError {
    /// A feature slice did not match the expected dimension.
    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// The expected number of features.
        expected: usize,
        /// The actual number of features provided.
        actual: usize,
    },

    /// An empty feature slice was provided where at least one feature is required.
    #[error("empty features are not allowed")]
    EmptyFeatures,

    /// A rolling window size of zero was provided.
    #[error("invalid window size: must be greater than zero")]
    InvalidWindowSize,

    /// A learning rate that is not strictly positive was provided.
    #[error("invalid learning rate: {0} (must be finite and > 0)")]
    InvalidLearningRate(f64),

    /// A generic numeric parameter was invalid.
    #[error("invalid parameter `{name}`: {value}")]
    InvalidParameter {
        /// The name of the parameter.
        name: &'static str,
        /// The invalid value.
        value: f64,
    },

    /// A NaN or Infinity value was encountered.
    #[error("non-finite value for `{field}`: {value}")]
    NonFiniteValue {
        /// Which field or quantity the bad value belongs to.
        field: &'static str,
        /// The offending value.
        value: f64,
    },

    /// A probability outside `(0, 1)` was provided.
    #[error("invalid probability: {0} (must be in (0, 1))")]
    InvalidProbability(f64),

    /// Not enough data has been observed to compute the requested quantity.
    #[error("insufficient data to compute the requested quantity")]
    InsufficientData,

    /// A serialized snapshot used an incompatible format version.
    #[error("incompatible state version: expected {expected}, got {actual}")]
    IncompatibleStateVersion {
        /// The format version the loader expects.
        expected: u32,
        /// The format version found in the snapshot.
        actual: u32,
    },

    /// Sparse features were not sorted by FeatureId.
    #[error("sparse features must be sorted by FeatureId")]
    UnsortedFeatureIds,

    /// Duplicate FeatureId encountered in sparse features.
    #[error("duplicate feature id: {0}")]
    DuplicateFeatureId(u64),

    /// FeatureHasher dimension is invalid (must be > 0).
    #[error("invalid hash dimension: {0} (must be > 0)")]
    InvalidHashDimension(usize),

    /// An unknown category was encountered by a categorical encoder.
    #[error("unknown category: {0}")]
    UnknownCategory(String),

    /// A missing value (NaN) was encountered where it is not allowed.
    #[error("missing value (NaN) at index {index}")]
    MissingValue {
        /// The index of the missing value.
        index: usize,
    },

    /// A window size or buffer capacity was invalid (must be > 0).
    #[error("invalid capacity: {0} (must be greater than zero)")]
    InvalidCapacity(usize),

    /// A significance level or probability threshold was outside `(0, 1)`.
    #[error("invalid significance level: {0} (must be in (0, 1))")]
    InvalidSignificanceLevel(f64),

    /// A bandit arm count was invalid (must be greater than zero).
    #[error("invalid arm count: {0} (must be greater than zero)")]
    InvalidArmCount(usize),

    /// An epsilon value for epsilon-greedy was outside `[0, 1]`.
    #[error("invalid epsilon: {0} (must be in [0, 1])")]
    InvalidEpsilon(f64),

    /// A bandit arm index was out of range.
    #[error("invalid arm index: {actual} (must be < {expected})")]
    InvalidArm {
        /// The number of arms (upper bound).
        expected: usize,
        /// The offending arm index.
        actual: usize,
    },

    /// A reward value was outside the valid range for the given bandit type.
    #[error("invalid reward: {0} (must be finite and in the valid range)")]
    InvalidReward(f64),

    /// A feature count was invalid (must be greater than zero).
    #[error("invalid feature count: {0} (must be greater than zero)")]
    InvalidFeatureCount(usize),

    /// A restored model violated one of its internal invariants.
    #[error("invalid model state: {0}")]
    InvalidState(String),
}

/// Helper to validate that a value is finite.
pub(crate) fn ensure_finite(field: &'static str, value: f64) -> Result<(), RillError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(RillError::NonFiniteValue { field, value })
    }
}

/// Add two values without allowing a finite-input overflow to poison state.
pub(crate) fn checked_finite_add(
    current: f64,
    delta: f64,
    field: &'static str,
) -> Result<f64, RillError> {
    let value = current + delta;
    ensure_finite(field, value)?;
    Ok(value)
}

/// Increment a long-running counter without allowing wraparound.
pub(crate) fn checked_increment(value: u64, field: &'static str) -> Result<u64, RillError> {
    value
        .checked_add(1)
        .ok_or_else(|| RillError::InvalidState(format!("{field} counter overflow")))
}

/// Helper to validate a feature slice's length and finiteness.
pub(crate) fn validate_features(expected: usize, features: &[f64]) -> Result<(), RillError> {
    if features.is_empty() {
        return Err(RillError::EmptyFeatures);
    }
    if features.len() != expected {
        return Err(RillError::DimensionMismatch {
            expected,
            actual: features.len(),
        });
    }
    for (i, &v) in features.iter().enumerate() {
        if !v.is_finite() {
            return Err(RillError::NonFiniteValue {
                field: "feature",
                value: features[i],
            });
        }
    }
    Ok(())
}

/// Helper to validate a single finite scalar target/label.
pub(crate) fn ensure_finite_target(value: f64) -> Result<(), RillError> {
    ensure_finite("target", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_mismatch_display() {
        let e = RillError::DimensionMismatch {
            expected: 3,
            actual: 2,
        };
        assert!(format!("{e}").contains("expected 3"));
        assert!(format!("{e}").contains("got 2"));
    }

    #[test]
    fn ensure_finite_passes_for_normal_values() {
        assert!(ensure_finite("x", 1.5).is_ok());
        assert!(ensure_finite("x", -1e10).is_ok());
        assert!(ensure_finite("x", 0.0).is_ok());
    }

    #[test]
    fn ensure_finite_rejects_nan_and_infinity() {
        assert!(ensure_finite("x", f64::NAN).is_err());
        assert!(ensure_finite("x", f64::INFINITY).is_err());
        assert!(ensure_finite("x", f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn validate_features_checks_dimension() {
        assert!(validate_features(3, &[1.0, 2.0, 3.0]).is_ok());
        assert!(validate_features(3, &[1.0, 2.0]).is_err());
    }

    #[test]
    fn validate_features_rejects_empty() {
        assert!(matches!(
            validate_features(0, &[]),
            Err(RillError::EmptyFeatures)
        ));
    }

    #[test]
    fn validate_features_rejects_non_finite() {
        assert!(validate_features(2, &[1.0, f64::NAN]).is_err());
        assert!(validate_features(2, &[f64::INFINITY, 2.0]).is_err());
    }

    #[test]
    fn invalid_arm_count_display() {
        let e = RillError::InvalidArmCount(0);
        assert!(format!("{e}").contains("invalid arm count"));
        assert!(format!("{e}").contains("0"));
    }

    #[test]
    fn invalid_epsilon_display() {
        let e = RillError::InvalidEpsilon(-0.5);
        assert!(format!("{e}").contains("invalid epsilon"));
        assert!(format!("{e}").contains("-0.5"));
    }

    #[test]
    fn invalid_arm_display() {
        let e = RillError::InvalidArm {
            expected: 3,
            actual: 5,
        };
        assert!(format!("{e}").contains("5"));
        assert!(format!("{e}").contains("3"));
    }

    #[test]
    fn invalid_reward_display() {
        let e = RillError::InvalidReward(f64::NAN);
        assert!(format!("{e}").contains("invalid reward"));
    }

    #[test]
    fn invalid_feature_count_display() {
        let e = RillError::InvalidFeatureCount(0);
        assert!(format!("{e}").contains("invalid feature count"));
        assert!(format!("{e}").contains("0"));
    }
}

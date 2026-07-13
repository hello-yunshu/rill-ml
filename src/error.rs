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
}

/// Helper to validate that a value is finite.
pub(crate) fn ensure_finite(field: &'static str, value: f64) -> Result<(), RillError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(RillError::NonFiniteValue { field, value })
    }
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
}

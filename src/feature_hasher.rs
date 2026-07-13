//! Feature hashing for dimensionality reduction.
//!
//! Maps high-dimensional sparse features into a fixed-dimensional dense
//! vector using a deterministic hash function. Supports signed hashing
//! to reduce collision bias.
//!
//! # Examples
//!
//! ```
//! use rill_ml::feature_hasher::FeatureHasher;
//! use rill_ml::sparse::SparseFeatures;
//!
//! let hasher = FeatureHasher::new(8, 42).unwrap();
//! let sf = SparseFeatures::from_sorted(vec![(1, 3.0), (5, -2.0)]).unwrap();
//! let dense = hasher.transform(&sf).unwrap();
//! assert_eq!(dense.len(), 8);
//! ```

use crate::error::{RillError, checked_finite_add, ensure_finite};
use crate::sparse::{FeatureId, SparseFeatures};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Configuration for [`FeatureHasher`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FeatureHasherConfig {
    /// Output dimension. Must be > 0.
    pub dimension: usize,
    /// Random seed for reproducible hashing.
    pub seed: u64,
    /// Whether to use signed hashing (alternate sign based on hash bit).
    pub signed: bool,
}

impl Default for FeatureHasherConfig {
    fn default() -> Self {
        Self {
            dimension: 1024,
            seed: 0,
            signed: true,
        }
    }
}

/// Fixed-dimension feature hasher.
///
/// Uses two independent hash functions:
/// - The first determines the target bucket (`hash1 % dimension`).
/// - The second determines the sign (`hash2 & 1`) when `signed = true`.
///
/// The hash is deterministic given the same `seed`, ensuring reproducible
/// output across runs.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FeatureHasher {
    config: FeatureHasherConfig,
}

impl FeatureHasher {
    /// Create a new hasher with the given dimension and seed.
    ///
    /// Uses signed hashing by default.
    pub fn new(dimension: usize, seed: u64) -> Result<Self, RillError> {
        Self::with_config(FeatureHasherConfig {
            dimension,
            seed,
            signed: true,
        })
    }

    /// Create a new hasher with a custom configuration.
    pub fn with_config(config: FeatureHasherConfig) -> Result<Self, RillError> {
        if config.dimension == 0 {
            return Err(RillError::InvalidHashDimension(config.dimension));
        }
        Ok(Self { config })
    }

    /// The output dimension.
    pub const fn dimension(&self) -> usize {
        self.config.dimension
    }

    /// The random seed.
    pub const fn seed(&self) -> u64 {
        self.config.seed
    }

    /// Whether signed hashing is enabled.
    pub const fn signed(&self) -> bool {
        self.config.signed
    }

    /// Hash a `FeatureId` to a `(bucket, sign)` pair.
    fn hash_id(&self, id: FeatureId) -> (usize, f64) {
        let bucket = self.hash_bucket(id);
        let sign = if self.config.signed {
            self.hash_sign(id)
        } else {
            1.0
        };
        (bucket, sign)
    }

    /// Compute the bucket index for a feature id.
    fn hash_bucket(&self, id: FeatureId) -> usize {
        let mut hasher = DefaultHasher::new();
        self.config.seed.hash(&mut hasher);
        id.hash(&mut hasher);
        (hasher.finish() as usize) % self.config.dimension
    }

    /// Compute the sign for a feature id (signed hashing).
    fn hash_sign(&self, id: FeatureId) -> f64 {
        let mut hasher = DefaultHasher::new();
        (self.config.seed.wrapping_mul(0x517cc1b727220a95)).hash(&mut hasher);
        id.hash(&mut hasher);
        if hasher.finish() & 1 == 1 { -1.0 } else { 1.0 }
    }

    /// Hash a string feature name to a `FeatureId`.
    pub fn hash_string(&self, name: &str) -> FeatureId {
        let mut hasher = DefaultHasher::new();
        self.config.seed.hash(&mut hasher);
        name.hash(&mut hasher);
        hasher.finish()
    }

    /// Create `SparseFeatures` from string name/value pairs.
    ///
    /// Each string is hashed to a `FeatureId`, then the pairs are sorted
    /// and duplicates are merged by summing values.
    pub fn hash_strings(&self, pairs: &[(&str, f64)]) -> Result<SparseFeatures, RillError> {
        let mut ids: Vec<(FeatureId, f64)> = Vec::with_capacity(pairs.len());
        for (name, value) in pairs {
            ensure_finite("hash_value", *value)?;
            ids.push((self.hash_string(name), *value));
        }
        SparseFeatures::from_unsorted(ids)
    }

    /// Transform `SparseFeatures` into a dense `Vec<f64>`.
    ///
    /// Each feature's value is added to its target bucket (multiplied by
    /// the sign if signed hashing is enabled). Collisions cause values to
    /// accumulate.
    pub fn transform(&self, features: &SparseFeatures) -> Result<Vec<f64>, RillError> {
        if self.config.dimension == 0 {
            return Err(RillError::InvalidHashDimension(0));
        }
        features.validate()?;
        let mut output = vec![0.0; self.config.dimension];
        for &(id, value) in features.values() {
            ensure_finite("sparse_value", value)?;
            let (bucket, sign) = self.hash_id(id);
            output[bucket] = checked_finite_add(output[bucket], sign * value, "hashed feature")?;
        }
        Ok(output)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FeatureHasher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct FeatureHasherState {
            config: FeatureHasherConfig,
        }

        let state = FeatureHasherState::deserialize(deserializer)?;
        Self::with_config(state.config).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reproducible_output() {
        let h = FeatureHasher::new(16, 42).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(1, 3.0), (5, -2.0), (10, 1.0)]).unwrap();
        let out1 = h.transform(&sf).unwrap();
        let out2 = h.transform(&sf).unwrap();
        assert_eq!(out1, out2);
    }

    #[test]
    fn collision_overflow_is_rejected() {
        let hasher = FeatureHasher::with_config(FeatureHasherConfig {
            dimension: 1,
            seed: 42,
            signed: false,
        })
        .unwrap();
        let features = SparseFeatures::from_sorted(vec![(1, f64::MAX), (2, f64::MAX)]).unwrap();
        assert!(hasher.transform(&features).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_zero_dimension() {
        let malformed = r#"{"config":{"dimension":0,"seed":0,"signed":true}}"#;
        assert!(serde_json::from_str::<FeatureHasher>(malformed).is_err());
    }

    #[test]
    fn different_seeds_produce_different_output() {
        let h1 = FeatureHasher::new(16, 1).unwrap();
        let h2 = FeatureHasher::new(16, 2).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(1, 1.0), (2, 2.0)]).unwrap();
        let out1 = h1.transform(&sf).unwrap();
        let out2 = h2.transform(&sf).unwrap();
        assert_ne!(out1, out2);
    }

    #[test]
    fn signed_hashing_produces_negatives() {
        let h = FeatureHasher::with_config(FeatureHasherConfig {
            dimension: 256,
            seed: 42,
            signed: true,
        })
        .unwrap();
        let sf =
            SparseFeatures::from_sorted((0..100).map(|i| (i, 1.0)).collect::<Vec<_>>()).unwrap();
        let out = h.transform(&sf).unwrap();
        // With 100 features and signed hashing, at least some should be negative
        assert!(out.iter().any(|&v| v < 0.0));
    }

    #[test]
    fn unsigned_hashing_all_positive() {
        let h = FeatureHasher::with_config(FeatureHasherConfig {
            dimension: 256,
            seed: 42,
            signed: false,
        })
        .unwrap();
        let sf =
            SparseFeatures::from_sorted((0..100).map(|i| (i, 1.0)).collect::<Vec<_>>()).unwrap();
        let out = h.transform(&sf).unwrap();
        assert!(out.iter().all(|&v| v >= 0.0));
    }

    #[test]
    fn dimension_one_all_same_bucket() {
        let h = FeatureHasher::new(1, 42).unwrap();
        let sf = SparseFeatures::from_sorted(vec![(1, 3.0), (2, 5.0)]).unwrap();
        let out = h.transform(&sf).unwrap();
        assert_eq!(out.len(), 1);
        // Both values land in bucket 0, with signs
        assert!(out[0].abs() > 0.0);
    }

    #[test]
    fn empty_features_returns_zeros() {
        let h = FeatureHasher::new(8, 42).unwrap();
        let sf = SparseFeatures::new();
        let out = h.transform(&sf).unwrap();
        assert_eq!(out, vec![0.0; 8]);
    }

    #[test]
    fn string_hash_reproducible() {
        let h = FeatureHasher::new(8, 42).unwrap();
        let id1 = h.hash_string("user_id");
        let id2 = h.hash_string("user_id");
        assert_eq!(id1, id2);
    }

    #[test]
    fn different_strings_different_ids() {
        let h = FeatureHasher::new(8, 42).unwrap();
        let id1 = h.hash_string("user_id");
        let id2 = h.hash_string("device_id");
        assert_ne!(id1, id2);
    }

    #[test]
    fn hash_strings_creates_sorted_features() {
        let h = FeatureHasher::new(8, 42).unwrap();
        let sf = h
            .hash_strings(&[("alpha", 1.0), ("beta", 2.0), ("gamma", 3.0)])
            .unwrap();
        // Should be valid sorted sparse features
        assert!(sf.validate().is_ok());
        assert_eq!(sf.len(), 3);
    }

    #[test]
    fn invalid_dimension_rejected() {
        assert!(matches!(
            FeatureHasher::new(0, 42),
            Err(RillError::InvalidHashDimension(0))
        ));
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let h = FeatureHasher::new(16, 42).unwrap();
        let json = serde_json::to_string(&h).unwrap();
        let restored: FeatureHasher = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.dimension(), 16);
        assert_eq!(restored.seed(), 42);
        assert!(restored.signed());
    }
}

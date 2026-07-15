//! Sparse feature representation for high-dimensional data.
//!
//! RillML uses a sorted `(FeatureId, value)` vector instead of
//! `HashMap<String, f64>` as the core sparse representation. This keeps
//! serialization deterministic and memory predictable.

use crate::error::{RillError, checked_finite_add};

/// Feature identifier type. Use integers, not strings.
pub type FeatureId = u64;

/// Sparse feature vector: sorted `(FeatureId, value)` pairs.
///
/// # Requirements
///
/// - Sorted by `FeatureId` in ascending order.
/// - No duplicate `FeatureId`s.
/// - Zero values may be omitted.
/// - Not a `HashMap<String, f64>`.
///
/// # Examples
///
/// ```
/// use rill_ml::sparse::SparseFeatures;
///
/// let features = SparseFeatures::from_sorted(vec![
///     (1, 0.5),
///     (3, 2.0),
///     (7, -1.0),
/// ]).unwrap();
/// assert_eq!(features.len(), 3);
/// assert_eq!(features.get(3), Some(2.0));
/// ```
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SparseFeatures {
    values: Vec<(FeatureId, f64)>,
}

impl SparseFeatures {
    /// Create an empty sparse feature vector.
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Build from a `Vec` that is already sorted by `FeatureId` and contains
    /// no duplicates.
    ///
    /// Returns an error if the input is unsorted, has duplicates, or contains
    /// non-finite values.
    pub fn from_sorted(values: Vec<(FeatureId, f64)>) -> Result<Self, RillError> {
        let result = Self { values };
        result.validate()?;
        Ok(result)
    }

    /// Build from an unsorted `Vec`, sorting and merging duplicates by
    /// summing their values.
    ///
    /// Non-finite values are rejected after merging.
    pub fn from_unsorted(mut values: Vec<(FeatureId, f64)>) -> Result<Self, RillError> {
        if values.is_empty() {
            return Ok(Self::new());
        }
        values.sort_by_key(|(id, _)| *id);

        let mut merged: Vec<(FeatureId, f64)> = Vec::with_capacity(values.len());
        for (id, val) in values {
            if let Some(last) = merged.last_mut()
                && last.0 == id
            {
                last.1 = checked_finite_add(last.1, val, "sparse value")?;
                continue;
            }
            merged.push((id, val));
        }
        let result = Self { values: merged };
        result.validate_finite()?;
        Ok(result)
    }

    /// Append a single feature. The `id` must be strictly greater than the
    /// last inserted id.
    pub fn push(&mut self, id: FeatureId, value: f64) -> Result<(), RillError> {
        if let Some(&(last_id, _)) = self.values.last()
            && id <= last_id
        {
            return Err(RillError::UnsortedFeatureIds);
        }
        if !value.is_finite() {
            return Err(RillError::NonFiniteValue {
                field: "sparse_value",
                value,
            });
        }
        self.values.push((id, value));
        Ok(())
    }

    /// Access the internal `(FeatureId, value)` slice.
    pub fn values(&self) -> &[(FeatureId, f64)] {
        &self.values
    }

    /// Number of non-zero (explicitly stored) features.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether no features are stored.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Look up the value for a given `FeatureId` via binary search.
    pub fn get(&self, id: FeatureId) -> Option<f64> {
        self.values
            .binary_search_by_key(&id, |(fid, _)| *fid)
            .ok()
            .map(|idx| self.values[idx].1)
    }

    /// Iterate over `(FeatureId, &value)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (FeatureId, f64)> + '_ {
        self.values.iter().map(|&(id, val)| (id, val))
    }

    /// Validate sorting, uniqueness, and finiteness.
    pub fn validate(&self) -> Result<(), RillError> {
        for window in self.values.windows(2) {
            if window[0].0 >= window[1].0 {
                if window[0].0 == window[1].0 {
                    return Err(RillError::DuplicateFeatureId(window[0].0));
                }
                return Err(RillError::UnsortedFeatureIds);
            }
        }
        self.validate_finite()
    }

    fn validate_finite(&self) -> Result<(), RillError> {
        for &(_, val) in &self.values {
            if !val.is_finite() {
                return Err(RillError::NonFiniteValue {
                    field: "sparse_value",
                    value: val,
                });
            }
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SparseFeatures {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct SparseFeaturesState {
            values: Vec<(FeatureId, f64)>,
        }

        let state = SparseFeaturesState::deserialize(deserializer)?;
        Self::from_sorted(state.values).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_construction() {
        let sf = SparseFeatures::new();
        assert!(sf.is_empty());
        assert_eq!(sf.len(), 0);
    }

    #[test]
    fn from_sorted_success() {
        let sf = SparseFeatures::from_sorted(vec![(1, 0.5), (3, 2.0), (7, -1.0)]).unwrap();
        assert_eq!(sf.len(), 3);
    }

    #[test]
    fn from_sorted_unsorted_rejected() {
        let result = SparseFeatures::from_sorted(vec![(3, 1.0), (1, 2.0)]);
        assert!(matches!(result, Err(RillError::UnsortedFeatureIds)));
    }

    #[test]
    fn from_sorted_duplicate_rejected() {
        let result = SparseFeatures::from_sorted(vec![(1, 0.5), (1, 2.0)]);
        assert!(matches!(result, Err(RillError::DuplicateFeatureId(1))));
    }

    #[test]
    fn duplicate_merge_rejects_overflow() {
        let result = SparseFeatures::from_unsorted(vec![(1, f64::MAX), (1, f64::MAX)]);
        assert!(result.is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_invalid_state() {
        let unsorted = r#"{"values":[[2,1.0],[1,1.0]]}"#;
        assert!(serde_json::from_str::<SparseFeatures>(unsorted).is_err());
    }

    #[test]
    fn from_unsorted_sorts_and_merges() {
        let sf = SparseFeatures::from_unsorted(vec![(3, 1.0), (1, 2.0), (3, 0.5)]).unwrap();
        assert_eq!(sf.len(), 2);
        assert_eq!(sf.get(1), Some(2.0));
        assert_eq!(sf.get(3), Some(1.5));
    }

    #[test]
    fn push_success() {
        let mut sf = SparseFeatures::new();
        sf.push(1, 0.5).unwrap();
        sf.push(5, 2.0).unwrap();
        assert_eq!(sf.len(), 2);
    }

    #[test]
    fn push_unsorted_rejected() {
        let mut sf = SparseFeatures::new();
        sf.push(5, 1.0).unwrap();
        assert!(sf.push(3, 2.0).is_err());
    }

    #[test]
    fn push_duplicate_rejected() {
        let mut sf = SparseFeatures::new();
        sf.push(5, 1.0).unwrap();
        assert!(sf.push(5, 2.0).is_err());
    }

    #[test]
    fn get_binary_search() {
        let sf = SparseFeatures::from_sorted(vec![(1, 10.0), (5, 50.0), (10, 100.0)]).unwrap();
        assert_eq!(sf.get(1), Some(10.0));
        assert_eq!(sf.get(5), Some(50.0));
        assert_eq!(sf.get(10), Some(100.0));
        assert_eq!(sf.get(3), None);
        assert_eq!(sf.get(100), None);
    }

    #[test]
    fn non_finite_rejected() {
        let result = SparseFeatures::from_sorted(vec![(1, f64::NAN)]);
        assert!(result.is_err());
        let result = SparseFeatures::from_sorted(vec![(1, f64::INFINITY)]);
        assert!(result.is_err());
    }

    #[test]
    fn iter_works() {
        let sf = SparseFeatures::from_sorted(vec![(1, 0.5), (3, 2.0)]).unwrap();
        let collected: Vec<(FeatureId, f64)> = sf.iter().collect();
        assert_eq!(collected, vec![(1, 0.5), (3, 2.0)]);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let sf = SparseFeatures::from_sorted(vec![(1, 0.5), (3, 2.0), (7, -1.0)]).unwrap();
        let json = serde_json::to_string(&sf).unwrap();
        let restored: SparseFeatures = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored.get(3), Some(2.0));
    }
}

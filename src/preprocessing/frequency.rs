//! Online frequency encoder for categorical string features.
//!
//! Each category is mapped to its observed frequency `count / total`,
//! updated incrementally as new samples are seen.

use std::collections::BTreeMap;

use crate::error::{RillError, checked_increment};

/// Online frequency encoder for string features.
///
/// Maps each category to its observed frequency `count / total`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FrequencyEncoder {
    category_counts: BTreeMap<String, u64>,
    total: u64,
    samples_seen: u64,
}

impl FrequencyEncoder {
    /// Create a new empty encoder.
    pub fn new() -> Self {
        Self {
            category_counts: BTreeMap::new(),
            total: 0,
            samples_seen: 0,
        }
    }

    /// The per-category counts.
    pub fn category_counts(&self) -> &BTreeMap<String, u64> {
        &self.category_counts
    }

    /// The total number of category observations seen.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Return the observed frequency for each input string.
    ///
    /// Unknown categories map to `0.0`. Before any sample has been seen,
    /// all outputs are `0.0`.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `features` is empty.
    pub fn transform_strs(&self, features: &[&str]) -> Result<Vec<f64>, RillError> {
        if features.is_empty() {
            return Err(RillError::EmptyFeatures);
        }
        if self.total == 0 {
            return Ok(vec![0.0; features.len()]);
        }
        let total = self.total as f64;
        Ok(features
            .iter()
            .map(|&feat| {
                self.category_counts
                    .get(feat)
                    .map(|&c| c as f64 / total)
                    .unwrap_or(0.0)
            })
            .collect())
    }

    /// Increment counts for each category in `features` and increment
    /// `samples_seen`.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `features` is empty.
    pub fn update_strs(&mut self, features: &[&str]) -> Result<(), RillError> {
        if features.is_empty() {
            return Err(RillError::EmptyFeatures);
        }
        for &feat in features {
            let count = self.category_counts.entry(feat.to_string()).or_insert(0);
            *count = checked_increment(*count, "category_count")?;
        }
        self.total = self.total.checked_add(features.len() as u64).ok_or_else(|| {
            RillError::InvalidState("total counter overflow".to_string())
        })?;
        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
        Ok(())
    }

    /// How many samples have been seen.
    pub fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    /// Reset the encoder to its initial empty state.
    pub fn reset(&mut self) {
        self.category_counts.clear();
        self.total = 0;
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_calculation() {
        let mut enc = FrequencyEncoder::new();
        // "a" x2, "b" x1 -> total 3
        enc.update_strs(&["a"]).unwrap();
        enc.update_strs(&["a", "b"]).unwrap();
        let out = enc.transform_strs(&["a"]).unwrap();
        assert!((out[0] - 2.0 / 3.0).abs() < 1e-12);
        let out = enc.transform_strs(&["b"]).unwrap();
        assert!((out[0] - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn unknown_category_returns_zero() {
        let mut enc = FrequencyEncoder::new();
        enc.update_strs(&["a"]).unwrap();
        let out = enc.transform_strs(&["z"]).unwrap();
        assert_eq!(out, vec![0.0]);
    }

    #[test]
    fn multiple_updates_accumulate() {
        let mut enc = FrequencyEncoder::new();
        enc.update_strs(&["a", "a"]).unwrap();
        enc.update_strs(&["a"]).unwrap();
        // "a" count = 3, total = 3 -> freq = 1.0
        let out = enc.transform_strs(&["a"]).unwrap();
        assert!((out[0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut enc = FrequencyEncoder::new();
        enc.update_strs(&["a", "b"]).unwrap();
        enc.reset();
        assert_eq!(enc.total(), 0);
        assert_eq!(enc.samples_seen(), 0);
        assert!(enc.category_counts().is_empty());
    }

    #[test]
    fn multiple_features_return_one_frequency_each() {
        let mut enc = FrequencyEncoder::new();
        // "a" x1, "b" x3 -> total 4
        enc.update_strs(&["b", "b", "b", "a"]).unwrap();
        let out = enc.transform_strs(&["a", "b"]).unwrap();
        assert!((out[0] - 0.25).abs() < 1e-12);
        assert!((out[1] - 0.75).abs() < 1e-12);
    }

    #[test]
    fn total_tracks_observations() {
        let mut enc = FrequencyEncoder::new();
        enc.update_strs(&["a", "b"]).unwrap();
        assert_eq!(enc.total(), 2);
        enc.update_strs(&["c"]).unwrap();
        assert_eq!(enc.total(), 3);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut enc = FrequencyEncoder::new();
        enc.update_strs(&["a", "b", "a"]).unwrap();
        let json = serde_json::to_string(&enc).unwrap();
        let restored: FrequencyEncoder = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.total(), enc.total());
        assert_eq!(restored.samples_seen(), enc.samples_seen());
        assert_eq!(restored.category_counts(), enc.category_counts());
    }
}

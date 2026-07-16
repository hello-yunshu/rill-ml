//! Online ordinal encoder for categorical string features.
//!
//! Categories are discovered online: `update_strs` adds new categories
//! to the mapping, `transform_strs` produces the integer index (as `f64`)
//! of each category using the current mapping.

use crate::error::{RillError, checked_increment};

/// Online ordinal encoder for string features.
///
/// Categories are discovered incrementally via [`update_strs`](Self::update_strs)
/// and kept in sorted order. [`transform_strs`](Self::transform_strs) maps each
/// string to its index in the sorted category list.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrdinalEncoder {
    categories: Vec<String>,
    samples_seen: u64,
}

impl OrdinalEncoder {
    /// Create a new empty encoder.
    pub fn new() -> Self {
        Self {
            categories: Vec::new(),
            samples_seen: 0,
        }
    }

    /// The known categories, kept in sorted order.
    pub fn categories(&self) -> &[String] {
        &self.categories
    }

    /// Find the index of `category` using binary search.
    pub fn category_index(&self, category: &str) -> Option<usize> {
        self.categories
            .binary_search_by(|c| c.as_str().cmp(category))
            .ok()
    }

    /// Add a category if not already present, keeping the list sorted.
    pub fn fit_one(&mut self, category: &str) {
        match self
            .categories
            .binary_search_by(|c| c.as_str().cmp(category))
        {
            Ok(_) => {}
            Err(idx) => self.categories.insert(idx, category.to_string()),
        }
    }

    /// Encode each string as its category index (as `f64`).
    ///
    /// Output length = `features.len()` (one value per input string).
    ///
    /// # Errors
    /// - [`RillError::EmptyFeatures`] if `features` is empty.
    /// - [`RillError::UnknownCategory`] if any string is not in the known
    ///   categories.
    ///
    /// Before any category has been seen, returns an empty vector.
    pub fn transform_strs(&self, features: &[&str]) -> Result<Vec<f64>, RillError> {
        if features.is_empty() {
            return Err(RillError::EmptyFeatures);
        }
        if self.categories.is_empty() {
            return Ok(Vec::new());
        }
        features
            .iter()
            .map(|&feat| {
                self.category_index(feat)
                    .map(|idx| idx as f64)
                    .ok_or_else(|| RillError::UnknownCategory(feat.to_string()))
            })
            .collect()
    }

    /// Add all new categories from `features` and increment `samples_seen`.
    ///
    /// # Errors
    /// Returns [`RillError::EmptyFeatures`] if `features` is empty.
    pub fn update_strs(&mut self, features: &[&str]) -> Result<(), RillError> {
        if features.is_empty() {
            return Err(RillError::EmptyFeatures);
        }
        for &feat in features {
            self.fit_one(feat);
        }
        self.samples_seen = checked_increment(self.samples_seen, "samples_seen")?;
        Ok(())
    }

    /// How many samples have been seen.
    pub fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    /// Reset the encoder to its initial empty state.
    pub fn reset(&mut self) {
        self.categories.clear();
        self.samples_seen = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_category_returns_correct_index() {
        let mut enc = OrdinalEncoder::new();
        enc.update_strs(&["b", "a", "c"]).unwrap();
        // sorted categories: ["a", "b", "c"] -> indices 0, 1, 2
        let out = enc.transform_strs(&["a"]).unwrap();
        assert_eq!(out, vec![0.0]);
        let out = enc.transform_strs(&["b"]).unwrap();
        assert_eq!(out, vec![1.0]);
        let out = enc.transform_strs(&["c"]).unwrap();
        assert_eq!(out, vec![2.0]);
    }

    #[test]
    fn unknown_category_rejected() {
        let mut enc = OrdinalEncoder::new();
        enc.update_strs(&["a", "b"]).unwrap();
        assert!(matches!(
            enc.transform_strs(&["z"]),
            Err(RillError::UnknownCategory(_))
        ));
    }

    #[test]
    fn new_category_added_on_update() {
        let mut enc = OrdinalEncoder::new();
        enc.update_strs(&["b"]).unwrap();
        assert_eq!(enc.categories(), &["b"]);
        enc.update_strs(&["a"]).unwrap();
        assert_eq!(enc.categories(), &["a", "b"]);
    }

    #[test]
    fn multiple_features_return_one_index_each() {
        let mut enc = OrdinalEncoder::new();
        enc.update_strs(&["a", "b", "c"]).unwrap();
        let out = enc.transform_strs(&["c", "a"]).unwrap();
        assert_eq!(out, vec![2.0, 0.0]);
    }

    #[test]
    fn reset_clears_state() {
        let mut enc = OrdinalEncoder::new();
        enc.update_strs(&["a", "b"]).unwrap();
        enc.reset();
        assert!(enc.categories().is_empty());
        assert_eq!(enc.samples_seen(), 0);
    }

    #[test]
    fn empty_input_rejected() {
        let mut enc = OrdinalEncoder::new();
        assert!(matches!(
            enc.update_strs(&[]),
            Err(RillError::EmptyFeatures)
        ));
        assert!(matches!(
            enc.transform_strs(&[]),
            Err(RillError::EmptyFeatures)
        ));
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut enc = OrdinalEncoder::new();
        enc.update_strs(&["b", "a", "c"]).unwrap();
        let json = serde_json::to_string(&enc).unwrap();
        let restored: OrdinalEncoder = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.categories(), enc.categories());
        assert_eq!(restored.samples_seen(), enc.samples_seen());
    }
}

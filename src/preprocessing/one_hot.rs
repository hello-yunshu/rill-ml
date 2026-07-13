//! Online one-hot encoder for categorical string features.
//!
//! Categories are discovered online: `update_strs` adds new categories
//! to the mapping, `transform_strs` produces a one-hot vector using
//! the current mapping.

use crate::error::RillError;

/// Online one-hot encoder for string features.
///
/// Categories are discovered incrementally via [`update_strs`](Self::update_strs).
/// Before any category is seen, `transform_strs` returns an empty vector.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OneHotEncoder {
    categories: Vec<String>,
    samples_seen: u64,
}

impl OneHotEncoder {
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

    /// One-hot encode each string.
    ///
    /// Output length = `features.len() * categories.len()`. Each group of
    /// `categories.len()` consecutive values has a `1.0` at the category
    /// index and `0.0` elsewhere.
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
        let n_cats = self.categories.len();
        let mut out = vec![0.0; features.len() * n_cats];
        for (i, &feat) in features.iter().enumerate() {
            let idx = self
                .category_index(feat)
                .ok_or_else(|| RillError::UnknownCategory(feat.to_string()))?;
            out[i * n_cats + idx] = 1.0;
        }
        Ok(out)
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
        self.samples_seen += 1;
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
    fn known_categories_encode_correctly() {
        let mut enc = OneHotEncoder::new();
        enc.update_strs(&["b", "a", "c"]).unwrap();
        // categories are sorted: ["a", "b", "c"]
        let out = enc.transform_strs(&["a"]).unwrap();
        assert_eq!(out, vec![1.0, 0.0, 0.0]);
        let out = enc.transform_strs(&["c"]).unwrap();
        assert_eq!(out, vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn unknown_category_rejected() {
        let mut enc = OneHotEncoder::new();
        enc.update_strs(&["a", "b"]).unwrap();
        assert!(matches!(
            enc.transform_strs(&["z"]),
            Err(RillError::UnknownCategory(_))
        ));
    }

    #[test]
    fn new_category_added_on_update() {
        let mut enc = OneHotEncoder::new();
        enc.update_strs(&["a"]).unwrap();
        assert_eq!(enc.categories(), &["a"]);
        enc.update_strs(&["b"]).unwrap();
        assert_eq!(enc.categories(), &["a", "b"]);
    }

    #[test]
    fn multiple_features_produce_concatenated_vectors() {
        let mut enc = OneHotEncoder::new();
        enc.update_strs(&["a", "b"]).unwrap();
        // two features, two categories -> length 4
        let out = enc.transform_strs(&["a", "b"]).unwrap();
        assert_eq!(out, vec![1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn reset_clears_state() {
        let mut enc = OneHotEncoder::new();
        enc.update_strs(&["a", "b"]).unwrap();
        enc.reset();
        assert!(enc.categories().is_empty());
        assert_eq!(enc.samples_seen(), 0);
    }

    #[test]
    fn samples_seen_tracks_updates() {
        let mut enc = OneHotEncoder::new();
        assert_eq!(enc.samples_seen(), 0);
        enc.update_strs(&["a"]).unwrap();
        assert_eq!(enc.samples_seen(), 1);
        enc.update_strs(&["b", "c"]).unwrap();
        assert_eq!(enc.samples_seen(), 2);
    }

    #[test]
    fn empty_input_rejected() {
        let mut enc = OneHotEncoder::new();
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
    fn empty_categories_returns_empty_vec() {
        let enc = OneHotEncoder::new();
        // no categories seen yet -> empty vec, not an error
        let out = enc.transform_strs(&["a"]).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serde_roundtrip() {
        let mut enc = OneHotEncoder::new();
        enc.update_strs(&["b", "a", "c"]).unwrap();
        let json = serde_json::to_string(&enc).unwrap();
        let restored: OneHotEncoder = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.categories(), enc.categories());
        assert_eq!(restored.samples_seen(), enc.samples_seen());
    }
}

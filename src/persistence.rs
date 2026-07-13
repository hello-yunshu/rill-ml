//! Model state persistence via a versioned [`Snapshot`] envelope.
//!
//! Only available when the `serde` feature is enabled.

use crate::error::RillError;

/// The current snapshot format version.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// A versioned envelope around a serializable model state.
///
/// Versioning is centralized here so individual models do not need to
/// duplicate format-version fields.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use rill_ml::persistence::Snapshot;
/// use rill_ml::stats::Mean;
/// use rill_ml::OnlineStatistic;
///
/// let mut mean = Mean::new();
/// mean.update(1.0).unwrap();
/// mean.update(2.0).unwrap();
///
/// let snap = Snapshot::new(mean);
/// let json = serde_json::to_string(&snap).unwrap();
/// let restored: Snapshot<Mean> = serde_json::from_str(&json).unwrap();
/// let m = restored.into_model().unwrap();
/// assert!((m.value() - 1.5).abs() < 1e-12);
/// # }
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Snapshot<T> {
    /// The format version of this snapshot.
    pub format_version: u32,
    /// The model state.
    pub model: T,
}

impl<T> Snapshot<T> {
    /// Wrap a model in a new snapshot with the current format version.
    pub fn new(model: T) -> Self {
        Self {
            format_version: SNAPSHOT_FORMAT_VERSION,
            model,
        }
    }

    /// Consume the snapshot and return the model, verifying the format version.
    ///
    /// Returns [`RillError::IncompatibleStateVersion`] if the version does not
    /// match [`SNAPSHOT_FORMAT_VERSION`].
    pub fn into_model(self) -> Result<T, RillError> {
        if self.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(RillError::IncompatibleStateVersion {
                expected: SNAPSHOT_FORMAT_VERSION,
                actual: self.format_version,
            });
        }
        Ok(self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Mean;
    #[cfg(feature = "serde")]
    use crate::traits::OnlineStatistic;

    #[cfg(feature = "serde")]
    #[test]
    fn snapshot_roundtrip() {
        let mut mean = Mean::new();
        mean.update(1.0).unwrap();
        mean.update(2.0).unwrap();
        let snap = Snapshot::new(mean);
        let json = serde_json::to_string(&snap).unwrap();
        let restored: Snapshot<Mean> = serde_json::from_str(&json).unwrap();
        let m = restored.into_model().unwrap();
        assert!((m.value() - 1.5).abs() < 1e-12);
        assert_eq!(m.count(), 2);
    }

    #[test]
    fn incompatible_version_rejected() {
        let snap = Snapshot {
            format_version: 999,
            model: Mean::new(),
        };
        assert!(snap.into_model().is_err());
    }
}

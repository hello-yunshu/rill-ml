//! Model state persistence via a versioned [`Snapshot`] envelope.
//!
//! Only available when the `serde` feature is enabled.

use crate::error::RillError;

/// The current snapshot format version.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Maximum byte length accepted by the validated restore entry points
/// (`from_json` in Python/WASM bindings and
/// [`Snapshot::from_json_validated`]).
///
/// The limit is intentionally generous (64 MiB) so that legitimately large
/// model state (e.g. high-dimensional FTRL or LinUCB) is not rejected, while
/// still bounding memory growth from untrusted JSON input. The limit is
/// enforced on the raw JSON byte length *before* deserialization so a
/// malicious payload cannot allocate a large intermediate `serde_json::Value`
/// tree.
pub const MAX_SNAPSHOT_JSON_BYTES: usize = 64 * 1024 * 1024;

/// Unified state-validation interface for restorable model types.
///
/// Implementations enforce type-specific invariants that cannot be violated
/// by a semantically valid `serde` deserialization. This trait is the single
/// hook used by [`Snapshot::into_validated_model`] and by every Python/WASM
/// `from_json` entry point, so adding a new restorable type only requires
/// implementing this trait once.
///
/// # When to implement
///
/// Implement `ValidateState` for every public type that can appear inside a
/// [`Snapshot`] and be restored from untrusted JSON. At minimum this covers
/// all types exposed via Python and WASM `from_json`.
///
/// # What to check
///
/// - Dimensions / vector lengths match the type's own recorded feature count.
/// - All stored floating-point values are finite.
/// - Counts and statistics are non-negative.
/// - Optimizer parameter counts match the model's feature count.
/// - Encoder mapping consistency (no dangling indices).
/// - Pipeline transformer/model dimensions agree.
/// - Bandit arm state length matches the configured arm count.
/// - Drift detector buffer length respects the configured capacity.
///
/// # Errors
///
/// Return [`RillError::InvalidState`] with a descriptive message so callers
/// can distinguish validation failures from version mismatches.
pub trait ValidateState {
    /// Validate the in-memory state of this type.
    ///
    /// This method must be idempotent and must not mutate `self`.
    fn validate_state(&self) -> Result<(), RillError>;
}

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

    /// Consume the snapshot, verify its format version, and run an
    /// application-provided model-state validator before returning the model.
    ///
    /// The snapshot envelope can only validate its own version field because
    /// `T` may be an application type. Use this method at trust boundaries to
    /// enforce model-specific invariants before activating restored state.
    ///
    /// # Errors
    ///
    /// Returns [`RillError::IncompatibleStateVersion`] for a version mismatch,
    /// or propagates the validator's error.
    pub fn into_model_with_validation<F>(self, validate: F) -> Result<T, RillError>
    where
        F: FnOnce(&T) -> Result<(), RillError>,
    {
        let model = self.into_model()?;
        validate(&model)?;
        Ok(model)
    }
}

impl<T: ValidateState> Snapshot<T> {
    /// Consume the snapshot, verify its format version, and run the
    /// type-specific [`ValidateState`] validator before returning the model.
    ///
    /// This is the required restore path at trust boundaries (Python/WASM
    /// `from_json`, IPC state restore, etc.). It is atomic: on error, no
    /// model is returned and no half-validated state is activated.
    ///
    /// # Errors
    ///
    /// Returns [`RillError::IncompatibleStateVersion`] for a version mismatch,
    /// or propagates the [`ValidateState`] error.
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
    /// let snap = Snapshot::new(mean);
    /// let json = serde_json::to_string(&snap).unwrap();
    /// let restored: Snapshot<Mean> = serde_json::from_str(&json).unwrap();
    /// let m = restored.into_validated_model().unwrap();
    /// assert_eq!(m.count(), 1);
    /// # }
    /// ```
    pub fn into_validated_model(self) -> Result<T, RillError> {
        let model = self.into_model()?;
        model.validate_state()?;
        Ok(model)
    }

    /// Deserialize a snapshot from JSON, enforce the byte-size limit, verify
    /// the format version, and run the type-specific [`ValidateState`]
    /// validator before returning the model.
    ///
    /// This is the single entry point that Python/WASM `from_json` and any
    /// other untrusted-state restore path must call. It enforces
    /// [`MAX_SNAPSHOT_JSON_BYTES`] on the raw input *before* deserialization.
    ///
    /// # Errors
    ///
    /// - [`RillError::InvalidState`] if the input exceeds
    ///   [`MAX_SNAPSHOT_JSON_BYTES`].
    /// - [`RillError::IncompatibleStateVersion`] for a version mismatch.
    /// - Propagates the serde error if the JSON is malformed.
    /// - Propagates the [`ValidateState`] error if the model state is invalid.
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
    /// let json = serde_json::to_string(&Snapshot::new(mean)).unwrap();
    /// let m: Mean = Snapshot::from_json_validated(&json).unwrap();
    /// assert_eq!(m.count(), 1);
    /// # }
    /// ```
    #[cfg(feature = "serde")]
    pub fn from_json_validated(json: &str) -> Result<T, RillError>
    where
        T: serde::de::DeserializeOwned,
    {
        if json.len() > MAX_SNAPSHOT_JSON_BYTES {
            return Err(RillError::InvalidState(format!(
                "snapshot JSON exceeds the maximum byte limit ({} > {})",
                json.len(),
                MAX_SNAPSHOT_JSON_BYTES
            )));
        }
        let snap: Snapshot<T> =
            serde_json::from_str(json).map_err(|e| RillError::InvalidState(e.to_string()))?;
        snap.into_validated_model()
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

    #[test]
    fn application_validation_runs_before_activation() {
        let snap = Snapshot::new(Mean::new());
        let result = snap.into_model_with_validation(|_| {
            Err(RillError::InvalidState(
                "application check failed".to_owned(),
            ))
        });
        assert!(matches!(result, Err(RillError::InvalidState(_))));
    }
}

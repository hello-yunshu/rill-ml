//! Deterministic feature-schema and model identity descriptors.
//!
//! Descriptors contain schema metadata, never feature values or training data.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::RillError;
#[cfg(feature = "serde")]
use crate::ValidateState;

/// Maximum number of features in one descriptor.
pub const MAX_SCHEMA_FEATURES: usize = 4_096;
/// Maximum UTF-8 byte length for a descriptor string.
pub const MAX_DESCRIPTOR_STRING_BYTES: usize = 256;
/// Maximum metadata entries on a single feature.
pub const MAX_FEATURE_METADATA_ENTRIES: usize = 64;

/// Optional numeric domain declared for a feature.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct FeatureConstraint {
    /// Inclusive lower bound.
    pub min: Option<f64>,
    /// Inclusive upper bound.
    pub max: Option<f64>,
}

impl FeatureConstraint {
    /// Validate finite bounds and their ordering.
    pub fn validate(&self) -> Result<(), RillError> {
        if self.min.is_some_and(|value| !value.is_finite())
            || self.max.is_some_and(|value| !value.is_finite())
        {
            return Err(RillError::InvalidState(
                "feature constraints must be finite".to_owned(),
            ));
        }
        if let (Some(min), Some(max)) = (self.min, self.max)
            && min > max
        {
            return Err(RillError::InvalidState(
                "feature constraint minimum exceeds maximum".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Semantic description of one ordered input feature.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct FeatureDescriptor {
    /// Stable feature name.
    pub name: String,
    /// Optional unit such as `seconds` or `bytes`.
    pub unit: Option<String>,
    /// Optional caller-defined transform identifier.
    pub transform: Option<String>,
    /// Optional numeric domain.
    pub constraint: Option<FeatureConstraint>,
    /// Bounded semantic metadata. Ordered storage makes hashing canonical.
    pub metadata: BTreeMap<String, String>,
}

impl FeatureDescriptor {
    /// Create a descriptor with no optional metadata.
    pub fn new(name: impl Into<String>) -> Result<Self, RillError> {
        let descriptor = Self {
            name: name.into(),
            unit: None,
            transform: None,
            constraint: None,
            metadata: BTreeMap::new(),
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Validate size, numeric, and metadata bounds.
    pub fn validate(&self) -> Result<(), RillError> {
        validate_string("feature name", &self.name, false)?;
        if let Some(unit) = &self.unit {
            validate_string("feature unit", unit, false)?;
        }
        if let Some(transform) = &self.transform {
            validate_string("feature transform", transform, false)?;
        }
        if self.metadata.len() > MAX_FEATURE_METADATA_ENTRIES {
            return Err(RillError::InvalidState(format!(
                "feature metadata exceeds limit {}",
                MAX_FEATURE_METADATA_ENTRIES
            )));
        }
        for (key, value) in &self.metadata {
            validate_string("feature metadata key", key, false)?;
            validate_string("feature metadata value", value, true)?;
        }
        if let Some(constraint) = self.constraint {
            constraint.validate()?;
        }
        Ok(())
    }
}

/// Deterministic SHA-256 identity of a feature schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FeatureSchemaHash([u8; 32]);

impl FeatureSchemaHash {
    /// Raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hexadecimal digest.
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
        }
        output
    }
}

impl fmt::Display for FeatureSchemaHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// Ordered feature schema with an explicit caller-owned version.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct FeatureSchema {
    /// Non-zero schema version.
    pub version: u32,
    /// Ordered feature descriptors. Order is part of the hash.
    pub features: Vec<FeatureDescriptor>,
}

impl FeatureSchema {
    /// Construct and validate an ordered schema.
    pub fn new(version: u32, features: Vec<FeatureDescriptor>) -> Result<Self, RillError> {
        let schema = Self { version, features };
        schema.validate()?;
        Ok(schema)
    }

    /// Validate version, capacity, descriptor, and unique-name invariants.
    pub fn validate(&self) -> Result<(), RillError> {
        if self.version == 0 {
            return Err(RillError::InvalidState(
                "feature schema version must be non-zero".to_owned(),
            ));
        }
        if self.features.is_empty() || self.features.len() > MAX_SCHEMA_FEATURES {
            return Err(RillError::InvalidState(format!(
                "feature count must be in 1..={MAX_SCHEMA_FEATURES}"
            )));
        }
        let mut names = std::collections::BTreeSet::new();
        for feature in &self.features {
            feature.validate()?;
            if !names.insert(feature.name.as_str()) {
                return Err(RillError::InvalidState(format!(
                    "duplicate feature name `{}`",
                    feature.name
                )));
            }
        }
        Ok(())
    }

    /// Compute a canonical SHA-256 digest without relying on JSON map order.
    pub fn hash(&self) -> Result<FeatureSchemaHash, RillError> {
        self.validate()?;
        let mut hasher = Sha256::new();
        hasher.update(b"rill-feature-schema-v1\0");
        hasher.update(self.version.to_be_bytes());
        write_len(&mut hasher, self.features.len());
        for feature in &self.features {
            write_string(&mut hasher, &feature.name);
            write_optional_string(&mut hasher, feature.unit.as_deref());
            write_optional_string(&mut hasher, feature.transform.as_deref());
            match feature.constraint {
                None => hasher.update([0]),
                Some(constraint) => {
                    hasher.update([1]);
                    write_optional_f64(&mut hasher, constraint.min);
                    write_optional_f64(&mut hasher, constraint.max);
                }
            }
            write_len(&mut hasher, feature.metadata.len());
            for (key, value) in &feature.metadata {
                write_string(&mut hasher, key);
                write_string(&mut hasher, value);
            }
        }
        Ok(FeatureSchemaHash(hasher.finalize().into()))
    }
}

/// Algorithm and state-format identity independent of product semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct AlgorithmDescriptor {
    /// Stable algorithm name.
    pub name: String,
    /// Algorithm implementation/version label.
    pub algorithm_version: String,
    /// State schema/version label.
    pub state_version: String,
}

impl AlgorithmDescriptor {
    /// Construct and validate algorithm identity.
    pub fn new(
        name: impl Into<String>,
        algorithm_version: impl Into<String>,
        state_version: impl Into<String>,
    ) -> Result<Self, RillError> {
        let descriptor = Self {
            name: name.into(),
            algorithm_version: algorithm_version.into(),
            state_version: state_version.into(),
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Validate bounded non-empty strings.
    pub fn validate(&self) -> Result<(), RillError> {
        validate_string("algorithm name", &self.name, false)?;
        validate_string("algorithm version", &self.algorithm_version, false)?;
        validate_string("algorithm state version", &self.state_version, false)
    }
}

/// Identity checked when activating persisted model state.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ModelDescriptor {
    /// Algorithm/state identity.
    pub algorithm: AlgorithmDescriptor,
    /// Exact ordered feature schema identity.
    pub feature_schema_hash: FeatureSchemaHash,
    /// Optional caller-generated configuration digest.
    pub configuration_digest: Option<[u8; 32]>,
}

impl ModelDescriptor {
    /// Construct and validate a descriptor.
    pub fn new(
        algorithm: AlgorithmDescriptor,
        feature_schema_hash: FeatureSchemaHash,
        configuration_digest: Option<[u8; 32]>,
    ) -> Result<Self, RillError> {
        let descriptor = Self {
            algorithm,
            feature_schema_hash,
            configuration_digest,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Validate nested identity state.
    pub fn validate(&self) -> Result<(), RillError> {
        self.algorithm.validate()
    }

    /// Reject state activated under a different feature schema.
    pub fn ensure_schema(&self, schema: &FeatureSchema) -> Result<(), RillError> {
        let actual = schema.hash()?;
        if actual != self.feature_schema_hash {
            return Err(RillError::InvalidState(format!(
                "feature schema hash mismatch: expected {}, got {}",
                self.feature_schema_hash, actual
            )));
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl ValidateState for FeatureDescriptor {
    fn validate_state(&self) -> Result<(), RillError> {
        self.validate()
    }
}

#[cfg(feature = "serde")]
impl ValidateState for FeatureSchema {
    fn validate_state(&self) -> Result<(), RillError> {
        self.validate()
    }
}

#[cfg(feature = "serde")]
impl ValidateState for AlgorithmDescriptor {
    fn validate_state(&self) -> Result<(), RillError> {
        self.validate()
    }
}

#[cfg(feature = "serde")]
impl ValidateState for ModelDescriptor {
    fn validate_state(&self) -> Result<(), RillError> {
        self.validate()
    }
}

fn validate_string(field: &str, value: &str, allow_empty: bool) -> Result<(), RillError> {
    if (!allow_empty && value.is_empty()) || value.len() > MAX_DESCRIPTOR_STRING_BYTES {
        return Err(RillError::InvalidState(format!(
            "{field} must contain {}..={MAX_DESCRIPTOR_STRING_BYTES} bytes",
            usize::from(!allow_empty)
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(RillError::InvalidState(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

fn write_len(hasher: &mut Sha256, value: usize) {
    hasher.update((value as u64).to_be_bytes());
}

fn write_string(hasher: &mut Sha256, value: &str) {
    write_len(hasher, value.len());
    hasher.update(value.as_bytes());
}

fn write_optional_string(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            write_string(hasher, value);
        }
        None => hasher.update([0]),
    }
}

fn write_optional_f64(hasher: &mut Sha256, value: Option<f64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_bits().to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn feature(name: &str) -> FeatureDescriptor {
        FeatureDescriptor::new(name).unwrap()
    }

    #[test]
    fn feature_order_and_version_change_hash() {
        let a = FeatureSchema::new(1, vec![feature("a"), feature("b")]).unwrap();
        let b = FeatureSchema::new(1, vec![feature("b"), feature("a")]).unwrap();
        let c = FeatureSchema::new(2, vec![feature("a"), feature("b")]).unwrap();
        assert_ne!(a.hash().unwrap(), b.hash().unwrap());
        assert_ne!(a.hash().unwrap(), c.hash().unwrap());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn metadata_insertion_and_json_map_order_do_not_change_hash() {
        let mut first = feature("latency");
        first
            .metadata
            .insert("source".to_owned(), "host".to_owned());
        first
            .metadata
            .insert("kind".to_owned(), "numeric".to_owned());
        let schema = FeatureSchema::new(1, vec![first]).unwrap();

        let expected = schema.hash().unwrap();
        let json = r#"{"version":1,"features":[{"name":"latency","unit":null,"transform":null,"constraint":null,"metadata":{"kind":"numeric","source":"host"}}]}"#;
        let decoded: FeatureSchema = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.hash().unwrap(), expected);
    }

    #[test]
    fn descriptor_rejects_schema_mismatch() {
        let schema = FeatureSchema::new(1, vec![feature("x")]).unwrap();
        let descriptor = ModelDescriptor::new(
            AlgorithmDescriptor::new("linucb", "1", "1").unwrap(),
            schema.hash().unwrap(),
            None,
        )
        .unwrap();
        descriptor.ensure_schema(&schema).unwrap();
        let other = FeatureSchema::new(1, vec![feature("y")]).unwrap();
        assert!(descriptor.ensure_schema(&other).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_preserves_identity() {
        let schema = FeatureSchema::new(1, vec![feature("x")]).unwrap();
        let json = serde_json::to_string(&schema).unwrap();
        let restored: FeatureSchema = serde_json::from_str(&json).unwrap();
        restored.validate_state().unwrap();
        assert_eq!(restored.hash().unwrap(), schema.hash().unwrap());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn golden_schema_fixture_has_stable_hash() {
        let schema: FeatureSchema = serde_json::from_str(include_str!(
            "../tests/fixtures/descriptor/feature-schema-v1.json"
        ))
        .unwrap();
        schema.validate_state().unwrap();
        assert_eq!(
            schema.hash().unwrap().to_hex(),
            "7ac14564ad6e4c1581185d4e8f84cb42ff5e939a3f08df4213be34cdbc8e73e2"
        );
    }

    proptest! {
        #[test]
        fn hash_is_deterministic_for_valid_names(names in prop::collection::vec("[a-z]{1,12}", 1..32)) {
            let mut unique = names;
            unique.sort();
            unique.dedup();
            prop_assume!(!unique.is_empty());
            let features = unique.iter().map(|name| feature(name)).collect();
            let schema = FeatureSchema::new(1, features).unwrap();
            prop_assert_eq!(schema.hash().unwrap(), schema.hash().unwrap());
        }
    }
}

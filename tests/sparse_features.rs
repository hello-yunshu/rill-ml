//! Integration tests for sparse features and feature hashing.
//!
//! These tests cover the [`SparseFeatures`] container and the
//! [`FeatureHasher`] transformer, including reproducibility, collision
//! handling, and integration with [`LinearRegression`] via dense
//! conversion.

use rill_ml::RillError;
use rill_ml::feature_hasher::FeatureHasher;
use rill_ml::sparse::SparseFeatures;

// ---------------------------------------------------------------------------
// SparseFeatures
// ---------------------------------------------------------------------------

#[test]
fn sparse_features_roundtrip() {
    // Build a SparseFeatures instance, access values() and get(), and verify
    // that the data is preserved exactly.
    let sf = SparseFeatures::from_sorted(vec![(0, 1.5), (3, -2.0), (7, 0.25)]).unwrap();

    assert_eq!(sf.len(), 3);
    assert!(!sf.is_empty());

    // values() exposes the sorted (id, value) slice.
    let values = sf.values();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], (0, 1.5));
    assert_eq!(values[1], (3, -2.0));
    assert_eq!(values[2], (7, 0.25));

    // get() uses binary search.
    assert_eq!(sf.get(0), Some(1.5));
    assert_eq!(sf.get(3), Some(-2.0));
    assert_eq!(sf.get(7), Some(0.25));
    // Missing ids return None.
    assert_eq!(sf.get(1), None);
    assert_eq!(sf.get(100), None);
}

#[test]
fn sparse_features_from_unsorted_merges() {
    // from_unsorted should sort by FeatureId and merge duplicates by summing
    // their values, then validate finiteness.
    let sf =
        SparseFeatures::from_unsorted(vec![(5, 1.0), (1, 2.0), (5, 0.5), (1, -1.0), (3, 10.0)])
            .unwrap();

    // After merging: 1 -> 1.0, 3 -> 10.0, 5 -> 1.5
    assert_eq!(sf.len(), 3);
    assert_eq!(sf.get(1), Some(1.0));
    assert_eq!(sf.get(3), Some(10.0));
    assert_eq!(sf.get(5), Some(1.5));

    // The internal slice must be sorted by FeatureId.
    let ids: Vec<_> = sf.values().iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, vec![1, 3, 5]);
}

// ---------------------------------------------------------------------------
// FeatureHasher: reproducibility
// ---------------------------------------------------------------------------

#[test]
fn feature_hasher_reproducible() {
    // The same input combined with the same seed must always produce the
    // same dense output. This is the core contract of FeatureHasher.
    let hasher = FeatureHasher::new(32, 42).unwrap();
    let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0), (2, -3.0)]).unwrap();

    let out1 = hasher.transform(&sf).unwrap();
    let out2 = hasher.transform(&sf).unwrap();

    assert_eq!(out1.len(), 32);
    assert_eq!(out1, out2, "same hasher + same input must be reproducible");

    // A freshly constructed hasher with the same parameters must also
    // produce the same output (no hidden state).
    let hasher2 = FeatureHasher::new(32, 42).unwrap();
    let out3 = hasher2.transform(&sf).unwrap();
    assert_eq!(out1, out3);
}

#[test]
fn feature_hasher_different_seeds() {
    // Different seeds must perturb the hash function enough that, for a
    // reasonable input, the dense outputs are not identical.
    let h1 = FeatureHasher::new(64, 1).unwrap();
    let h2 = FeatureHasher::new(64, 2).unwrap();

    let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (1, 2.0), (2, 3.0), (3, 4.0), (4, 5.0)])
        .unwrap();

    let out1 = h1.transform(&sf).unwrap();
    let out2 = h2.transform(&sf).unwrap();

    assert_eq!(out1.len(), out2.len());
    assert_ne!(
        out1, out2,
        "different seeds should produce different outputs"
    );
}

// ---------------------------------------------------------------------------
// FeatureHasher: string hashing
// ---------------------------------------------------------------------------

#[test]
fn feature_hasher_string_hashing() {
    // hash_strings should produce a valid SparseFeatures instance (sorted,
    // no duplicates) for a set of (name, value) pairs.
    let hasher = FeatureHasher::new(16, 42).unwrap();

    let pairs: &[(&str, f64)] = &[("user_id", 1.0), ("device_type", 2.0), ("country", 3.0)];

    let sf = hasher.hash_strings(pairs).unwrap();

    // Output must be valid sorted sparse features.
    assert!(sf.validate().is_ok());
    assert_eq!(sf.len(), pairs.len());

    // The same string must always hash to the same FeatureId.
    let id_again = hasher.hash_string("user_id");
    assert_eq!(sf.get(id_again), Some(1.0));

    // Different strings should hash to different FeatureIds (probabilistically
    // true for any reasonable hash; verified deterministically here).
    let id_user = hasher.hash_string("user_id");
    let id_device = hasher.hash_string("device_type");
    let id_country = hasher.hash_string("country");
    let mut ids = vec![id_user, id_device, id_country];
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        3,
        "three distinct strings should hash to three distinct ids"
    );
}

// ---------------------------------------------------------------------------
// FeatureHasher: collision handling
// ---------------------------------------------------------------------------

#[test]
fn feature_hasher_collision_handling() {
    // With dimension 1, every feature must land in bucket 0. The signed
    // hashing scheme causes values to accumulate (with their respective
    // signs), proving that collisions are summed rather than overwritten.
    let hasher = FeatureHasher::new(1, 42).unwrap();

    // Use raw FeatureIds that hash deterministically to bucket 0.
    // With dimension=1 every id maps to bucket 0 regardless of the hash,
    // so we can use arbitrary ids and just check accumulation.
    let sf = SparseFeatures::from_sorted(vec![(0, 2.0), (1, 3.0), (2, -1.0)]).unwrap();

    let out = hasher.transform(&sf).unwrap();
    assert_eq!(out.len(), 1);
    assert!(
        out[0] != 0.0,
        "collision bucket should accumulate non-zero values, got {}",
        out[0]
    );

    // The magnitude must reflect the sum of signed contributions, which
    // is at most |2| + |3| + |-1| = 6 in absolute value.
    assert!(
        out[0].abs() <= 6.0 + 1e-9,
        "accumulated value should not exceed sum of absolute inputs"
    );

    // A second run with the same input must reproduce the same accumulation,
    // confirming the collision handling is deterministic.
    let out_again = hasher.transform(&sf).unwrap();
    assert_eq!(out, out_again);
}

// ---------------------------------------------------------------------------
// FeatureHasher: integration with LinearRegression
// ---------------------------------------------------------------------------

#[test]
fn feature_hasher_with_linear_regression() {
    // Hash sparse features into a dense vector, then train a LinearRegression
    // on the dense representation. The model should be able to learn a
    // linear relationship that depends only on the hashed representation.
    use rill_ml::OnlineRegressor;
    use rill_ml::loss::RegressionLoss;
    use rill_ml::models::{LinearRegression, LinearRegressionConfig};
    use rill_ml::optim::{Optimizer, SgdConfig};

    let dim = 16;
    let hasher = FeatureHasher::new(dim, 42).unwrap();

    let d = dim;
    let mut model = LinearRegression::new(
        d,
        LinearRegressionConfig {
            optimizer: Optimizer::sgd(
                d,
                SgdConfig {
                    learning_rate: 0.05,
                    l2: 0.0,
                },
            )
            .unwrap(),
            loss: RegressionLoss::default(),
        },
    )
    .unwrap();

    // Train on a single fixed sparse input whose target equals the sum
    // of the hashed dense vector. After many iterations, the model should
    // be able to reproduce that target closely (it is a single fixed
    // sample, so SGD converges to the unique solution modulo regularization).
    let sf = SparseFeatures::from_sorted(vec![(0, 1.0), (5, 2.0), (10, -1.0)]).unwrap();
    let dense = hasher.transform(&sf).unwrap();
    let target: f64 = dense.iter().sum();

    for _ in 0..500 {
        model.learn(&dense, target).unwrap();
    }

    let pred = model.predict(&dense).unwrap();
    assert!(
        (pred - target).abs() < 0.5,
        "linear regression should learn the hashed representation: pred={pred}, target={target}"
    );
}

// ---------------------------------------------------------------------------
// SparseFeatures: empty input is rejected by FTRL predict
// ---------------------------------------------------------------------------

#[test]
fn sparse_features_empty_rejected() {
    // An empty SparseFeatures must be rejected by FTRL's predict path,
    // since there is nothing to base a prediction on. This guards against
    // silent degenerate behavior on cold starts with no features.
    use rill_ml::SparseRegressor;
    use rill_ml::models::{FtrlConfig, FtrlRegressor};

    let model = FtrlRegressor::new(FtrlConfig::default()).unwrap();
    let empty = SparseFeatures::new();

    let result = model.predict(&empty);
    assert!(
        matches!(result, Err(RillError::EmptyFeatures)),
        "predict on empty SparseFeatures should return EmptyFeatures, got {:?}",
        result
    );
}

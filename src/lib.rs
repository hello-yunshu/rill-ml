//! # RillML
//!
//! RillML (Rill) is a lightweight adaptive intelligence runtime for native and
//! edge applications. This crate is its adaptive intelligence core library:
//! lightweight, serializable online machine learning for Rust applications
//! and streaming data.
//!
//! RillML provides incremental learning primitives that can be embedded
//! directly in native Rust applications: online statistics, preprocessors,
//! linear/logistic regression, evaluation metrics, pipelines, progressive
//! evaluation, drift detection, online decision-making (bandits), and optional
//! serde-based state persistence.
//!
//! ## Quick start
//!
//! ```rust
//! use rill_ml::{
//!     metrics::Mae,
//!     models::{LinearRegression, LinearRegressionConfig},
//!     optim::{Optimizer, SgdConfig},
//!     pipeline::RegressionPipeline,
//!     preprocessing::StandardScaler,
//!     Metric, OnlineRegressor,
//! };
//!
//! let feature_count = 2;
//! let scaler = StandardScaler::new(feature_count).unwrap();
//! let mut sgd = SgdConfig::default();
//! sgd.learning_rate = 0.05;
//! sgd.l2 = 0.0;
//! let optimizer = Optimizer::sgd(feature_count, sgd).unwrap();
//! let mut lr_config = LinearRegressionConfig::default();
//! lr_config.optimizer = optimizer;
//! let regression = LinearRegression::new(feature_count, lr_config).unwrap();
//! let mut model = RegressionPipeline::new(scaler, regression).unwrap();
//! let mut mae = Mae::default();
//!
//! let samples = [
//!     ([0.1, 0.2], 0.5),
//!     ([0.3, 0.8], 1.4),
//!     ([0.6, 0.4], 1.1),
//! ];
//! for (features, target) in samples {
//!     let prediction = model.predict(&features).unwrap();
//!     mae.update(target, prediction).unwrap();
//!     model.learn(&features, target).unwrap();
//! }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "bandit")]
#[cfg_attr(docsrs, doc(cfg(feature = "bandit")))]
pub mod bandit;
pub mod decision;
pub mod descriptor;
pub mod diagnostics;
pub mod drift;
pub mod error;
pub mod evaluate;
pub mod feature_hasher;
pub mod loss;
pub mod metrics;
pub mod models;
pub mod optim;
pub mod persistence;
pub mod pipeline;
pub mod preprocessing;
#[cfg(feature = "bandit")]
pub mod replay;
pub mod sparse;
pub mod stats;
pub mod traits;
pub mod weighted;

pub use error::RillError;
pub use evaluate::{BinaryClassificationSample, RegressionSample};
pub use persistence::{MAX_SNAPSHOT_JSON_BYTES, SNAPSHOT_FORMAT_VERSION, Snapshot, ValidateState};
pub use traits::{
    Metric, OnlineBinaryClassifier, OnlineRegressor, OnlineStatistic, SparseClassifier,
    SparseRegressor, Transformer,
};
pub use weighted::{WeightedOnlineBinaryClassifier, WeightedOnlineRegressor, WeightedStatistic};

/// Version of the `rill-ml` crate as compiled into this library.
/// Additive constant; reflects the Stable crate version of this build.
pub const RILL_VERSION: &str = env!("CARGO_PKG_VERSION");

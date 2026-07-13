//! # RillML
//!
//! Lightweight, serializable online machine learning for Rust applications
//! and streaming data.
//!
//! RillML provides incremental learning primitives that can be embedded
//! directly in native Rust applications: online statistics, preprocessors,
//! linear/logistic regression, evaluation metrics, pipelines, progressive
//! evaluation, drift detection, and optional serde-based state persistence.
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
//! let optimizer = Optimizer::sgd(
//!     feature_count,
//!     SgdConfig { learning_rate: 0.05, l2: 0.0 },
//! ).unwrap();
//! let regression = LinearRegression::new(
//!     feature_count,
//!     LinearRegressionConfig { optimizer, loss: Default::default() },
//! ).unwrap();
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
pub mod sparse;
pub mod stats;
pub mod traits;

pub use error::RillError;
pub use evaluate::{BinaryClassificationSample, RegressionSample};
pub use traits::{
    Metric, OnlineBinaryClassifier, OnlineRegressor, OnlineStatistic, SparseClassifier,
    SparseRegressor, Transformer,
};

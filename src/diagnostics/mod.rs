//! Diagnostics for online models.
//!
//! This module provides bounded-memory diagnostic primitives that help
//! upper-layer applications answer:
//!
//! - How many samples has the model seen?
//! - What is the recent error?
//! - Is the model still warming up?
//! - Is the model beating its baseline?
//! - Are the model parameters healthy (no NaN / Infinity)?
//! - What is a reasonable prediction interval?
//!
//! Diagnostics are intentionally decoupled from the core model traits.
//! A model implementation remains free to return a plain prediction; the
//! diagnostic wrappers here layer on top without polluting the base API.

pub(crate) mod baseline_comparator;
pub(crate) mod model_health;
pub(crate) mod model_selector;
pub(crate) mod prediction_interval;
pub(crate) mod prediction_report;
pub(crate) mod training_summary;
pub(crate) mod warmup;

pub use baseline_comparator::{BaselineComparator, ComparatorEntry, SwitchReason};
pub use model_health::ModelHealthReport;
pub use model_selector::{OnlineModelSelector, SelectorConfig};
pub use prediction_interval::{PredictionInterval, ResidualInterval, ResidualIntervalConfig};
pub use prediction_report::{Confidence, PredictionReport, PredictionReporter};
pub use training_summary::{TrainingSummary, TrainingSummaryConfig};
pub use warmup::{WarmupConfig, WarmupState, WarmupTracker};

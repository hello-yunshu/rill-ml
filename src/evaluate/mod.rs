//! Progressive (prequential) evaluation.
//!
//! The evaluation order is fixed:
//! ```text
//! predict → metric.update → learn
//! ```
//! This ensures each prediction is made *before* the model sees the current
//! sample, giving an honest estimate of generalization on streaming data.

pub mod progressive;

pub use progressive::{
    BinaryClassificationSample, ProgressiveStep, RegressionSample, evaluate_binary_classification,
    evaluate_regression, evaluate_regression_with_steps,
};

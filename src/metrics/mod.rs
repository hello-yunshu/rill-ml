//! Online evaluation metrics.
//!
//! All metrics implement the [`Metric`](crate::traits::Metric) trait and use
//! bounded memory. Rolling metrics store per-sample contributions in a
//! fixed-size window.

pub mod classification;
pub mod regression;
pub mod rolling;

pub use classification::{Accuracy, F1Score, LogLoss, Precision, Recall};
pub use regression::{Mae, Mse, R2, Rmse};
pub use rolling::{RollingAccuracy, RollingMae, RollingMse};

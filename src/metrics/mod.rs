//! Online evaluation metrics.
//!
//! All metrics implement the [`Metric`](crate::traits::Metric) trait and use
//! bounded memory. Rolling metrics store per-sample contributions in a
//! fixed-size window.

pub(crate) mod classification;
pub(crate) mod regression;
pub(crate) mod rolling;

pub use classification::{Accuracy, F1Score, LogLoss, Precision, Recall};
pub use regression::{Mae, Mse, R2, Rmse};
pub use rolling::{RollingAccuracy, RollingMae, RollingMse};

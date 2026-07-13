//! Loss functions for online models.
//!
//! Losses are represented as concrete enums, not trait objects, to keep
//! serialization and state management simple.

pub mod huber;
pub mod log_loss;
pub mod squared;

pub use huber::HuberLoss;
pub use log_loss::BinaryLogLoss;
pub use squared::SquaredError;

/// Regression loss variants.
///
/// Used by [`LinearRegression`](crate::models::LinearRegression) to select
/// the loss function applied to each update.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RegressionLoss {
    /// `0.5 * (y - y_hat)^2`
    #[default]
    SquaredError,
    /// Huber loss, robust to outliers.
    Huber(HuberLoss),
}

impl RegressionLoss {
    /// Compute the loss value given a prediction and target.
    pub fn loss(&self, prediction: f64, target: f64) -> f64 {
        match self {
            RegressionLoss::SquaredError => SquaredError::loss(prediction, target),
            RegressionLoss::Huber(h) => h.loss(prediction, target),
        }
    }

    /// Compute the derivative of the loss with respect to the prediction.
    pub fn gradient(&self, prediction: f64, target: f64) -> f64 {
        match self {
            RegressionLoss::SquaredError => SquaredError::gradient(prediction, target),
            RegressionLoss::Huber(h) => h.gradient(prediction, target),
        }
    }
}

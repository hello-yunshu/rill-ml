//! Optimizers for online linear models.
//!
//! Optimizers are represented as a concrete enum to avoid trait-object
//! overhead and simplify serialization. The internal parameter vector has
//! length `feature_count + 1`, where the last position holds the intercept.

pub mod adagrad;
pub mod sgd;

pub use adagrad::{AdaGrad, AdaGradConfig};
pub use sgd::{Sgd, SgdConfig};

/// Concrete optimizer enum wrapping all supported optimizers.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Optimizer {
    /// Stochastic gradient descent with optional L2 regularization.
    Sgd(Sgd),
    /// AdaGrad with per-parameter squared gradient accumulation.
    AdaGrad(AdaGrad),
}

impl Optimizer {
    /// Create an SGD optimizer for `feature_count` features (plus intercept).
    pub fn sgd(feature_count: usize, config: SgdConfig) -> Result<Self, RillError> {
        Ok(Optimizer::Sgd(Sgd::new(feature_count, config)?))
    }

    /// Create an AdaGrad optimizer for `feature_count` features (plus intercept).
    pub fn adagrad(feature_count: usize, config: AdaGradConfig) -> Result<Self, RillError> {
        Ok(Optimizer::AdaGrad(AdaGrad::new(feature_count, config)?))
    }

    /// The number of parameters this optimizer manages (features + intercept).
    pub fn param_count(&self) -> usize {
        match self {
            Optimizer::Sgd(o) => o.param_count(),
            Optimizer::AdaGrad(o) => o.param_count(),
        }
    }

    /// Number of samples the optimizer has processed.
    pub fn samples_seen(&self) -> u64 {
        match self {
            Optimizer::Sgd(o) => o.samples_seen(),
            Optimizer::AdaGrad(o) => o.samples_seen(),
        }
    }

    /// Apply a gradient step to `weights` (length `feature_count`) and
    /// `intercept` (single value). The gradient vector passed in must have
    /// the same length as `weights`; the intercept gradient is passed
    /// separately.
    pub fn step(
        &mut self,
        weights: &mut [f64],
        intercept: &mut f64,
        grad_weights: &[f64],
        grad_intercept: f64,
    ) -> Result<(), RillError> {
        match self {
            Optimizer::Sgd(o) => o.step(weights, intercept, grad_weights, grad_intercept),
            Optimizer::AdaGrad(o) => o.step(weights, intercept, grad_weights, grad_intercept),
        }
    }

    /// Reset the optimizer to its initial state.
    pub fn reset(&mut self) {
        match self {
            Optimizer::Sgd(o) => o.reset(),
            Optimizer::AdaGrad(o) => o.reset(),
        }
    }
}

use crate::error::RillError;

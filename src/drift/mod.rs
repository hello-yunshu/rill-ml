//! Drift detection and adaptation.
//!
//! This module provides bounded-memory drift detection algorithms, a decoupled
//! action/strategy layer, decay-aware learning utilities, and a
//! [`DriftAwareModel`] wrapper that integrates drift detection into the
//! predict → learn loop.
//!
//! ## Overview
//!
//! - **Detectors**: [`PageHinkley`], [`Adwin`], [`Kswin`] — each implements
//!   the [`DriftDetector`] trait and reports a [`DriftLevel`]
//!   (None / Warning / Drift).
//! - **Actions**: [`DriftAction`] describes what to do when drift is detected.
//!   [`DriftStrategy`] maps a level to an action, keeping detection and
//!   response decoupled.
//! - **Decay learning**: [`TimeDecayedMean`], [`LearningRateScheduler`],
//!   [`FixedWindowBuffer`] — utilities for adapting to non-stationary streams.
//! - **Wrapper**: [`DriftAwareModel`] wraps a model + detector + strategy and
//!   automatically responds to drift during `learn`. It does **not**
//!   auto-reset the model by default.
//!
//! ## Quick start
//!
//! ```rust
//! use rill_ml::drift::{DriftAction, DriftLevel, PageHinkley, StaticStrategy};
//! use rill_ml::drift::{DriftDetector, DriftStrategy};
//!
//! let mut detector = PageHinkley::default();
//! let strategy = StaticStrategy::new(
//!     DriftAction::ReduceConfidence,
//!     DriftAction::ResetModel,
//! );
//!
//! // Feed a stable stream.
//! for _ in 0..100 {
//!     detector.update(0.0).unwrap();
//! }
//! assert_eq!(detector.level(), DriftLevel::None);
//!
//! // Introduce a sudden shift.
//! for _ in 0..50 {
//!     detector.update(5.0).unwrap();
//! }
//! assert!(detector.detected());
//! let action = strategy.decide(detector.level(), detector.samples_seen());
//! assert_eq!(action, DriftAction::ResetModel);
//! ```
//!
//! [`DriftAwareModel`]: crate::drift::DriftAwareModel
//! [`PageHinkley`]: crate::drift::PageHinkley
//! [`Adwin`]: crate::drift::Adwin
//! [`Kswin`]: crate::drift::Kswin
//! [`TimeDecayedMean`]: crate::drift::TimeDecayedMean
//! [`LearningRateScheduler`]: crate::drift::LearningRateScheduler
//! [`FixedWindowBuffer`]: crate::drift::FixedWindowBuffer

pub(crate) mod action;
pub(crate) mod adwin;
pub(crate) mod aware_model;
pub(crate) mod decay;
pub(crate) mod detector;
pub(crate) mod kswin;
pub(crate) mod page_hinkley;
pub(crate) mod strategy;

pub use action::{DriftAction, DriftEvent};
pub use adwin::{Adwin, AdwinConfig};
pub use aware_model::DriftAwareModel;
pub use decay::{FixedWindowBuffer, LearningRateScheduler, TimeDecayedMean};
pub use detector::{DriftDetector, DriftLevel};
pub use kswin::{Kswin, KswinConfig};
pub use page_hinkley::{PageHinkley, PageHinkleyConfig};
pub use strategy::{DriftStrategy, StaticStrategy};

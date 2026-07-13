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
//! [`DriftAwareModel`]: crate::drift::aware_model::DriftAwareModel
//! [`PageHinkley`]: crate::drift::page_hinkley::PageHinkley
//! [`Adwin`]: crate::drift::adwin::Adwin
//! [`Kswin`]: crate::drift::kswin::Kswin
//! [`TimeDecayedMean`]: crate::drift::decay::TimeDecayedMean
//! [`LearningRateScheduler`]: crate::drift::decay::LearningRateScheduler
//! [`FixedWindowBuffer`]: crate::drift::decay::FixedWindowBuffer

pub mod action;
pub mod adwin;
pub mod aware_model;
pub mod decay;
pub mod detector;
pub mod kswin;
pub mod page_hinkley;
pub mod strategy;

pub use action::{DriftAction, DriftEvent};
pub use adwin::{Adwin, AdwinConfig};
pub use aware_model::DriftAwareModel;
pub use decay::{FixedWindowBuffer, LearningRateScheduler, TimeDecayedMean};
pub use detector::{DriftDetector, DriftLevel};
pub use kswin::{Kswin, KswinConfig};
pub use page_hinkley::{PageHinkley, PageHinkleyConfig};
pub use strategy::{DriftStrategy, StaticStrategy};

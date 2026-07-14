//! Tokio Stream adapters for RillML progressive evaluation.
//!
//! This crate provides async counterparts of `rill_ml::evaluate` that consume
//! any `futures::Stream` instead of an `IntoIterator`. The core contract
//! `predict → metric.update → learn` is preserved exactly.
//!
//! # Example
//!
//! ```no_run
//! # use rill_ml::metrics::Mae;
//! # use rill_ml::models::{BaselineConfig, MeanRegressor};
//! # use rill_ml::OnlineRegressor;
//! # use rill_ml_tokio::progressive_regress_stream;
//! # use rill_ml::evaluate::RegressionSample;
//! # use tokio_stream::wrappers::ReceiverStream;
//! #
//! # async fn demo() {
//! let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
//! let mut mae = Mae::default();
//! let (_tx, rx) = tokio::sync::mpsc::channel::<RegressionSample>(8);
//! let stream = ReceiverStream::new(rx);
//! let final_mae = progressive_regress_stream(&mut model, &mut mae, stream).await.unwrap();
//! # }
//! ```
//!
//! Core models stay synchronous. This crate only adds the async driver; no
//! async runtime is forced on the caller beyond what `tokio_stream::Stream`
//! already implies.

use futures::Stream;
use rill_ml::error::RillError;
use rill_ml::evaluate::{BinaryClassificationSample, RegressionSample};
use rill_ml::traits::{Metric, OnlineBinaryClassifier, OnlineRegressor};

/// Run progressive evaluation over a `Stream` of regression samples.
///
/// Applies the `predict → metric.update → learn` order to every item yielded
/// by `stream`. The model and metric are updated in place. Returns the final
/// metric value (or `None` if the metric has no value yet, e.g. an empty
/// stream).
///
/// # Errors
///
/// Propagates `RillError` from any `predict`, `metric.update`, or `learn`
/// call. A stream error is converted into `RillError::InvalidInput`.
pub async fn progressive_regress_stream<S, M, Met>(
    model: &mut M,
    metric: &mut Met,
    stream: S,
) -> Result<Option<f64>, RillError>
where
    S: Stream<Item = RegressionSample>,
    M: OnlineRegressor,
    Met: Metric<Truth = f64, Prediction = f64>,
{
    futures::pin_mut!(stream);
    while let Some(sample) = stream.next().await {
        let prediction = model.predict(&sample.features)?;
        metric.update(sample.target, prediction)?;
        model.learn(&sample.features, sample.target)?;
    }
    Ok(metric.value())
}

/// Run progressive evaluation over a `Stream` of binary classification
/// samples.
///
/// Applies the `predict → metric.update → learn` order to every item yielded
/// by `stream`. Returns the final metric value.
///
/// # Errors
///
/// Propagates `RillError` from any `predict`, `metric.update`, or `learn`
/// call.
pub async fn progressive_classify_stream<S, M, Met>(
    model: &mut M,
    metric: &mut Met,
    stream: S,
) -> Result<Option<f64>, RillError>
where
    S: Stream<Item = BinaryClassificationSample>,
    M: OnlineBinaryClassifier,
    Met: Metric<Truth = bool, Prediction = bool>,
{
    futures::pin_mut!(stream);
    while let Some(sample) = stream.next().await {
        let prediction = model.predict(&sample.features)?;
        metric.update(sample.target, prediction)?;
        model.learn(&sample.features, sample.target)?;
    }
    Ok(metric.value())
}

// Re-export the `StreamExt` trait so callers can `use rill_ml_tokio::StreamExt`
// without pulling in `futures` themselves. `stream.next()` requires this in
// scope.
pub use futures::StreamExt;

#[cfg(test)]
mod tests {
    use super::*;
    use rill_ml::OnlineBinaryClassifier;
    use rill_ml::evaluate::{BinaryClassificationSample, RegressionSample};
    use rill_ml::loss::BinaryLogLoss;
    use rill_ml::metrics::{Accuracy, Mae};
    use rill_ml::models::{
        BaselineConfig, LogisticRegression, LogisticRegressionConfig, MeanRegressor,
    };
    use rill_ml::optim::Optimizer;
    use rill_ml::traits::OnlineRegressor;
    use tokio_stream::iter;
    use tokio_stream::wrappers::UnboundedReceiverStream;

    #[tokio::test]
    async fn stream_matches_sync_evaluation() {
        let samples = vec![
            RegressionSample {
                features: vec![],
                target: 10.0,
            },
            RegressionSample {
                features: vec![],
                target: 20.0,
            },
            RegressionSample {
                features: vec![],
                target: 30.0,
            },
        ];

        // Sync reference
        let mut sync_model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let mut sync_mae = Mae::default();
        let sync_final =
            rill_ml::evaluate::evaluate_regression(&mut sync_model, &mut sync_mae, samples.clone())
                .unwrap();

        // Stream version
        let mut async_model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let mut async_mae = Mae::default();
        let stream = iter(samples);
        let async_final = progressive_regress_stream(&mut async_model, &mut async_mae, stream)
            .await
            .unwrap();

        assert_eq!(sync_final, async_final);
        assert_eq!(sync_model.samples_seen(), async_model.samples_seen());
    }

    #[tokio::test]
    async fn empty_stream_leaves_metric_unchanged() {
        let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let mut mae = Mae::default();
        let stream = iter::<Vec<RegressionSample>>(vec![]);
        let final_value = progressive_regress_stream(&mut model, &mut mae, stream)
            .await
            .unwrap();
        assert!(final_value.is_none());
        assert_eq!(model.samples_seen(), 0);
    }

    #[tokio::test]
    async fn mpsc_channel_stream_works() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RegressionSample>();
        let stream = UnboundedReceiverStream::new(rx);

        let producer = tokio::spawn(async move {
            for i in 0..10u64 {
                let target = i as f64;
                tx.send(RegressionSample {
                    features: vec![],
                    target,
                })
                .unwrap();
            }
        });

        let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
        let mut mae = Mae::default();
        let final_value = progressive_regress_stream(&mut model, &mut mae, stream)
            .await
            .unwrap();
        producer.await.unwrap();
        assert!(final_value.is_some());
        assert_eq!(model.samples_seen(), 10);
    }

    #[tokio::test]
    async fn classify_stream_matches_sync_evaluation() {
        let samples = vec![
            BinaryClassificationSample {
                features: vec![1.0, 0.5],
                target: true,
            },
            BinaryClassificationSample {
                features: vec![-1.0, -0.5],
                target: false,
            },
            BinaryClassificationSample {
                features: vec![2.0, 1.0],
                target: true,
            },
        ];

        let make_model = || {
            let optimizer = Optimizer::sgd(
                2,
                rill_ml::optim::SgdConfig {
                    learning_rate: 0.1,
                    l2: 0.0,
                },
            )
            .unwrap();
            LogisticRegression::new(
                2,
                LogisticRegressionConfig {
                    optimizer,
                    loss: BinaryLogLoss::new(),
                },
            )
            .unwrap()
        };

        // Sync reference
        let mut sync_model = make_model();
        let mut sync_acc = Accuracy::default();
        let sync_final = rill_ml::evaluate::evaluate_binary_classification(
            &mut sync_model,
            &mut sync_acc,
            samples.clone(),
        )
        .unwrap();

        // Stream version
        let mut async_model = make_model();
        let mut async_acc = Accuracy::default();
        let stream = iter(samples);
        let async_final = progressive_classify_stream(&mut async_model, &mut async_acc, stream)
            .await
            .unwrap();

        assert_eq!(sync_final, async_final);
        assert_eq!(sync_model.samples_seen(), async_model.samples_seen());
    }

    #[tokio::test]
    async fn classify_empty_stream_leaves_metric_unchanged() {
        let optimizer = Optimizer::sgd(
            1,
            rill_ml::optim::SgdConfig {
                learning_rate: 0.1,
                l2: 0.0,
            },
        )
        .unwrap();
        let mut model = LogisticRegression::new(
            1,
            LogisticRegressionConfig {
                optimizer,
                loss: BinaryLogLoss::new(),
            },
        )
        .unwrap();
        let mut acc = Accuracy::default();
        let stream = iter::<Vec<BinaryClassificationSample>>(vec![]);
        let final_value = progressive_classify_stream(&mut model, &mut acc, stream)
            .await
            .unwrap();
        assert!(final_value.is_none());
        assert_eq!(model.samples_seen(), 0);
    }

    #[tokio::test]
    async fn classify_stream_learns_separable_data() {
        let samples: Vec<BinaryClassificationSample> = (0..200)
            .map(|i| {
                let label = i % 2 == 0;
                let x = if label { 2.0 } else { -2.0 };
                BinaryClassificationSample {
                    features: vec![x],
                    target: label,
                }
            })
            .collect();
        let n = samples.len();

        let optimizer = Optimizer::sgd(
            1,
            rill_ml::optim::SgdConfig {
                learning_rate: 0.1,
                l2: 0.0,
            },
        )
        .unwrap();
        let mut model = LogisticRegression::new(
            1,
            LogisticRegressionConfig {
                optimizer,
                loss: BinaryLogLoss::new(),
            },
        )
        .unwrap();
        let mut acc = Accuracy::default();
        let stream = iter(samples);
        let final_acc = progressive_classify_stream(&mut model, &mut acc, stream)
            .await
            .unwrap();
        assert_eq!(model.samples_seen(), n as u64);
        // After 200 samples of linearly separable data, accuracy should be high.
        assert!(final_acc.unwrap() > 0.8);
    }
}

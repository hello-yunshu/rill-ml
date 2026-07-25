//! Classification metrics: Accuracy, Precision, Recall, F1, LogLoss.

use crate::error::{RillError, checked_finite_add, checked_increment, ensure_finite};
use crate::loss::log_loss::BinaryLogLoss;
use crate::traits::Metric;

/// Accuracy for binary classification.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Accuracy {
    correct: u64,
    count: u64,
}

impl Metric for Accuracy {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        let next_count = checked_increment(self.count, "accuracy sample")?;
        let next_correct = if truth == prediction {
            checked_increment(self.correct, "accuracy correct")?
        } else {
            self.correct
        };
        self.count = next_count;
        self.correct = next_correct;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.correct as f64 / self.count as f64)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.correct = 0;
        self.count = 0;
    }
}

/// Precision for the positive class.
///
/// `samples_seen()` reports the total number of successfully incorporated
/// observations (including true negatives), not `TP + FP`. The confusion
/// counts are kept separately so the metric remains computable.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Precision {
    true_positive: u64,
    false_positive: u64,
    /// Total observations successfully incorporated via `update`.
    /// Restored from serde as-is; older states without this field are
    /// rejected because the true-negative count cannot be reconstructed
    /// from `TP`/`FP` alone.
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Precision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct PrecisionState {
            true_positive: u64,
            false_positive: u64,
            samples_seen: u64,
        }

        let state = PrecisionState::deserialize(deserializer)?;
        // Internal consistency: samples_seen must be at least the
        // confusion counts, since every TP/FP contributes one observation.
        // Use checked_add (not saturating_add) so an illegal huge state
        // cannot be silently accepted after saturation.
        let confusion = state
            .true_positive
            .checked_add(state.false_positive)
            .ok_or_else(|| serde::de::Error::custom("precision tp + fp overflow"))?;
        if state.samples_seen < confusion {
            return Err(serde::de::Error::custom("precision samples_seen < tp + fp"));
        }
        Ok(Precision {
            true_positive: state.true_positive,
            false_positive: state.false_positive,
            samples_seen: state.samples_seen,
        })
    }
}

impl Metric for Precision {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        let next_samples = checked_increment(self.samples_seen, "precision samples_seen")?;
        let next_tp = if truth && prediction {
            checked_increment(self.true_positive, "precision true positive")?
        } else {
            self.true_positive
        };
        let next_fp = if !truth && prediction {
            checked_increment(self.false_positive, "precision false positive")?
        } else {
            self.false_positive
        };
        self.samples_seen = next_samples;
        self.true_positive = next_tp;
        self.false_positive = next_fp;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        let denominator = self.true_positive as f64 + self.false_positive as f64;
        if denominator == 0.0 {
            None
        } else {
            Some(self.true_positive as f64 / denominator)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        self.true_positive = 0;
        self.false_positive = 0;
        self.samples_seen = 0;
    }
}

/// Recall for the positive class.
///
/// `samples_seen()` reports the total number of successfully incorporated
/// observations (including true negatives), not `TP + FN`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Recall {
    true_positive: u64,
    false_negative: u64,
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Recall {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct RecallState {
            true_positive: u64,
            false_negative: u64,
            samples_seen: u64,
        }

        let state = RecallState::deserialize(deserializer)?;
        // Use checked_add (not saturating_add) so an illegal huge state
        // cannot be silently accepted after saturation.
        let confusion = state
            .true_positive
            .checked_add(state.false_negative)
            .ok_or_else(|| serde::de::Error::custom("recall tp + fn overflow"))?;
        if state.samples_seen < confusion {
            return Err(serde::de::Error::custom("recall samples_seen < tp + fn"));
        }
        Ok(Recall {
            true_positive: state.true_positive,
            false_negative: state.false_negative,
            samples_seen: state.samples_seen,
        })
    }
}

impl Metric for Recall {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        let next_samples = checked_increment(self.samples_seen, "recall samples_seen")?;
        let next_tp = if truth && prediction {
            checked_increment(self.true_positive, "recall true positive")?
        } else {
            self.true_positive
        };
        let next_fn = if truth && !prediction {
            checked_increment(self.false_negative, "recall false negative")?
        } else {
            self.false_negative
        };
        self.samples_seen = next_samples;
        self.true_positive = next_tp;
        self.false_negative = next_fn;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        let denominator = self.true_positive as f64 + self.false_negative as f64;
        if denominator == 0.0 {
            None
        } else {
            Some(self.true_positive as f64 / denominator)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        self.true_positive = 0;
        self.false_negative = 0;
        self.samples_seen = 0;
    }
}

/// F1 score, the harmonic mean of precision and recall.
///
/// `samples_seen()` reports the total number of successfully incorporated
/// observations (including true negatives), not `TP + FP + FN`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct F1Score {
    true_positive: u64,
    false_positive: u64,
    false_negative: u64,
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for F1Score {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct F1State {
            true_positive: u64,
            false_positive: u64,
            false_negative: u64,
            samples_seen: u64,
        }

        let state = F1State::deserialize(deserializer)?;
        // Use checked_add (not saturating_add) so an illegal huge state
        // cannot be silently accepted after saturation. The chained add
        // (tp + fp + fn) must be checked at every step; saturating_add
        // could hide overflow at the first add and produce a wrong sum.
        let tp_fp = state
            .true_positive
            .checked_add(state.false_positive)
            .ok_or_else(|| serde::de::Error::custom("f1 tp + fp overflow"))?;
        let confusion = tp_fp
            .checked_add(state.false_negative)
            .ok_or_else(|| serde::de::Error::custom("f1 tp + fp + fn overflow"))?;
        if state.samples_seen < confusion {
            return Err(serde::de::Error::custom("f1 samples_seen < tp + fp + fn"));
        }
        Ok(F1Score {
            true_positive: state.true_positive,
            false_positive: state.false_positive,
            false_negative: state.false_negative,
            samples_seen: state.samples_seen,
        })
    }
}

impl Metric for F1Score {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        let next_samples = checked_increment(self.samples_seen, "F1 samples_seen")?;
        let next_tp = if truth && prediction {
            checked_increment(self.true_positive, "F1 true positive")?
        } else {
            self.true_positive
        };
        let next_fp = if !truth && prediction {
            checked_increment(self.false_positive, "F1 false positive")?
        } else {
            self.false_positive
        };
        let next_fn = if truth && !prediction {
            checked_increment(self.false_negative, "F1 false negative")?
        } else {
            self.false_negative
        };
        self.samples_seen = next_samples;
        self.true_positive = next_tp;
        self.false_positive = next_fp;
        self.false_negative = next_fn;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        let denominator = 2.0 * self.true_positive as f64
            + self.false_positive as f64
            + self.false_negative as f64;
        if denominator == 0.0 {
            None
        } else {
            Some(2.0 * self.true_positive as f64 / denominator)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn reset(&mut self) {
        self.true_positive = 0;
        self.false_positive = 0;
        self.false_negative = 0;
        self.samples_seen = 0;
    }
}

/// Binary log loss (cross-entropy).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LogLoss {
    loss: BinaryLogLoss,
    sum_loss: f64,
    count: u64,
}

impl Default for LogLoss {
    fn default() -> Self {
        Self {
            loss: BinaryLogLoss::new(),
            sum_loss: 0.0,
            count: 0,
        }
    }
}

impl Metric for LogLoss {
    type Truth = bool;
    type Prediction = f64;

    fn update(&mut self, truth: bool, prediction: f64) -> Result<(), RillError> {
        ensure_finite("probability", prediction)?;
        if !(0.0..=1.0).contains(&prediction) {
            return Err(RillError::InvalidProbability(prediction));
        }
        let loss = self.loss.loss(prediction, truth);
        ensure_finite("log loss", loss)?;
        let next_sum = checked_finite_add(self.sum_loss, loss, "log loss sum")?;
        let next_count = checked_increment(self.count, "log loss sample")?;
        self.sum_loss = next_sum;
        self.count = next_count;
        Ok(())
    }

    fn value(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum_loss / self.count as f64)
        }
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.sum_loss = 0.0;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accuracy_basic() {
        let mut m = Accuracy::default();
        m.update(true, true).unwrap();
        m.update(false, false).unwrap();
        m.update(true, false).unwrap();
        assert!((m.value().unwrap() - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn precision_basic() {
        let mut m = Precision::default();
        m.update(true, true).unwrap(); // tp
        m.update(false, true).unwrap(); // fp
        m.update(true, false).unwrap(); // fn
        assert!((m.value().unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn recall_basic() {
        let mut m = Recall::default();
        m.update(true, true).unwrap(); // tp
        m.update(false, true).unwrap(); // fp
        m.update(true, false).unwrap(); // fn
        assert!((m.value().unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn f1_basic() {
        let mut m = F1Score::default();
        m.update(true, true).unwrap(); // tp=1
        m.update(false, true).unwrap(); // fp=1
        m.update(true, false).unwrap(); // fn=1
        // F1 = 2*1 / (2*1 + 1 + 1) = 0.5
        assert!((m.value().unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn f1_perfect_is_one() {
        let mut m = F1Score::default();
        m.update(true, true).unwrap();
        m.update(false, false).unwrap();
        assert!((m.value().unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn log_loss_basic() {
        let mut m = LogLoss::default();
        m.update(true, 0.9).unwrap();
        m.update(false, 0.1).unwrap();
        let expected = (-0.9_f64.ln() + -0.9_f64.ln()) / 2.0;
        assert!((m.value().unwrap() - expected).abs() < 1e-9);
    }

    #[test]
    fn log_loss_rejects_invalid_probability() {
        let mut m = LogLoss::default();
        assert!(m.update(true, 1.5).is_err());
        assert!(m.update(true, -0.1).is_err());
        assert!(m.update(true, f64::NAN).is_err());
    }

    #[test]
    fn empty_metrics_return_none() {
        assert!(Accuracy::default().value().is_none());
        assert!(Precision::default().value().is_none());
        assert!(Recall::default().value().is_none());
        assert!(F1Score::default().value().is_none());
        assert!(LogLoss::default().value().is_none());
    }

    #[test]
    fn precision_no_predictions_returns_none() {
        let mut m = Precision::default();
        m.update(true, false).unwrap();
        m.update(false, false).unwrap();
        assert!(m.value().is_none());
    }

    // -----------------------------------------------------------------
    // Metric::samples_seen() contract: every successful update must
    // increment the count by exactly one, including true negatives.
    // -----------------------------------------------------------------

    #[test]
    fn samples_seen_counts_all_observations() {
        let mut p = Precision::default();
        let mut r = Recall::default();
        let mut f = F1Score::default();
        let mut a = Accuracy::default();

        // All four confusion-matrix cells.
        let cases = [(true, true), (true, false), (false, true), (false, false)];
        for (truth, pred) in cases {
            p.update(truth, pred).unwrap();
            r.update(truth, pred).unwrap();
            f.update(truth, pred).unwrap();
            a.update(truth, pred).unwrap();
        }

        assert_eq!(p.samples_seen(), 4);
        assert_eq!(r.samples_seen(), 4);
        assert_eq!(f.samples_seen(), 4);
        assert_eq!(a.samples_seen(), 4);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn samples_seen_overflow_is_atomic() {
        // Restore a near-overflow Precision via serde, then attempt one
        // more update. The counter must overflow without mutating state.
        let json = format!(
            "{{\"true_positive\":1,\"false_positive\":1,\"samples_seen\":{}}}",
            u64::MAX
        );
        let mut p: Precision = serde_json::from_str(&json).unwrap();
        let result = p.update(true, true);
        assert!(result.is_err(), "expected overflow");
        assert_eq!(p.samples_seen(), u64::MAX);
        assert_eq!(p.true_positive, 1);
        assert_eq!(p.false_positive, 1);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn precision_serde_rejects_missing_samples_seen() {
        // Old state without samples_seen must be rejected: true-negative
        // count cannot be reconstructed from TP/FP alone.
        let json = "{\"true_positive\":1,\"false_positive\":1}";
        assert!(serde_json::from_str::<Precision>(json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn precision_serde_rejects_inconsistent_samples_seen() {
        // samples_seen < tp + fp is internally inconsistent.
        let json = "{\"true_positive\":5,\"false_positive\":5,\"samples_seen\":3}";
        assert!(serde_json::from_str::<Precision>(json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn recall_serde_rejects_missing_samples_seen() {
        let json = "{\"true_positive\":1,\"false_negative\":1}";
        assert!(serde_json::from_str::<Recall>(json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn f1_serde_rejects_missing_samples_seen() {
        let json = "{\"true_positive\":1,\"false_positive\":1,\"false_negative\":1}";
        assert!(serde_json::from_str::<F1Score>(json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn metric_serde_roundtrip_preserves_samples_seen() {
        let mut p = Precision::default();
        for _ in 0..10 {
            p.update(true, true).unwrap();
        }
        let json = serde_json::to_string(&p).unwrap();
        let restored: Precision = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.samples_seen(), 10);
        assert_eq!(restored.true_positive, 10);
    }

    #[test]
    fn reset_clears_samples_seen() {
        let mut p = Precision::default();
        p.update(true, true).unwrap();
        p.update(false, false).unwrap();
        assert_eq!(p.samples_seen(), 2);
        p.reset();
        assert_eq!(p.samples_seen(), 0);
        assert_eq!(p.true_positive, 0);
        assert_eq!(p.false_positive, 0);
    }

    // -----------------------------------------------------------------
    // serde overflow rejection: checked_add must reject illegal huge
    // states instead of silently saturating them to u64::MAX.
    // -----------------------------------------------------------------

    #[test]
    #[cfg(feature = "serde")]
    fn precision_serde_rejects_tp_fp_overflow() {
        // TP + FP overflows u64; saturating_add would hide this and
        // compare against u64::MAX, wrongly rejecting only because
        // samples_seen < u64::MAX rather than because of overflow.
        let json = format!(
            "{{\"true_positive\":{},\"false_positive\":{},\"samples_seen\":{}}}",
            u64::MAX,
            1u64,
            u64::MAX
        );
        assert!(serde_json::from_str::<Precision>(&json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn recall_serde_rejects_tp_fn_overflow() {
        let json = format!(
            "{{\"true_positive\":{},\"false_negative\":{},\"samples_seen\":{}}}",
            u64::MAX,
            1u64,
            u64::MAX
        );
        assert!(serde_json::from_str::<Recall>(&json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn f1_serde_rejects_tp_fp_overflow() {
        // First add (tp + fp) overflows.
        let json = format!(
            "{{\"true_positive\":{},\"false_positive\":{},\"false_negative\":0,\"samples_seen\":{}}}",
            u64::MAX,
            1u64,
            u64::MAX
        );
        assert!(serde_json::from_str::<F1Score>(&json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn f1_serde_rejects_tp_fp_fn_overflow() {
        // tp + fp = u64::MAX (does not overflow), then + 1 overflows.
        let json = format!(
            "{{\"true_positive\":{},\"false_positive\":0,\"false_negative\":{},\"samples_seen\":{}}}",
            u64::MAX,
            1u64,
            u64::MAX
        );
        assert!(serde_json::from_str::<F1Score>(&json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn metric_serde_accepts_max_boundary() {
        // samples_seen == u64::MAX with TP = u64::MAX, FP/FN = 0 is the
        // largest legal boundary state and must be accepted.
        let p_json = format!(
            "{{\"true_positive\":{},\"false_positive\":0,\"samples_seen\":{}}}",
            u64::MAX,
            u64::MAX
        );
        let p: Precision = serde_json::from_str(&p_json).unwrap();
        assert_eq!(p.samples_seen(), u64::MAX);

        let r_json = format!(
            "{{\"true_positive\":{},\"false_negative\":0,\"samples_seen\":{}}}",
            u64::MAX,
            u64::MAX
        );
        let r: Recall = serde_json::from_str(&r_json).unwrap();
        assert_eq!(r.samples_seen(), u64::MAX);

        let f_json = format!(
            "{{\"true_positive\":{},\"false_positive\":0,\"false_negative\":0,\"samples_seen\":{}}}",
            u64::MAX,
            u64::MAX
        );
        let f: F1Score = serde_json::from_str(&f_json).unwrap();
        assert_eq!(f.samples_seen(), u64::MAX);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn recall_serde_rejects_inconsistent_samples_seen() {
        // samples_seen < tp + fn is internally inconsistent.
        let json = "{\"true_positive\":5,\"false_negative\":5,\"samples_seen\":3}";
        assert!(serde_json::from_str::<Recall>(json).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn f1_serde_rejects_inconsistent_samples_seen() {
        // samples_seen < tp + fp + fn is internally inconsistent.
        let json =
            "{\"true_positive\":2,\"false_positive\":2,\"false_negative\":2,\"samples_seen\":3}";
        assert!(serde_json::from_str::<F1Score>(json).is_err());
    }
}

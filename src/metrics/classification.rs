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
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Precision {
    true_positive: u64,
    false_positive: u64,
}

impl Metric for Precision {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        match (truth, prediction) {
            (true, true) => {
                self.true_positive =
                    checked_increment(self.true_positive, "precision true positive")?
            }
            (false, true) => {
                self.false_positive =
                    checked_increment(self.false_positive, "precision false positive")?
            }
            _ => {}
        }
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
        self.true_positive.saturating_add(self.false_positive)
    }

    fn reset(&mut self) {
        self.true_positive = 0;
        self.false_positive = 0;
    }
}

/// Recall for the positive class.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Recall {
    true_positive: u64,
    false_negative: u64,
}

impl Metric for Recall {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        match (truth, prediction) {
            (true, true) => {
                self.true_positive = checked_increment(self.true_positive, "recall true positive")?
            }
            (true, false) => {
                self.false_negative =
                    checked_increment(self.false_negative, "recall false negative")?
            }
            _ => {}
        }
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
        self.true_positive.saturating_add(self.false_negative)
    }

    fn reset(&mut self) {
        self.true_positive = 0;
        self.false_negative = 0;
    }
}

/// F1 score, the harmonic mean of precision and recall.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct F1Score {
    true_positive: u64,
    false_positive: u64,
    false_negative: u64,
}

impl Metric for F1Score {
    type Truth = bool;
    type Prediction = bool;

    fn update(&mut self, truth: bool, prediction: bool) -> Result<(), RillError> {
        match (truth, prediction) {
            (true, true) => {
                self.true_positive = checked_increment(self.true_positive, "F1 true positive")?
            }
            (false, true) => {
                self.false_positive = checked_increment(self.false_positive, "F1 false positive")?
            }
            (true, false) => {
                self.false_negative = checked_increment(self.false_negative, "F1 false negative")?
            }
            _ => {}
        }
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
        self.true_positive
            .saturating_add(self.false_positive)
            .saturating_add(self.false_negative)
    }

    fn reset(&mut self) {
        self.true_positive = 0;
        self.false_positive = 0;
        self.false_negative = 0;
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
}

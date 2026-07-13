//! Model parameter health checks.
//!
//! Provides a bounded-memory snapshot of model parameter health, detecting
//! NaN / Infinity contamination and reporting basic weight statistics.
//!
//! This module is intentionally decoupled from any model trait: callers pass
//! raw parameter slices and receive a plain report. Construction never panics
//! and never returns `Result` — NaN and Infinity are detection targets, not
//! errors.

/// Snapshot of model parameter health.
///
/// Computed from a flat slice of weights and an optional intercept. Detects
/// NaN / Infinity contamination and reports basic weight statistics. Does
/// not store the parameters themselves.
///
/// # Examples
///
/// ```
/// use rill_ml::diagnostics::ModelHealthReport;
///
/// let report = ModelHealthReport::from_parameters(&[0.1, -0.2, 0.5], Some(1.0));
/// assert_eq!(report.parameter_count(), 4);
/// assert!(report.is_healthy());
/// assert_eq!(report.weight_range(), Some(1.2));
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ModelHealthReport {
    parameter_count: usize,
    weight_min: Option<f64>,
    weight_max: Option<f64>,
    has_nan: bool,
    has_infinity: bool,
    state_size_bytes: usize,
}

impl ModelHealthReport {
    /// Build a report by scanning a slice of weights plus an optional intercept.
    ///
    /// `parameter_count` = `weights.len() + intercept.is_some() as usize`.
    /// `state_size_bytes` = `parameter_count * 8` (one `f64` per parameter).
    ///
    /// Empty input yields `weight_min == weight_max == None` and a healthy
    /// report (no NaN / Infinity detected).
    pub fn from_parameters(weights: &[f64], intercept: Option<f64>) -> Self {
        let mut has_nan = false;
        let mut has_infinity = false;
        let mut weight_min: Option<f64> = None;
        let mut weight_max: Option<f64> = None;

        for &v in weights.iter().chain(intercept.iter()) {
            if v.is_nan() {
                has_nan = true;
            }
            if v.is_infinite() {
                has_infinity = true;
            }
            weight_min = Some(match weight_min {
                None => v,
                Some(m) => m.min(v),
            });
            weight_max = Some(match weight_max {
                None => v,
                Some(m) => m.max(v),
            });
        }

        let parameter_count = weights.len() + intercept.is_some() as usize;
        let state_size_bytes = parameter_count * 8;

        Self {
            parameter_count,
            weight_min,
            weight_max,
            has_nan,
            has_infinity,
            state_size_bytes,
        }
    }

    /// Total number of parameters (weights plus optional intercept).
    pub const fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    /// Minimum weight value, or `None` if no parameters were supplied.
    pub const fn weight_min(&self) -> Option<f64> {
        self.weight_min
    }

    /// Maximum weight value, or `None` if no parameters were supplied.
    pub const fn weight_max(&self) -> Option<f64> {
        self.weight_max
    }

    /// Whether any parameter is NaN.
    pub const fn has_nan(&self) -> bool {
        self.has_nan
    }

    /// Whether any parameter is positive or negative Infinity.
    pub const fn has_infinity(&self) -> bool {
        self.has_infinity
    }

    /// Estimated state size in bytes (`parameter_count * 8`).
    pub const fn state_size_bytes(&self) -> usize {
        self.state_size_bytes
    }

    /// Whether the parameters are free of NaN and Infinity.
    pub fn is_healthy(&self) -> bool {
        !self.has_nan && !self.has_infinity
    }

    /// Spread of weights (`weight_max - weight_min`), or `None` if no data.
    pub fn weight_range(&self) -> Option<f64> {
        match (self.weight_min, self.weight_max) {
            (Some(lo), Some(hi)) => Some(hi - lo),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_weights() {
        let report = ModelHealthReport::from_parameters(&[], None);
        assert_eq!(report.parameter_count(), 0);
        assert_eq!(report.weight_min(), None);
        assert_eq!(report.weight_max(), None);
        assert!(!report.has_nan());
        assert!(!report.has_infinity());
        assert_eq!(report.state_size_bytes(), 0);
        assert_eq!(report.weight_range(), None);
        assert!(report.is_healthy());
    }

    #[test]
    fn single_weight() {
        let report = ModelHealthReport::from_parameters(&[0.5], None);
        assert_eq!(report.parameter_count(), 1);
        assert_eq!(report.weight_min(), Some(0.5));
        assert_eq!(report.weight_max(), Some(0.5));
        assert_eq!(report.state_size_bytes(), 8);
        assert_eq!(report.weight_range(), Some(0.0));
        assert!(report.is_healthy());
    }

    #[test]
    fn multiple_weights_with_intercept() {
        let report = ModelHealthReport::from_parameters(&[0.1, -0.2, 0.5], Some(1.0));
        assert_eq!(report.parameter_count(), 4);
        assert_eq!(report.weight_min(), Some(-0.2));
        assert_eq!(report.weight_max(), Some(1.0));
        assert_eq!(report.state_size_bytes(), 32);
        assert_eq!(report.weight_range(), Some(1.2));
        assert!(report.is_healthy());
    }

    #[test]
    fn detects_nan() {
        let report = ModelHealthReport::from_parameters(&[0.1, f64::NAN, 0.5], None);
        assert!(report.has_nan());
        assert!(!report.has_infinity());
        assert!(!report.is_healthy());
    }

    #[test]
    fn detects_infinity() {
        let report = ModelHealthReport::from_parameters(&[0.1, f64::INFINITY, 0.5], None);
        assert!(!report.has_nan());
        assert!(report.has_infinity());
        assert!(!report.is_healthy());
    }

    #[test]
    fn detects_neg_infinity() {
        let report = ModelHealthReport::from_parameters(&[0.1, f64::NEG_INFINITY, 0.5], None);
        assert!(!report.has_nan());
        assert!(report.has_infinity());
        assert!(!report.is_healthy());
    }

    #[test]
    fn weight_range_correct() {
        let report = ModelHealthReport::from_parameters(&[3.0, -1.0, 2.0, 0.5], None);
        assert_eq!(report.weight_min(), Some(-1.0));
        assert_eq!(report.weight_max(), Some(3.0));
        assert_eq!(report.weight_range(), Some(4.0));
    }

    #[test]
    fn state_size_calculation() {
        let report = ModelHealthReport::from_parameters(&[0.0; 10], Some(0.0));
        assert_eq!(report.parameter_count(), 11);
        assert_eq!(report.state_size_bytes(), 88);
    }

    #[test]
    fn healthy_model() {
        let report = ModelHealthReport::from_parameters(&[0.1, 0.2, 0.3], Some(0.05));
        assert!(report.is_healthy());
        assert!(!report.has_nan());
        assert!(!report.has_infinity());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let report = ModelHealthReport::from_parameters(&[0.1, -0.2, 0.5], Some(1.0));
        let json = serde_json::to_string(&report).unwrap();
        let restored: ModelHealthReport = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.parameter_count(), 4);
        assert_eq!(restored.weight_min(), Some(-0.2));
        assert_eq!(restored.weight_max(), Some(1.0));
        assert!(!restored.has_nan());
        assert!(!restored.has_infinity());
        assert_eq!(restored.state_size_bytes(), 32);
        assert!(restored.is_healthy());
    }
}

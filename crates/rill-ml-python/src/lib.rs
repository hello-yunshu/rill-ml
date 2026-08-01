//! Python bindings for RillML core subset via PyO3.
//!
//! Exposes online statistics, scalers, linear/logistic regression, and
//! pipelines to Python with River-style `predict_one` / `learn_one` /
//! `transform_one` / `fit_one` method names. Snapshot persistence is
//! available via each class's `to_json` method.

// PyO3 0.22's `#[pymethods]` macro generates `unsafe fn` bodies; Rust 2024
// warns on unsafe operations inside unsafe fns without an explicit unsafe
// block. We allow this pattern crate-wide to keep the binding code readable.
#![allow(unsafe_op_in_unsafe_fn)]

use ::rill_ml::bandit::{ContextualBandit, LinUcb, LinUcbConfig};
use ::rill_ml::loss::{BinaryLogLoss, RegressionLoss};
use ::rill_ml::models::{
    LinearRegression, LinearRegressionConfig, LogisticRegression, LogisticRegressionConfig,
};
use ::rill_ml::optim::{Optimizer, SgdConfig};
use ::rill_ml::persistence::Snapshot;
use ::rill_ml::pipeline::{ClassificationPipeline, RegressionPipeline};
use ::rill_ml::preprocessing::StandardScaler;
use ::rill_ml::replay::{DecisionReplayConfig, DecisionReplayHarness, DecisionReplayRecord};
use ::rill_ml::stats::{ExponentiallyWeightedMean, Mean, Variance, VarianceKind};
use ::rill_ml::traits::{OnlineBinaryClassifier, OnlineRegressor, OnlineStatistic, Transformer};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

fn to_err<E: std::fmt::Display>(_py: Python, e: E) -> PyErr {
    PyRuntimeError::new_err(format!("{}", e))
}

fn vec_from_list(sl: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    sl.extract::<Vec<f64>>()
}

/// Online mean accumulator.
#[pyclass(name = "Mean")]
pub struct PyMean {
    inner: Mean,
}

#[pymethods]
impl PyMean {
    #[new]
    fn new() -> Self {
        PyMean { inner: Mean::new() }
    }

    fn update(&mut self, py: Python, x: f64) -> PyResult<()> {
        self.inner.update(x).map_err(|e| to_err(py, e))
    }

    #[getter]
    fn value(&self) -> f64 {
        self.inner.value()
    }

    #[getter]
    fn count(&self) -> u64 {
        self.inner.count()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyMean {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

impl Default for PyMean {
    fn default() -> Self {
        Self::new()
    }
}

/// Preview LinUCB binding with explainable per-arm scores.
#[pyclass(name = "LinUcb")]
pub struct PyLinUcb {
    inner: LinUcb,
}

#[pymethods]
impl PyLinUcb {
    #[new]
    fn new(py: Python, arm_count: usize, feature_count: usize, alpha: f64) -> PyResult<Self> {
        let mut config = LinUcbConfig::default();
        config.arm_count = arm_count;
        config.feature_count = feature_count;
        config.alpha = alpha;
        Ok(Self {
            inner: LinUcb::new(config).map_err(|error| to_err(py, error))?,
        })
    }

    /// Return `(arm, exploitation, exploration_bonus, total_score)` rows.
    fn score_all(
        &self,
        py: Python,
        context: Bound<'_, PyAny>,
    ) -> PyResult<Vec<(usize, f64, f64, f64)>> {
        let context = vec_from_list(&context)?;
        self.inner
            .score_all(&context)
            .map(|scores| {
                scores
                    .into_iter()
                    .map(|score| {
                        (
                            score.arm,
                            score.exploitation,
                            score.exploration_bonus,
                            score.total_score,
                        )
                    })
                    .collect()
            })
            .map_err(|error| to_err(py, error))
    }

    fn select_deterministic(&self, py: Python, context: Bound<'_, PyAny>) -> PyResult<usize> {
        let context = vec_from_list(&context)?;
        self.inner
            .select_deterministic(&context)
            .map_err(|error| to_err(py, error))
    }

    fn update(
        &mut self,
        py: Python,
        arm: usize,
        context: Bound<'_, PyAny>,
        reward: f64,
    ) -> PyResult<()> {
        let context = vec_from_list(&context)?;
        self.inner
            .update(arm, &context, reward)
            .map_err(|error| to_err(py, error))
    }

    #[getter]
    fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(Self {
            inner: Snapshot::from_json_validated(json).map_err(|error| to_err(py, error))?,
        })
    }
}

/// Run the Rust decision replay harness and return its JSON report. This is a
/// development/verification helper; Python is not a Runtime dependency.
#[pyfunction]
#[pyo3(signature = (records_json, arm_count, feature_count, alpha, max_feedback_delay, model_generation, feature_schema_hash))]
#[allow(clippy::too_many_arguments)]
fn replay_decisions(
    py: Python,
    records_json: &str,
    arm_count: usize,
    feature_count: usize,
    alpha: f64,
    max_feedback_delay: u64,
    model_generation: u64,
    feature_schema_hash: String,
) -> PyResult<String> {
    let records: Vec<DecisionReplayRecord> = serde_json::from_str(records_json)
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    let mut model_config = LinUcbConfig::default();
    model_config.arm_count = arm_count;
    model_config.feature_count = feature_count;
    model_config.alpha = alpha;
    let model = LinUcb::new(model_config).map_err(|error| to_err(py, error))?;
    let config = DecisionReplayConfig::new(
        records.len().max(1),
        max_feedback_delay,
        model_generation,
        feature_schema_hash,
    )
    .map_err(|error| to_err(py, error))?;
    let mut harness =
        DecisionReplayHarness::new(model, config).map_err(|error| to_err(py, error))?;
    let report = harness
        .replay(&records)
        .map_err(|error| to_err(py, error))?;
    serde_json::to_string(&report).map_err(|error| PyRuntimeError::new_err(error.to_string()))
}

/// Online variance (Welford). `value`/`stddev` return `None` until enough
/// samples have been observed.
#[pyclass(name = "Variance")]
pub struct PyVariance {
    inner: Variance,
}

#[pymethods]
impl PyVariance {
    #[new]
    fn new(kind: &str) -> PyResult<Self> {
        let k = match kind {
            "population" => VarianceKind::Population,
            "sample" => VarianceKind::Sample,
            other => {
                return Err(PyRuntimeError::new_err(format!(
                    "unknown variance kind: {} (expected 'population' or 'sample')",
                    other
                )));
            }
        };
        Ok(PyVariance {
            inner: Variance::new(k),
        })
    }

    fn update(&mut self, py: Python, x: f64) -> PyResult<()> {
        self.inner.update(x).map_err(|e| to_err(py, e))
    }

    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }

    #[getter]
    fn stddev(&self) -> Option<f64> {
        self.inner.std_dev()
    }

    #[getter]
    fn mean(&self) -> f64 {
        self.inner.mean()
    }

    #[getter]
    fn count(&self) -> u64 {
        self.inner.count()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyVariance {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

/// Exponentially weighted mean.
#[pyclass(name = "EWMean")]
pub struct PyEWMean {
    inner: ExponentiallyWeightedMean,
}

#[pymethods]
impl PyEWMean {
    #[new]
    fn new(py: Python, alpha: f64) -> PyResult<Self> {
        Ok(PyEWMean {
            inner: ExponentiallyWeightedMean::new(alpha).map_err(|e| to_err(py, e))?,
        })
    }

    fn update(&mut self, py: Python, x: f64) -> PyResult<()> {
        self.inner.update(x).map_err(|e| to_err(py, e))
    }

    #[getter]
    fn value(&self) -> f64 {
        self.inner.value()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyEWMean {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

/// Online standard scaler.
#[pyclass(name = "StandardScaler")]
pub struct PyStandardScaler {
    inner: StandardScaler,
}

#[pymethods]
impl PyStandardScaler {
    #[new]
    fn new(py: Python, feature_count: usize) -> PyResult<Self> {
        Ok(PyStandardScaler {
            inner: StandardScaler::new(feature_count).map_err(|e| to_err(py, e))?,
        })
    }

    fn transform_one(&self, py: Python, x: Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
        let v = vec_from_list(&x)?;
        self.inner.transform(&v).map_err(|e| to_err(py, e))
    }

    fn fit_one(&mut self, py: Python, x: Bound<'_, PyAny>) -> PyResult<()> {
        let v = vec_from_list(&x)?;
        self.inner.update(&v).map_err(|e| to_err(py, e))
    }

    fn learn_one(&mut self, py: Python, x: Bound<'_, PyAny>) -> PyResult<()> {
        self.fit_one(py, x)
    }

    #[getter]
    fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyStandardScaler {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

/// Online linear regression.
#[pyclass(name = "LinearRegression")]
pub struct PyLinearRegression {
    inner: LinearRegression,
}

#[pymethods]
impl PyLinearRegression {
    #[new]
    fn new(py: Python, feature_count: usize, learning_rate: f64) -> PyResult<Self> {
        let mut sgd = SgdConfig::default();
        sgd.learning_rate = learning_rate;
        sgd.l2 = 0.0;
        let optimizer = Optimizer::sgd(feature_count, sgd).map_err(|e| to_err(py, e))?;
        let mut lr = LinearRegressionConfig::default();
        lr.optimizer = optimizer;
        lr.loss = RegressionLoss::default();
        let model = LinearRegression::new(feature_count, lr).map_err(|e| to_err(py, e))?;
        Ok(PyLinearRegression { inner: model })
    }

    fn predict_one(&self, py: Python, x: Bound<'_, PyAny>) -> PyResult<f64> {
        let v = vec_from_list(&x)?;
        self.inner.predict(&v).map_err(|e| to_err(py, e))
    }

    fn learn_one(&mut self, py: Python, x: Bound<'_, PyAny>, y: f64) -> PyResult<()> {
        let v = vec_from_list(&x)?;
        self.inner.learn(&v, y).map_err(|e| to_err(py, e))
    }

    #[getter]
    fn weights(&self) -> Vec<f64> {
        self.inner.weights().to_vec()
    }

    #[getter]
    fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyLinearRegression {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

/// Online logistic regression.
#[pyclass(name = "LogisticRegression")]
pub struct PyLogisticRegression {
    inner: LogisticRegression,
}

#[pymethods]
impl PyLogisticRegression {
    #[new]
    fn new(py: Python, feature_count: usize, learning_rate: f64) -> PyResult<Self> {
        let mut sgd = SgdConfig::default();
        sgd.learning_rate = learning_rate;
        sgd.l2 = 0.0;
        let optimizer = Optimizer::sgd(feature_count, sgd).map_err(|e| to_err(py, e))?;
        let mut lr = LogisticRegressionConfig::default();
        lr.optimizer = optimizer;
        lr.loss = BinaryLogLoss::new();
        let model = LogisticRegression::new(feature_count, lr).map_err(|e| to_err(py, e))?;
        Ok(PyLogisticRegression { inner: model })
    }

    fn predict_one(&self, py: Python, x: Bound<'_, PyAny>) -> PyResult<bool> {
        let v = vec_from_list(&x)?;
        self.inner.predict(&v).map_err(|e| to_err(py, e))
    }

    fn predict_proba_one(&self, py: Python, x: Bound<'_, PyAny>) -> PyResult<f64> {
        let v = vec_from_list(&x)?;
        self.inner.predict_proba(&v).map_err(|e| to_err(py, e))
    }

    fn learn_one(&mut self, py: Python, x: Bound<'_, PyAny>, y: bool) -> PyResult<()> {
        let v = vec_from_list(&x)?;
        self.inner.learn(&v, y).map_err(|e| to_err(py, e))
    }

    #[getter]
    fn weights(&self) -> Vec<f64> {
        self.inner.weights().to_vec()
    }

    #[getter]
    fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyLogisticRegression {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

/// Regression pipeline: `StandardScaler` + `LinearRegression`.
#[pyclass(name = "RegressionPipeline")]
pub struct PyRegressionPipeline {
    inner: RegressionPipeline<StandardScaler, LinearRegression>,
}

#[pymethods]
impl PyRegressionPipeline {
    #[new]
    fn new(py: Python, feature_count: usize, learning_rate: f64) -> PyResult<Self> {
        let scaler = StandardScaler::new(feature_count).map_err(|e| to_err(py, e))?;
        let mut sgd = SgdConfig::default();
        sgd.learning_rate = learning_rate;
        sgd.l2 = 0.0;
        let optimizer = Optimizer::sgd(feature_count, sgd).map_err(|e| to_err(py, e))?;
        let mut lr = LinearRegressionConfig::default();
        lr.optimizer = optimizer;
        lr.loss = RegressionLoss::default();
        let model = LinearRegression::new(feature_count, lr).map_err(|e| to_err(py, e))?;
        let pipe = RegressionPipeline::new(scaler, model).map_err(|e| to_err(py, e))?;
        Ok(PyRegressionPipeline { inner: pipe })
    }

    fn predict_one(&self, py: Python, x: Bound<'_, PyAny>) -> PyResult<f64> {
        let v = vec_from_list(&x)?;
        self.inner.predict(&v).map_err(|e| to_err(py, e))
    }

    fn learn_one(&mut self, py: Python, x: Bound<'_, PyAny>, y: f64) -> PyResult<()> {
        let v = vec_from_list(&x)?;
        self.inner.learn(&v, y).map_err(|e| to_err(py, e))
    }

    #[getter]
    fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyRegressionPipeline {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

/// Classification pipeline: `StandardScaler` + `LogisticRegression`.
#[pyclass(name = "ClassificationPipeline")]
pub struct PyClassificationPipeline {
    inner: ClassificationPipeline<StandardScaler, LogisticRegression>,
}

#[pymethods]
impl PyClassificationPipeline {
    #[new]
    fn new(py: Python, feature_count: usize, learning_rate: f64) -> PyResult<Self> {
        let scaler = StandardScaler::new(feature_count).map_err(|e| to_err(py, e))?;
        let mut sgd = SgdConfig::default();
        sgd.learning_rate = learning_rate;
        sgd.l2 = 0.0;
        let optimizer = Optimizer::sgd(feature_count, sgd).map_err(|e| to_err(py, e))?;
        let mut lr = LogisticRegressionConfig::default();
        lr.optimizer = optimizer;
        lr.loss = BinaryLogLoss::new();
        let model = LogisticRegression::new(feature_count, lr).map_err(|e| to_err(py, e))?;
        let pipe = ClassificationPipeline::new(scaler, model).map_err(|e| to_err(py, e))?;
        Ok(PyClassificationPipeline { inner: pipe })
    }

    fn predict_one(&self, py: Python, x: Bound<'_, PyAny>) -> PyResult<bool> {
        let v = vec_from_list(&x)?;
        self.inner.predict(&v).map_err(|e| to_err(py, e))
    }

    fn predict_proba_one(&self, py: Python, x: Bound<'_, PyAny>) -> PyResult<f64> {
        let v = vec_from_list(&x)?;
        self.inner.predict_proba(&v).map_err(|e| to_err(py, e))
    }

    fn learn_one(&mut self, py: Python, x: Bound<'_, PyAny>, y: bool) -> PyResult<()> {
        let v = vec_from_list(&x)?;
        self.inner.learn(&v, y).map_err(|e| to_err(py, e))
    }

    #[getter]
    fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&Snapshot::new(self.inner.clone()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(py: Python, json: &str) -> PyResult<Self> {
        Ok(PyClassificationPipeline {
            inner: Snapshot::from_json_validated(json).map_err(|e| to_err(py, e))?,
        })
    }
}

/// Snapshot helper namespace.
///
/// Each model class already exposes its own `to_json` method (serialization
/// via `Snapshot<T>`). `Snapshot` is exported as a namespace marker so
/// downstream code can feature-detect the binding at runtime.
#[pyclass(name = "Snapshot")]
pub struct PySnapshot;

#[pymethods]
impl PySnapshot {
    /// Returns the snapshot format version supported by this build.
    #[staticmethod]
    fn format_version() -> u32 {
        ::rill_ml::persistence::SNAPSHOT_FORMAT_VERSION
    }

    /// Serialize any object exposing a `to_json` method.
    #[staticmethod]
    fn to_json(obj: &Bound<'_, PyAny>) -> PyResult<String> {
        obj.call_method0("to_json")?.extract::<String>()
    }
}

#[pymodule]
fn rill_ml(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLinUcb>()?;
    m.add_class::<PyMean>()?;
    m.add_class::<PyVariance>()?;
    m.add_class::<PyEWMean>()?;
    m.add_class::<PyStandardScaler>()?;
    m.add_class::<PyLinearRegression>()?;
    m.add_class::<PyLogisticRegression>()?;
    m.add_class::<PyRegressionPipeline>()?;
    m.add_class::<PyClassificationPipeline>()?;
    m.add_class::<PySnapshot>()?;
    m.add_function(wrap_pyfunction!(replay_decisions, m)?)?;
    Ok(())
}

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

use ::rill_ml::loss::{BinaryLogLoss, RegressionLoss};
use ::rill_ml::models::{
    LinearRegression, LinearRegressionConfig, LogisticRegression, LogisticRegressionConfig,
};
use ::rill_ml::optim::{Optimizer, SgdConfig};
use ::rill_ml::persistence::Snapshot;
use ::rill_ml::pipeline::{ClassificationPipeline, RegressionPipeline};
use ::rill_ml::preprocessing::StandardScaler;
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
        let snap: Snapshot<Mean> = serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyMean {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
        })
    }
}

impl Default for PyMean {
    fn default() -> Self {
        Self::new()
    }
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
        let snap: Snapshot<Variance> = serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyVariance {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
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
        let snap: Snapshot<ExponentiallyWeightedMean> =
            serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyEWMean {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
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
        let snap: Snapshot<StandardScaler> =
            serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyStandardScaler {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
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
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(|e| to_err(py, e))?;
        let model = LinearRegression::new(
            feature_count,
            LinearRegressionConfig {
                optimizer,
                loss: RegressionLoss::default(),
            },
        )
        .map_err(|e| to_err(py, e))?;
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
        let snap: Snapshot<LinearRegression> =
            serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyLinearRegression {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
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
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(|e| to_err(py, e))?;
        let model = LogisticRegression::new(
            feature_count,
            LogisticRegressionConfig {
                optimizer,
                loss: BinaryLogLoss::new(),
            },
        )
        .map_err(|e| to_err(py, e))?;
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
        let snap: Snapshot<LogisticRegression> =
            serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyLogisticRegression {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
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
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(|e| to_err(py, e))?;
        let model = LinearRegression::new(
            feature_count,
            LinearRegressionConfig {
                optimizer,
                loss: RegressionLoss::default(),
            },
        )
        .map_err(|e| to_err(py, e))?;
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
        let snap: Snapshot<RegressionPipeline<StandardScaler, LinearRegression>> =
            serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyRegressionPipeline {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
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
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(|e| to_err(py, e))?;
        let model = LogisticRegression::new(
            feature_count,
            LogisticRegressionConfig {
                optimizer,
                loss: BinaryLogLoss::new(),
            },
        )
        .map_err(|e| to_err(py, e))?;
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
        let snap: Snapshot<ClassificationPipeline<StandardScaler, LogisticRegression>> =
            serde_json::from_str(json).map_err(|e| to_err(py, e))?;
        Ok(PyClassificationPipeline {
            inner: snap.into_model().map_err(|e| to_err(py, e))?,
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
    m.add_class::<PyMean>()?;
    m.add_class::<PyVariance>()?;
    m.add_class::<PyEWMean>()?;
    m.add_class::<PyStandardScaler>()?;
    m.add_class::<PyLinearRegression>()?;
    m.add_class::<PyLogisticRegression>()?;
    m.add_class::<PyRegressionPipeline>()?;
    m.add_class::<PyClassificationPipeline>()?;
    m.add_class::<PySnapshot>()?;
    Ok(())
}

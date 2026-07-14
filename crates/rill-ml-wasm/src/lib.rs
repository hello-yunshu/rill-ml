//! WebAssembly bindings for RillML core subset.
//!
//! Exposes online statistics, scalers, linear/logistic regression, pipelines,
//! and snapshot serialization to the browser via `wasm-bindgen`.
//!
//! All classes expose `to_json()` (serialization via `Snapshot<T>`).
//! `from_json` is provided as associated functions where feasible.

use js_sys::Float64Array;
use rill_ml::loss::{BinaryLogLoss, RegressionLoss};
use rill_ml::models::{
    LinearRegression, LinearRegressionConfig, LogisticRegression, LogisticRegressionConfig,
};
use rill_ml::optim::{Optimizer, SgdConfig};
use rill_ml::persistence::Snapshot;
use rill_ml::pipeline::{ClassificationPipeline, RegressionPipeline};
use rill_ml::preprocessing::StandardScaler;
use rill_ml::stats::{ExponentiallyWeightedMean, Mean, Variance, VarianceKind};
use rill_ml::traits::{OnlineBinaryClassifier, OnlineRegressor, OnlineStatistic, Transformer};
use wasm_bindgen::prelude::*;

fn js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&format!("{}", e))
}

fn slice_from_js(arr: &Float64Array) -> Vec<f64> {
    arr.to_vec()
}

/// Library version string (matches `rill-ml-wasm` crate version).
#[wasm_bindgen]
pub fn _rill_ml_wasm_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Online mean accumulator.
#[wasm_bindgen]
pub struct WasmMean {
    inner: Mean,
}

#[wasm_bindgen]
impl WasmMean {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmMean {
        WasmMean { inner: Mean::new() }
    }

    pub fn update(&mut self, x: f64) -> Result<(), JsValue> {
        self.inner.update(x).map_err(js_err)
    }

    pub fn value(&self) -> f64 {
        self.inner.value()
    }

    pub fn count(&self) -> u64 {
        self.inner.count()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmMean, JsValue> {
        let snap: Snapshot<Mean> = serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmMean {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

impl Default for WasmMean {
    fn default() -> Self {
        Self::new()
    }
}

/// Online variance accumulator (Welford). `value`/`std_dev` return `null`
/// until enough samples have been observed.
#[wasm_bindgen]
pub struct WasmVariance {
    inner: Variance,
}

#[wasm_bindgen]
impl WasmVariance {
    #[wasm_bindgen(constructor)]
    pub fn new(kind: &str) -> Result<WasmVariance, JsValue> {
        let k = match kind {
            "population" => VarianceKind::Population,
            "sample" => VarianceKind::Sample,
            other => {
                return Err(JsValue::from_str(&format!(
                    "unknown variance kind: {} (expected 'population' or 'sample')",
                    other
                )));
            }
        };
        Ok(WasmVariance {
            inner: Variance::new(k),
        })
    }

    pub fn update(&mut self, x: f64) -> Result<(), JsValue> {
        self.inner.update(x).map_err(js_err)
    }

    pub fn value(&self) -> Option<f64> {
        self.inner.value()
    }

    pub fn std_dev(&self) -> Option<f64> {
        self.inner.std_dev()
    }

    pub fn mean(&self) -> f64 {
        self.inner.mean()
    }

    pub fn count(&self) -> u64 {
        self.inner.count()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmVariance, JsValue> {
        let snap: Snapshot<Variance> = serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmVariance {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

/// Exponentially weighted mean.
#[wasm_bindgen]
pub struct WasmEWMean {
    inner: ExponentiallyWeightedMean,
}

#[wasm_bindgen]
impl WasmEWMean {
    #[wasm_bindgen(constructor)]
    pub fn new(alpha: f64) -> Result<WasmEWMean, JsValue> {
        Ok(WasmEWMean {
            inner: ExponentiallyWeightedMean::new(alpha).map_err(js_err)?,
        })
    }

    pub fn update(&mut self, x: f64) -> Result<(), JsValue> {
        self.inner.update(x).map_err(js_err)
    }

    pub fn value(&self) -> f64 {
        self.inner.value()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmEWMean, JsValue> {
        let snap: Snapshot<ExponentiallyWeightedMean> =
            serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmEWMean {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

/// Online standard scaler.
#[wasm_bindgen]
pub struct WasmStandardScaler {
    inner: StandardScaler,
}

#[wasm_bindgen]
impl WasmStandardScaler {
    #[wasm_bindgen(constructor)]
    pub fn new(feature_count: usize) -> Result<WasmStandardScaler, JsValue> {
        Ok(WasmStandardScaler {
            inner: StandardScaler::new(feature_count).map_err(js_err)?,
        })
    }

    pub fn transform(&self, x: &Float64Array) -> Result<Float64Array, JsValue> {
        let v = slice_from_js(x);
        let out = self.inner.transform(&v).map_err(js_err)?;
        Ok(Float64Array::from(&out[..]))
    }

    pub fn update(&mut self, x: &Float64Array) -> Result<(), JsValue> {
        self.inner.update(&slice_from_js(x)).map_err(js_err)
    }

    /// Alias for `update` (Transformer has no separate `learn`).
    pub fn learn_one(&mut self, x: &Float64Array) -> Result<(), JsValue> {
        self.inner.update(&slice_from_js(x)).map_err(js_err)
    }

    pub fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmStandardScaler, JsValue> {
        let snap: Snapshot<StandardScaler> = serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmStandardScaler {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

/// Online linear regression.
#[wasm_bindgen]
pub struct WasmLinearRegression {
    inner: LinearRegression,
}

#[wasm_bindgen]
impl WasmLinearRegression {
    #[wasm_bindgen(constructor)]
    pub fn new(feature_count: usize, learning_rate: f64) -> Result<WasmLinearRegression, JsValue> {
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(js_err)?;
        let model = LinearRegression::new(
            feature_count,
            LinearRegressionConfig {
                optimizer,
                loss: RegressionLoss::default(),
            },
        )
        .map_err(js_err)?;
        Ok(WasmLinearRegression { inner: model })
    }

    pub fn predict(&self, x: &Float64Array) -> Result<f64, JsValue> {
        self.inner.predict(&slice_from_js(x)).map_err(js_err)
    }

    pub fn learn(&mut self, x: &Float64Array, y: f64) -> Result<(), JsValue> {
        self.inner.learn(&slice_from_js(x), y).map_err(js_err)
    }

    pub fn weights(&self) -> Float64Array {
        Float64Array::from(self.inner.weights())
    }

    pub fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmLinearRegression, JsValue> {
        let snap: Snapshot<LinearRegression> = serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmLinearRegression {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

/// Online logistic regression.
#[wasm_bindgen]
pub struct WasmLogisticRegression {
    inner: LogisticRegression,
}

#[wasm_bindgen]
impl WasmLogisticRegression {
    #[wasm_bindgen(constructor)]
    pub fn new(
        feature_count: usize,
        learning_rate: f64,
    ) -> Result<WasmLogisticRegression, JsValue> {
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(js_err)?;
        let model = LogisticRegression::new(
            feature_count,
            LogisticRegressionConfig {
                optimizer,
                loss: BinaryLogLoss::new(),
            },
        )
        .map_err(js_err)?;
        Ok(WasmLogisticRegression { inner: model })
    }

    pub fn predict(&self, x: &Float64Array) -> Result<bool, JsValue> {
        self.inner.predict(&slice_from_js(x)).map_err(js_err)
    }

    pub fn predict_proba(&self, x: &Float64Array) -> Result<f64, JsValue> {
        self.inner.predict_proba(&slice_from_js(x)).map_err(js_err)
    }

    pub fn learn(&mut self, x: &Float64Array, y: bool) -> Result<(), JsValue> {
        self.inner.learn(&slice_from_js(x), y).map_err(js_err)
    }

    pub fn weights(&self) -> Float64Array {
        Float64Array::from(self.inner.weights())
    }

    pub fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmLogisticRegression, JsValue> {
        let snap: Snapshot<LogisticRegression> = serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmLogisticRegression {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

/// Regression pipeline: `StandardScaler` + `LinearRegression`.
#[wasm_bindgen]
pub struct WasmRegressionPipeline {
    inner: RegressionPipeline<StandardScaler, LinearRegression>,
}

#[wasm_bindgen]
impl WasmRegressionPipeline {
    #[wasm_bindgen(constructor)]
    pub fn new(
        feature_count: usize,
        learning_rate: f64,
    ) -> Result<WasmRegressionPipeline, JsValue> {
        let scaler = StandardScaler::new(feature_count).map_err(js_err)?;
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(js_err)?;
        let model = LinearRegression::new(
            feature_count,
            LinearRegressionConfig {
                optimizer,
                loss: RegressionLoss::default(),
            },
        )
        .map_err(js_err)?;
        let pipe = RegressionPipeline::new(scaler, model).map_err(js_err)?;
        Ok(WasmRegressionPipeline { inner: pipe })
    }

    pub fn predict(&self, x: &Float64Array) -> Result<f64, JsValue> {
        self.inner.predict(&slice_from_js(x)).map_err(js_err)
    }

    pub fn learn(&mut self, x: &Float64Array, y: f64) -> Result<(), JsValue> {
        self.inner.learn(&slice_from_js(x), y).map_err(js_err)
    }

    pub fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmRegressionPipeline, JsValue> {
        let snap: Snapshot<RegressionPipeline<StandardScaler, LinearRegression>> =
            serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmRegressionPipeline {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

/// Classification pipeline: `StandardScaler` + `LogisticRegression`.
#[wasm_bindgen]
pub struct WasmClassificationPipeline {
    inner: ClassificationPipeline<StandardScaler, LogisticRegression>,
}

#[wasm_bindgen]
impl WasmClassificationPipeline {
    #[wasm_bindgen(constructor)]
    pub fn new(
        feature_count: usize,
        learning_rate: f64,
    ) -> Result<WasmClassificationPipeline, JsValue> {
        let scaler = StandardScaler::new(feature_count).map_err(js_err)?;
        let optimizer = Optimizer::sgd(
            feature_count,
            SgdConfig {
                learning_rate,
                l2: 0.0,
            },
        )
        .map_err(js_err)?;
        let model = LogisticRegression::new(
            feature_count,
            LogisticRegressionConfig {
                optimizer,
                loss: BinaryLogLoss::new(),
            },
        )
        .map_err(js_err)?;
        let pipe = ClassificationPipeline::new(scaler, model).map_err(js_err)?;
        Ok(WasmClassificationPipeline { inner: pipe })
    }

    pub fn predict(&self, x: &Float64Array) -> Result<bool, JsValue> {
        self.inner.predict(&slice_from_js(x)).map_err(js_err)
    }

    pub fn predict_proba(&self, x: &Float64Array) -> Result<f64, JsValue> {
        self.inner.predict_proba(&slice_from_js(x)).map_err(js_err)
    }

    pub fn learn(&mut self, x: &Float64Array, y: bool) -> Result<(), JsValue> {
        self.inner.learn(&slice_from_js(x), y).map_err(js_err)
    }

    pub fn samples_seen(&self) -> u64 {
        self.inner.samples_seen()
    }

    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&Snapshot::new(self.inner.clone())).map_err(js_err)
    }

    pub fn from_json(json: &str) -> Result<WasmClassificationPipeline, JsValue> {
        let snap: Snapshot<ClassificationPipeline<StandardScaler, LogisticRegression>> =
            serde_json::from_str(json).map_err(js_err)?;
        Ok(WasmClassificationPipeline {
            inner: snap.into_model().map_err(js_err)?,
        })
    }
}

/// Marker type for the RillML snapshot namespace.
///
/// Each `WasmX` class exposes its own `to_json` method (serialization via
/// `Snapshot<T>`). `WasmSnapshot` itself is exported as an empty marker so
/// that downstream code can feature-detect the binding at runtime.
#[wasm_bindgen]
pub struct WasmSnapshot;

#[wasm_bindgen]
impl WasmSnapshot {
    /// Returns the snapshot format version supported by this build.
    pub fn format_version() -> u32 {
        rill_ml::persistence::SNAPSHOT_FORMAT_VERSION
    }
}

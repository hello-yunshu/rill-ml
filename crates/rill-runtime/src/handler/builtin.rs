//! Built-in handler that executes linear-regression inference in-process.
//!
//! This handler is preserved for backwards compatibility and as a fallback
//! when a WASM handler is not available. It does not cross a sandbox
//! boundary; the runtime binary selects it via `--builtin-handler
//! linear-regression`.

use serde::Deserialize;
use serde_json::Value;

use crate::package::LoadedModelPack;
use crate::server::InvokeHandler;

pub const LINEAR_REGRESSION_CAPABILITY: &str = "rillml.linearRegression.predict";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LinearRegressionModel {
    kind: String,
    weights: Vec<f64>,
    intercept: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LinearRegressionInput {
    features: Vec<f64>,
}

/// Business-neutral linear-regression handler used by the distributed runtime binary.
#[derive(Debug, Clone)]
pub struct LinearRegressionInvokeHandler {
    weights: Vec<f64>,
    intercept: f64,
}

impl LinearRegressionInvokeHandler {
    pub fn from_pack(pack: &LoadedModelPack) -> Result<Self, String> {
        if pack.manifest.capabilities.as_slice() != [LINEAR_REGRESSION_CAPABILITY] {
            return Err(format!(
                "standalone runtime requires exactly the {LINEAR_REGRESSION_CAPABILITY} capability"
            ));
        }
        let model: LinearRegressionModel = serde_json::from_value(pack.model.clone())
            .map_err(|error| format!("invalid linear-regression model: {error}"))?;
        if model.kind != "linearRegression" {
            return Err("unsupported built-in model kind".into());
        }
        if model.weights.is_empty() || model.weights.len() > 65_536 {
            return Err("linear-regression weights must contain 1..=65536 values".into());
        }
        if !model.intercept.is_finite() || model.weights.iter().any(|value| !value.is_finite()) {
            return Err("linear-regression model values must be finite".into());
        }
        Ok(Self {
            weights: model.weights,
            intercept: model.intercept,
        })
    }
}

impl InvokeHandler for LinearRegressionInvokeHandler {
    fn invoke(&self, capability: &str, input: &Value) -> Result<Value, String> {
        if capability != LINEAR_REGRESSION_CAPABILITY {
            return Err("unsupported capability".into());
        }
        let input: LinearRegressionInput = serde_json::from_value(input.clone())
            .map_err(|error| format!("invalid linear-regression input: {error}"))?;
        if input.features.len() != self.weights.len() {
            return Err(format!(
                "expected {} features, received {}",
                self.weights.len(),
                input.features.len()
            ));
        }
        if input.features.iter().any(|value| !value.is_finite()) {
            return Err("linear-regression input values must be finite".into());
        }
        let prediction = self
            .weights
            .iter()
            .zip(&input.features)
            .try_fold(self.intercept, |sum, (weight, feature)| {
                let next = sum + weight * feature;
                next.is_finite().then_some(next)
            })
            .ok_or_else(|| "linear-regression prediction overflowed".to_string())?;
        Ok(serde_json::json!({ "prediction": prediction }))
    }
}

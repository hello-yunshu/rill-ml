use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[path = "../backend.rs"]
mod backend;

use clap::{Parser, Subcommand};
use ed25519_dalek::VerifyingKey;
use fs2::FileExt;
#[cfg(feature = "wasm")]
use rill_runtime::effective_capabilities;
use rill_runtime::{
    HandlerIdentity, HandlerPackError, InvokeHandler, LinearRegressionInvokeHandler,
    LoadedHandlerPack, ModelPackError, RuntimeEngine, StatefulHandlerMetadataV2,
    StatefulHandlerResultV2, StatefulHandlerV2, StatefulRuntimeConfigV3, StatefulRuntimeEngineV3,
    StatefulRuntimeSnapshotV3, TrustStore, load_model_pack,
};
use rill_runtime_protocol::{
    MAX_MESSAGE_BYTES, MIN_RUNTIME_API_VERSION, RUNTIME_API_VERSION, RuntimeRequest,
    RuntimeResponse, RuntimeResponseV2, error_code,
};
use thiserror::Error;

const DEFAULT_FEATURE_SCHEMA_HASH: &str =
    "99c44934c1bfca8fdffb93122d29418d7fe7eb0d81d80f8b2bf4fbdd153151ab";

/// Maximum serialized JSON accepted on disk for preview snapshots. The
/// runtime's internal snapshot budget remains 512 KiB; this larger envelope
/// permits JSON number-array expansion and historical whitespace while still
/// bounding disk reads before parsing.
const MAX_STATE_FILE_JSON_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    name = "rill-runtime",
    version,
    about = "Signed-model local inference runtime"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print additive runtime qualification metadata without starting IPC.
    Diagnostics,
    /// Serve newline-delimited JSON requests over stdin/stdout.
    Serve {
        #[arg(long)]
        pack: PathBuf,
        /// Trusted Ed25519 public key for model packs, as KEY_ID=64_HEX_CHARS.
        /// May be repeated. `--model-trust-key` is the primary name;
        /// `--trust-key` is a deprecated alias kept for 1.x compatibility.
        #[arg(long = "model-trust-key", alias = "trust-key")]
        model_trust_keys: Vec<String>,
        /// Trusted Ed25519 public key for handler packs, as KEY_ID=64_HEX_CHARS.
        /// May be repeated.
        #[arg(long = "handler-trust-key")]
        handler_trust_keys: Vec<String>,
        /// Path to a signed `.rillhandler` file. Mutually exclusive with
        /// `--builtin-handler`.
        #[arg(long)]
        handler: Option<PathBuf>,
        /// Select a built-in handler by name. Currently only
        /// `linear-regression` is supported, and is retained as an explicit
        /// compatibility path. Mutually exclusive with `--handler`.
        #[arg(long)]
        builtin_handler: Option<String>,
    },
    /// Explicit opt-in Preview Stateful Runtime v3 subprocess surface.
    /// Stable `serve` remains v1/v2-only and is never switched implicitly.
    PreviewServe {
        /// Atomic runtime snapshot path. The file contains handler state and
        /// the delayed decision ledger and is preserved across restart.
        #[arg(long)]
        state: PathBuf,
        #[arg(
            long,
            default_value = DEFAULT_FEATURE_SCHEMA_HASH
        )]
        feature_schema_hash: String,
        #[arg(long, default_value_t = 0)]
        model_generation: u64,
    },
    /// Verify and print metadata for a signed model package.
    InspectPack {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long = "model-trust-key", alias = "trust-key", required = true)]
        trust_keys: Vec<String>,
    },
    /// Verify and print metadata for a signed handler package.
    InspectHandler {
        #[arg(long)]
        handler: PathBuf,
        #[arg(long = "handler-trust-key", required = true)]
        handler_trust_keys: Vec<String>,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("model package error: {0}")]
    Pack(#[from] ModelPackError),
    #[error("handler package error: {0}")]
    HandlerPack(#[from] HandlerPackError),
    #[error("invalid trusted key: {0}")]
    TrustKey(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("runtime handler error: {0}")]
    Handler(String),
    #[error("IPC message exceeds {MAX_MESSAGE_BYTES} bytes")]
    MessageTooLarge,
    #[error("--handler and --builtin-handler are mutually exclusive")]
    ConflictingHandlerOption,
    #[error("unknown built-in handler: {0}")]
    UnknownBuiltinHandler(String),
    #[error("preview runtime error: {0}")]
    Preview(String),
    #[error("state snapshot exceeds the {MAX_STATE_FILE_JSON_BYTES}-byte disk JSON limit")]
    StateSnapshotTooLarge,
    #[error(
        "no --handler or --builtin-handler specified; \
         pass --handler PATH to load a signed .rillhandler, \
         or --builtin-handler linear-regression for the deprecated built-in path"
    )]
    MissingHandlerOption,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("rill-runtime: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Diagnostics => {
            println!(
                "{}",
                serde_json::json!({
                    "schemaVersion": 1,
                    "backend": runtime_backend(),
                    "pointerWidth": usize::BITS,
                    "endianness": runtime_endianness(),
                    "arch": std::env::consts::ARCH,
                    "os": std::env::consts::OS,
                    "platform": backend::platform_identity(),
                    "targetEnv": backend::target_environment(),
                })
            );
            Ok(())
        }
        Command::Serve {
            pack,
            model_trust_keys,
            handler_trust_keys,
            handler,
            builtin_handler,
        } => {
            if handler.is_some() && builtin_handler.is_some() {
                return Err(CliError::ConflictingHandlerOption);
            }
            let model_trust = parse_trust_store(&model_trust_keys)?;
            let (loaded, _) = load_model_pack(File::open(&pack)?, &model_trust)?;

            let (invoke_handler, identity) = match (&handler, &builtin_handler) {
                (Some(handler_path), None) => {
                    let handler_trust = parse_trust_store(&handler_trust_keys)?;
                    let (loaded_handler, _) =
                        rill_runtime::load_handler_pack(File::open(handler_path)?, &handler_trust)?;
                    build_wasm_handler(&loaded, &loaded_handler)?
                }
                (None, Some(name)) => {
                    let name = name.as_str();
                    if name != "linear-regression" {
                        return Err(CliError::UnknownBuiltinHandler(name.into()));
                    }
                    eprintln!(
                        "rill-runtime: --builtin-handler linear-regression is deprecated; \
                         use --handler with a signed .rillhandler in future releases"
                    );
                    let handler = LinearRegressionInvokeHandler::from_pack(&loaded)
                        .map_err(CliError::Handler)?;
                    let identity = HandlerIdentity {
                        handler_id: "rillml.builtin.linear-regression".into(),
                        handler_version: env!("CARGO_PKG_VERSION").into(),
                        handler_api_version: 0,
                        effective_capabilities: loaded.manifest.capabilities.clone(),
                    };
                    (Arc::new(handler) as Arc<dyn InvokeHandler>, identity)
                }
                (None, None) => {
                    // 1.0 contract: no implicit fallback. The runtime must
                    // fail to start when neither --handler nor
                    // --builtin-handler is passed. The previous behaviour
                    // silently fell back to the deprecated built-in handler,
                    // which contradicted the 1.0 deprecation policy.
                    return Err(CliError::MissingHandlerOption);
                }
                _ => return Err(CliError::ConflictingHandlerOption),
            };

            let engine = RuntimeEngine::new(loaded)
                .with_invoke_handler(invoke_handler)
                .with_handler_identity(identity);
            serve(engine)
        }
        Command::PreviewServe {
            state,
            feature_schema_hash,
            model_generation,
        } => preview_serve(state, feature_schema_hash, model_generation),
        Command::InspectPack { pack, trust_keys } => {
            let trust = parse_trust_store(&trust_keys)?;
            let (_, inspection) = load_model_pack(File::open(pack)?, &trust)?;
            println!("{}", serde_json::to_string_pretty(&inspection)?);
            Ok(())
        }
        Command::InspectHandler {
            handler,
            handler_trust_keys,
        } => {
            let trust = parse_trust_store(&handler_trust_keys)?;
            let (_, inspection) = rill_runtime::load_handler_pack(File::open(handler)?, &trust)?;
            println!("{}", serde_json::to_string_pretty(&inspection)?);
            Ok(())
        }
    }
}

fn runtime_backend() -> &'static str {
    backend::runtime_backend()
}

fn runtime_endianness() -> &'static str {
    if cfg!(target_endian = "big") {
        "big"
    } else {
        "little"
    }
}

#[derive(Debug)]
struct PreviewBuiltinHandler {
    metadata: StatefulHandlerMetadataV2,
}

impl PreviewBuiltinHandler {
    fn new() -> Self {
        Self {
            metadata: StatefulHandlerMetadataV2 {
                id: "rillml.preview.stateful-runtime".into(),
                version: "3.0.0-preview".into(),
                api_version: 2,
                capabilities: vec![
                    "org.rill.preview.observe".into(),
                    "org.rill.preview.decide".into(),
                    "org.rill.preview.feedback".into(),
                    "org.rill.preview.inspect".into(),
                    "org.rill.preview.snapshot".into(),
                    "org.rill.preview.reset".into(),
                ],
                state_schema_version: 2,
            },
        }
    }
}

fn builtin_state_error(detail: &'static str) -> rill_runtime::StatefulHandlerErrorV2 {
    rill_runtime::StatefulHandlerErrorV2::with_detail(
        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
        detail,
    )
}

fn builtin_event_error(detail: &'static str) -> rill_runtime::StatefulHandlerErrorV2 {
    rill_runtime::StatefulHandlerErrorV2::with_detail(
        rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
        detail,
    )
}

fn validate_builtin_state(
    state: &serde_json::Value,
) -> Result<(), rill_runtime::StatefulHandlerErrorV2> {
    if state
        .get("handlerStateVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
    {
        return Err(rill_runtime::StatefulHandlerErrorV2::with_detail(
            rill_runtime::StatefulHandlerErrorKindV2::IncompatibleVersion,
            "contextual learner state version is not supported",
        ));
    }
    for counter in ["decisions", "feedback", "observations", "inspections"] {
        if state
            .get(counter)
            .and_then(serde_json::Value::as_u64)
            .is_none()
        {
            return Err(builtin_state_error("learner counter is invalid"));
        }
    }
    let feature_count = state
        .get("featureCount")
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .filter(|count| *count <= 32)
        .ok_or_else(|| builtin_state_error("learner feature count is invalid"))?;
    let weights = state
        .get("weights")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| builtin_state_error("learner weights are invalid"))?;
    let bias = state
        .get("bias")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| builtin_state_error("learner bias must be finite"))?;
    if bias.abs() > 1000.0 {
        return Err(builtin_state_error(
            "learner bias exceeds its persisted range",
        ));
    }
    if weights.len() != feature_count {
        return Err(builtin_state_error("learner weight width is inconsistent"));
    }
    if weights.iter().any(|value| {
        value
            .as_f64()
            .is_none_or(|number| !number.is_finite() || number.abs() > 1000.0)
    }) {
        return Err(builtin_state_error(
            "learner weights must be finite and within range",
        ));
    }
    let actions = state
        .get("actions")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| builtin_state_error("learner actions are invalid"))?;
    if feature_count == 0 && !actions.is_empty() {
        return Err(builtin_state_error(
            "actions exist before a feature width is set",
        ));
    }
    for (id, row) in actions {
        if id.is_empty() || id.len() > 96 {
            return Err(builtin_state_error("stored action id is invalid"));
        }
        let features: Vec<f64> = row
            .get("features")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| builtin_state_error("stored action features are invalid"))?
            .iter()
            .map(serde_json::Value::as_f64)
            .collect::<Option<Vec<_>>>()
            .filter(|values| values.iter().all(|value| value.is_finite()))
            .ok_or_else(|| builtin_state_error("stored action features must be finite"))?;
        if features.len() != feature_count
            || row
                .get("samples")
                .and_then(serde_json::Value::as_u64)
                .is_none()
            || row
                .get("lastReward")
                .is_some_and(|value| value.as_f64().is_none_or(|number| !number.is_finite()))
        {
            return Err(builtin_state_error("stored action state is inconsistent"));
        }
    }
    if state
        .get("lastSelectedActionId")
        .is_some_and(|value| !value.is_null() && value.as_str().is_none())
    {
        return Err(builtin_state_error("last selected action id is invalid"));
    }
    if state
        .get("lastDeterministicSeed")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(builtin_state_error("last deterministic seed is invalid"));
    }
    Ok(())
}

fn finite_dot_product(
    bias: f64,
    weights: &[f64],
    features: &[f64],
) -> Result<f64, rill_runtime::StatefulHandlerErrorV2> {
    if weights.len() != features.len() {
        return Err(builtin_state_error("learner vector widths do not match"));
    }
    let mut sum = bias;
    for (weight, feature) in weights.iter().zip(features) {
        let product = weight * feature;
        if !product.is_finite() {
            return Err(builtin_event_error("learner multiplication overflow"));
        }
        sum += product;
        if !sum.is_finite() {
            return Err(builtin_event_error("learner accumulation overflow"));
        }
    }
    Ok(sum)
}

impl StatefulHandlerV2 for PreviewBuiltinHandler {
    fn metadata(&self) -> &StatefulHandlerMetadataV2 {
        &self.metadata
    }

    fn handle(
        &self,
        event_json: &[u8],
        current_state: &[u8],
        deterministic_seed: Option<u64>,
    ) -> Result<StatefulHandlerResultV2, rill_runtime::StatefulHandlerErrorV2> {
        let mut state: serde_json::Value = serde_json::from_slice(current_state).map_err(|_| {
            rill_runtime::StatefulHandlerErrorV2::new(
                rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
            )
        })?;
        let event: serde_json::Value = serde_json::from_slice(event_json).map_err(|_| {
            rill_runtime::StatefulHandlerErrorV2::new(
                rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
            )
        })?;
        validate_builtin_state(&state)?;
        let method = event
            .get("method")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let counter = match method {
            "observe" => "observations",
            "decide" => "decisions",
            "feedback" => "feedback",
            _ => "inspections",
        };
        let current = state
            .get(counter)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| builtin_state_error("learner counter is invalid"))?;
        let next = current
            .checked_add(1)
            .ok_or_else(|| builtin_state_error("learner counter overflow"))?;
        state[counter] = serde_json::json!(next);

        // This handler is deliberately generic: consumers provide opaque
        // action IDs and bounded feature vectors. The runtime owns a small,
        // deterministic online linear model; it never interprets product
        // vocabulary, action position, or host state.
        let mut selected_action = None;
        let mut scores = Vec::new();
        if method == "decide" {
            let actions = event
                .get("context")
                .and_then(|value| value.get("actions").or_else(|| value.get("arms")))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::with_detail(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                        "context.actions is required",
                    )
                })?;
            if actions.is_empty() || actions.len() > 128 {
                return Err(rill_runtime::StatefulHandlerErrorV2::with_detail(
                    rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                    "actions array is outside the bounded runtime limit",
                ));
            }
            let mut action_rows = BTreeMap::new();
            let mut feature_count = None;
            for action in &actions {
                let id = action
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.is_empty() && id.len() <= 96)
                    .ok_or_else(|| {
                        rill_runtime::StatefulHandlerErrorV2::with_detail(
                            rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                            "every action requires a bounded opaque id",
                        )
                    })?;
                let features = action
                    .get("features")
                    .and_then(serde_json::Value::as_array)
                    .filter(|features| !features.is_empty() && features.len() <= 32)
                    .ok_or_else(|| {
                        rill_runtime::StatefulHandlerErrorV2::with_detail(
                            rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                            "every action requires 1..32 features",
                        )
                    })?;
                if feature_count.is_some_and(|count| count != features.len()) {
                    return Err(rill_runtime::StatefulHandlerErrorV2::with_detail(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                        "all actions must use the same feature width",
                    ));
                }
                feature_count = Some(features.len());
                let numbers: Vec<f64> = features
                    .iter()
                    .map(serde_json::Value::as_f64)
                    .collect::<Option<Vec<_>>>()
                    .filter(|values| values.iter().all(|value| value.is_finite()))
                    .ok_or_else(|| {
                        rill_runtime::StatefulHandlerErrorV2::with_detail(
                            rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                            "features must be finite numbers",
                        )
                    })?;
                if action_rows.insert(id.to_owned(), numbers).is_some() {
                    return Err(rill_runtime::StatefulHandlerErrorV2::with_detail(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                        "action ids must be unique",
                    ));
                }
            }
            let feature_count = feature_count.expect("non-empty actions validated above");
            let weights = state
                .get("weights")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            if !weights.is_empty() && weights.len() != feature_count {
                return Err(rill_runtime::StatefulHandlerErrorV2::with_detail(
                    rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                    "feature width does not match the learner model",
                ));
            }
            if weights.is_empty() {
                state["weights"] = serde_json::json!(vec![0.0f64; feature_count]);
            }
            let weights: Vec<f64> = state
                .get("weights")
                .and_then(serde_json::Value::as_array)
                .expect("weights initialized above")
                .iter()
                .map(serde_json::Value::as_f64)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            let bias = state
                .get("bias")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let stored_actions = state
                .get_mut("actions")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            for (id, features) in &action_rows {
                let row = stored_actions
                    .entry(id.clone())
                    .or_insert_with(|| serde_json::json!({"features": features, "samples": 0u64}));
                row["features"] = serde_json::json!(features);
            }
            let stored = state
                .get("actions")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            let mut best: Option<(bool, f64, String)> = None;
            for (id, features) in action_rows {
                let score = finite_dot_product(bias, &weights, &features)?;
                let samples = stored
                    .get(&id)
                    .and_then(|row| row.get("samples"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                scores.push(serde_json::json!({"id": id, "score": score}));
                let candidate = (samples == 0, score, id.clone());
                if best.as_ref().is_none_or(|current| {
                    (candidate.0 && !current.0)
                        || (candidate.0 == current.0
                            && (candidate.1 > current.1
                                || (candidate.1 == current.1 && candidate.2 < current.2)))
                }) {
                    best = Some(candidate);
                }
            }
            selected_action = best.map(|(_, _, id)| id);
            state["lastSelectedActionId"] = serde_json::json!(selected_action);
            state["featureCount"] = serde_json::json!(feature_count);
            if let Some(seed) = deterministic_seed {
                state["lastDeterministicSeed"] = serde_json::json!(seed);
            }
        } else if method == "feedback" {
            let selected = event
                .get("selectedActionId")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 96)
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::with_detail(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                        "feedback requires selectedActionId",
                    )
                })?;
            let reward = event
                .get("reward")
                .and_then(serde_json::Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                    )
                })?;
            let actions = state
                .get_mut("actions")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            let row = actions.get_mut(selected).ok_or_else(|| {
                rill_runtime::StatefulHandlerErrorV2::with_detail(
                    rill_runtime::StatefulHandlerErrorKindV2::InvalidEvent,
                    "feedback action is not present in the decision ledger",
                )
            })?;
            let samples = row
                .get("samples")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| builtin_state_error("stored action sample count is invalid"))?
                .checked_add(1)
                .ok_or_else(|| builtin_state_error("stored action sample count overflow"))?;
            row["samples"] = serde_json::json!(samples);
            row["lastReward"] = serde_json::json!(reward);
            let features = event
                .get("decisionContext")
                .and_then(|context| context.get("selectedActionFeatures"))
                .cloned()
                .or_else(|| row.get("features").cloned())
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            let features: Vec<f64> = features
                .as_array()
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?
                .iter()
                .map(serde_json::Value::as_f64)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            let old_weights: Vec<f64> = state
                .get("weights")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?
                .iter()
                .map(serde_json::Value::as_f64)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            let bias = state
                .get("bias")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let prediction = finite_dot_product(bias, &old_weights, &features)?;
            let rate = 0.1 / (samples as f64).sqrt();
            let raw_error = reward - prediction;
            if !raw_error.is_finite() {
                return Err(builtin_event_error("feedback error overflow"));
            }
            let error = raw_error.clamp(-1000.0, 1000.0);
            let scaled_error = rate * error;
            if !scaled_error.is_finite() {
                return Err(builtin_event_error("feedback update overflow"));
            }
            let weights = state
                .get_mut("weights")
                .and_then(serde_json::Value::as_array_mut)
                .ok_or_else(|| {
                    rill_runtime::StatefulHandlerErrorV2::new(
                        rill_runtime::StatefulHandlerErrorKindV2::InvalidState,
                    )
                })?;
            for (slot, (weight, feature)) in weights.iter_mut().zip(features).enumerate() {
                let delta = scaled_error * feature;
                let raw_next = old_weights[slot] + delta;
                if !delta.is_finite() || !raw_next.is_finite() {
                    return Err(builtin_event_error("feedback weight update overflow"));
                }
                let next = raw_next.clamp(-1000.0, 1000.0);
                *weight = serde_json::json!(next);
            }
            let raw_bias = bias + scaled_error;
            if !raw_bias.is_finite() {
                return Err(builtin_event_error("feedback bias update overflow"));
            }
            state["bias"] = serde_json::json!(raw_bias.clamp(-1000.0, 1000.0));
        }
        let output = serde_json::json!({
            "accepted": true,
            "method": method,
            "selectedActionId": selected_action,
            "scores": scores,
            "learner": "bounded-contextual-linear-v1",
            "stateCounters": state,
        });
        Ok(StatefulHandlerResultV2 {
            output,
            next_state: serde_json::to_vec(&state).map_err(|_| {
                rill_runtime::StatefulHandlerErrorV2::new(
                    rill_runtime::StatefulHandlerErrorKindV2::Internal,
                )
            })?,
        })
    }
}

fn preview_serve(
    state_path: PathBuf,
    feature_schema_hash: String,
    model_generation: u64,
) -> Result<(), CliError> {
    let _state_lock = StateFileLock::acquire(&state_path)?;
    let handler = Arc::new(PreviewBuiltinHandler::new());
    let config = StatefulRuntimeConfigV3::new(
        rill_runtime_protocol::v3::IdentityV3 {
            name: "rill-runtime".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        model_generation,
        feature_schema_hash,
        handler.metadata.capabilities.clone(),
        br#"{"handlerStateVersion":2,"decisions":0,"feedback":0,"observations":0,"inspections":0,"weights":[],"bias":0.0,"featureCount":0,"actions":{}}"#.to_vec(),
    );
    let engine = StatefulRuntimeEngineV3::new(config, handler)
        .map_err(|error| CliError::Preview(error.to_string()))?;
    if state_path.exists() {
        let file = File::open(&state_path)?;
        let bytes = read_bounded_state_file(file, MAX_STATE_FILE_JSON_BYTES)?;
        let snapshot: StatefulRuntimeSnapshotV3 = serde_json::from_slice(&bytes)
            .map_err(|error| CliError::Preview(format!("invalid state snapshot: {error}")))?;
        for partition in &snapshot.partitions {
            validate_builtin_snapshot_state(&partition.handler_snapshot.state)?;
            if let Some(previous) = &partition.previous_good {
                validate_builtin_snapshot_state(&previous.state)?;
            }
            if let Some(candidate) = &partition.candidate {
                validate_builtin_snapshot_state(&candidate.state)?;
            }
        }
        engine
            .restore_runtime(snapshot)
            .map_err(|error| CliError::Preview(format!("state recovery rejected: {error}")))?;
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());
    let mut line = Vec::new();
    loop {
        line.clear();
        let bytes_read = (&mut input)
            .take((MAX_MESSAGE_BYTES + 2) as u64)
            .read_until(b'\n', &mut line)?;
        if bytes_read == 0 {
            break;
        }
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_MESSAGE_BYTES {
            return Err(CliError::MessageTooLarge);
        }
        let before = engine
            .snapshot_runtime()
            .map_err(|error| CliError::Preview(error.to_string()))?;
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CliError::Preview(error.to_string()))?
            .as_millis();
        let now = u64::try_from(now_millis).map_err(|_| {
            CliError::Preview("system clock exceeds Unix milliseconds range".into())
        })?;
        let response = engine.handle_preview_json_at(&line, now);
        let after = engine
            .snapshot_runtime()
            .map_err(|error| CliError::Preview(error.to_string()))?;
        if after.checksum_sha256 != before.checksum_sha256 {
            write_atomic_snapshot(&state_path, &after)?;
        }
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn write_atomic_snapshot(
    path: &PathBuf,
    snapshot: &StatefulRuntimeSnapshotV3,
) -> Result<(), CliError> {
    let bytes = serde_json::to_vec(snapshot)?;
    if bytes.len() as u64 > MAX_STATE_FILE_JSON_BYTES {
        return Err(CliError::StateSnapshotTooLarge);
    }
    let temp = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn read_bounded_state_file(reader: impl Read, max_bytes: u64) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    reader
        .take(
            max_bytes
                .checked_add(1)
                .ok_or(CliError::StateSnapshotTooLarge)?,
        )
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(CliError::StateSnapshotTooLarge);
    }
    Ok(bytes)
}

fn validate_builtin_snapshot_state(bytes: &[u8]) -> Result<(), CliError> {
    let state: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| CliError::Preview(format!("invalid built-in handler state: {error}")))?;
    validate_builtin_state(&state)
        .map_err(|error| CliError::Preview(format!("built-in handler state rejected: {error}")))
}

struct StateFileLock {
    _file: File,
}

impl StateFileLock {
    fn acquire(state_path: &Path) -> Result<Self, io::Error> {
        let lock_path = state_path.with_extension("lock");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(lock_path)?;
        file.try_lock_exclusive().map_err(|error| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("state path is already owned by another preview runtime: {error}"),
            )
        })?;
        file.set_len(0)?;
        writeln!(file, "pid={}", std::process::id())?;
        file.sync_all()?;
        Ok(Self { _file: file })
    }
}

#[cfg(feature = "wasm")]
fn build_wasm_handler(
    loaded: &rill_runtime::LoadedModelPack,
    handler_pack: &LoadedHandlerPack,
) -> Result<(Arc<dyn InvokeHandler>, HandlerIdentity), CliError> {
    let effective = effective_capabilities(
        &loaded.manifest.capabilities,
        &handler_pack.manifest.capabilities,
    )
    .map_err(|e| CliError::Handler(e.to_string()))?;

    let wasm_handler = rill_runtime::WasmInvokeHandler::new(handler_pack, &loaded.model)
        .map_err(|e| CliError::Handler(e.to_string()))?;

    let identity = HandlerIdentity {
        handler_id: handler_pack.manifest.id.clone(),
        handler_version: handler_pack.manifest.version.clone(),
        handler_api_version: handler_pack.manifest.handler_api_version,
        effective_capabilities: effective,
    };
    Ok((Arc::new(wasm_handler) as Arc<dyn InvokeHandler>, identity))
}

#[cfg(not(feature = "wasm"))]
fn build_wasm_handler(
    _loaded: &rill_runtime::LoadedModelPack,
    _handler_pack: &LoadedHandlerPack,
) -> Result<(Arc<dyn InvokeHandler>, HandlerIdentity), CliError> {
    Err(CliError::Handler(
        "WASM handler support requires the 'wasm' feature (not compiled in)".into(),
    ))
}

fn parse_trust_store(values: &[String]) -> Result<TrustStore, CliError> {
    let mut keys = BTreeMap::new();
    for value in values {
        let (key_id, encoded) = value
            .split_once('=')
            .ok_or_else(|| CliError::TrustKey("expected KEY_ID=HEX".into()))?;
        if key_id.is_empty() || key_id.len() > 96 {
            return Err(CliError::TrustKey("invalid key id".into()));
        }
        let bytes = hex::decode(encoded)
            .map_err(|_| CliError::TrustKey(format!("{key_id} is not valid hexadecimal")))?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| CliError::TrustKey(format!("{key_id} must contain 32 bytes")))?;
        let key = VerifyingKey::from_bytes(&bytes)
            .map_err(|_| CliError::TrustKey(format!("{key_id} is not a valid Ed25519 key")))?;
        if keys.insert(key_id.to_string(), key).is_some() {
            return Err(CliError::TrustKey(format!("duplicate key id {key_id}")));
        }
    }
    Ok(TrustStore(keys))
}

fn serve(engine: RuntimeEngine) -> Result<(), CliError> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());
    let mut line = Vec::new();
    loop {
        line.clear();
        let bytes_read = (&mut input)
            .take((MAX_MESSAGE_BYTES + 2) as u64)
            .read_until(b'\n', &mut line)?;
        if bytes_read == 0 {
            break;
        }
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if line.len() > MAX_MESSAGE_BYTES {
            return Err(CliError::MessageTooLarge);
        }
        if line.is_empty() {
            continue;
        }
        let response = match serde_json::from_slice::<RuntimeRequest>(&line) {
            Ok(request) => {
                let api_version = request.api_version();
                let engine_response = engine.handle(request);
                if api_version >= RUNTIME_API_VERSION {
                    EngineResponseJson::V2(engine_response.to_v2(api_version))
                } else {
                    EngineResponseJson::V1(engine_response.to_v1(api_version))
                }
            }
            Err(_) => EngineResponseJson::V1(RuntimeResponse::Error {
                request_id: String::new(),
                api_version: MIN_RUNTIME_API_VERSION,
                code: error_code::INVALID_JSON.into(),
                message: "request is not valid protocol JSON".into(),
                retryable: false,
            }),
        };
        match response {
            EngineResponseJson::V1(v1) => serde_json::to_writer(&mut output, &v1)?,
            EngineResponseJson::V2(v2) => serde_json::to_writer(&mut output, &v2)?,
        }
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

/// Helper to track which wire version to serialise.
enum EngineResponseJson {
    V1(RuntimeResponse),
    V2(RuntimeResponseV2),
}

#[cfg(test)]
mod tests {
    use super::*;

    struct GrowingReader {
        remaining: usize,
        bytes_read: usize,
    }

    impl Read for GrowingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Ok(0);
            }
            let count = buffer.len().min(self.remaining).min(7);
            buffer[..count].fill(b' ');
            self.remaining -= count;
            self.bytes_read += count;
            Ok(count)
        }
    }

    #[test]
    fn state_file_budget_accepts_boundary_and_rejects_growth_at_plus_one() {
        let exact = vec![b' '; 32];
        assert_eq!(
            read_bounded_state_file(exact.as_slice(), 32).unwrap().len(),
            32
        );

        let mut growing = GrowingReader {
            remaining: 64,
            bytes_read: 0,
        };
        assert!(matches!(
            read_bounded_state_file(&mut growing, 32),
            Err(CliError::StateSnapshotTooLarge)
        ));
        assert_eq!(growing.bytes_read, 33, "reader must stop at budget + 1");
    }

    #[test]
    fn state_file_budget_rejects_sparse_size_before_json_parsing() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("oversized-state.json");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_STATE_FILE_JSON_BYTES + 1).unwrap();
        let before_len = fs::metadata(&path).unwrap().len();
        let result = File::open(&path).and_then(|file| {
            read_bounded_state_file(file, MAX_STATE_FILE_JSON_BYTES)
                .map(|_| ())
                .map_err(|error| io::Error::other(error.to_string()))
        });
        assert!(result.is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), before_len);
    }

    #[test]
    fn state_file_save_budget_preserves_previous_file() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("state.json");
        fs::write(&path, b"previous-valid-snapshot").unwrap();
        let oversized = StatefulRuntimeSnapshotV3 {
            format_version: StatefulRuntimeSnapshotV3::FORMAT_VERSION,
            partitions: vec![rill_runtime::PartitionRuntimeSnapshotV3 {
                client_identity_name: "client".into(),
                partition_key: "partition".into(),
                handler_snapshot: rill_runtime::StatefulStateSnapshotV2 {
                    state_schema_version: 2,
                    state_generation: 1,
                    state: vec![0; MAX_STATE_FILE_JSON_BYTES as usize + 1],
                    checksum_sha256: "0".repeat(64),
                },
                previous_good: None,
                candidate: None,
                pending_decisions: Default::default(),
                completed_decisions: Default::default(),
            }],
            checksum_sha256: "0".repeat(64),
        };

        assert!(matches!(
            write_atomic_snapshot(&path, &oversized),
            Err(CliError::StateSnapshotTooLarge)
        ));
        assert_eq!(fs::read(&path).unwrap(), b"previous-valid-snapshot");
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
    }

    #[test]
    fn trust_store_rejects_duplicate_ids() {
        let key = hex::encode([3u8; 32]);
        let error = parse_trust_store(&[format!("same={key}"), format!("same={key}")]).unwrap_err();
        assert!(error.to_string().contains("duplicate key id"));
    }

    #[test]
    fn trust_store_rejects_short_keys() {
        // Valid hex (16 bytes) but not the required 32 bytes.
        let error =
            parse_trust_store(&["short=00112233445566778899aabbccddeeff".into()]).unwrap_err();
        assert!(error.to_string().contains("must contain 32 bytes"));
    }

    #[test]
    fn preview_handler_accepts_contextual_actions() {
        let handler = PreviewBuiltinHandler::new();
        let state = br#"{"handlerStateVersion":2,"decisions":0,"feedback":0,"observations":0,"inspections":0,"weights":[],"bias":0.0,"featureCount":0,"actions":{}}"#;
        let event = br#"{"method":"decide","context":{"actions":[{"id":"opaque-a","features":[1.0,0.0]},{"id":"opaque-b","features":[0.0,1.0]}]}}"#;
        let result = handler.handle(event, state, Some(11));
        assert!(
            result.is_ok(),
            "handler error: {:?}",
            result.err().map(|e| e.detail().map(str::to_owned))
        );
    }

    fn learner_state(feature_count: usize, weights: &[f64], actions: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "handlerStateVersion": 2,
            "decisions": 0,
            "feedback": 0,
            "observations": 0,
            "inspections": 0,
            "weights": weights,
            "bias": 0.0,
            "featureCount": feature_count,
            "actions": actions,
        }))
        .unwrap()
    }

    #[test]
    fn preview_handler_rejects_multiply_and_accumulation_overflow_atomically() {
        let handler = PreviewBuiltinHandler::new();
        let cases = [
            (
                learner_state(1, &[1000.0], serde_json::json!({})),
                br#"{"method":"decide","context":{"actions":[{"id":"a","features":[1e308]}]}}"#.as_slice(),
                "learner multiplication overflow",
            ),
            (
                learner_state(1, &[1000.0], serde_json::json!({})),
                br#"{"method":"decide","context":{"actions":[{"id":"a","features":[-1e308]}]}}"#.as_slice(),
                "learner multiplication overflow",
            ),
            (
                learner_state(2, &[1000.0, 1000.0], serde_json::json!({})),
                br#"{"method":"decide","context":{"actions":[{"id":"a","features":[1e305,1e305]}]}}"#.as_slice(),
                "learner accumulation overflow",
            ),
        ];
        for (state, event, expected_detail) in cases {
            let original = state.clone();
            let error = handler.handle(event, &state, None).unwrap_err();
            assert_eq!(error.detail(), Some(expected_detail));
            assert_eq!(
                state, original,
                "failed evaluation must not mutate input state"
            );
        }
    }

    #[test]
    fn preview_handler_rejects_feedback_update_overflow_atomically() {
        let handler = PreviewBuiltinHandler::new();
        let state = learner_state(
            1,
            &[0.0],
            serde_json::json!({"a":{"features":[1e308],"samples":0}}),
        );
        let original = state.clone();
        let event = br#"{"method":"feedback","selectedActionId":"a","reward":1e308}"#;
        let error = handler.handle(event, &state, None).unwrap_err();
        assert_eq!(error.detail(), Some("feedback weight update overflow"));
        assert_eq!(state, original);

        let prediction_overflow = learner_state(
            1,
            &[1000.0],
            serde_json::json!({"a":{"features":[1e308],"samples":1}}),
        );
        let prediction_error = handler
            .handle(event, &prediction_overflow, None)
            .unwrap_err();
        assert_eq!(
            prediction_error.detail(),
            Some("learner multiplication overflow")
        );
    }

    #[test]
    fn preview_handler_rejects_inconsistent_or_extreme_version_two_snapshots() {
        let legal_history = learner_state(
            1,
            &[1000.0],
            serde_json::json!({"a":{"features":[1.0],"samples":3,"lastReward":2.0}}),
        );
        validate_builtin_snapshot_state(&legal_history).unwrap();

        let extreme = learner_state(1, &[1e308], serde_json::json!({}));
        assert!(validate_builtin_snapshot_state(&extreme).is_err());
        let wrong_width = learner_state(2, &[0.0], serde_json::json!({}));
        assert!(validate_builtin_snapshot_state(&wrong_width).is_err());
        let bad_counter = br#"{"handlerStateVersion":2,"decisions":"x","feedback":0,"observations":0,"inspections":0,"weights":[],"bias":0.0,"featureCount":0,"actions":{}}"#;
        assert!(validate_builtin_snapshot_state(bad_counter).is_err());
        let bad_action = learner_state(
            1,
            &[0.0],
            serde_json::json!({"a":{"features":[1.0],"samples":"many"}}),
        );
        assert!(validate_builtin_snapshot_state(&bad_action).is_err());
    }
}

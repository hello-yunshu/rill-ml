use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    path::PathBuf,
    sync::Arc,
};

use clap::{Parser, Subcommand};
use ed25519_dalek::VerifyingKey;
#[cfg(feature = "wasm")]
use rill_runtime::handler::effective_capabilities;
use rill_runtime::{
    HandlerIdentity, InvokeHandler, LinearRegressionInvokeHandler, LoadedHandlerPack,
    RuntimeEngine, TrustStore,
    handler_package::HandlerPackError,
    package::{ModelPackError, load_model_pack},
};
use rill_runtime_protocol::{
    MAX_MESSAGE_BYTES, MIN_RUNTIME_API_VERSION, RUNTIME_API_VERSION, RuntimeRequest,
    RuntimeResponse, RuntimeResponseV2,
};
use thiserror::Error;

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
    /// Serve newline-delimited JSON requests over stdin/stdout.
    Serve {
        #[arg(long)]
        pack: PathBuf,
        /// Trusted Ed25519 public key for model packs, as KEY_ID=64_HEX_CHARS.
        /// May be repeated. `--model-trust-key` is the documented name;
        /// `--trust-key` is a deprecated alias.
        #[arg(long = "trust-key", alias = "model-trust-key")]
        trust_keys: Vec<String>,
        /// Trusted Ed25519 public key for handler packs, as KEY_ID=64_HEX_CHARS.
        /// May be repeated.
        #[arg(long = "handler-trust-key")]
        handler_trust_keys: Vec<String>,
        /// Path to a signed `.rillhandler` file. Mutually exclusive with
        /// `--builtin-handler`.
        #[arg(long)]
        handler: Option<PathBuf>,
        /// Select a built-in handler by name. Currently only
        /// `linear-regression` is supported. Mutually exclusive with
        /// `--handler`.
        #[arg(long)]
        builtin_handler: Option<String>,
    },
    /// Verify and print metadata for a signed model package.
    InspectPack {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long = "trust-key", alias = "model-trust-key", required = true)]
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
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("rill-runtime: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Serve {
            pack,
            trust_keys,
            handler_trust_keys,
            handler,
            builtin_handler,
        } => {
            if handler.is_some() && builtin_handler.is_some() {
                return Err(CliError::ConflictingHandlerOption);
            }
            let model_trust = parse_trust_store(&trust_keys)?;
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
                    eprintln!(
                        "rill-runtime: no --handler or --builtin-handler specified; \
                         defaulting to built-in linear-regression (deprecated)"
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
                _ => return Err(CliError::ConflictingHandlerOption),
            };

            let engine = RuntimeEngine::new(loaded)
                .with_invoke_handler(invoke_handler)
                .with_handler_identity(identity);
            serve(engine)
        }
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
                code: "invalidJson".into(),
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
}

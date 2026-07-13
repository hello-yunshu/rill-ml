use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use ed25519_dalek::VerifyingKey;
use rill_runtime::{
    RuntimeEngine, TrustStore,
    package::{ModelPackError, load_model_pack},
};
use rill_runtime_protocol::{
    MAX_MESSAGE_BYTES, RUNTIME_API_VERSION, RuntimeRequest, RuntimeResponse,
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
        /// Trusted Ed25519 public key as KEY_ID=64_HEX_CHARS. May be repeated.
        #[arg(long = "trust-key", required = true)]
        trust_keys: Vec<String>,
    },
    /// Verify and print metadata for a signed model package.
    InspectPack {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long = "trust-key", required = true)]
        trust_keys: Vec<String>,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("model package error: {0}")]
    Pack(#[from] ModelPackError),
    #[error("invalid trusted key: {0}")]
    TrustKey(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IPC message exceeds {MAX_MESSAGE_BYTES} bytes")]
    MessageTooLarge,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("rill-runtime: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Serve { pack, trust_keys } => {
            let trust = parse_trust_store(&trust_keys)?;
            let (loaded, _) = load_model_pack(File::open(pack)?, &trust)?;
            serve(RuntimeEngine::new(loaded))
        }
        Command::InspectPack { pack, trust_keys } => {
            let trust = parse_trust_store(&trust_keys)?;
            let (_, inspection) = load_model_pack(File::open(pack)?, &trust)?;
            println!("{}", serde_json::to_string_pretty(&inspection)?);
            Ok(())
        }
    }
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
            Ok(request) => engine.handle(request),
            Err(_) => RuntimeResponse::Error {
                request_id: String::new(),
                api_version: RUNTIME_API_VERSION,
                code: "invalidJson".into(),
                message: "request is not valid protocol JSON".into(),
                retryable: false,
            },
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
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
}

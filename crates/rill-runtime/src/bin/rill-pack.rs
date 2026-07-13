use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rill_runtime::{
    TrustStore,
    package::{build_signed_model_pack, load_model_pack, sign_release_index, verify_release_index},
};
use rill_runtime_protocol::{
    BatteryModelConfig, ModelPackManifest, ReleaseIndexPayload, SignedReleaseIndex,
};
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(
    name = "rill-pack",
    version,
    about = "Build and verify signed Rill model packages"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a package. Reads the 32-byte signing seed only from RILL_SIGNING_KEY_HEX.
    Create {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify a package using one Ed25519 public key.
    Verify {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long)]
        key_id: String,
        #[arg(long)]
        public_key_hex: String,
    },
    /// Sign a release-index payload with RILL_SIGNING_KEY_HEX.
    SignIndex {
        #[arg(long)]
        payload: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify a signed release index.
    VerifyIndex {
        #[arg(long)]
        index: PathBuf,
        #[arg(long)]
        key_id: String,
        #[arg(long)]
        public_key_hex: String,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("package error: {0}")]
    Package(#[from] rill_runtime::package::ModelPackError),
    #[error("release-index error: {0}")]
    ReleaseIndex(#[from] rill_runtime::package::ReleaseIndexError),
    #[error("RILL_SIGNING_KEY_HEX must contain exactly one 32-byte hexadecimal signing seed")]
    SigningKey,
    #[error("public key must contain exactly 32 hexadecimal bytes")]
    PublicKey,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("rill-pack: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Create {
            manifest,
            model,
            output,
        } => {
            let manifest: ModelPackManifest = serde_json::from_slice(&fs::read(manifest)?)?;
            let model: BatteryModelConfig = serde_json::from_slice(&fs::read(model)?)?;
            let signing_key = signing_key_from_environment()?;
            let bytes = build_signed_model_pack(&manifest, &model, &signing_key)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(output, bytes)?;
            Ok(())
        }
        Command::Verify {
            pack,
            key_id,
            public_key_hex,
        } => {
            let public_key = decode_32(&public_key_hex).ok_or(CliError::PublicKey)?;
            let public_key =
                VerifyingKey::from_bytes(&public_key).map_err(|_| CliError::PublicKey)?;
            let trust = TrustStore([(key_id, public_key)].into_iter().collect());
            let (_, inspection) = load_model_pack(fs::File::open(pack)?, &trust)?;
            println!("{}", serde_json::to_string_pretty(&inspection)?);
            Ok(())
        }
        Command::SignIndex { payload, output } => {
            let payload: ReleaseIndexPayload = serde_json::from_slice(&fs::read(payload)?)?;
            let index = sign_release_index(payload, &signing_key_from_environment()?)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(output, serde_json::to_vec_pretty(&index)?)?;
            Ok(())
        }
        Command::VerifyIndex {
            index,
            key_id,
            public_key_hex,
        } => {
            let public_key = decode_32(&public_key_hex).ok_or(CliError::PublicKey)?;
            let public_key =
                VerifyingKey::from_bytes(&public_key).map_err(|_| CliError::PublicKey)?;
            let trust = TrustStore([(key_id, public_key)].into_iter().collect());
            let index: SignedReleaseIndex = serde_json::from_slice(&fs::read(index)?)?;
            verify_release_index(&index, &trust)?;
            println!("{}", serde_json::to_string_pretty(&index.payload)?);
            Ok(())
        }
    }
}

fn signing_key_from_environment() -> Result<SigningKey, CliError> {
    let encoded = std::env::var("RILL_SIGNING_KEY_HEX").map_err(|_| CliError::SigningKey)?;
    let bytes = decode_32(&encoded).ok_or(CliError::SigningKey)?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn decode_32(encoded: &str) -> Option<[u8; 32]> {
    hex::decode(encoded).ok()?.try_into().ok()
}

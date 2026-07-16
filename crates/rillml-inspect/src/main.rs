//! `rillml-inspect` — CLI for inspecting RillML snapshots and version info.
//!
//! Subcommands:
//! - `version`: print RillML version, snapshot format version, MSRV.
//! - `view-snapshot --path <file>`: print envelope fields of a Snapshot JSON.
//! - `summary --path <file>`: print a model-state summary.
//! - `validate --path <file>`: validate Snapshot format version and structure.

use clap::{Parser, Subcommand};
use rill_ml::persistence::SNAPSHOT_FORMAT_VERSION;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const RILL_ML_VERSION: &str = env!("CARGO_PKG_VERSION");
const MSRV: &str = "1.94";

#[derive(Parser)]
#[command(
    name = "rillml-inspect",
    version,
    about = "Inspect RillML snapshots and version info."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print RillML version, snapshot format version, and MSRV.
    Version,
    /// Read a Snapshot JSON file and print its envelope fields.
    ViewSnapshot {
        #[arg(long)]
        path: PathBuf,
    },
    /// Print a model-state summary from a Snapshot JSON file.
    Summary {
        #[arg(long)]
        path: PathBuf,
    },
    /// Validate a Snapshot JSON file (format version + structure).
    Validate {
        #[arg(long)]
        path: PathBuf,
    },
}

fn read_json(path: &PathBuf) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    serde_json::from_str(&text).map_err(|e| format!("parse JSON: {}", e))
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => {
            println!("rill-ml: {}", RILL_ML_VERSION);
            println!("snapshot_format_version: {}", SNAPSHOT_FORMAT_VERSION);
            println!("msrv: {}", MSRV);
        }
        Command::ViewSnapshot { path } => {
            let v: Value = read_json(&path)?;
            println!("file: {}", path.display());
            println!(
                "format_version: {}",
                v.get("format_version")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0)
            );
            let has_model = v.get("model").is_some();
            println!("has_model: {}", has_model);
            if let Some(obj) = v.as_object() {
                println!(
                    "top_level_keys: {}",
                    obj.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        }
        Command::Summary { path } => {
            let v: Value = read_json(&path)?;
            let model = v.get("model").ok_or("missing `model` field")?;
            println!("file: {}", path.display());
            println!(
                "format_version: {}",
                v.get("format_version")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0)
            );
            if let Some(obj) = model.as_object() {
                println!(
                    "model_keys: {}",
                    obj.keys().cloned().collect::<Vec<_>>().join(", ")
                );
                if let Some(ss) = obj.get("samples_seen").and_then(|x| x.as_u64()) {
                    println!("samples_seen: {}", ss);
                }
                if let Some(w) = obj.get("weights").and_then(|x| x.as_array()) {
                    println!("weights_len: {}", w.len());
                    let minmax = w
                        .iter()
                        .filter_map(|x| x.as_f64())
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), v| {
                            (mn.min(v), mx.max(v))
                        });
                    println!("weights_range: [{}, {}]", minmax.0, minmax.1);
                }
                if let Some(m) = obj.get("mean").and_then(|x| x.as_f64()) {
                    println!("mean: {}", m);
                }
                if let Some(c) = obj.get("count").and_then(|x| x.as_u64()) {
                    println!("count: {}", c);
                }
            }
        }
        Command::Validate { path } => {
            let v: Value = read_json(&path)?;
            let fv = v.get("format_version").and_then(|x| x.as_u64());
            match fv {
                Some(f) if f as u32 == SNAPSHOT_FORMAT_VERSION => {}
                Some(f) => {
                    return Err(format!(
                        "format_version mismatch: expected {}, got {}",
                        SNAPSHOT_FORMAT_VERSION, f
                    ));
                }
                None => return Err("missing `format_version` field".to_string()),
            }
            if v.get("model").is_none() {
                return Err("missing `model` field".to_string());
            }
            println!(
                "OK: {} is a valid RillML snapshot (format_version={})",
                path.display(),
                SNAPSHOT_FORMAT_VERSION
            );
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

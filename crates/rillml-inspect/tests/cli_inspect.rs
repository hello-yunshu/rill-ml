//! Integration tests for the `rillml-inspect` CLI binary.

use std::fs;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rillml-inspect"))
}

#[test]
fn version_outputs_version() {
    let output = bin().arg("version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rill-ml: 0.6.0"));
    assert!(stdout.contains("snapshot_format_version: 1"));
}

#[test]
fn view_snapshot_reads_json() {
    let dir = std::env::temp_dir().join("rillml_inspect_test_snap.json");
    let json = r#"{"format_version":1,"model":{"count":2,"mean":1.5}}"#;
    fs::write(&dir, json).unwrap();
    let output = bin()
        .args(["view-snapshot", "--path", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("format_version: 1"));
    assert!(stdout.contains("has_model: true"));
    fs::remove_file(&dir).ok();
}

#[test]
fn validate_rejects_bad_version() {
    let dir = std::env::temp_dir().join("rillml_inspect_test_bad.json");
    let json = r#"{"format_version":999,"model":{}}"#;
    fs::write(&dir, json).unwrap();
    let output = bin()
        .args(["validate", "--path", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mismatch"));
    fs::remove_file(&dir).ok();
}

#[test]
fn summary_reports_weights() {
    let dir = std::env::temp_dir().join("rillml_inspect_test_lr.json");
    let json =
        r#"{"format_version":1,"model":{"weights":[1.0,2.0],"samples_seen":5,"intercept":0.5}}"#;
    fs::write(&dir, json).unwrap();
    let output = bin()
        .args(["summary", "--path", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("weights_len: 2"));
    assert!(stdout.contains("samples_seen: 5"));
    fs::remove_file(&dir).ok();
}

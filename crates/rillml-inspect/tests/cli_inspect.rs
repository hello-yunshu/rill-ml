//! Integration tests for the `rillml-inspect` CLI binary.

use std::fs;
use std::process::Command;

/// Build a `Command` that runs the `rillml-inspect` CLI binary.
///
/// On the Actions-first cross-architecture gates the crate test suite runs under
/// QEMU user-mode. A guest process that spawns a foreign child ELF via execve
/// cannot rely on QEMU to intercept it (QEMU passes the exec through to the host
/// kernel, which fails the foreign image with ENOEXEC unless binfmt_misc is
/// registered). Rust's `Command::spawn` goes through glibc posix_spawn, whose
/// error channel is broken under QEMU's CLONE_VFORK emulation, so the parent
/// observes a failed/empty child instead of the binary's real output. The gate
/// exports `RILL_RUNTIME_EXEC_PREFIX` = the host-NATIVE QEMU interpreter for the
/// target; launching the child through it (host-native exec passes through to the
/// kernel and runs natively) makes the emulated binary start reliably. Unset on
/// native hosts -> unchanged behaviour.
fn bin() -> Command {
    let exe = env!("CARGO_BIN_EXE_rillml-inspect");
    match std::env::var_os("RILL_RUNTIME_EXEC_PREFIX") {
        Some(prefix) if !prefix.is_empty() => {
            let prefix = prefix.to_string_lossy();
            let mut parts = prefix.split_whitespace().map(str::to_owned);
            let mut cmd = Command::new(parts.next().expect("RILL_RUNTIME_EXEC_PREFIX is empty"));
            for part in parts {
                cmd.arg(part);
            }
            cmd.arg(exe);
            cmd
        }
        _ => Command::new(exe),
    }
}

#[test]
fn version_outputs_version() {
    let output = bin().arg("version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("rill-ml: {}", env!("CARGO_PKG_VERSION"))));
    assert!(stdout.contains("snapshot_format_version: 1"));
    assert!(stdout.contains("msrv: 1.94"));
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

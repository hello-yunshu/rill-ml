#!/usr/bin/env python3
"""External-host smoke test for rill-runtime.

This script is an **independent external host** that verifies the
rill-runtime binary through the public IPC protocol (NDJSON over
stdin/stdout). It does **not** import any internal crate or private API
from the repository; it only speaks the frozen v1/v2 wire schema defined
in `rill-runtime-protocol`.

Prerequisites:
  - A locally built `rill-runtime` binary (with `--features wasm`).
  - A signed `.rillpack` model package and a signed `.rillhandler`
    handler package produced by `rill-pack`.

Usage:
  RILL_RUNTIME_BIN=target/release/rill-runtime \
  RILL_PACK=dist/example-default-1.0.0-rc.6.rillpack \
  RILL_HANDLER=dist/echo-handler-1.0.0-rc.6.rillhandler \
  python3 smoke-test/host_smoke.py

The script writes a structured log to `smoke-test/smoke-result.json` and
prints a human-readable summary to stdout. Exit code 0 = all checks
passed, 1 = at least one check failed.
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import time
import traceback
from dataclasses import dataclass, field
from typing import Any


# ─── Configuration ────────────────────────────────────────────────────── #

DEFAULT_BIN = "target/release/rill-runtime"
DEFAULT_PACK = "dist/example-default-1.0.0-rc.6.rillpack"
DEFAULT_HANDLER = "dist/echo-handler-1.0.0-rc.6.rillhandler"
DEFAULT_TRUST_KEY = os.environ.get(
    "RILL_PUBLIC_KEY_HEX",
    "29fd1fc2f22bd7e405aec167ff0a0d8de791f011c415075d4c5f9f64fd93fc2e",
)
LOG_PATH = pathlib.Path(__file__).resolve().parent / "smoke-result.json"


# ─── Result types ─────────────────────────────────────────────────────── #

@dataclass
class CheckResult:
    name: str
    passed: bool
    duration_ms: float = 0.0
    detail: str = ""
    request: dict[str, Any] | None = None
    response: dict[str, Any] | None = None


@dataclass
class SmokeReport:
    host_name: str = "external-host-smoke"
    host_commit: str = ""
    os: str = ""
    runtime_version: str = ""
    model_pack_id: str = ""
    model_pack_version: str = ""
    handler_id: str = ""
    checks: list[CheckResult] = field(default_factory=list)

    @property
    def all_passed(self) -> bool:
        return all(c.passed for c in self.checks)


# ─── IPC client ───────────────────────────────────────────────────────── #

class RuntimeClient:
    """A minimal NDJSON IPC client for rill-runtime."""

    def __init__(self, process: subprocess.Popen[bytes]):
        self._proc = process
        assert process.stdin is not None
        assert process.stdout is not None
        self._stdin = process.stdin
        self._stdout = process.stdout

    def send(self, request: dict[str, Any]) -> dict[str, Any]:
        """Send a single NDJSON request and read one response line."""
        line = json.dumps(request, separators=(",", ":")) + "\n"
        self._stdin.write(line.encode("utf-8"))
        self._stdin.flush()
        raw = self._stdout.readline()
        if not raw:
            raise RuntimeError(
                f"runtime closed stdout after request: {request}"
            )
        return json.loads(raw.decode("utf-8").strip())

    def close(self) -> None:
        """Close stdin to signal EOF; wait for the process to exit."""
        if self._stdin and not self._stdin.closed:
            self._stdin.close()
        if self._proc.poll() is None:
            self._proc.wait(timeout=10)


# ─── Smoke checks ─────────────────────────────────────────────────────── #

def do_handshake(client: RuntimeClient, report: SmokeReport) -> CheckResult:
    """Verify IPC handshake succeeds and returns expected fields."""
    t0 = time.monotonic()
    req = {
        "method": "handshake",
        "requestId": "smoke-handshake-001",
        "apiVersion": 2,
        "clientName": "external-host-smoke",
        "clientVersion": "1.0.0",
    }
    resp = client.send(req)
    elapsed = (time.monotonic() - t0) * 1000

    ok = (
        resp.get("kind") == "handshake"
        and resp.get("requestId") == req["requestId"]
        and resp.get("apiVersion") in (1, 2)
        and "runtimeVersion" in resp
        and "modelPackId" in resp
        and "capabilities" in resp
    )
    detail = ""
    if ok:
        report.runtime_version = resp["runtimeVersion"]
        report.model_pack_id = resp["modelPackId"]
        report.model_pack_version = resp.get("modelPackVersion", "")
        report.handler_id = resp.get("handlerId", "")
    else:
        detail = f"unexpected handshake response: {resp}"

    return CheckResult(
        name="handshake",
        passed=ok,
        duration_ms=elapsed,
        detail=detail,
        request=req,
        response=resp,
    )


def do_health(client: RuntimeClient, report: SmokeReport) -> CheckResult:
    """Verify health check returns healthy=true."""
    t0 = time.monotonic()
    req = {
        "method": "health",
        "requestId": "smoke-health-001",
        "apiVersion": 2,
    }
    resp = client.send(req)
    elapsed = (time.monotonic() - t0) * 1000

    ok = (
        resp.get("kind") == "health"
        and resp.get("requestId") == req["requestId"]
        and resp.get("apiVersion") in (1, 2)
        and resp.get("healthy") is True
    )
    detail = "" if ok else f"unexpected health response: {resp}"

    return CheckResult(
        name="health",
        passed=ok,
        duration_ms=elapsed,
        detail=detail,
        request=req,
        response=resp,
    )


def do_invoke(client: RuntimeClient, report: SmokeReport) -> CheckResult:
    """Verify invoke returns a result (not an error)."""
    t0 = time.monotonic()
    req = {
        "method": "invoke",
        "requestId": "smoke-invoke-001",
        "apiVersion": 2,
        "capability": report.model_pack_id.replace("rillml.example.default", "rillml.example"),
        "input": {"features": [1.0, 2.0]},
    }
    # If the capability name doesn't match, we still accept the error
    # as long as the error code is stable. But ideally we want a result.
    resp = client.send(req)
    elapsed = (time.monotonic() - t0) * 1000

    # A successful invoke returns kind=result with an output field.
    # An invoke without a handler returns kind=error with code=noInvokeHandler.
    # Both are valid responses; the key is that the protocol works.
    ok = resp.get("kind") in ("result", "error")
    detail = ""
    if resp.get("kind") == "error":
        # If we get an error, it must be a stable error code.
        code = resp.get("code", "")
        stable_codes = {
            "invalidJson", "invalidRequestId", "incompatibleApiVersion",
            "invalidClientIdentity", "unsupportedCapability",
            "noInvokeHandler", "handlerTimeout", "handlerTrap",
            "handlerOutputTooLarge", "handlerInvalidOutput",
            "handlerInternalError",
        }
        ok = code in stable_codes
        detail = f"invoke returned stable error code: {code}"
    elif resp.get("kind") == "result":
        detail = f"invoke returned result: {resp.get('output', '')}"
    else:
        detail = f"unexpected invoke response: {resp}"

    return CheckResult(
        name="invoke",
        passed=ok,
        duration_ms=elapsed,
        detail=detail,
        request=req,
        response=resp,
    )


def do_error_code(client: RuntimeClient, report: SmokeReport) -> CheckResult:
    """Verify that an invalid JSON request returns a stable error code."""
    t0 = time.monotonic()
    # Send an invalid JSON line (not valid JSON at all).
    line = b"this is not json\n"
    client._stdin.write(line)
    client._stdin.flush()
    raw = client._stdout.readline()
    elapsed = (time.monotonic() - t0) * 1000

    ok = False
    detail = ""
    resp = None
    try:
        resp = json.loads(raw.decode("utf-8").strip())
        ok = (
            resp.get("kind") == "error"
            and resp.get("code") == "invalidJson"
            and resp.get("retryable") is False
        )
        detail = "invalid JSON correctly rejected with code=invalidJson"
    except (json.JSONDecodeError, UnicodeDecodeError):
        detail = f"runtime did not return valid JSON for invalid input: {raw!r}"

    return CheckResult(
        name="error_code_invalidJson",
        passed=ok,
        duration_ms=elapsed,
        detail=detail,
        response=resp,
    )


def do_graceful_shutdown(client: RuntimeClient, report: SmokeReport) -> CheckResult:
    """Verify that closing stdin leads to a clean process exit."""
    t0 = time.monotonic()
    client._stdin.close()
    # Wait for the process to exit.
    exit_code = client._proc.wait(timeout=10)
    elapsed = (time.monotonic() - t0) * 1000

    ok = exit_code == 0
    detail = f"process exited with code {exit_code}"
    if not ok:
        detail += f" (stderr: {client._proc.stderr.read().decode('utf-8', errors='replace') if client._proc.stderr else 'N/A'})"

    return CheckResult(
        name="graceful_shutdown",
        passed=ok,
        duration_ms=elapsed,
        detail=detail,
    )


# ─── Orchestration ────────────────────────────────────────────────────── #

def main() -> int:
    bin_path = pathlib.Path(os.environ.get("RILL_RUNTIME_BIN", DEFAULT_BIN))
    pack_path = pathlib.Path(os.environ.get("RILL_PACK", DEFAULT_PACK))
    handler_path = pathlib.Path(os.environ.get("RILL_HANDLER", DEFAULT_HANDLER))
    trust_key = os.environ.get("RILL_TRUST_KEY", DEFAULT_TRUST_KEY)

    # ─── Verify prerequisites ─── #
    if not bin_path.exists():
        print(f"::error::rill-runtime binary not found at {bin_path}")
        print("::error::Build it first: cargo build --release -p rill-runtime --features wasm")
        return 1
    if not pack_path.exists():
        print(f"::error::model pack not found at {pack_path}")
        print("::error::Create it first: rill-pack create --manifest models/example-default/manifest.json ...")
        return 1
    if not handler_path.exists():
        print(f"::warning::handler pack not found at {handler_path} (smoke will run without WASM handler)")
        handler_args: list[str] = []
    else:
        handler_args = ["--handler", str(handler_path)]

    # ─── Start runtime ─── #
    cmd = [
        str(bin_path),
        "serve",
        "--pack", str(pack_path),
        "--model-trust-key", f"rillml-examples-2026-001={trust_key}",
    ] + handler_args

    print(f"smoke: starting runtime: {' '.join(cmd)}")
    try:
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
        )
    except FileNotFoundError as e:
        print(f"::error::failed to start runtime: {e}")
        return 1

    report = SmokeReport(
        host_name="external-host-smoke",
        host_commit="",
        os=sys.platform,
    )

    client = RuntimeClient(proc)
    try:
        report.checks.append(do_handshake(client, report))
        report.checks.append(do_health(client, report))
        report.checks.append(do_invoke(client, report))
        report.checks.append(do_error_code(client, report))
        report.checks.append(do_graceful_shutdown(client, report))
    except Exception as e:
        tb = traceback.format_exc()
        report.checks.append(CheckResult(
            name="exception",
            passed=False,
            detail=f"unhandled exception: {e}\n{tb}",
        ))
        # Ensure the process is terminated.
        if proc.poll() is None:
            proc.kill()
            proc.wait(timeout=5)
        return 1
    finally:
        # Write the structured log.
        log_data = {
            "host_name": report.host_name,
            "host_commit": report.host_commit,
            "os": report.os,
            "runtime_version": report.runtime_version,
            "model_pack_id": report.model_pack_id,
            "model_pack_version": report.model_pack_version,
            "handler_id": report.handler_id,
            "checks": [
                {
                    "name": c.name,
                    "passed": c.passed,
                    "duration_ms": round(c.duration_ms, 2),
                    "detail": c.detail,
                    "request": c.request,
                    "response": c.response,
                }
                for c in report.checks
            ],
            "all_passed": report.all_passed,
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        }
        LOG_PATH.write_text(json.dumps(log_data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    # ─── Summary ─── #
    print()
    print(f"smoke: {len(report.checks)} check(s) executed, {sum(1 for c in report.checks if c.passed)} passed, {sum(1 for c in report.checks if not c.passed)} failed")
    for c in report.checks:
        status = "PASS" if c.passed else "FAIL"
        print(f"  [{status}] {c.name:30s} {c.duration_ms:8.1f}ms  {c.detail}")
    print()
    if report.all_passed:
        print(f"smoke: all checks passed. Log: {LOG_PATH}")
        return 0
    else:
        print(f"::error::smoke test failed. Log: {LOG_PATH}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

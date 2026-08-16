#!/usr/bin/env python3
"""Verify a published Rill PM adapter release as an independent external host.

Downloads the signed release index and the ``rill-pm-adapter`` artifact for the
host platform, verifies the binary's size/SHA-256 against the signed index,
then starts the adapter on a fresh Unix-domain socket and performs a real
``pm-rill-shadow`` v1 round-trip (status / observe / validated outcome) plus
fail-closed negative cases (wrong contract, wrong protocol version, oversized
frame). It imports no Rill crate or repository-private Python module.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import platform
import re
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass, field
from typing import Any


DEFAULT_INDEX_URL = (
    "https://github.com/hello-yunshu/rill-ml/releases/download/"
    "local-ai-candidate/candidate-index.json"
)
DEFAULT_PUBLIC_KEY = (
    "29fd1fc2f22bd7e405aec167ff0a0d8de"
    "791f011c415075d4c5f9f64fd93fc2e"
)
DEFAULT_KEY_ID = "rillml-examples-2026-001"

# pm-rill-shadow v1 constants (independent of Rill Runtime IPC).
CONTRACT = "pm-rill-shadow"
PROTOCOL_VERSION = 1
CAPABILITIES = {
    "context-partitioned-model",
    "goal-partition",
    "validated-outcome",
    "decision-ledger",
    "model-health",
}

# SemVer, allowing prerelease (e.g. 1.2.0-rc.1) and build metadata. The adapter
# and the Rill library report their crate versions, which carry a -rc.N suffix
# on candidate releases, so the strict \d+\.\d+\.\d+ form would fail those.
SEMVER_RE = re.compile(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?")


@dataclass
class CheckResult:
    name: str
    passed: bool
    duration_ms: float = 0.0
    detail: str = ""
    request: dict[str, Any] | None = None
    response: dict[str, Any] | None = None


@dataclass
class ArtifactEvidence:
    kind: str
    name: str
    version: str
    url: str
    sha256: str
    size: int


@dataclass
class SmokeReport:
    host_name: str = "external-adapter-host-smoke"
    host_commit: str = ""
    os: str = ""
    architecture: str = ""
    index_url: str = ""
    index_sha256: str = ""
    index_channel: str = ""
    index_publisher_key_id: str = ""
    expected_version: str = ""
    adapter_version: str = ""
    rill_version: str = ""
    artifacts: list[ArtifactEvidence] = field(default_factory=list)
    commands: list[list[str]] = field(default_factory=list)
    checks: list[CheckResult] = field(default_factory=list)
    timestamp: str = ""

    @property
    def all_passed(self) -> bool:
        return bool(self.checks) and all(check.passed for check in self.checks)


class AdapterClient:
    """Minimal NDJSON client for the frozen pm-rill-shadow v1 contract."""

    def __init__(self, sock: socket.socket):
        self.sock = sock
        self.buffer = b""

    def send(self, request: dict[str, Any]) -> dict[str, Any]:
        line = json.dumps(request, separators=(",", ":")) + "\n"
        self.sock.sendall(line.encode("utf-8"))
        raw = self._readline(timeout=10.0)
        if not raw:
            raise RuntimeError(f"adapter closed connection after request {request}")
        return json.loads(raw.decode("utf-8"))

    def _readline(self, timeout: float) -> bytes:
        self.sock.settimeout(timeout)
        deadline = time.monotonic() + timeout
        while b"\n" not in self.buffer:
            if time.monotonic() > deadline:
                raise RuntimeError("timed out waiting for adapter response")
            chunk = self.sock.recv(65536)
            if not chunk:
                return b""
            self.buffer += chunk
        line, _, self.buffer = self.buffer.partition(b"\n")
        return line


def elapsed_ms(start: float) -> float:
    return (time.monotonic() - start) * 1000


def record_check(
    report: SmokeReport,
    name: str,
    action: Any,
) -> Any:
    start = time.monotonic()
    try:
        detail, value = action()
    except Exception as exc:
        report.checks.append(
            CheckResult(
                name=name,
                passed=False,
                duration_ms=elapsed_ms(start),
                detail=str(exc),
            )
        )
        raise
    report.checks.append(
        CheckResult(
            name=name,
            passed=True,
            duration_ms=elapsed_ms(start),
            detail=detail,
        )
    )
    return value


def validate_https_url(url: str) -> None:
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" or not parsed.hostname:
        raise RuntimeError(f"release URL is not HTTPS: {url!r}")
    if parsed.username or parsed.password or parsed.fragment:
        raise RuntimeError(f"release URL contains forbidden URL components: {url!r}")


def download(
    url: str,
    destination: pathlib.Path,
    timeout: float,
    attempts: int,
) -> None:
    validate_https_url(url)
    for attempt in range(1, attempts + 1):
        request = urllib.request.Request(
            url,
            headers={"User-Agent": "rill-ml-adapter-host-smoke/1.0"},
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                validate_https_url(response.geturl())
                destination.write_bytes(response.read())
            return
        except (urllib.error.URLError, TimeoutError):
            if attempt == attempts:
                raise
            time.sleep(min(2 ** (attempt - 1), 8))


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_artifact_file(item: dict[str, Any], path: pathlib.Path) -> None:
    expected_size = item.get("size")
    expected_hash = item.get("sha256")
    if not isinstance(expected_size, int) or expected_size < 0:
        raise RuntimeError(f"invalid artifact size in index: {item!r}")
    if not isinstance(expected_hash, str) or len(expected_hash) != 64:
        raise RuntimeError(f"invalid artifact SHA-256 in index: {item!r}")
    actual_size = path.stat().st_size
    actual_hash = sha256_file(path)
    if actual_size != expected_size:
        raise RuntimeError(
            f"{path.name}: size mismatch (expected {expected_size}, got {actual_size})"
        )
    if actual_hash.lower() != expected_hash.lower():
        raise RuntimeError(
            f"{path.name}: SHA-256 mismatch (expected {expected_hash}, got {actual_hash})"
        )


def run_checked(report: SmokeReport, command: list[str]) -> subprocess.CompletedProcess[str]:
    report.commands.append(command)
    return subprocess.run(
        command,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


def host_commit() -> str:
    env_sha = os.environ.get("GITHUB_SHA", "")
    if env_sha:
        return env_sha
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return ""


def platform_target() -> tuple[str, str]:
    target_os = {"darwin": "macos", "win32": "windows"}.get(sys.platform, sys.platform)
    machine = platform.machine().lower()
    target_arch = {
        "arm64": "aarch64",
        "amd64": "x86_64",
        "x64": "x86_64",
    }.get(machine, machine)
    return target_os, target_arch


def select_artifact(
    payload: dict[str, Any],
    *,
    kind: str,
    version: str,
    target_os: str | None = None,
    target_arch: str | None = None,
    artifact_id: str | None = None,
) -> dict[str, Any]:
    matches = []
    for item in payload.get("artifacts", []):
        if item.get("kind") != kind or item.get("version") != version:
            continue
        if target_os is not None and item.get("targetOs") != target_os:
            continue
        if target_arch is not None and item.get("targetArch") != target_arch:
            continue
        if artifact_id is not None and item.get("id") != artifact_id:
            continue
        matches.append(item)
    if len(matches) != 1:
        raise RuntimeError(
            f"expected exactly one {kind} artifact for version={version!r}, "
            f"os={target_os!r}, arch={target_arch!r}, id={artifact_id!r}; "
            f"found {len(matches)}"
        )
    return matches[0]


def status(client: AdapterClient, report: SmokeReport, request_id: str) -> dict[str, Any]:
    request = {
        "op": "status",
        "contract": CONTRACT,
        "protocolVersion": PROTOCOL_VERSION,
        "requestId": request_id,
    }
    start = time.monotonic()
    response = client.send(request)
    capabilities = response.get("capabilities")
    passed = (
        response.get("ok") is True
        and response.get("contract") == CONTRACT
        and response.get("protocolVersion") == PROTOCOL_VERSION
        and response.get("requestId") == request_id
        and response.get("state") in ("collecting", "learning", "degraded")
        and isinstance(capabilities, list)
        and CAPABILITIES.issubset(set(capabilities))
        and isinstance(response.get("adapterVersion"), str)
        and SEMVER_RE.fullmatch(response.get("adapterVersion", "")) is not None
        and isinstance(response.get("rillVersion"), str)
        and SEMVER_RE.fullmatch(response.get("rillVersion", "")) is not None
        and isinstance(response.get("modelHealth"), dict)
    )
    report.adapter_version = str(response.get("adapterVersion", ""))
    report.rill_version = str(response.get("rillVersion", ""))
    report.checks.append(
        CheckResult(
            name="status_round_trip",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="contract/protocolVersion/capabilities/health verified"
            if passed
            else "unexpected status response",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"status verification failed: {response}")
    return response


def observe(client: AdapterClient, report: SmokeReport, request_id: str) -> dict[str, Any]:
    request = {
        "op": "observe",
        "contract": CONTRACT,
        "protocolVersion": PROTOCOL_VERSION,
        "requestId": request_id,
        "deviceProfile": "smoke-x86_64-openwrt",
        "capabilityHash": "smoke-caps-001",
        "topologyGeneration": 1,
        "pathId": "wan",
        "routeIdentity": "default",
        "workloadClass": {"class": "interactive", "priority": "high"},
        "measurementClass": "tcp-throughput",
        "contextKey": "wan|interactive|smoke",
        "goal": "latency",
        "integrationFingerprint": "pm-smoke-001",
        "availableActions": [
            {"id": "fastpath-nft"},
            {"id": "squash", "risk": "high"},
        ],
    }
    start = time.monotonic()
    response = client.send(request)
    recommendation = response.get("recommendation")
    passed = (
        response.get("ok") is True
        and response.get("contract") == CONTRACT
        and response.get("protocolVersion") == PROTOCOL_VERSION
        and response.get("requestId") == request_id
        and isinstance(response.get("decisionId"), str)
        and len(response.get("decisionId", "")) == 32
        and isinstance(recommendation, dict)
        and recommendation.get("actionId") in ("fastpath-nft", "squash")
        and recommendation.get("advisory") is True
        and isinstance(recommendation.get("confidence"), (int, float))
    )
    report.checks.append(
        CheckResult(
            name="observe_round_trip",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="stable decisionId + advisory recommendation returned"
            if passed
            else "unexpected observe response",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"observe verification failed: {response}")
    return response


def outcome(
    client: AdapterClient,
    report: SmokeReport,
    request_id: str,
    decision_id: str,
    action_id: str,
) -> None:
    request = {
        "op": "outcome",
        "contract": CONTRACT,
        "protocolVersion": PROTOCOL_VERSION,
        "requestId": request_id,
        "decisionId": decision_id,
        "contextKey": "wan|interactive|smoke",
        "actionId": action_id,
        "sessionId": "smoke-session-001",
        "goal": "latency",
        "modelGeneration": 1,
        "validated": True,
        "reward": 1.0,
    }
    start = time.monotonic()
    response = client.send(request)
    passed = (
        response.get("ok") is True
        and response.get("accepted") is True
        and response.get("contract") == CONTRACT
        and response.get("protocolVersion") == PROTOCOL_VERSION
    )
    report.checks.append(
        CheckResult(
            name="validated_outcome_round_trip",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="validated outcome accepted by the decision ledger"
            if passed
            else "unexpected outcome response",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"outcome verification failed: {response}")


def wrong_contract(client: AdapterClient, report: SmokeReport) -> None:
    request = {
        "op": "status",
        "contract": "rill-runtime-ipc-v2",
        "protocolVersion": PROTOCOL_VERSION,
        "requestId": "smoke-wrong-contract",
    }
    start = time.monotonic()
    response = client.send(request)
    error = response.get("error")
    passed = (
        response.get("ok") is False
        and isinstance(error, dict)
        and error.get("code") == "wrongContract"
        and error.get("retryable") is False
    )
    report.checks.append(
        CheckResult(
            name="wrong_contract_fails_closed",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="wrong contract rejected with fail-closed error"
            if passed
            else "unexpected wrong-contract response",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"wrong-contract verification failed: {response}")


def wrong_protocol_version(client: AdapterClient, report: SmokeReport) -> None:
    request = {
        "op": "status",
        "contract": CONTRACT,
        "protocolVersion": 999,
        "requestId": "smoke-wrong-version",
    }
    start = time.monotonic()
    response = client.send(request)
    error = response.get("error")
    passed = (
        response.get("ok") is False
        and isinstance(error, dict)
        and error.get("code") == "wrongProtocolVersion"
        and error.get("retryable") is False
    )
    report.checks.append(
        CheckResult(
            name="wrong_protocol_version_fails_closed",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="unsupported protocol version rejected with fail-closed error"
            if passed
            else "unexpected wrong-version response",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"wrong-protocol-version verification failed: {response}")


def oversize_frame(work_dir: pathlib.Path, binary: pathlib.Path) -> None:
    """An oversized frame must fail closed: connection closed, nothing parsed."""
    sock_path = work_dir / "oversize.sock"
    state_dir = work_dir / "oversize-state"
    state_dir.mkdir(parents=True, exist_ok=True)
    process = subprocess.Popen(
        [
            str(binary),
            "--socket",
            str(sock_path),
            "--state-dir",
            str(state_dir),
            "--max-message",
            "128",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        deadline = time.monotonic() + 10
        while not sock_path.exists():
            if process.poll() is not None:
                stderr = process.stderr.read().decode("utf-8", errors="replace")
                raise RuntimeError(f"adapter exited early: {stderr!r}")
            if time.monotonic() > deadline:
                raise RuntimeError("adapter did not create socket in time")
            time.sleep(0.05)
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect(str(sock_path))
        try:
            # 4 KB payload on a 128-byte max frame -> TooLarge -> close.
            sock.sendall(b"x" * 4096 + b"\n")
            raw = b""
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                chunk = sock.recv(65536)
                if not chunk:
                    break
                raw += chunk
            if raw:
                raise RuntimeError(f"adapter parsed an oversized frame: {raw!r}")
        finally:
            sock.close()
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def graceful_shutdown(client: AdapterClient, report: SmokeReport, process: subprocess.Popen) -> None:
    """The adapter is a long-running daemon, so a client disconnect (EOF) must
    NOT stop it. Graceful shutdown is driven by SIGTERM (service-manager stop):
    after the client drops the connection the process must still be alive, and
    after SIGTERM it must exit 0 promptly."""
    start = time.monotonic()
    # 1. Client disconnect (EOF): daemon must keep serving, not exit.
    client.sock.close()
    time.sleep(0.2)
    if process.poll() is not None:
        exit_code = process.poll()
        detail = f"adapter exited with code {exit_code} after client EOF (daemon must keep running)"
        report.checks.append(
            CheckResult(
                name="graceful_shutdown_on_eof",
                passed=False,
                duration_ms=elapsed_ms(start),
                detail=detail,
            )
        )
        raise RuntimeError(detail)
    # 2. SIGTERM: daemon must stop cleanly with exit code 0.
    process.send_signal(signal.SIGTERM)
    try:
        exit_code = process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        exit_code = process.wait(timeout=5)
    passed = exit_code == 0
    detail = f"adapter exited with code {exit_code} after SIGTERM"
    report.checks.append(
        CheckResult(
            name="graceful_shutdown_on_eof",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail=detail,
        )
    )
    if not passed:
        raise RuntimeError(detail)


def write_report(report: SmokeReport, log_path: pathlib.Path) -> None:
    report.timestamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    payload = asdict(report)
    payload["all_passed"] = report.all_passed
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--index-url",
        default=os.environ.get("RILL_RELEASE_INDEX_URL", DEFAULT_INDEX_URL),
    )
    parser.add_argument(
        "--expected-version",
        default=os.environ.get("RILL_EXPECTED_VERSION", ""),
        required="RILL_EXPECTED_VERSION" not in os.environ,
    )
    parser.add_argument(
        "--expected-channel",
        choices=("stable", "candidate"),
        default=os.environ.get("RILL_EXPECTED_CHANNEL"),
    )
    parser.add_argument(
        "--publisher-key-id",
        default=os.environ.get("RILL_PUBLISHER_KEY_ID", DEFAULT_KEY_ID),
    )
    parser.add_argument(
        "--public-key-hex",
        default=os.environ.get("RILL_PUBLIC_KEY_HEX", DEFAULT_PUBLIC_KEY),
    )
    parser.add_argument(
        "--adapter-id",
        default="rill-pm-adapter",
        help="Stable adapter artifact id to select from the release index.",
    )
    parser.add_argument(
        "--log",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent / "adapter-smoke-result.json",
    )
    parser.add_argument("--work-dir", type=pathlib.Path)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--download-attempts", type=int, default=4)
    return parser.parse_args()


def run(args: argparse.Namespace, report: SmokeReport, work_dir: pathlib.Path) -> None:
    if args.download_attempts < 1:
        raise RuntimeError("--download-attempts must be at least 1")
    index_path = work_dir / pathlib.PurePosixPath(
        urllib.parse.urlparse(args.index_url).path
    ).name
    record_check(
        report,
        "download_signed_index",
        lambda: (
            f"downloaded {args.index_url}",
            download(
                args.index_url,
                index_path,
                args.timeout,
                args.download_attempts,
            ),
        ),
    )
    report.index_sha256 = sha256_file(index_path)
    envelope = json.loads(index_path.read_text(encoding="utf-8"))
    payload = envelope.get("payload")
    if not isinstance(payload, dict):
        raise RuntimeError("signed index does not contain an object payload")
    report.index_channel = str(payload.get("channel", ""))
    report.index_publisher_key_id = str(payload.get("publisherKeyId", ""))
    if report.index_publisher_key_id != args.publisher_key_id:
        raise RuntimeError("signed index publisher key ID does not match the trusted key")
    if args.expected_channel and report.index_channel != args.expected_channel:
        raise RuntimeError(
            f"index channel {report.index_channel!r} != {args.expected_channel!r}"
        )

    target_os, target_arch = platform_target()
    item = select_artifact(
        payload,
        kind="pm-adapter",
        version=args.expected_version,
        target_os=target_os,
        target_arch=target_arch,
        artifact_id=args.adapter_id,
    )
    url = str(item["url"])
    name = pathlib.PurePosixPath(urllib.parse.urlparse(url).path).name
    if not name:
        raise RuntimeError(f"artifact URL has no filename: {url!r}")
    adapter_path = work_dir / name

    def fetch_and_hash() -> tuple[str, pathlib.Path]:
        download(url, adapter_path, args.timeout, args.download_attempts)
        verify_artifact_file(item, adapter_path)
        return f"{name}: size and SHA-256 match the signed index", adapter_path

    record_check(report, "download_verify_pm_adapter", fetch_and_hash)
    report.artifacts.append(
        ArtifactEvidence(
            kind="pm-adapter",
            name=name,
            version=str(item["version"]),
            url=url,
            sha256=str(item["sha256"]),
            size=int(item["size"]),
        )
    )
    adapter_path.chmod(adapter_path.stat().st_mode | stat.S_IXUSR)

    def check_version() -> tuple[str, None]:
        completed = run_checked(report, [str(adapter_path), "--version"])
        output = completed.stdout.strip()
        if not output.startswith("rill-pm-adapter"):
            raise RuntimeError(f"unexpected --version output: {output!r}")
        return output, None

    record_check(report, "adapter_version_metadata", check_version)

    sock_path = work_dir / "rill.sock"
    state_dir = work_dir / "state"
    state_dir.mkdir(parents=True, exist_ok=True)
    command = [
        str(adapter_path),
        "--socket",
        str(sock_path),
        "--state-dir",
        str(state_dir),
    ]
    report.commands.append(command)
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        deadline = time.monotonic() + 10
        while not sock_path.exists():
            if process.poll() is not None:
                stderr = process.stderr.read().decode("utf-8", errors="replace")
                raise RuntimeError(f"adapter exited early: {stderr!r}")
            if time.monotonic() > deadline:
                raise RuntimeError("adapter did not create socket in time")
            time.sleep(0.05)
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect(str(sock_path))
        client = AdapterClient(sock)
        status(client, report, "smoke-status-001")
        observed = observe(client, report, "smoke-observe-001")
        action_id = str(observed["recommendation"]["actionId"])
        outcome(
            client,
            report,
            "smoke-outcome-001",
            str(observed["decisionId"]),
            action_id,
        )
        wrong_contract(client, report)
        wrong_protocol_version(client, report)
        graceful_shutdown(client, report, process)
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)

    # Oversize frame runs against its own short-lived adapter instance.
    record_check(report, "oversize_frame_fails_closed", lambda: ("closed", oversize_frame(work_dir, adapter_path)))


def main() -> int:
    args = parse_args()
    report = SmokeReport(
        host_commit=host_commit(),
        os=sys.platform,
        architecture=platform.machine(),
        index_url=args.index_url,
        expected_version=args.expected_version,
    )
    try:
        if args.work_dir is not None:
            args.work_dir.mkdir(parents=True, exist_ok=True)
            run(args, report, args.work_dir)
        else:
            with tempfile.TemporaryDirectory(prefix="rill-adapter-smoke-") as temp:
                run(args, report, pathlib.Path(temp))
    except Exception as exc:
        if not report.checks or report.checks[-1].passed:
            report.checks.append(
                CheckResult(
                    name="unhandled_exception",
                    passed=False,
                    detail=f"{exc}\n{traceback.format_exc()}",
                )
            )
    finally:
        write_report(report, args.log)

    passed = sum(check.passed for check in report.checks)
    print(
        f"smoke: {passed}/{len(report.checks)} checks passed; "
        f"log={args.log}"
    )
    for check in report.checks:
        status_text = "PASS" if check.passed else "FAIL"
        print(f"  [{status_text}] {check.name}: {check.detail}")
    return 0 if report.all_passed else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Verify a published Rill Runtime release as an independent external host.

The smoke test downloads a signed release index and its runtime/model/handler
artifacts, verifies them through the public ``rill-pack`` CLI, then exercises
the frozen NDJSON IPC protocol. It imports no Rill crate or repository-private
Python module.

By default the runtime artifact is selected from the host's own OS/CPU
(``platform_target``). To re-verify a *foreign* published asset directly on a
GitHub Actions host under user-mode QEMU (Actions-first: no target-arch
container needed), pass ``--target-os`` / ``--target-arch`` to select the
artifact; the downloaded foreign binary is then run through the registered
QEMU binfmt handler instead of natively.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import platform
import shlex
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
    host_name: str = "external-host-smoke"
    host_commit: str = ""
    os: str = ""
    architecture: str = ""
    index_url: str = ""
    index_sha256: str = ""
    index_channel: str = ""
    index_publisher_key_id: str = ""
    expected_version: str = ""
    runtime_version: str = ""
    model_pack_id: str = ""
    model_pack_version: str = ""
    handler_id: str = ""
    handler_version: str = ""
    artifacts: list[ArtifactEvidence] = field(default_factory=list)
    commands: list[list[str]] = field(default_factory=list)
    checks: list[CheckResult] = field(default_factory=list)
    timestamp: str = ""

    @property
    def all_passed(self) -> bool:
        return bool(self.checks) and all(check.passed for check in self.checks)


class RuntimeClient:
    """Minimal NDJSON client for the frozen public IPC contract."""

    def __init__(self, process: subprocess.Popen[bytes]):
        self.process = process
        assert process.stdin is not None
        assert process.stdout is not None
        self.stdin = process.stdin
        self.stdout = process.stdout

    def send(self, request: dict[str, Any]) -> dict[str, Any]:
        line = json.dumps(request, separators=(",", ":")) + "\n"
        self.stdin.write(line.encode("utf-8"))
        self.stdin.flush()
        raw = self.stdout.readline()
        if not raw:
            stderr = ""
            if self.process.poll() is not None and self.process.stderr is not None:
                stderr = self.process.stderr.read().decode("utf-8", errors="replace")
            raise RuntimeError(
                f"runtime closed stdout after request {request}; stderr={stderr!r}"
            )
        return json.loads(raw.decode("utf-8"))


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
            headers={"User-Agent": "rill-ml-external-host-smoke/1.0"},
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
    # On FreeBSD sys.platform embeds the version (e.g. "freebsd15"), but the
    # release index records the OS as "freebsd" (see build-release-index.py
    # RUNTIMES), so normalize.
    target_os = {
        "darwin": "macos",
        "win32": "windows",
    }.get(sys.platform, sys.platform)
    if sys.platform.startswith("freebsd"):
        target_os = "freebsd"
    machine = platform.machine().lower()
    target_arch = {
        "arm64": "aarch64",
        "amd64": "x86_64",
        "x64": "x86_64",
        # Containers/QEMU report uname -m as armv7l / ppc64le, but the release
        # index records those targets as armv7 / powerpc64le (see
        # build-release-index.py RUNTIMES), so normalize before lookup.
        "armv7l": "armv7",
        "ppc64le": "powerpc64le",
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
            # On Linux the gnu and musl runtime builds of the same OS+arch are
            # distinguished by the stable artifact ``id`` (rill-runtime vs
            # rill-runtime-musl); the caller pins the exact asset it wants.
            continue
        matches.append(item)
    if len(matches) != 1:
        raise RuntimeError(
            f"expected exactly one {kind} artifact for version={version!r}, "
            f"os={target_os!r}, arch={target_arch!r}, id={artifact_id!r}; "
            f"found {len(matches)}"
        )
    return matches[0]


def handshake(client: RuntimeClient, report: SmokeReport, expected_version: str) -> list[str]:
    request = {
        "method": "handshake",
        "requestId": "smoke-handshake-001",
        "apiVersion": 2,
        "clientName": "external-host-smoke",
        "clientVersion": "1.0.0",
    }
    start = time.monotonic()
    response = client.send(request)
    capabilities = response.get("effectiveCapabilities")
    passed = (
        response.get("kind") == "handshake"
        and response.get("requestId") == request["requestId"]
        and response.get("apiVersion") == 2
        and response.get("runtimeVersion") == expected_version
        and response.get("modelPackVersion") == expected_version
        and response.get("handlerVersion") == expected_version
        and isinstance(capabilities, list)
        and bool(capabilities)
    )
    report.runtime_version = str(response.get("runtimeVersion", ""))
    report.model_pack_id = str(response.get("modelPackId", ""))
    report.model_pack_version = str(response.get("modelPackVersion", ""))
    report.handler_id = str(response.get("handlerId", ""))
    report.handler_version = str(response.get("handlerVersion", ""))
    report.checks.append(
        CheckResult(
            name="ipc_handshake",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="v2 identities and effective capabilities verified"
            if passed
            else "unexpected handshake response",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"handshake verification failed: {response}")
    return [str(capability) for capability in capabilities]


def health(client: RuntimeClient, report: SmokeReport) -> None:
    request = {
        "method": "health",
        "requestId": "smoke-health-001",
        "apiVersion": 2,
    }
    start = time.monotonic()
    response = client.send(request)
    passed = (
        response.get("kind") == "health"
        and response.get("requestId") == request["requestId"]
        and response.get("apiVersion") == 2
        and response.get("healthy") is True
        and response.get("modelPackId") == report.model_pack_id
    )
    report.checks.append(
        CheckResult(
            name="ipc_health",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="runtime reported healthy=true" if passed else "unexpected health response",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"health verification failed: {response}")


def invoke(client: RuntimeClient, report: SmokeReport, capability: str) -> None:
    request = {
        "method": "invoke",
        "requestId": "smoke-invoke-001",
        "apiVersion": 2,
        "capability": capability,
        "input": {"features": [1.0, 2.0]},
    }
    start = time.monotonic()
    response = client.send(request)
    passed = (
        response.get("kind") == "result"
        and response.get("requestId") == request["requestId"]
        and response.get("apiVersion") == 2
        and "output" in response
    )
    report.checks.append(
        CheckResult(
            name="ipc_invoke",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="signed WASM handler returned a result"
            if passed
            else "invoke did not return a result",
            request=request,
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"invoke verification failed: {response}")


def invalid_json(client: RuntimeClient, report: SmokeReport) -> None:
    start = time.monotonic()
    client.stdin.write(b"this is not json\n")
    client.stdin.flush()
    raw = client.stdout.readline()
    response = json.loads(raw.decode("utf-8"))
    passed = (
        response.get("kind") == "error"
        and response.get("code") == "invalidJson"
        and response.get("retryable") is False
    )
    report.checks.append(
        CheckResult(
            name="ipc_invalid_json",
            passed=passed,
            duration_ms=elapsed_ms(start),
            detail="stable invalidJson error code verified"
            if passed
            else "unexpected invalid-JSON response",
            response=response,
        )
    )
    if not passed:
        raise RuntimeError(f"invalid-JSON verification failed: {response}")


def graceful_shutdown(client: RuntimeClient, report: SmokeReport) -> None:
    start = time.monotonic()
    client.stdin.close()
    exit_code = client.process.wait(timeout=10)
    passed = exit_code == 0
    detail = f"runtime exited with code {exit_code}"
    if not passed and client.process.stderr is not None:
        detail += "; stderr=" + client.process.stderr.read().decode(
            "utf-8", errors="replace"
        )
    report.checks.append(
        CheckResult(
            name="graceful_shutdown",
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
        "--rill-pack-bin",
        type=pathlib.Path,
        default=pathlib.Path(os.environ.get("RILL_PACK_BIN", "target/release/rill-pack")),
    )
    parser.add_argument(
        "--publisher-key-id",
        default=os.environ.get("RILL_PUBLISHER_KEY_ID", DEFAULT_KEY_ID),
    )
    parser.add_argument(
        "--runtime-id",
        default=os.environ.get("RILL_RUNTIME_ID", "rill-runtime"),
        help="Stable runtime artifact id to select (rill-runtime for GNU, "
        "rill-runtime-musl for musl). Lets a job pin the exact libc variant "
        "of the same OS+arch.",
    )
    parser.add_argument(
        "--target-os",
        default=os.environ.get("RILL_TARGET_OS"),
        help="Override the target OS used to select the runtime artifact "
        "(e.g. 'linux'). Defaults to the running host's OS. Use to re-verify a "
        "foreign published asset under user-mode QEMU.",
    )
    parser.add_argument(
        "--target-arch",
        default=os.environ.get("RILL_TARGET_ARCH"),
        help="Override the target CPU arch used to select the runtime artifact "
        "(e.g. 'riscv64', 'loongarch64'). Defaults to the running host's CPU. "
        "Use to re-verify a foreign published asset under user-mode QEMU.",
    )
    parser.add_argument(
        "--exec-prefix",
        default=os.environ.get("RILL_EXEC_PREFIX"),
        help="Shell tokens prepended to the runtime `serve` command. On a native "
        "host re-verifying a foreign published asset under direct user-mode QEMU, "
        "pass e.g. 'qemu-riscv64-static -L /usr/riscv64-linux-gnu'. Defaults to "
        "executing the downloaded binary directly (native or transparent binfmt).",
    )
    parser.add_argument(
        "--public-key-hex",
        default=os.environ.get("RILL_PUBLIC_KEY_HEX", DEFAULT_PUBLIC_KEY),
    )
    parser.add_argument(
        "--log",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent / "smoke-result.json",
    )
    parser.add_argument("--work-dir", type=pathlib.Path)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--download-attempts", type=int, default=4)
    return parser.parse_args()


def run(args: argparse.Namespace, report: SmokeReport, work_dir: pathlib.Path) -> None:
    if not args.rill_pack_bin.is_file():
        raise RuntimeError(f"rill-pack not found at {args.rill_pack_bin}")
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

    def verify_index() -> tuple[str, None]:
        completed = run_checked(
            report,
            [
                str(args.rill_pack_bin),
                "verify-index",
                "--index",
                str(index_path),
                "--key-id",
                args.publisher_key_id,
                "--public-key-hex",
                args.public_key_hex,
            ],
        )
        return completed.stdout.strip(), None

    record_check(report, "verify_index_signature", verify_index)
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
    # Explicit target overrides let a host process re-verify a foreign published
    # asset under user-mode QEMU (Actions-first) instead of inside a target-arch
    # container. When not provided, fall back to the running host's OS/CPU.
    target_os = args.target_os or target_os
    target_arch = args.target_arch or target_arch
    selected = [
        select_artifact(
            payload,
            kind="runtime",
            version=args.expected_version,
            target_os=target_os,
            target_arch=target_arch,
            artifact_id=args.runtime_id,
        ),
        select_artifact(payload, kind="model", version=args.expected_version),
        select_artifact(payload, kind="handler", version=args.expected_version),
    ]
    paths: dict[str, pathlib.Path] = {}
    for item in selected:
        kind = str(item["kind"])
        url = str(item["url"])
        name = pathlib.PurePosixPath(urllib.parse.urlparse(url).path).name
        if not name:
            raise RuntimeError(f"artifact URL has no filename: {url!r}")
        path = work_dir / name

        def fetch_and_hash() -> tuple[str, pathlib.Path]:
            download(url, path, args.timeout, args.download_attempts)
            verify_artifact_file(item, path)
            return f"{name}: size and SHA-256 match the signed index", path

        paths[kind] = record_check(report, f"download_verify_{kind}", fetch_and_hash)
        report.artifacts.append(
            ArtifactEvidence(
                kind=kind,
                name=name,
                version=str(item["version"]),
                url=url,
                sha256=str(item["sha256"]),
                size=int(item["size"]),
            )
        )

    def verify_model() -> tuple[str, None]:
        completed = run_checked(
            report,
            [
                str(args.rill_pack_bin),
                "verify",
                "--pack",
                str(paths["model"]),
                "--key-id",
                args.publisher_key_id,
                "--public-key-hex",
                args.public_key_hex,
            ],
        )
        return completed.stdout.strip(), None

    def verify_handler() -> tuple[str, None]:
        completed = run_checked(
            report,
            [
                str(args.rill_pack_bin),
                "inspect-handler",
                "--handler",
                str(paths["handler"]),
                "--key-id",
                args.publisher_key_id,
                "--public-key-hex",
                args.public_key_hex,
            ],
        )
        return completed.stdout.strip(), None

    record_check(report, "verify_model_signature", verify_model)
    record_check(report, "verify_handler_signature", verify_handler)

    runtime = paths["runtime"]
    if os.name != "nt":
        runtime.chmod(runtime.stat().st_mode | stat.S_IXUSR)
    command = [
        str(runtime),
        "serve",
        "--pack",
        str(paths["model"]),
        "--model-trust-key",
        f"{args.publisher_key_id}={args.public_key_hex}",
        "--handler",
        str(paths["handler"]),
        "--handler-trust-key",
        f"{args.publisher_key_id}={args.public_key_hex}",
    ]
    # Direct user-mode QEMU (Actions-first): when re-verifying a foreign
    # published asset, prepend an explicit emulator (e.g. `qemu-riscv64-static
    # -L /usr/riscv64-linux-gnu`) so the downloaded binary runs on the native
    # host instead of inside a target-arch container.
    if args.exec_prefix:
        command = shlex.split(args.exec_prefix) + command
    report.commands.append(command)
    process = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )
    client = RuntimeClient(process)
    try:
        capabilities = handshake(client, report, args.expected_version)
        health(client, report)
        invoke(client, report, capabilities[0])
        invalid_json(client, report)
        graceful_shutdown(client, report)
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)


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
            with tempfile.TemporaryDirectory(prefix="rill-host-smoke-") as temp:
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
        status = "PASS" if check.passed else "FAIL"
        print(f"  [{status}] {check.name}: {check.detail}")
    return 0 if report.all_passed else 1


if __name__ == "__main__":
    raise SystemExit(main())

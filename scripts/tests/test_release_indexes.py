from __future__ import annotations

import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from typing import Optional


ROOT = pathlib.Path(__file__).resolve().parents[2]
PUBLISHER = "test-publisher"


class ReleaseIndexHelpersTest(unittest.TestCase):
    def test_macos_runtime_release_is_arm64_only(self) -> None:
        workflow = (ROOT / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
        release_index = (ROOT / "scripts/build-release-index.py").read_text(encoding="utf-8")
        self.assertNotIn("x86_64-apple-darwin", workflow)
        self.assertNotIn('("macos", "x86_64"', release_index)
        self.assertIn("aarch64-apple-darwin", workflow)
        self.assertIn('("macos", "aarch64"', release_index)

    def test_windows_arm64_runtime_is_built_and_indexed(self) -> None:
        # Phase 4: aarch64-pc-windows-msvc must be built on the native
        # Windows ARM64 runner (`windows-11-arm`) and listed in the release
        # index with the stable <os>-<arch> naming contract.
        workflow = (ROOT / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
        release_index = (ROOT / "scripts/build-release-index.py").read_text(encoding="utf-8")
        self.assertIn("aarch64-pc-windows-msvc", workflow)
        self.assertIn("windows-11-arm", workflow)
        self.assertIn('("windows", "aarch64"', release_index)
        self.assertIn("windows-aarch64.exe", release_index)

    def test_riscv64_and_armv7_runtimes_are_built_and_indexed(self) -> None:
        # Phase: promote linux riscv64 and armv7 to Supported. Each must be
        # cross-built by build-runtime-cross and listed in the release index
        # with the stable <os>-<arch> naming contract.
        workflow = (ROOT / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
        release_index = (ROOT / "scripts/build-release-index.py").read_text(encoding="utf-8")
        self.assertIn("riscv64gc-unknown-linux-gnu", workflow)
        self.assertIn("linux-riscv64", release_index)
        self.assertIn("armv7-unknown-linux-gnueabihf", workflow)
        self.assertIn("linux-armv7", release_index)

    def test_s390x_powerpc64le_loongarch64_and_freebsd_runtimes_are_built_and_indexed(self) -> None:
        # Phase: promote linux s390x / powerpc64le / loongarch64 and
        # x86_64-unknown-freebsd to Supported. Each must be cross-built by
        # build-runtime-cross and listed in the release index with the stable
        # <os>-<arch> naming contract. FreeBSD is re-verified in a native VM
        # (post-release-verify-freebsd), not Docker/QEMU.
        workflow = (ROOT / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
        release_index = (ROOT / "scripts/build-release-index.py").read_text(encoding="utf-8")
        self.assertIn("s390x-unknown-linux-gnu", workflow)
        self.assertIn("linux-s390x", release_index)
        self.assertIn("powerpc64le-unknown-linux-gnu", workflow)
        self.assertIn("linux-powerpc64le", release_index)
        self.assertIn("loongarch64-unknown-linux-gnu", workflow)
        self.assertIn("linux-loongarch64", release_index)
        self.assertIn("x86_64-unknown-freebsd", workflow)
        self.assertIn("freebsd-x86_64", release_index)
        self.assertIn("post-release-verify-freebsd", workflow)

    def test_pm_adapter_musl_targets_are_built_and_indexed(self) -> None:
        # CORE BLOCKER A: the rill-pm-adapter must be cross-built by
        # build-pm-adapter-cross for the musl Linux targets PM consumes
        # (x86_64 + aarch64) and listed in the release index as a distinct
        # ``pm-adapter`` artifact kind with the pm-rill-shadow protocol
        # version.
        workflow = (ROOT / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
        release_index = (ROOT / "scripts/build-release-index.py").read_text(encoding="utf-8")
        self.assertIn("build-pm-adapter-cross", workflow)
        self.assertIn("x86_64-unknown-linux-musl", workflow)
        self.assertIn("aarch64-unknown-linux-musl", workflow)
        self.assertIn("rill-pm-adapter-{version}-linux-x86_64-musl", release_index)
        self.assertIn("rill-pm-adapter-{version}-linux-aarch64-musl", release_index)
        self.assertIn('"kind": "pm-adapter"', release_index)
        self.assertIn("pmAdapterProtocolVersion", release_index)
        self.assertIn("dist/rill-pm-adapter-*", workflow)
        self.assertIn("adapter-host-smoke", workflow)

    def test_model_only_release_preserves_runtime_and_rejects_downgrade(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            model = temp / "model.rillpack"
            model.write_bytes(b"signed-model-v0.7.0")
            runtime = {
                "kind": "runtime",
                "id": "rill-runtime",
                "version": "0.5.0",
                "runtimeApiVersion": 2,
                "targetOs": "macos",
                "targetArch": "aarch64",
                "url": "https://example.invalid/runtime",
                "sha256": "00" * 32,
                "size": 1,
            }
            current = {
                "payload": {
                    "schemaVersion": 2,
                    "channel": "stable",
                    "generatedAt": "2026-07-13T00:00:00Z",
                    "publisherKeyId": PUBLISHER,
                    "artifacts": [
                        runtime,
                        {
                            "kind": "model",
                            "id": "rillml.example.default",
                            "version": "0.5.0",
                            "runtimeApiVersion": 2,
                            "url": "https://example.invalid/model-0.5.0",
                            "sha256": "11" * 32,
                            "size": 1,
                        },
                    ],
                },
                "signature": "test fixture; workflow verifies it before this helper runs",
            }
            current_path = temp / "stable-index.json"
            current_path.write_text(json.dumps(current), encoding="utf-8")
            output = temp / "next-payload.json"

            advanced = self.run_model_update(current_path, model, "0.7.0", output)
            self.assertEqual(advanced.returncode, 0, advanced.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(
                [item for item in payload["artifacts"] if item["kind"] == "runtime"],
                [runtime],
            )
            next_model = next(
                item for item in payload["artifacts"] if item["kind"] == "model"
            )
            self.assertEqual(next_model["version"], "0.7.0")
            self.assertEqual(
                next_model["sha256"], hashlib.sha256(model.read_bytes()).hexdigest()
            )

            downgrade = self.run_model_update(
                current_path, model, "0.5.0", temp / "downgrade.json"
            )
            self.assertNotEqual(downgrade.returncode, 0)
            self.assertIn("must increase version", downgrade.stderr)

    def test_runtime_release_preserves_a_newer_model(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            version = "0.7.0"
            for name in (
                f"rill-runtime-{version}-linux-x86_64",
                f"rill-runtime-{version}-macos-aarch64",
                f"rill-runtime-{version}-windows-x86_64.exe",
                f"rill-runtime-{version}-windows-aarch64.exe",
            ):
                (temp / name).write_bytes(name.encode())
            newer_model = {
                "kind": "model",
                "id": "rillml.example.default",
                "version": "0.8.0",
                "runtimeApiVersion": 2,
                "url": "https://example.invalid/model-0.8.0",
                "sha256": "22" * 32,
                "size": 2,
            }
            current = temp / "current.json"
            current.write_text(
                json.dumps(
                    {
                        "payload": {
                            "artifacts": [newer_model],
                            "publisherKeyId": PUBLISHER,
                        },
                        "signature": "verified before helper invocation",
                    }
                ),
                encoding="utf-8",
            )
            output = temp / "payload.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/build-release-index.py"),
                    "--release-dir",
                    str(temp),
                    "--version",
                    version,
                    "--tag",
                    f"runtime-v{version}",
                    "--repository",
                    "example/rill-ml",
                    "--publisher-key-id",
                    PUBLISHER,
                    "--generated-at",
                    "2026-07-13T01:00:00Z",
                    "--existing-index",
                    str(current),
                    "--output",
                    str(output),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            models = [item for item in payload["artifacts"] if item["kind"] == "model"]
            runtimes = [
                item for item in payload["artifacts"] if item["kind"] == "runtime"
            ]
            self.assertEqual(models, [newer_model])
            self.assertEqual(
                {(item["targetOs"], item["targetArch"]) for item in runtimes},
                {
                    ("linux", "x86_64"),
                    ("macos", "aarch64"),
                    ("windows", "x86_64"),
                    ("windows", "aarch64"),
                },
            )
            self.assertTrue(all(item["version"] == version for item in runtimes))

    def test_verify_release_assets_accepts_matching_files_and_rejects_tampering(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            version = "0.7.0"
            runtime = temp / f"rill-runtime-{version}-linux-x86_64"
            model = temp / f"example-default-{version}.rillpack"
            runtime.write_bytes(b"runtime")
            model.write_bytes(b"model")
            artifacts = []
            for path in (runtime, model):
                artifacts.append(
                    {
                        "version": version,
                        "url": f"https://example.invalid/{path.name}",
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                        "size": path.stat().st_size,
                    }
                )
            index = temp / "stable-index.json"
            index.write_text(
                json.dumps({"payload": {"artifacts": artifacts}}), encoding="utf-8"
            )

            valid = self.run_asset_verifier(index, temp, version)
            self.assertEqual(valid.returncode, 0, valid.stderr)

            expected_model_name = f"example-default-{version}.rillpack"
            model.rename(temp / expected_model_name)
            artifacts[-1]["url"] = "https://example.invalid/newer-model.rillpack"
            artifacts[-1]["version"] = "0.8.0"
            index.write_text(
                json.dumps({"payload": {"artifacts": artifacts}}), encoding="utf-8"
            )
            superseded = self.run_asset_verifier(index, temp, version)
            self.assertEqual(superseded.returncode, 0, superseded.stderr)

            runtime.write_bytes(b"tampered")
            tampered = self.run_asset_verifier(index, temp, version)
            self.assertNotEqual(tampered.returncode, 0)
            self.assertIn("differs from the signed immutable asset", tampered.stderr)

    @staticmethod
    def run_model_update(
        current: pathlib.Path,
        model: pathlib.Path,
        version: str,
        output: pathlib.Path,
        *,
        url: Optional[str] = None,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/update-model-release-index.py"),
                "--signed-index",
                str(current),
                "--model",
                str(model),
                "--model-id",
                "rillml.example.default",
                "--version",
                version,
                "--url",
                url if url is not None else f"https://example.invalid/model-{version}",
                "--publisher-key-id",
                PUBLISHER,
                "--generated-at",
                "2026-07-13T01:00:00Z",
                "--output",
                str(output),
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    @staticmethod
    def run_asset_verifier(
        index: pathlib.Path, release_dir: pathlib.Path, version: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/verify-release-assets.py"),
                "--index",
                str(index),
                "--release-dir",
                str(release_dir),
                "--version",
                version,
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_build_release_index_rejects_existing_index_with_forbidden_url(self) -> None:
        # When merging an existing index, URLs from the prior index must be
        # re-validated. A tampered prior index with a forbidden scheme must
        # not survive the merge into the new payload.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            version = "0.7.0"
            for name in (
                f"rill-runtime-{version}-linux-x86_64",
                f"rill-runtime-{version}-macos-aarch64",
                f"rill-runtime-{version}-windows-x86_64.exe",
                f"rill-runtime-{version}-windows-aarch64.exe",
                f"example-default-{version}.rillpack",
            ):
                (temp / name).write_bytes(name.encode())
            # Existing index with a forbidden-URL model that is "newer" so
            # the merge path tries to retain it.
            existing = temp / "existing.json"
            existing.write_text(
                json.dumps(
                    {
                        "payload": {
                            "schemaVersion": 2,
                            "channel": "stable",
                            "generatedAt": "2026-07-13T00:00:00Z",
                            "publisherKeyId": PUBLISHER,
                            "artifacts": [
                                {
                                    "kind": "model",
                                    "id": "rillml.example.default",
                                    "version": "0.9.0",
                                    "runtimeApiVersion": 2,
                                    "url": "file:///etc/passwd",
                                    "sha256": "00" * 32,
                                    "size": 1,
                                }
                            ],
                        },
                        "signature": "verified before helper invocation",
                    }
                ),
                encoding="utf-8",
            )
            output = temp / "payload.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/build-release-index.py"),
                    "--release-dir",
                    str(temp),
                    "--version",
                    version,
                    "--tag",
                    f"v{version}",
                    "--repository",
                    "example/rill-ml",
                    "--publisher-key-id",
                    PUBLISHER,
                    "--generated-at",
                    "2026-07-13T01:00:00Z",
                    "--existing-index",
                    str(existing),
                    "--output",
                    str(output),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0, result.stderr)
            self.assertIn("forbidden URL scheme", result.stderr)

    def test_update_model_release_rejects_non_https_url(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            model = temp / "model.rillpack"
            model.write_bytes(b"signed-model-v0.7.0")
            runtime = {
                "kind": "runtime",
                "id": "rill-runtime",
                "version": "0.5.0",
                "runtimeApiVersion": 2,
                "targetOs": "macos",
                "targetArch": "aarch64",
                "url": "https://example.invalid/runtime",
                "sha256": "00" * 32,
                "size": 1,
            }
            current = temp / "stable-index.json"
            current.write_text(
                json.dumps(
                    {
                        "payload": {
                            "schemaVersion": 2,
                            "channel": "stable",
                            "generatedAt": "2026-07-13T00:00:00Z",
                            "publisherKeyId": PUBLISHER,
                            "artifacts": [runtime],
                        },
                        "signature": "verified before helper invocation",
                    }
                ),
                encoding="utf-8",
            )
            output = temp / "next-payload.json"

            for bad_url, expected_fragment in (
                ("file:///etc/passwd", "forbidden URL scheme"),
                ("data:text/plain,evil", "forbidden URL scheme"),
                ("javascript:alert(1)", "forbidden URL scheme"),
                ("http://example.invalid/model", "https scheme"),
                ("https://user:pass@example.invalid/model", "credentials"),
                ("https:///model", "non-empty host"),
                ("https://example.invalid/model#fragment", "fragment"),
            ):
                result = self.run_model_update(current, model, "0.7.0", output, url=bad_url)
                self.assertNotEqual(
                    result.returncode,
                    0,
                    f"expected URL {bad_url!r} to be rejected; stderr: {result.stderr}",
                )
                self.assertIn(
                    expected_fragment,
                    result.stderr,
                    f"URL {bad_url!r} rejected but error missing {expected_fragment!r}: {result.stderr}",
                )

    def test_update_model_release_rejects_retained_forbidden_url(self) -> None:
        # A previously-signed index with a forbidden URL should not survive
        # re-validation when the next model-only update runs.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            model = temp / "model.rillpack"
            model.write_bytes(b"signed-model-v0.7.0")
            runtime = {
                "kind": "runtime",
                "id": "rill-runtime",
                "version": "0.5.0",
                "runtimeApiVersion": 2,
                "targetOs": "macos",
                "targetArch": "aarch64",
                "url": "file:///etc/passwd",  # forbidden
                "sha256": "00" * 32,
                "size": 1,
            }
            current = temp / "stable-index.json"
            current.write_text(
                json.dumps(
                    {
                        "payload": {
                            "schemaVersion": 2,
                            "channel": "stable",
                            "generatedAt": "2026-07-13T00:00:00Z",
                            "publisherKeyId": PUBLISHER,
                            "artifacts": [runtime],
                        },
                        "signature": "verified before helper invocation",
                    }
                ),
                encoding="utf-8",
            )
            output = temp / "next-payload.json"
            result = self.run_model_update(
                current, model, "0.7.0", output, url="https://example.invalid/model-0.7.0"
            )
            self.assertNotEqual(result.returncode, 0, result.stderr)
            self.assertIn("forbidden URL scheme", result.stderr)

    def test_update_model_release_rejects_localhost_in_production(self) -> None:
        # localhost URLs must be rejected by default so a production
        # release index can never point a downstream client at the local
        # machine. The ``RILL_ALLOW_LOCALHOST_URLS`` opt-in is exercised
        # by ``test_update_model_release_allows_localhost_in_test_mode``.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            model = temp / "model.rillpack"
            model.write_bytes(b"signed-model-v0.7.0")
            runtime = {
                "kind": "runtime",
                "id": "rill-runtime",
                "version": "0.5.0",
                "runtimeApiVersion": 2,
                "targetOs": "macos",
                "targetArch": "aarch64",
                "url": "https://example.invalid/runtime",
                "sha256": "00" * 32,
                "size": 1,
            }
            current = temp / "stable-index.json"
            current.write_text(
                json.dumps(
                    {
                        "payload": {
                            "schemaVersion": 2,
                            "channel": "stable",
                            "generatedAt": "2026-07-13T00:00:00Z",
                            "publisherKeyId": PUBLISHER,
                            "artifacts": [runtime],
                        },
                        "signature": "verified before helper invocation",
                    }
                ),
                encoding="utf-8",
            )
            output = temp / "next-payload.json"
            # Ensure no inherited env var leaks across tests.
            env = {k: v for k, v in os.environ.items() if k != "RILL_ALLOW_LOCALHOST_URLS"}
            for localhost_url in (
                "https://localhost/model-0.7.0",
                "https://127.0.0.1/model-0.7.0",
                "https://[::1]/model-0.7.0",
            ):
                result = subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / "scripts/update-model-release-index.py"),
                        "--signed-index",
                        str(current),
                        "--model",
                        str(model),
                        "--model-id",
                        "rillml.example.default",
                        "--version",
                        "0.7.0",
                        "--url",
                        localhost_url,
                        "--publisher-key-id",
                        PUBLISHER,
                        "--generated-at",
                        "2026-07-13T01:00:00Z",
                        "--output",
                        str(output),
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                    env=env,
                )
                self.assertNotEqual(
                    result.returncode,
                    0,
                    f"expected localhost URL {localhost_url!r} to be rejected; stderr: {result.stderr}",
                )
                self.assertIn("must not point at localhost", result.stderr)

    def test_update_model_release_allows_localhost_in_test_mode(self) -> None:
        # Setting RILL_ALLOW_LOCALHOST_URLS=1 opts in to localhost URLs so
        # tests and local development can point the index at a fixture
        # server. Production pipelines must never set this variable.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            model = temp / "model.rillpack"
            model.write_bytes(b"signed-model-v0.7.0")
            runtime = {
                "kind": "runtime",
                "id": "rill-runtime",
                "version": "0.5.0",
                "runtimeApiVersion": 2,
                "targetOs": "macos",
                "targetArch": "aarch64",
                "url": "https://example.invalid/runtime",
                "sha256": "00" * 32,
                "size": 1,
            }
            current = temp / "stable-index.json"
            current.write_text(
                json.dumps(
                    {
                        "payload": {
                            "schemaVersion": 2,
                            "channel": "stable",
                            "generatedAt": "2026-07-13T00:00:00Z",
                            "publisherKeyId": PUBLISHER,
                            "artifacts": [runtime],
                        },
                        "signature": "verified before helper invocation",
                    }
                ),
                encoding="utf-8",
            )
            output = temp / "next-payload.json"
            env = dict(os.environ)
            env["RILL_ALLOW_LOCALHOST_URLS"] = "1"
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/update-model-release-index.py"),
                    "--signed-index",
                    str(current),
                    "--model",
                    str(model),
                    "--model-id",
                    "rillml.example.default",
                    "--version",
                    "0.7.0",
                    "--url",
                    "https://localhost:8443/model-0.7.0",
                    "--publisher-key-id",
                    PUBLISHER,
                    "--generated-at",
                    "2026-07-13T01:00:00Z",
                    "--output",
                    str(output),
                ],
                capture_output=True,
                text=True,
                check=False,
                env=env,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            models = [item for item in payload["artifacts"] if item["kind"] == "model"]
            self.assertEqual(len(models), 1)
            self.assertEqual(models[0]["url"], "https://localhost:8443/model-0.7.0")

    def test_linux_gnu_and_musl_release_assets_coexist_with_distinct_ids(self) -> None:
        # B1: gnu and musl builds of the same Linux OS+arch must coexist in a
        # single v2 index without colliding. The libc variant is encoded in the
        # stable artifact ``id`` (rill-runtime vs rill-runtime-musl), not in a
        # new schema field, so the frozen v2 wire contract is preserved.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            version = "1.0.0"
            for name in (
                f"rill-runtime-{version}-linux-x86_64",
                f"rill-runtime-{version}-linux-x86_64-musl",
                f"rill-runtime-{version}-linux-aarch64",
                f"rill-runtime-{version}-linux-aarch64-musl",
                f"example-default-{version}.rillpack",
            ):
                (temp / name).write_bytes(name.encode())
            output = temp / "payload.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/build-release-index.py"),
                    "--release-dir", str(temp),
                    "--version", version,
                    "--tag", f"v{version}",
                    "--repository", "example/rill-ml",
                    "--publisher-key-id", PUBLISHER,
                    "--generated-at", "2026-07-13T01:00:00Z",
                    "--output", str(output),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(payload["schemaVersion"], 2)
            runtimes = [a for a in payload["artifacts"] if a["kind"] == "runtime"]
            linux = [a for a in runtimes if a["targetOs"] == "linux"]
            self.assertEqual(len(linux), 4)
            # gnu -> rill-runtime, musl -> rill-runtime-musl, never colliding.
            self.assertEqual(linux[0]["id"], "rill-runtime")
            musl = [a for a in linux if a["id"] == "rill-runtime-musl"]
            gnu = [a for a in linux if a["id"] == "rill-runtime"]
            self.assertEqual(len(musl), 2)
            self.assertEqual(len(gnu), 2)
            self.assertNotIn("targetLibc", linux[0])
            # identities are unique across the whole index
            identities = {
                (a["kind"], a["id"], a.get("targetOs"), a.get("targetArch"))
                for a in payload["artifacts"]
            }
            self.assertEqual(
                len(identities),
                len(payload["artifacts"]),
                "artifact identities must be unique",
            )

    def test_build_release_index_defaults_to_stable_channel(self) -> None:
        # Without --channel, the payload must carry "channel": "stable" so
        # existing release behaviour is unchanged.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            version = "0.7.0"
            for name in (
                f"rill-runtime-{version}-linux-x86_64",
                f"rill-runtime-{version}-macos-aarch64",
                f"rill-runtime-{version}-windows-x86_64.exe",
                f"rill-runtime-{version}-windows-aarch64.exe",
                f"example-default-{version}.rillpack",
            ):
                (temp / name).write_bytes(name.encode())
            output = temp / "payload.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/build-release-index.py"),
                    "--release-dir", str(temp),
                    "--version", version,
                    "--tag", f"v{version}",
                    "--repository", "example/rill-ml",
                    "--publisher-key-id", PUBLISHER,
                    "--generated-at", "2026-07-13T01:00:00Z",
                    "--output", str(output),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(payload["channel"], "stable")

    def test_build_release_index_supports_candidate_channel(self) -> None:
        # With --channel candidate, the payload must carry
        # "channel": "candidate" so downstream clients can distinguish an
        # RC index from a stable index.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            version = "1.0.0-rc.1"
            for name in (
                f"rill-runtime-{version}-linux-x86_64",
                f"rill-runtime-{version}-macos-aarch64",
                f"rill-runtime-{version}-windows-x86_64.exe",
                f"rill-runtime-{version}-windows-aarch64.exe",
                f"example-default-{version}.rillpack",
            ):
                (temp / name).write_bytes(name.encode())
            output = temp / "payload.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/build-release-index.py"),
                    "--release-dir", str(temp),
                    "--version", version,
                    "--tag", f"v{version}",
                    "--repository", "example/rill-ml",
                    "--publisher-key-id", PUBLISHER,
                    "--generated-at", "2026-07-13T01:00:00Z",
                    "--channel", "candidate",
                    "--output", str(output),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(payload["channel"], "candidate")

    def test_build_release_index_rejects_invalid_channel(self) -> None:
        # Only "stable" and "candidate" are valid channels.
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            version = "1.0.0-rc.1"
            for name in (
                f"rill-runtime-{version}-linux-x86_64",
                f"rill-runtime-{version}-macos-aarch64",
                f"rill-runtime-{version}-windows-x86_64.exe",
                f"rill-runtime-{version}-windows-aarch64.exe",
                f"example-default-{version}.rillpack",
            ):
                (temp / name).write_bytes(name.encode())
            output = temp / "payload.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/build-release-index.py"),
                    "--release-dir", str(temp),
                    "--version", version,
                    "--tag", f"v{version}",
                    "--repository", "example/rill-ml",
                    "--publisher-key-id", PUBLISHER,
                    "--generated-at", "2026-07-13T01:00:00Z",
                    "--channel", "beta",
                    "--output", str(output),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)

    def test_update_model_release_preserves_candidate_channel(self) -> None:
        # A model-only update on a candidate index must keep
        # "channel": "candidate" — it must not silently downgrade to
        # "stable".
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            model = temp / "model.rillpack"
            model.write_bytes(b"signed-model-rc")
            runtime = {
                "kind": "runtime",
                "id": "rill-runtime",
                "version": "1.0.0-rc.1",
                "runtimeApiVersion": 2,
                "targetOs": "macos",
                "targetArch": "aarch64",
                "url": "https://example.invalid/runtime",
                "sha256": "00" * 32,
                "size": 1,
            }
            current = temp / "candidate-index.json"
            current.write_text(
                json.dumps(
                    {
                        "payload": {
                            "schemaVersion": 2,
                            "channel": "candidate",
                            "generatedAt": "2026-07-13T00:00:00Z",
                            "publisherKeyId": PUBLISHER,
                            "artifacts": [
                                runtime,
                                {
                                    "kind": "model",
                                    "id": "rillml.example.default",
                                    "version": "1.0.0-rc.1",
                                    "runtimeApiVersion": 2,
                                    "url": "https://example.invalid/model-rc.1",
                                    "sha256": "11" * 32,
                                    "size": 1,
                                },
                            ],
                        },
                        "signature": "verified before helper invocation",
                    }
                ),
                encoding="utf-8",
            )
            output = temp / "next-payload.json"
            result = self.run_model_update(
                current, model, "1.0.0-rc.2", output,
                url="https://example.invalid/model-rc.2",
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(payload["channel"], "candidate")
            next_model = next(
                item for item in payload["artifacts"] if item["kind"] == "model"
            )
            self.assertEqual(next_model["version"], "1.0.0-rc.2")

    def test_pipeline_yml_accepts_prerelease_tags(self) -> None:
        # The version-validation regex in pipeline.yml must accept SemVer
        # prerelease tags like v1.0.0-rc.1, not just stable v1.0.0.
        workflow = (ROOT / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
        # The regex must include the optional prerelease group.
        self.assertIn(
            r"^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$",
            workflow,
            "pipeline.yml must accept SemVer prerelease tags",
        )
        # The --prerelease flag must be conditionally passed to gh release.
        self.assertIn("--prerelease", workflow)
        # The candidate pointer release must be referenced.
        self.assertIn("local-ai-candidate", workflow)
        self.assertIn("candidate-index.json", workflow)

    def test_pipeline_yml_does_not_touch_stable_pointer_for_prerelease(self) -> None:
        # The pipeline must route prerelease versions to local-ai-candidate
        # and stable versions to local-ai-stable — the two channels must
        # never cross.
        workflow = (ROOT / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
        # The IS_PRERELEASE output must drive the pointer selection.
        self.assertIn("IS_PRERELEASE", workflow)
        self.assertIn("POINTER_RELEASE", workflow)
        self.assertIn("pointer_release=local-ai-candidate", workflow)
        self.assertIn("pointer_release=local-ai-stable", workflow)


if __name__ == "__main__":
    unittest.main()

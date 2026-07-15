import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
PUBLISHER = "test-publisher"


class ReleaseIndexHelpersTest(unittest.TestCase):
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
                f"rill-runtime-{version}-macos-x86_64",
                f"rill-runtime-{version}-macos-aarch64",
                f"rill-runtime-{version}-windows-x86_64.exe",
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
            self.assertEqual(len(runtimes), 4)
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
                f"https://example.invalid/model-{version}",
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


if __name__ == "__main__":
    unittest.main()

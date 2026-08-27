from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from platform_support import load_platforms, targets  # noqa: E402


class PlatformSupportTest(unittest.TestCase):
    def test_registry_is_valid_and_full_runtime_uses_default_features(self) -> None:
        entries = load_platforms(ROOT)
        self.assertGreaterEqual(len(entries), 10)
        runtime = targets(ROOT, surface="runtime_supported")
        self.assertIn("riscv64gc-unknown-linux-gnu", runtime)
        self.assertIn("riscv64gc-unknown-linux-musl", runtime)
        self.assertIn("armv7-unknown-linux-musleabihf", runtime)
        self.assertIn("i686-unknown-linux-musl", runtime)
        self.assertNotIn("armv7-unknown-linux-gnueabihf", runtime)
        for entry in entries:
            if entry["runtime_supported"]:
                self.assertEqual(entry["runtime_features"], "default")
            else:
                self.assertFalse(entry["release_asset"])
            if entry["target_libc"] == "musl":
                self.assertIn(entry["runtime_backend"], {"cranelift", "pulley32", "pulley32be"})
                self.assertIn(entry["pointer_width"], {32, 64})
                self.assertIn(entry["endianness"], {"little", "big"})

    def test_consistency_gate_passes(self) -> None:
        result = subprocess.run(
            [sys.executable, str(SCRIPTS / "platform_consistency.py"), "--json"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "PASS")


class ReleaseCompatibilityTest(unittest.TestCase):
    def test_baselines_are_read_from_release_plan(self) -> None:
        authoritative = subprocess.run(
            [sys.executable, str(SCRIPTS / "release_compatibility.py"), "--authoritative"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        historical = subprocess.run(
            [sys.executable, str(SCRIPTS / "release_compatibility.py"), "--historical"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.split()
        self.assertEqual(authoritative, "1.5.3")
        self.assertEqual(historical, ["1.5.1", "1.5.0", "1.3.0", "1.2.0", "1.0.0"])
        self.assertNotIn(authoritative, historical)


class ReleaseIndexSurfaceTest(unittest.TestCase):
    def test_core_only_runtime_asset_is_not_indexed(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            release_dir = pathlib.Path(temp_name)
            version = "1.5.2"
            for name in (
                f"rill-runtime-{version}-linux-x86_64",
                f"rill-runtime-{version}-linux-armv7",
                f"example-default-{version}.rillpack",
            ):
                (release_dir / name).write_bytes(name.encode())
            output = release_dir / "index-payload.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPTS / "build-release-index.py"),
                    "--release-dir",
                    str(release_dir),
                    "--version",
                    version,
                    "--tag",
                    "v1.5.2",
                    "--repository",
                    "example/rill-ml",
                    "--publisher-key-id",
                    "test-key",
                    "--generated-at",
                    "2026-08-25T00:00:00Z",
                    "--output",
                    str(output),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            runtime_urls = {
                item["url"]
                for item in payload["artifacts"]
                if item["kind"] == "runtime"
            }
            self.assertIn("linux-x86_64", next(iter(runtime_urls)))
            self.assertNotIn("linux-armv7", " ".join(runtime_urls))


if __name__ == "__main__":
    unittest.main()

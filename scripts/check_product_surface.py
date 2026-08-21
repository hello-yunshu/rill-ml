#!/usr/bin/env python3
"""Fail-closed documentation/product-surface drift gate.

The gate intentionally compares the user-facing docs with the concrete
manifest, protocol constants, and production CLI source. It is small and
stdlib-only so it can run offline in every CI job and in release admission.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 runners may not ship tomllib.
    tomllib = None


def check(condition: bool, name: str, detail: str) -> dict[str, str]:
    return {"name": name, "status": "PASS" if condition else "FAIL", "detail": detail}


def read(root: Path, relative: str) -> str:
    return (root / relative).read_text(encoding="utf-8")


def manifest_versions(root: Path) -> tuple[str, str, str]:
    cargo_text = read(root, "Cargo.toml")
    plan_text = read(root, "release-plan.toml")
    if tomllib is not None:
        cargo = tomllib.loads(cargo_text)
        plan = tomllib.loads(plan_text)
        return (
            cargo["workspace"]["package"]["version"],
            plan["stable"]["version"],
            plan["preview"]["version"],
        )

    # The fallback intentionally parses only the three scalar fields this
    # gate needs. It avoids a runtime dependency for older offline runners.
    def section_value(text: str, section: str, key: str) -> str:
        match = re.search(
            rf"(?ms)^\[{re.escape(section)}\]\s*(.*?)(?=^\[|\Z)", text
        )
        if match is None:
            raise ValueError(f"missing TOML section [{section}]")
        value = re.search(rf"(?m)^\s*{re.escape(key)}\s*=\s*[\"']([^\"']+)", match.group(1))
        if value is None:
            raise ValueError(f"missing {key} in [{section}]")
        return value.group(1)

    return (
        section_value(cargo_text, "workspace.package", "version"),
        section_value(plan_text, "stable", "version"),
        section_value(plan_text, "preview", "version"),
    )


def run(root: Path) -> dict[str, object]:
    try:
        package_version, stable_version, preview_version = manifest_versions(root)
    except (OSError, ValueError) as error:
        return {"status": "FAIL", "checks": [check(False, "manifest", str(error))]}
    runtime_doc = read(root, "RUNTIME.md")
    readme = read(root, "README.md")
    readme_en = read(root, "README.en.md")
    cli = read(root, "crates/rill-runtime/src/bin/rill-runtime.rs")
    protocol = read(root, "crates/rill-runtime-protocol/src/lib.rs")

    checks = [
        check(
            package_version == stable_version,
            "version.workspace-release-plan",
            f"Cargo workspace={package_version}, release-plan={stable_version}",
        ),
        check(
            f"当前 Stable 组版本为 `{stable_version}`" in readme
            and f"Preview 组为 `{preview_version}`" in readme,
            "docs.zh.version",
            f"README.md advertises Stable {stable_version} / Preview {preview_version}",
        ),
        check(
            f"Stable group is currently at `{stable_version}`" in readme_en
            and f"Preview group remains at `{preview_version}`" in readme_en,
            "docs.en.version",
            f"README.en.md advertises Stable {stable_version} / Preview {preview_version}",
        ),
        check(
            "library-only Preview" in runtime_doc
            and "未暴露为 production CLI/subprocess 入口" in runtime_doc,
            "preview-v3.library-only",
            "RUNTIME.md states that IPC v3 is library-only and not a production CLI entrypoint",
        ),
        check(
            "不指定 `--handler` 且不指定" in runtime_doc
            and "不会隐式" in runtime_doc
            and "--builtin-handler linear-regression` 是保留的、已弃用" in runtime_doc,
            "handler-fallback.docs",
            "RUNTIME.md documents explicit handler selection and fail-closed fallback semantics",
        ),
        check(
            "MissingHandlerOption" in cli
            and "return Err(CliError::MissingHandlerOption)" in cli
            and "--builtin-handler linear-regression is deprecated" in cli,
            "handler-fallback.binary",
            "production CLI rejects an omitted handler and marks the built-in path deprecated",
        ),
        check(
            re.search(r"pub const RUNTIME_API_VERSION:\s*u32\s*=\s*2\s*;", protocol) is not None
            and "pub mod v3;" in protocol,
            "protocol.stable-v2-preview-v3",
            "Stable runtime API remains v2 while the separate v3 module exists",
        ),
        check(
            "V3 当前是 opt-in 的开发接口" not in runtime_doc
            and "默认使用内置线性回归并打印弃用提示" not in runtime_doc,
            "docs.no-stale-surface",
            "known pre-1.3 product-surface drift wording is absent",
        ),
    ]
    status = "PASS" if all(item["status"] == "PASS" for item in checks) else "FAIL"
    return {
        "status": status,
        "stableVersion": stable_version,
        "previewVersion": preview_version,
        "checks": checks,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args()
    result = run(args.root.resolve())
    if args.as_json:
        print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":")))
    else:
        for item in result["checks"]:
            print(f"{item['status']} {item['name']}: {item['detail']}")
        print(f"{result['status']} product-surface")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())

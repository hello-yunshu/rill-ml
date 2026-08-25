"""Load and validate the machine-readable Stable platform evidence registry."""

from __future__ import annotations

from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - supported CI uses Python 3.11+
    try:
        import tomli as tomllib
    except ModuleNotFoundError:
        tomllib = None


REQUIRED_FIELDS = {
    "triple",
    "target_os",
    "target_arch",
    "target_libc",
    "core_supported",
    "runtime_supported",
    "release_asset",
    "runtime_features",
    "source_gate",
    "post_release_gate",
    "asset_suffix",
}


def load_platforms(root: Path) -> list[dict[str, object]]:
    if tomllib is None:
        raise RuntimeError("Python 3.11+ or the tomli package is required to read platform-support.toml")
    path = root / "platform-support.toml"
    document = tomllib.loads(path.read_text(encoding="utf-8"))
    platforms = document.get("target")
    if not isinstance(platforms, list) or not platforms:
        raise ValueError("platform-support.toml must contain at least one [[target]]")
    seen: set[str] = set()
    for entry in platforms:
        if not isinstance(entry, dict):
            raise ValueError("each [[target]] entry must be a table")
        missing = REQUIRED_FIELDS - entry.keys()
        if missing:
            raise ValueError(f"platform entry is missing fields: {sorted(missing)}")
        triple = str(entry["triple"])
        if triple in seen:
            raise ValueError(f"duplicate platform target: {triple}")
        seen.add(triple)
        if not isinstance(entry["core_supported"], bool) or not isinstance(
            entry["runtime_supported"], bool
        ):
            raise ValueError(f"support flags must be booleans for {triple}")
        if entry["runtime_supported"] and not entry["core_supported"]:
            raise ValueError(f"Full Runtime cannot be supported without Core: {triple}")
        if entry["runtime_supported"] and entry["runtime_features"] != "default":
            raise ValueError(f"Full Runtime must use default features: {triple}")
        if entry["runtime_supported"] and not entry["release_asset"]:
            raise ValueError(f"Full Runtime requires a release asset: {triple}")
        if entry["target_os"] == "linux" and entry["target_libc"] not in {"gnu", "musl"}:
            raise ValueError(f"Linux target has invalid libc: {triple}")
        if entry["target_os"] != "linux" and entry["target_libc"] != "none":
            raise ValueError(f"non-Linux target must use target_libc=none: {triple}")
    return [dict(entry) for entry in platforms]


def targets(root: Path, *, surface: str) -> set[str]:
    if surface not in {"core_supported", "runtime_supported"}:
        raise ValueError(f"unknown platform surface: {surface}")
    return {
        str(entry["triple"])
        for entry in load_platforms(root)
        if bool(entry[surface])
    }


def asset_name(root: Path, triple: str, version: str) -> str:
    for entry in load_platforms(root):
        if entry["triple"] == triple:
            return f"rill-runtime-{version}-{entry['asset_suffix']}"
    raise KeyError(triple)

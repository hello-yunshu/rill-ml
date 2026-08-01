"""Dependency-free inspection/export helpers for RillML replay artifacts.

This module is intentionally separate from the production Runtime. It reads
bounded JSON/JSONL, prepares plotting rows, performs Python offline reference
calculations, and exports CSV/JSON without requiring notebooks or NumPy.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path
from typing import Any, Iterable, Sequence

MAX_INPUT_BYTES = 16 * 1024 * 1024
MAX_RECORDS = 1_000_000
MODIFIED_Z_NORMAL_FACTOR = 0.6744897501960817


def load_records(path: str | Path) -> list[dict[str, Any]]:
    source = Path(path)
    if source.stat().st_size > MAX_INPUT_BYTES:
        raise ValueError("replay input exceeds 16 MiB")
    text = source.read_text(encoding="utf-8")
    stripped = text.lstrip()
    if stripped.startswith("["):
        value = json.loads(text, parse_constant=_reject_non_finite)
        if not isinstance(value, list):
            raise ValueError("replay JSON must be an array")
        records = value
    else:
        records = [
            json.loads(line, parse_constant=_reject_non_finite)
            for line in text.splitlines()
            if line.strip()
        ]
    if len(records) > MAX_RECORDS or not all(isinstance(row, dict) for row in records):
        raise ValueError("invalid replay record collection")
    return records


def export_json(records: Sequence[dict[str, Any]], path: str | Path) -> None:
    Path(path).write_text(
        json.dumps(records, ensure_ascii=False, allow_nan=False, indent=2) + "\n",
        encoding="utf-8",
    )


def export_csv(records: Sequence[dict[str, Any]], path: str | Path) -> None:
    fields = [
        "timestamp", "decision_id", "selected_arm", "outcome_time", "reward",
        "generation", "feature_schema_hash", "baseline_reward", "optimal_reward",
        "drift", "context",
    ]
    with Path(path).open("w", encoding="utf-8", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        for record in records:
            row = dict(record)
            row["context"] = json.dumps(record.get("context"), separators=(",", ":"), allow_nan=False)
            writer.writerow(row)


def score_curves(rows: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    """Flatten per-timestamp score arrays into plotting-friendly rows."""
    output: list[dict[str, Any]] = []
    for row in rows:
        timestamp = row["timestamp"]
        for score in row["scores"]:
            output.append({
                "timestamp": timestamp,
                "arm": score["arm"],
                "exploitation": _finite(score["exploitation"]),
                "exploration_bonus": _finite(score["exploration_bonus"]),
                "total_score": _finite(score["total_score"]),
            })
    return output


def drift_timeline(records: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {
            "timestamp": row["timestamp"],
            "drift": bool(row.get("drift", False)),
            "consensus": row.get("drift_consensus"),
        }
        for row in records
        if row.get("drift") or row.get("drift_consensus") is not None
    ]


def feedback_latencies(records: Iterable[dict[str, Any]]) -> list[int]:
    latencies: list[int] = []
    for row in records:
        outcome = row.get("outcome_time")
        if outcome is not None:
            latency = int(outcome) - int(row["timestamp"])
            if latency < 0:
                raise ValueError("outcome precedes decision")
            latencies.append(latency)
    return latencies


def baseline_comparison(records: Iterable[dict[str, Any]]) -> dict[str, float]:
    reward = 0.0
    baseline = 0.0
    for row in records:
        if row.get("reward") is None or row.get("baseline_reward") is None:
            continue
        reward = _finite(reward + _finite(row["reward"]))
        baseline = _finite(baseline + _finite(row["baseline_reward"]))
    return {"reward": reward, "baseline": baseline, "difference": _finite(reward - baseline)}


def quantile_reference(values: Sequence[float], quantiles: Sequence[float]) -> list[float]:
    """Python offline linear-interpolation reference (NumPy default style)."""
    if not values:
        raise ValueError("values must not be empty")
    ordered = sorted(_finite(value) for value in values)
    result: list[float] = []
    for quantile in quantiles:
        q = _finite(quantile)
        if not 0.0 <= q <= 1.0:
            raise ValueError("quantile must be in [0, 1]")
        position = (len(ordered) - 1) * q
        lower = math.floor(position)
        upper = math.ceil(position)
        fraction = position - lower
        result.append(_finite(ordered[lower] * (1.0 - fraction) + ordered[upper] * fraction))
    return result


def median_mad_reference(values: Sequence[float]) -> dict[str, float | int]:
    """Exact offline median/MAD reference for one bounded Rust window."""
    if not values:
        raise ValueError("values must not be empty")
    if len(values) > MAX_RECORDS:
        raise ValueError("values exceed offline reference limit")
    ordered = sorted(_finite(value) for value in values)
    median = _median_of_sorted(ordered)
    deviations = sorted(abs(value - median) for value in ordered)
    mad = _median_of_sorted(deviations)
    if not math.isfinite(mad):
        raise ValueError("median absolute deviation exceeds finite float range")
    return {"samples": len(ordered), "median": median, "mad": mad}


def modified_z_score_reference(values: Sequence[float], observation: float) -> float | None:
    """Return the modified z-score, or ``None`` when MAD is exactly zero."""
    observed = _finite(observation)
    summary = median_mad_reference(values)
    median = float(summary["median"])
    mad = float(summary["mad"])
    if mad == 0.0:
        return None
    delta = observed - median
    if not math.isfinite(delta):
        delta = observed / mad - median / mad
        score = MODIFIED_Z_NORMAL_FACTOR * delta
    else:
        score = MODIFIED_Z_NORMAL_FACTOR * (delta / mad)
    return _finite(score)


def _median_of_sorted(values: Sequence[float]) -> float:
    middle = len(values) // 2
    if len(values) % 2:
        return values[middle]
    lower = values[middle - 1]
    upper = values[middle]
    if math.copysign(1.0, lower) == math.copysign(1.0, upper):
        return _finite(lower + (upper - lower) / 2.0)
    return _finite(lower / 2.0 + upper / 2.0)


def _finite(value: Any) -> float:
    number = float(value)
    if not math.isfinite(number):
        raise ValueError("non-finite numeric value")
    return number


def _reject_non_finite(value: str) -> None:
    raise ValueError(f"non-finite JSON number: {value}")


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Inspect and export RillML decision replays")
    parser.add_argument("input")
    parser.add_argument("--csv")
    parser.add_argument("--json")
    args = parser.parse_args(argv)
    records = load_records(args.input)
    if args.csv:
        export_csv(records, args.csv)
    if args.json:
        export_json(records, args.json)
    print(json.dumps({
        "records": len(records),
        "feedback_latency": feedback_latencies(records),
        "baseline": baseline_comparison(records),
        "drift": drift_timeline(records),
    }, ensure_ascii=False, allow_nan=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

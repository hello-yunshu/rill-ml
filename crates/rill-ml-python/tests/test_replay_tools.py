import importlib.util
import json
from pathlib import Path

import pytest


SPEC = importlib.util.spec_from_file_location(
    "rill_replay",
    Path(__file__).parents[1] / "tools" / "rill_replay.py",
)
rill_replay = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(rill_replay)


def records():
    return [
        {
            "timestamp": 10,
            "decision_id": 1,
            "context": [1.0, 2.0],
            "selected_arm": 0,
            "outcome_time": 15,
            "reward": 0.75,
            "generation": 1,
            "feature_schema_hash": "schema-a",
            "baseline_reward": 0.25,
            "optimal_reward": 1.0,
            "drift": True,
        }
    ]


def test_json_csv_roundtrip_and_latency(tmp_path):
    json_path = tmp_path / "replay.json"
    csv_path = tmp_path / "replay.csv"
    rill_replay.export_json(records(), json_path)
    assert rill_replay.load_records(json_path) == records()
    rill_replay.export_csv(records(), csv_path)
    text = csv_path.read_text(encoding="utf-8")
    assert "selected_arm" in text
    assert rill_replay.feedback_latencies(records()) == [5]


def test_score_drift_baseline_and_quantile_reference():
    curves = rill_replay.score_curves([{
        "timestamp": 10,
        "scores": [{"arm": 0, "exploitation": 0.2, "exploration_bonus": 0.3, "total_score": 0.5}],
    }])
    assert curves[0]["total_score"] == 0.5
    assert rill_replay.drift_timeline(records())[0]["drift"] is True
    assert rill_replay.baseline_comparison(records())["difference"] == 0.5
    assert rill_replay.quantile_reference([1.0, 2.0, 3.0], [0.5]) == [2.0]


def test_median_mad_and_modified_z_reference():
    values = [1.0, 2.0, 100.0, 4.0, 5.0]
    assert rill_replay.median_mad_reference(values) == {
        "samples": 5,
        "median": 4.0,
        "mad": 2.0,
    }
    expected = rill_replay.MODIFIED_Z_NORMAL_FACTOR * 3.0
    assert rill_replay.modified_z_score_reference(values, 10.0) == expected
    assert rill_replay.modified_z_score_reference([7.0, 7.0, 7.0], 9.0) is None


def test_median_mad_reference_handles_minority_extreme_contamination():
    clean = [[-1.0, 0.0, 1.0][index % 3] for index in range(52)]
    summary = rill_replay.median_mad_reference(clean + [float.fromhex("0x1.fffffffffffffp+1023")] * 49)
    assert summary["median"] == 1.0
    assert summary["mad"] == 2.0


def test_invalid_inputs_are_rejected(tmp_path):
    path = tmp_path / "bad.json"
    path.write_text('[{"reward": NaN}]', encoding="utf-8")
    with pytest.raises(ValueError):
        rill_replay.load_records(path)
    with pytest.raises(ValueError):
        rill_replay.feedback_latencies([{"timestamp": 10, "outcome_time": 9}])
    with pytest.raises(ValueError):
        rill_replay.quantile_reference([], [0.5])
    with pytest.raises(ValueError):
        rill_replay.median_mad_reference([])
    with pytest.raises(ValueError):
        rill_replay.modified_z_score_reference([1.0], float("inf"))

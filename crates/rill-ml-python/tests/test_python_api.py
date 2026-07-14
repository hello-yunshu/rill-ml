"""Functional tests for the `rill_ml` Python bindings.

Run via `pytest tests/ -v` after `maturin develop --release`.
"""

import pytest


def test_mean():
    from rill_ml import Mean

    m = Mean()
    m.update(1.0)
    m.update(2.0)
    m.update(3.0)
    assert abs(m.value - 2.0) < 1e-12
    assert m.count == 3
    js = m.to_json()
    m2 = Mean.from_json(js)
    assert abs(m2.value - 2.0) < 1e-12


def test_variance():
    from rill_ml import Variance

    v = Variance("population")
    assert v.value is None
    for x in [1.0, 2.0, 3.0, 4.0, 5.0]:
        v.update(x)
    assert abs(v.value - 2.0) < 1e-12
    assert abs(v.stddev - 2.0 ** 0.5) < 1e-12
    assert abs(v.mean - 3.0) < 1e-12
    assert v.count == 5

    vs = Variance("sample")
    vs.update(5.0)
    assert vs.value is None  # sample variance needs >=2 samples

    js = v.to_json()
    v2 = Variance.from_json(js)
    assert abs(v2.value - 2.0) < 1e-12
    assert v2.count == 5


def test_ewmean():
    from rill_ml import EWMean

    ew = EWMean(0.5)
    ew.update(10.0)
    ew.update(20.0)
    assert abs(ew.value - 15.0) < 1e-9

    js = ew.to_json()
    ew2 = EWMean.from_json(js)
    assert abs(ew2.value - 15.0) < 1e-9


def test_standard_scaler():
    from rill_ml import StandardScaler

    sc = StandardScaler(2)
    sc.learn_one([1.0, 2.0])
    sc.learn_one([3.0, 4.0])
    out = sc.transform_one([2.0, 3.0])
    assert len(out) == 2
    assert sc.samples_seen == 2

    js = sc.to_json()
    sc2 = StandardScaler.from_json(js)
    assert sc2.samples_seen == 2
    out2 = sc2.transform_one([2.0, 3.0])
    assert len(out2) == 2


def test_linear_regression():
    from rill_ml import LinearRegression

    lr = LinearRegression(2, 0.1)
    for _ in range(50):
        lr.learn_one([1.0, 2.0], 5.0)
    pred = lr.predict_one([1.0, 2.0])
    assert abs(pred - 5.0) < 0.5
    assert len(lr.weights) == 2
    assert lr.samples_seen == 50
    js = lr.to_json()
    assert "format_version" in js
    lr2 = LinearRegression.from_json(js)
    assert lr2.samples_seen == 50
    pred2 = lr2.predict_one([1.0, 2.0])
    assert abs(pred2 - pred) < 1e-9


def test_logistic_regression():
    from rill_ml import LogisticRegression

    logr = LogisticRegression(2, 0.1)
    logr.learn_one([1.0, 2.0], True)
    logr.learn_one([-1.0, -2.0], False)
    pred = logr.predict_one([1.0, 2.0])
    assert isinstance(pred, bool)
    proba = logr.predict_proba_one([1.0, 2.0])
    assert 0.0 <= proba <= 1.0
    assert len(logr.weights) == 2
    js = logr.to_json()
    logr2 = LogisticRegression.from_json(js)
    assert logr2.samples_seen == 2
    proba2 = logr2.predict_proba_one([1.0, 2.0])
    assert abs(proba2 - proba) < 1e-9


def test_regression_pipeline():
    from rill_ml import RegressionPipeline

    pipe = RegressionPipeline(2, 0.05)
    pipe.learn_one([0.1, 0.2], 0.5)
    pred = pipe.predict_one([0.1, 0.2])
    assert isinstance(pred, float)
    assert pipe.samples_seen == 1
    js = pipe.to_json()
    assert "format_version" in js
    pipe2 = RegressionPipeline.from_json(js)
    assert pipe2.samples_seen == 1
    pred2 = pipe2.predict_one([0.1, 0.2])
    assert abs(pred2 - pred) < 1e-9


def test_classification_pipeline():
    from rill_ml import ClassificationPipeline

    cpipe = ClassificationPipeline(2, 0.05)
    cpipe.learn_one([0.1, 0.2], True)
    cpipe.learn_one([-0.1, -0.2], False)
    pred = cpipe.predict_one([0.1, 0.2])
    assert isinstance(pred, bool)
    proba = cpipe.predict_proba_one([0.1, 0.2])
    assert 0.0 <= proba <= 1.0
    assert cpipe.samples_seen == 2
    js = cpipe.to_json()
    cpipe2 = ClassificationPipeline.from_json(js)
    assert cpipe2.samples_seen == 2
    proba2 = cpipe2.predict_proba_one([0.1, 0.2])
    assert abs(proba2 - proba) < 1e-9


def test_snapshot_namespace():
    from rill_ml import Mean, Snapshot

    assert Snapshot.format_version() == 1
    m = Mean()
    m.update(1.0)
    js = Snapshot.to_json(m)
    assert "format_version" in js

// Node.js smoke test for the rill-ml-wasm Node build.
//
// Imports the wasm-pack `--target nodejs` build (pkg-node/) and exercises the
// public API surface end-to-end: learn/update/predict, snapshot
// serialization + restore, invalid-input rejection, and the 64 MiB snapshot
// size limit.
//
// Run from the crate directory after building:
//   wasm-pack build --target nodejs --out-dir pkg-node
//   node tests/node-smoke.mjs

import { strict as assert } from "node:assert";

import {
  WasmMean,
  WasmVariance,
  WasmLinearRegression,
  WasmLogisticRegression,
  _rill_ml_wasm_version,
} from "../pkg-node/rill_ml_wasm.js";

const MAX_SNAPSHOT_JSON_BYTES = 64 * 1024 * 1024;

let passed = 0;
function ok(label) {
  passed += 1;
  console.log(`ok ${passed} - ${label}`);
}

// Helper: robust error matching for values thrown by wasm-bindgen (which may
// be a plain string rather than an Error instance).
function throwsMatching(fn, re) {
  assert.throws(fn, (err) => {
    const text = String(err && err.message !== undefined ? err.message : err);
    return re.test(text);
  });
}

// 1. Library version matches the crate version.
assert.strictEqual(_rill_ml_wasm_version(), "0.15.0");
ok("_rill_ml_wasm_version() returns the crate version");

// 2. Online mean: update/value/count + snapshot roundtrip.
const mean = new WasmMean();
mean.update(1.0);
mean.update(2.0);
mean.update(3.0);
assert.ok(Math.abs(mean.value() - 2.0) < 1e-12, "mean value");
assert.strictEqual(mean.count(), 3n, "mean count");
const meanRestored = WasmMean.from_json(mean.to_json());
assert.ok(Math.abs(meanRestored.value() - 2.0) < 1e-9, "restored mean value");
assert.strictEqual(meanRestored.count(), 3n, "restored mean count");
ok("WasmMean update/value/count + to_json/from_json roundtrip");

// 3. Variance accumulator.
const variance = new WasmVariance("sample");
for (const x of [1.0, 2.0, 3.0, 4.0, 5.0]) {
  variance.update(x);
}
assert.ok(Math.abs(variance.mean() - 3.0) < 1e-12, "variance mean");
const varianceRestored = WasmVariance.from_json(variance.to_json());
assert.ok(
  Math.abs(varianceRestored.value() - variance.value()) < 1e-12,
  "restored variance value"
);
ok("WasmVariance update/mean + roundtrip");

// 4. Linear regression learns a constant target and predicts.
const lr = new WasmLinearRegression(1, 0.05);
const x = new Float64Array([2.0]);
for (let i = 0; i < 200; i += 1) {
  lr.learn(x, 10.0);
}
assert.ok(Math.abs(lr.predict(x) - 10.0) < 1.0, "linear prediction");
assert.strictEqual(lr.samples_seen(), 200n, "linear samples_seen");
const lrRestored = WasmLinearRegression.from_json(lr.to_json());
assert.ok(
  Math.abs(lrRestored.predict(x) - lr.predict(x)) < 1e-9,
  "restored linear prediction"
);
ok("WasmLinearRegression learn/predict + roundtrip");

// 5. Logistic regression probability is bounded.
const logr = new WasmLogisticRegression(2, 0.1);
logr.learn(new Float64Array([1.0, 2.0]), true);
logr.learn(new Float64Array([-1.0, -2.0]), false);
const proba = logr.predict_proba(new Float64Array([1.0, 2.0]));
assert.ok(proba >= 0.0 && proba <= 1.0, "probability in [0, 1]");
ok("WasmLogisticRegression learn/predict_proba");

// 6. Invalid input is rejected.
throwsMatching(() => WasmMean.from_json("not json"), /invalid/i);
throwsMatching(() => new WasmVariance("bogus"), /unknown variance kind/);
ok("from_json / constructor reject invalid input");

// 7. Snapshot size limit: input larger than 64 MiB is rejected.
const oversized = "x".repeat(MAX_SNAPSHOT_JSON_BYTES + 1);
throwsMatching(
  () => WasmMean.from_json(oversized),
  /exceeds the maximum byte limit/
);
ok("from_json rejects input above the 64 MiB snapshot size limit");

console.log(`\nAll ${passed} node smoke checks passed.`);

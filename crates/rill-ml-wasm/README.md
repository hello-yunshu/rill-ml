# rill-ml-wasm

WebAssembly bindings for [RillML](https://github.com/hello-yunshu/rill-ml) —
online machine learning that runs directly in the browser or in Node.js.
The crate builds a single WASM core and exposes it through two wasm-pack
targets, so there is one algorithm implementation shared by both runtimes.

## Install

```sh
npm install rill-ml-wasm
```

## Usage

### Browser (ESM, bundler)

The browser build is ESM and must be initialized before first use.

```js
import init, { WasmMean, WasmLinearRegression } from "rill-ml-wasm";

await init();

const mean = new WasmMean();
mean.update(1.0);
mean.update(2.0);
console.log(mean.value()); // 1.5
```

### Node.js (CommonJS)

The Node build auto-initializes on load.

```js
const { WasmLinearRegression } = require("rill-ml-wasm");

const lr = new WasmLinearRegression(1, 0.05);
const x = new Float64Array([2.0]);
for (let i = 0; i < 200; i++) lr.learn(x, 10.0);
console.log(lr.predict(x)); // ~10.0
```

Node ESM (`import` in an `.mjs` module) is also supported; the node build is
selected automatically by the package `exports` map.

## API

All methods that can fail **throw** on invalid input instead of returning
error values.

| Class | Purpose |
| --- | --- |
| `WasmMean` | Online mean accumulator |
| `WasmVariance` | Online variance / standard deviation (Welford) |
| `WasmEWMean` | Exponentially weighted mean |
| `WasmStandardScaler` | Online feature standardization |
| `WasmLinearRegression` | Online linear regression (SGD) |
| `WasmLogisticRegression` | Online logistic regression (SGD) |
| `WasmRegressionPipeline` | Scaler + linear regression pipeline |
| `WasmClassificationPipeline` | Scaler + logistic regression pipeline |
| `WasmSnapshot` | Snapshot format feature-detect marker (`format_version()`) |

Common methods:

- `update(x)` / `learn(x, y)` — feed a sample (throws on invalid input)
- `predict(x)` / `predict_proba(x)` / `transform(x)` — score a feature vector
- `value()` / `mean()` / `std_dev()` / `count()` / `samples_seen()` / `weights()`
- `to_json()` — serialize state to a snapshot JSON string
- `static from_json(json)` — restore state from a snapshot JSON string
- `_rill_ml_wasm_version()` — the library version

Feature vectors are passed as `Float64Array`; `count()` and `samples_seen()`
return `bigint`.

## Snapshot size limit

`from_json` rejects any JSON string larger than **64 MiB**
(`MAX_SNAPSHOT_JSON_BYTES`). The limit is enforced on the raw byte length
before parsing, so untrusted input cannot drive unbounded allocations. Invalid
JSON, an incompatible snapshot format version, or invalid model state are also
rejected with a thrown error.

## Building from source

```sh
# Native build (host) — wasm-pack web + node builds and wasm test suite
./scripts/wasm-pack-build.sh

# Docker-first build (canonical gate; requires Docker)
./scripts/docker-wasm-build.sh

# After building, run the Node smoke test from the crate directory
node tests/node-smoke.mjs
```

## License

MIT

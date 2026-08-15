/**
 * RillML WebAssembly bindings — TypeScript declarations.
 *
 * This file is the stable TypeScript surface of the `rill-ml-wasm` npm
 * package. The runtime implementations live in the wasm-pack builds:
 *   - `pkg/`      browser ESM build (requires `init()` before first use)
 *   - `pkg-node/` Node.js CJS build (auto-initializes on import/require)
 *
 * Ownership and error semantics:
 *   - All methods that can fail (update, learn, predict, predict_proba,
 *     transform, to_json, from_json and the constructors) THROW on invalid
 *     input rather than returning error values.
 *   - `from_json(json)` validates the input before deserializing: it throws
 *     if the JSON is malformed, if the snapshot format version is
 *     incompatible, or if the model state fails validation.
 *   - Snapshot size limit: `from_json` rejects any JSON string larger than
 *     64 MiB (`MAX_SNAPSHOT_JSON_BYTES`), enforced on the raw byte length
 *     before parsing so untrusted input cannot drive unbounded allocations.
 *   - `to_json()` serializes the model state; the returned string can be fed
 *     back into the matching `from_json`.
 *   - Counters (`count`, `samples_seen`) are returned as JavaScript `bigint`
 *     because the underlying Rust values are 64-bit unsigned integers.
 */

/**
 * Library version string (matches the `rill-ml-wasm` crate version).
 */
export function _rill_ml_wasm_version(): string;

/**
 * Online mean accumulator.
 */
export class WasmMean {
    /** Constructs a fresh accumulator. */
    constructor();
    /** Adds a sample. Throws on invalid input (e.g. non-finite values). */
    update(x: number): void;
    /** Current mean. */
    value(): number;
    /** Number of samples seen, as a `bigint`. */
    count(): bigint;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmMean;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Online variance accumulator (Welford). `value`/`std_dev` return
 * `undefined` until enough samples have been observed.
 */
export class WasmVariance {
    /**
     * Constructs a variance accumulator for `kind`: `"population"` or
     * `"sample"`. Throws for any other kind.
     */
    constructor(kind: string);
    /** Adds a sample. Throws on invalid input (e.g. non-finite values). */
    update(x: number): void;
    /** Variance estimate, or `undefined` before enough samples are seen. */
    value(): number | undefined;
    /** Standard deviation, or `undefined` before enough samples are seen. */
    std_dev(): number | undefined;
    /** Arithmetic mean of the samples. */
    mean(): number;
    /** Number of samples seen, as a `bigint`. */
    count(): bigint;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmVariance;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Exponentially weighted mean.
 */
export class WasmEWMean {
    /**
     * Constructs an exponentially weighted mean with smoothing factor
     * `alpha` (0 < alpha <= 1). Throws for invalid `alpha`.
     */
    constructor(alpha: number);
    /** Adds a sample. Throws on invalid input (e.g. non-finite values). */
    update(x: number): void;
    /** Current exponentially weighted mean. */
    value(): number;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmEWMean;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Online standard scaler.
 */
export class WasmStandardScaler {
    /** Constructs a scaler for `feature_count` features. Throws if invalid. */
    constructor(feature_count: number);
    /**
     * Updates running statistics with a feature vector. Throws if the input
     * length does not match `feature_count` or values are not finite.
     */
    update(x: Float64Array): void;
    /**
     * Standardizes a feature vector. Throws if the input length does not
     * match `feature_count`.
     */
    transform(x: Float64Array): Float64Array;
    /** Alias for `update`. */
    learn_one(x: Float64Array): void;
    /** Number of samples seen, as a `bigint`. */
    samples_seen(): bigint;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmStandardScaler;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Online linear regression.
 */
export class WasmLinearRegression {
    /**
     * Constructs a linear regression with `feature_count` features and the
     * given SGD `learning_rate`. Throws if the configuration is invalid.
     */
    constructor(feature_count: number, learning_rate: number);
    /**
     * Predicts a target for a feature vector. Throws if the input length
     * does not match `feature_count`.
     */
    predict(x: Float64Array): number;
    /**
     * Learns from a labeled sample. Throws if the input length does not
     * match `feature_count` or the target is not finite.
     */
    learn(x: Float64Array, y: number): void;
    /** Current model weights. */
    weights(): Float64Array;
    /** Number of samples seen, as a `bigint`. */
    samples_seen(): bigint;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmLinearRegression;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Online logistic regression.
 */
export class WasmLogisticRegression {
    /**
     * Constructs a logistic regression with `feature_count` features and the
     * given SGD `learning_rate`. Throws if the configuration is invalid.
     */
    constructor(feature_count: number, learning_rate: number);
    /**
     * Predicts the class (true/false) for a feature vector. Throws if the
     * input length does not match `feature_count`.
     */
    predict(x: Float64Array): boolean;
    /**
     * Predicts the positive-class probability for a feature vector. Throws
     * if the input length does not match `feature_count`.
     */
    predict_proba(x: Float64Array): number;
    /**
     * Learns from a labeled sample. Throws if the input length does not
     * match `feature_count`.
     */
    learn(x: Float64Array, y: boolean): void;
    /** Current model weights. */
    weights(): Float64Array;
    /** Number of samples seen, as a `bigint`. */
    samples_seen(): bigint;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmLogisticRegression;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Regression pipeline: `StandardScaler` + `LinearRegression`.
 */
export class WasmRegressionPipeline {
    /**
     * Constructs a regression pipeline with `feature_count` features and the
     * given SGD `learning_rate`. Throws if the configuration is invalid.
     */
    constructor(feature_count: number, learning_rate: number);
    /**
     * Predicts a target for a feature vector. Throws if the input length
     * does not match `feature_count`.
     */
    predict(x: Float64Array): number;
    /**
     * Learns from a labeled sample. Throws if the input length does not
     * match `feature_count`.
     */
    learn(x: Float64Array, y: number): void;
    /** Number of samples seen, as a `bigint`. */
    samples_seen(): bigint;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmRegressionPipeline;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Classification pipeline: `StandardScaler` + `LogisticRegression`.
 */
export class WasmClassificationPipeline {
    /**
     * Constructs a classification pipeline with `feature_count` features and
     * the given SGD `learning_rate`. Throws if the configuration is invalid.
     */
    constructor(feature_count: number, learning_rate: number);
    /**
     * Predicts the class (true/false) for a feature vector. Throws if the
     * input length does not match `feature_count`.
     */
    predict(x: Float64Array): boolean;
    /**
     * Predicts the positive-class probability for a feature vector. Throws
     * if the input length does not match `feature_count`.
     */
    predict_proba(x: Float64Array): number;
    /**
     * Learns from a labeled sample. Throws if the input length does not
     * match `feature_count`.
     */
    learn(x: Float64Array, y: boolean): void;
    /** Number of samples seen, as a `bigint`. */
    samples_seen(): bigint;
    /** Serializes the current state to a snapshot JSON string. */
    to_json(): string;
    /**
     * Restores state from a snapshot JSON string.
     * Throws on malformed JSON, on input larger than the 64 MiB snapshot
     * size limit, on an incompatible format version, or on invalid state.
     */
    static from_json(json: string): WasmClassificationPipeline;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/**
 * Marker type for the RillML snapshot namespace. Each `WasmX` class exposes
 * its own `to_json`/`from_json`; this marker lets downstream code
 * feature-detect the binding at runtime.
 */
export class WasmSnapshot {
    private constructor();
    /** Returns the snapshot format version supported by this build. */
    static format_version(): number;
    /** Frees the underlying WASM allocation (also available via `[Symbol.dispose]`). */
    free(): void;
}

/* ---------------------------------------------------------------------------
 * Browser (web ESM) initialization surface.
 *
 * The `pkg/` web build does not auto-initialize; call `init()` (async, from a
 * module scope) or `initSync()` before using the classes in a browser bundle.
 * The `pkg-node/` Node.js build auto-initializes and does not export these.
 * ------------------------------------------------------------------------- */

/** Input accepted by the web build's `init()`. */
export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

/** Input accepted by the web build's `initSync()`. */
export type SyncInitInput = BufferSource | WebAssembly.Module;

/** Result of `init()`/`initSync()`. Consumers normally ignore it. */
export interface InitOutput {
    readonly memory: WebAssembly.Memory;
}

/**
 * Initializes the WASM module from bytes or a precompiled
 * `WebAssembly.Module` (synchronous).
 */
export function initSync(module: SyncInitInput | { module: SyncInitInput }): InitOutput;

/**
 * Fetches and instantiates the WASM module (asynchronous). In a bundler the
 * default URL resolution is used; pass `module_or_path` to override.
 */
export default function init(
    module_or_path?: InitInput | Promise<InitInput> | { module_or_path: InitInput | Promise<InitInput> }
): Promise<InitOutput>;

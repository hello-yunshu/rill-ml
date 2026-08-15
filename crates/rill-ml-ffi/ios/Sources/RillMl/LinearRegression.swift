import CRillMl

/// Online linear regression trained with SGD (wraps
/// `rill_ml_linear_regression_*` in the Stable C ABI).
///
/// Ownership: the handle is owned by this instance and released exactly once
/// — either by an explicit `close()` or by `deinit`. Using an instance after
/// `close()` throws `RillMlError.invalidHandle`. A single instance must not
/// be shared concurrently between threads.
public final class LinearRegression {

    private var handle: UnsafeMutableRawPointer?

    /// Creates a new linear regression with the given number of features and
    /// SGD learning rate.
    public init(featureCount: Int, learningRate: Double) throws {
        handle = try RillMlFFI.handle(what: "linear regression") { err, elen in
            rill_ml_linear_regression_new(featureCount, learningRate, err, elen)
        }
    }

    private init(handle: UnsafeMutableRawPointer?) {
        self.handle = handle
    }

    private func checkedHandle() throws -> UnsafeMutableRawPointer {
        guard let handle else {
            throw RillMlError.invalidHandle("linear regression instance is closed")
        }
        return handle
    }

    /// Predicts `y` for the given feature vector.
    public func predict(_ features: [Double]) throws -> Double {
        let h = try checkedHandle()
        var out: Double = 0
        var err = [CChar](repeating: 0, count: 512)
        let code = features.withUnsafeBufferPointer { fp in
            err.withUnsafeMutableBufferPointer { bp in
                rill_ml_linear_regression_predict(
                    h, fp.baseAddress, features.count, &out, bp.baseAddress!, bp.count)
            }
        }
        try RillMlFFI.check(code, err)
        return out
    }

    /// Learns one labeled sample `(features, target)`.
    public func learn(features: [Double], target: Double) throws {
        let h = try checkedHandle()
        var err = [CChar](repeating: 0, count: 512)
        let code = features.withUnsafeBufferPointer { fp in
            err.withUnsafeMutableBufferPointer { bp in
                rill_ml_linear_regression_learn(
                    h, fp.baseAddress, features.count, target, bp.baseAddress!, bp.count)
            }
        }
        try RillMlFFI.check(code, err)
    }

    /// Copies the learned weights (one per feature).
    public func weights() throws -> [Double] {
        let h = try checkedHandle()
        var n: Int = 0
        var err = [CChar](repeating: 0, count: 512)
        // Query mode: `out == nil` writes the required element count to `n`.
        let qcode = err.withUnsafeMutableBufferPointer { bp in
            rill_ml_linear_regression_weights(h, nil, &n, bp.baseAddress!, bp.count)
        }
        try RillMlFFI.check(qcode, err)
        var out = [Double](repeating: 0, count: n)
        let code = out.withUnsafeMutableBufferPointer { obp in
            err.withUnsafeMutableBufferPointer { bp in
                rill_ml_linear_regression_weights(
                    h, obp.baseAddress, &n, bp.baseAddress!, bp.count)
            }
        }
        try RillMlFFI.check(code, err)
        return out
    }

    /// The learned intercept.
    public func intercept() throws -> Double {
        let h = try checkedHandle()
        var out: Double = 0
        var err = [CChar](repeating: 0, count: 512)
        let code = err.withUnsafeMutableBufferPointer { bp in
            rill_ml_linear_regression_intercept(h, &out, bp.baseAddress!, bp.count)
        }
        try RillMlFFI.check(code, err)
        return out
    }

    /// The number of learned samples.
    public func samplesSeen() throws -> UInt64 {
        let h = try checkedHandle()
        var out: UInt64 = 0
        var err = [CChar](repeating: 0, count: 512)
        let code = err.withUnsafeMutableBufferPointer { bp in
            rill_ml_linear_regression_samples_seen(h, &out, bp.baseAddress!, bp.count)
        }
        try RillMlFFI.check(code, err)
        return out
    }

    /// Serializes the model as a versioned JSON snapshot.
    public func toJSON() throws -> String {
        let h = try checkedHandle()
        return try RillMlFFI.string { buf, len, err, elen in
            rill_ml_linear_regression_to_json(h, buf, len, err, elen)
        }
    }

    /// Restores a model from a validated JSON snapshot.
    public static func fromJSON(_ json: String) throws -> LinearRegression {
        let h = try RillMlFFI.handle(what: "linear regression.fromJSON") { err, elen in
            rill_ml_linear_regression_from_json(json, err, elen)
        }
        return LinearRegression(handle: h)
    }

    /// Releases the native handle. Safe to call more than once; `deinit`
    /// calls this automatically.
    public func close() {
        if let handle {
            self.handle = nil
            _ = rill_ml_linear_regression_destroy(handle, nil, 0)
        }
    }

    deinit {
        close()
    }
}

import CRillMl

/// Online mean accumulator (wraps `rill_ml_mean_*` in the Stable C ABI).
///
/// Ownership: the handle is owned by this instance and released exactly once
/// — either by an explicit `close()` or by `deinit`. Using an instance after
/// `close()` throws `RillMlError.invalidHandle`. A single instance must not
/// be shared concurrently between threads.
public final class Mean {

    private var handle: UnsafeMutableRawPointer?

    /// Creates a new, empty online mean accumulator.
    public init() throws {
        handle = try RillMlFFI.handle(what: "mean") { err, elen in
            rill_ml_mean_new(err, elen)
        }
    }

    private init(handle: UnsafeMutableRawPointer?) {
        self.handle = handle
    }

    private func checkedHandle() throws -> UnsafeMutableRawPointer {
        guard let handle else {
            throw RillMlError.invalidHandle("mean instance is closed")
        }
        return handle
    }

    /// Updates the running mean with one observation.
    public func update(_ value: Double) throws {
        let h = try checkedHandle()
        try RillMlFFI.call { err, elen in
            rill_ml_mean_update(h, value, err, elen)
        }
    }

    /// The current mean value.
    public func value() throws -> Double {
        let h = try checkedHandle()
        var out: Double = 0
        var err = [CChar](repeating: 0, count: 512)
        let code = err.withUnsafeMutableBufferPointer { bp in
            rill_ml_mean_value(h, &out, bp.baseAddress!, bp.count)
        }
        try RillMlFFI.check(code, err)
        return out
    }

    /// The number of observations seen.
    public func count() throws -> UInt64 {
        let h = try checkedHandle()
        var out: UInt64 = 0
        var err = [CChar](repeating: 0, count: 512)
        let code = err.withUnsafeMutableBufferPointer { bp in
            rill_ml_mean_count(h, &out, bp.baseAddress!, bp.count)
        }
        try RillMlFFI.check(code, err)
        return out
    }

    /// Serializes the accumulator as a versioned JSON snapshot.
    public func toJSON() throws -> String {
        let h = try checkedHandle()
        return try RillMlFFI.string { buf, len, err, elen in
            rill_ml_mean_to_json(h, buf, len, err, elen)
        }
    }

    /// Restores an accumulator from a validated JSON snapshot.
    public static func fromJSON(_ json: String) throws -> Mean {
        let h = try RillMlFFI.handle(what: "mean.fromJSON") { err, elen in
            rill_ml_mean_from_json(json, err, elen)
        }
        return Mean(handle: h)
    }

    /// Releases the native handle. Safe to call more than once; `deinit`
    /// calls this automatically.
    public func close() {
        if let handle {
            self.handle = nil
            _ = rill_ml_mean_destroy(handle, nil, 0)
        }
    }

    deinit {
        close()
    }
}

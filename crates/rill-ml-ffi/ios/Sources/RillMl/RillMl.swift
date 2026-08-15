import CRillMl

/// Errors surfaced by the RillML Swift wrapper. Each case carries the
/// NUL-terminated message written by the C FFI.
public enum RillMlError: Error, CustomStringConvertible, Equatable {
    case invalidArgument(String)
    case invalidState(String)
    case panic(String)
    case io(String)
    case invalidHandle(String)
    case bufferTooSmall(String)
    case nullHandle(String)
    case unexpected(code: Int32, message: String)

    public var description: String {
        switch self {
        case .invalidArgument(let m): return "rill-ml: invalid argument: \(m)"
        case .invalidState(let m): return "rill-ml: invalid state: \(m)"
        case .panic(let m): return "rill-ml: caught panic: \(m)"
        case .io(let m): return "rill-ml: i/o error: \(m)"
        case .invalidHandle(let m): return "rill-ml: invalid handle: \(m)"
        case .bufferTooSmall(let m): return "rill-ml: buffer too small: \(m)"
        case .nullHandle(let m): return "rill-ml: null handle: \(m)"
        case .unexpected(let c, let m): return "rill-ml: unexpected error \(c): \(m)"
        }
    }
}

/// Top-level library info facade mirroring `rill_ml_version` /
/// `rill_ml_snapshot_format_version`. (Named `RillMlInfo` rather than
/// `RillMl` so the type name does not collide with the `RillMl` module name.)
public enum RillMlInfo {

    /// Returns the `rill-ml-ffi` crate version string (e.g. "0.15.0").
    public static func version() throws -> String {
        try RillMlFFI.string { buf, len, err, elen in
            rill_ml_version(buf, len, err, elen)
        }
    }

    /// The snapshot format version used by `toJSON()` / `fromJSON()`.
    public static var snapshotFormatVersion: Int32 {
        rill_ml_snapshot_format_version()
    }
}

/// Internal plumbing for the opaque-handle C ABI. Kept file-private to the
/// module so the wrapper surface stays small.
enum RillMlFFI {
    /// Maximum snapshot size the wrapper will request (mirrors the core
    /// MAX_SNAPSHOT_JSON_BYTES limit).
    static let maxSnapshotBytes = 64 << 20

    /// Converts a C error code + message into a Swift error.
    static func error(_ code: Int32, _ message: String) -> RillMlError {
        switch code {
        case RILL_ML_ERR_INVALID_ARGUMENT: return .invalidArgument(message)
        case RILL_ML_ERR_INVALID_STATE: return .invalidState(message)
        case RILL_ML_ERR_PANIC: return .panic(message)
        case RILL_ML_ERR_IO: return .io(message)
        case RILL_ML_ERR_INVALID_HANDLE: return .invalidHandle(message)
        case RILL_ML_ERR_BUFFER_TOO_SMALL: return .bufferTooSmall(message)
        default: return .unexpected(code: code, message: message)
        }
    }

    /// Throws when `code != RILL_ML_OK`.
    static func check(_ code: Int32, _ err: [CChar]) throws {
        if code != RILL_ML_OK {
            throw error(code, String(cString: err))
        }
    }

    /// Runs a fallible C call (one that returns an error code).
    @discardableResult
    static func call(_ body: (UnsafeMutablePointer<CChar>, Int) -> Int32) throws -> Int32 {
        var err = [CChar](repeating: 0, count: 512)
        let code = err.withUnsafeMutableBufferPointer { bp in
            body(bp.baseAddress!, bp.count)
        }
        try check(code, err)
        return code
    }

    /// Runs a "_new"-style call returning an owned handle, throwing
    /// `RillMlError.nullHandle` (carrying the FFI message) when it is NULL.
    static func handle(
        what: String,
        _ body: (UnsafeMutablePointer<CChar>, Int) -> UnsafeMutableRawPointer?
    ) throws -> UnsafeMutableRawPointer {
        var err = [CChar](repeating: 0, count: 512)
        let h = err.withUnsafeMutableBufferPointer { bp in
            body(bp.baseAddress!, bp.count)
        }
        guard let h else {
            throw RillMlError.nullHandle(
                String(cString: err).isEmpty ? what : String(cString: err))
        }
        return h
    }

    /// Runs a fixed/growing-buffer C call (e.g. `_version`, `_to_json`) and
    /// returns the NUL-terminated output string.
    static func string(
        _ body: (UnsafeMutablePointer<CChar>, Int, UnsafeMutablePointer<CChar>, Int) -> Int32
    ) throws -> String {
        var cap = 1 << 16 // 64 KiB initial
        while true {
            var err = [CChar](repeating: 0, count: 512)
            let out = UnsafeMutablePointer<CChar>.allocate(capacity: cap)
            defer { out.deallocate() }
            let code = err.withUnsafeMutableBufferPointer { bp in
                body(out, cap, bp.baseAddress!, bp.count)
            }
            if code == RILL_ML_OK {
                return String(cString: out)
            }
            if code == RILL_ML_ERR_BUFFER_TOO_SMALL && cap < maxSnapshotBytes {
                cap <<= 1
                continue
            }
            throw error(code, String(cString: err))
        }
    }
}

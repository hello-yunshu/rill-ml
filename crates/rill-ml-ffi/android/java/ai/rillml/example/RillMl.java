package ai.rillml.example;

/**
 * Minimal JNI binding to the RillML Stable C FFI (crates/rill-ml-ffi).
 *
 * <p>Architecture: Android App &rarr; JNI (this class + {@code jni/rill_ml_jni.c})
 * &rarr; rill-ml-ffi &rarr; rill-ml Core. Only the opaque-handle C ABI declared in
 * {@code include/rill_ml.h} is used; there is no second implementation of the math.
 *
 * <p>Error semantics: every {@code native} method validates the C error code and
 * throws {@link RillMlException} (carrying the FFI error message) when the call
 * fails. Handles are owned by the Java side; each wrapper releases its handle
 * exactly once in {@code close()} (finalize is kept as a safety net). Using a
 * closed handle is undefined behaviour.
 *
 * <p>Thread safety: a single handle must not be shared concurrently between
 * threads; distinct handles may be used from distinct threads.
 */
public final class RillMl {

    /** Thrown when a C FFI call returns a non-OK error code. */
    public static final class RillMlException extends RuntimeException {
        RillMlException(String message) {
            super(message);
        }
    }

    /* Error codes — must stay in sync with include/rill_ml.h. */
    private static final int OK = 0;
    private static final int ERR_INVALID_ARGUMENT = -1;
    private static final int ERR_INVALID_STATE = -2;
    private static final int ERR_PANIC = -3;
    private static final int ERR_IO = -4;
    private static final int ERR_INVALID_HANDLE = -5;
    private static final int ERR_BUFFER_TOO_SMALL = -6;

    static {
        System.loadLibrary("rill_ml_jni");
    }

    private RillMl() {
        // Static facade.
    }

    // ---- Version / format helpers ---------------------------------------

    /** Returns the rill-ml-ffi crate version string (e.g. "0.15.0"). */
    public static String version() {
        return nativeVersion();
    }

    /** Returns the snapshot format version used by toJson/fromJson. */
    public static int snapshotFormatVersion() {
        return nativeSnapshotFormatVersion();
    }

    private static native String nativeVersion();

    private static native int nativeSnapshotFormatVersion();

    // ---- Mean -----------------------------------------------------------

    private static native long nativeMeanNew();

    private static native void nativeMeanUpdate(long handle, double value);

    private static native double nativeMeanValue(long handle);

    private static native long nativeMeanCount(long handle);

    private static native String nativeMeanToJson(long handle);

    private static native long nativeMeanFromJson(String json);

    private static native void nativeMeanDestroy(long handle);

    /** Online mean accumulator (see {@code rill_ml_mean_*} in rill_ml.h). */
    public static final class Mean implements AutoCloseable {

        private long handle;

        private Mean(long handle) {
            this.handle = handle;
        }

        public static Mean create() {
            return new Mean(nativeMeanNew());
        }

        /** Update the running mean with one observation. */
        public void update(double value) {
            nativeMeanUpdate(handle, value);
        }

        /** Current mean value. */
        public double value() {
            return nativeMeanValue(handle);
        }

        /** Number of observations seen. */
        public long count() {
            return nativeMeanCount(handle);
        }

        /** Versioned JSON snapshot of the current state. */
        public String toJson() {
            return nativeMeanToJson(handle);
        }

        /** Restore a mean from a validated JSON snapshot. */
        public static Mean fromJson(String json) {
            return new Mean(nativeMeanFromJson(json));
        }

        /** Releases the native handle. Safe to call more than once. */
        @Override
        public void close() {
            if (handle != 0) {
                nativeMeanDestroy(handle);
                handle = 0;
            }
        }

        @Override
        protected void finalize() {
            close();
        }
    }

    // ---- LinearRegression ------------------------------------------------

    private static native long nativeLinearRegressionNew(int featureCount,
                                                          double learningRate);

    private static native double nativeLinearRegressionPredict(long handle,
                                                               double[] features);

    private static native void nativeLinearRegressionLearn(long handle,
                                                           double[] features,
                                                           double target);

    private static native double[] nativeLinearRegressionWeights(long handle);

    private static native double nativeLinearRegressionIntercept(long handle);

    private static native long nativeLinearRegressionSamplesSeen(long handle);

    private static native String nativeLinearRegressionToJson(long handle);

    private static native long nativeLinearRegressionFromJson(String json);

    private static native void nativeLinearRegressionDestroy(long handle);

    /** Online linear regression with SGD (see {@code rill_ml_linear_regression_*}). */
    public static final class LinearRegression implements AutoCloseable {

        private long handle;

        private LinearRegression(long handle) {
            this.handle = handle;
        }

        public static LinearRegression create(int featureCount, double learningRate) {
            return new LinearRegression(
                    nativeLinearRegressionNew(featureCount, learningRate));
        }

        /** Predicts y for the given feature vector. */
        public double predict(double[] features) {
            return nativeLinearRegressionPredict(handle, features);
        }

        /** Learns one labeled sample {@code (features, target)}. */
        public void learn(double[] features, double target) {
            nativeLinearRegressionLearn(handle, features, target);
        }

        /** Copies the learned weights (one per feature). */
        public double[] weights() {
            return nativeLinearRegressionWeights(handle);
        }

        /** The learned intercept. */
        public double intercept() {
            return nativeLinearRegressionIntercept(handle);
        }

        /** Number of learned samples. */
        public long samplesSeen() {
            return nativeLinearRegressionSamplesSeen(handle);
        }

        /** Versioned JSON snapshot of the current state. */
        public String toJson() {
            return nativeLinearRegressionToJson(handle);
        }

        /** Restore a model from a validated JSON snapshot. */
        public static LinearRegression fromJson(String json) {
            return new LinearRegression(nativeLinearRegressionFromJson(json));
        }

        /** Releases the native handle. Safe to call more than once. */
        @Override
        public void close() {
            if (handle != 0) {
                nativeLinearRegressionDestroy(handle);
                handle = 0;
            }
        }

        @Override
        protected void finalize() {
            close();
        }
    }
}

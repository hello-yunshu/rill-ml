# rill-ml-ffi · Android (JNI) binding example

A minimal, complete JNI example showing an Android app calling into the RillML
Core via the Stable C FFI:

```
Android App → JNI (java/ + jni/) → rill-ml-ffi → rill-ml Core
```

Only the opaque-handle C ABI declared in
[`../include/rill_ml.h`](../include/rill_ml.h) is used — there is no second
implementation of the math.

## Layout

```
android/
├── java/ai/rillml/example/RillMl.java   Java wrapper (native methods, error
│                                         mapping, handle lifetime management)
├── jni/rill_ml_jni.c                     JNI C shim (jni.h + ../include/rill_ml.h)
└── README.md
```

## Building the native library

The JNI shim is compiled with the Android NDK clang toolchain and linked
together with the `rill-ml-ffi` staticlib produced by
[`scripts/docker-build-android.sh`](../../../scripts/docker-build-android.sh).
That script builds `librill_ml_ffi.a` for `aarch64-linux-android` and
`x86_64-linux-android` inside a pinned Docker image (Docker-first; there is no
locally installed NDK requirement).

A minimal `Android.mk` for the JNI shim (place in an app's `jni/` directory):

```makefile
LOCAL_PATH := $(call my-dir)

include $(CLEAR_VARS)
LOCAL_MODULE    := rill_ml_jni
LOCAL_SRC_FILES := rill_ml_jni.c
LOCAL_C_INCLUDES := $(LOCAL_PATH)/../../include
LOCAL_STATIC_LIBRARIES := rill_ml_ffi
LOCAL_LDLIBS := -llog
include $(BUILD_SHARED_LIBRARY)

# rill_ml_ffi: built by scripts/docker-build-android.sh, referenced from
# $(TARGET_ARCH_ABI) (prebuilt/armeabi-v7a, arm64-v8a, x86_64, ...).
```

The Java class calls `System.loadLibrary("rill_ml_jni")`, so the shared
library must be named `librill_ml_jni.so` and packaged into the APK under
`lib/<abi>/`. With Gradle, set `android.defaultConfig.ndk.abiFilters` to the
ABIs you ship (e.g. `arm64-v8a`, `x86_64`).

## Ownership and error semantics

- **Handles.** `new`/`fromJson` return an opaque handle stored as a `long`.
  Each Java wrapper (`RillMl.Mean`, `RillMl.LinearRegression`) owns its
  handle and frees it exactly once in `close()`; `finalize()` is a safety
  net. Using a closed handle is undefined behaviour.
- **Thread safety.** A single handle must not be shared concurrently between
  threads; distinct handles may be used from distinct threads.
- **Errors.** Every fallible call returns an `int` error code (see the
  `RILL_ML_*` constants in `rill_ml.h`). The shim converts a non-`RILL_ML_OK`
  code into a `java.lang.RuntimeException` whose message carries the FFI
  error message; the Java API surfaces it as `RillMlException`.
- **Snapshots.** `toJson()`/`fromJson()` use versioned JSON envelopes. The
  shim grows its output buffer on `RILL_ML_ERR_BUFFER_TOO_SMALL` up to the
  core 64 MiB snapshot limit.

## Example

```java
try (RillMl.Mean mean = RillMl.Mean.create()) {
    mean.update(1.0);
    mean.update(2.0);
    mean.update(3.0);
    System.out.println(mean.value()); // 2.0

    String json = mean.toJson();
    try (RillMl.Mean restored = RillMl.Mean.fromJson(json)) {
        System.out.println(restored.count()); // 3
    }
}

try (RillMl.LinearRegression lr =
        RillMl.LinearRegression.create(/* featureCount */ 1, /* lr */ 0.05)) {
    for (int i = 0; i < 100; i++) {
        lr.learn(new double[]{2.0}, 10.0);
    }
    System.out.println(lr.predict(new double[]{2.0})); // ≈ 10.0
}
```

## Versioning

`RillMl.version()` returns the `rill-ml-ffi` crate version (`0.15.0`);
`RillMl.snapshotFormatVersion()` returns `1`.

/*
 * rill_ml_jni.c — JNI shim between an Android app and the RillML Stable C FFI.
 *
 * Architecture: Android App -> JNI (this file) -> rill-ml-ffi -> rill-ml Core.
 * Only the opaque-handle C ABI declared in ../../include/rill_ml.h is used.
 *
 * Conventions:
 *  - Opaque handles cross the JNI boundary as jlong (never as raw pointers).
 *  - Every non-OK error code is converted into a java.lang.RuntimeException
 *    carrying the FFI error message.
 *  - Caller-provided buffers are used for all FFI output (to_json grows the
 *    buffer on RILL_ML_ERR_BUFFER_TOO_SMALL up to the core 64 MiB snapshot cap).
 *
 * Built with the Android NDK clang toolchain and linked together with the
 * rill-ml-ffi staticlib (see the README in this directory).
 */

#include <jni.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include "../../include/rill_ml.h"

#define RILL_ML_JNI_ERR_CLS "java/lang/RuntimeException"
/* Mirrors the core snapshot-size limit (MAX_SNAPSHOT_JSON_BYTES). */
#define RILL_ML_JNI_MAX_SNAPSHOT_BYTES (64u << 20)

/* ---- error helpers ---------------------------------------------------- */

static void throw_msg(JNIEnv *env, const char *msg) {
    jclass cls = (*env)->FindClass(env, RILL_ML_JNI_ERR_CLS);
    if (cls == NULL) {
        return; /* an OutOfMemoryError is already pending */
    }
    (*env)->ThrowNew(env, cls, msg);
}

/* Throw a RuntimeException built from an FFI error code + message. */
static void throw_by_rc(JNIEnv *env, int rc, const char *err) {
    if (rc == RILL_ML_OK) {
        return;
    }
    char msg[576];
    snprintf(msg, sizeof(msg), "rill-ml-ffi error %d: %s",
             rc, (err != NULL && err[0] != '\0') ? err : "(no message)");
    throw_msg(env, msg);
}

/* Throw when a _new/_from_json call returned a NULL handle. */
static void throw_on_null(JNIEnv *env, const char *err) {
    throw_msg(env, (err != NULL && err[0] != '\0')
                       ? err
                       : "rill-ml-ffi returned a NULL handle");
}

/* ---- feature-array helper --------------------------------------------- */

/* Copies a jdoubleArray into a caller-owned double buffer; returns NULL and
 * throws when the array is null/empty or allocation fails. */
static double *array_to_doubles(JNIEnv *env, jdoubleArray arr, jsize *len) {
    *len = 0;
    if (arr == NULL) {
        throw_msg(env, "features array is null");
        return NULL;
    }
    *len = (*env)->GetArrayLength(env, arr);
    if (*len == 0) {
        throw_msg(env, "features array is empty");
        return NULL;
    }
    double *buf = (double *)malloc((size_t)*len * sizeof(double));
    if (buf == NULL) {
        throw_msg(env, "rill-ml-ffi out of memory");
        return NULL;
    }
    (*env)->GetDoubleArrayRegion(env, arr, 0, *len, buf);
    return buf;
}

/* ---- to_json / from_json helpers -------------------------------------- */

typedef int (*to_json_fn)(void *handle, char *buf, size_t buf_len,
                          char *error_buf, size_t error_buf_len);

/* Runs a C to_json, growing the buffer on RILL_ML_ERR_BUFFER_TOO_SMALL. */
static jstring to_json_string(JNIEnv *env, to_json_fn fn, void *handle) {
    size_t cap = 1u << 16; /* 64 KiB initial */
    char err[512];
    for (;;) {
        char *buf = (char *)malloc(cap);
        if (buf == NULL) {
            throw_msg(env, "rill-ml-ffi out of memory");
            return NULL;
        }
        int rc = fn(handle, buf, cap, err, sizeof(err));
        if (rc == RILL_ML_OK) {
            jstring out = (*env)->NewStringUTF(env, buf);
            free(buf);
            return out;
        }
        free(buf);
        if (rc == RILL_ML_ERR_BUFFER_TOO_SMALL && cap < RILL_ML_JNI_MAX_SNAPSHOT_BYTES) {
            cap <<= 1;
            continue;
        }
        throw_by_rc(env, rc, err);
        return NULL;
    }
}

static void *from_json_handle(JNIEnv *env, jstring json,
                              void *(*fn)(const char *json, char *error_buf,
                                          size_t error_buf_len)) {
    if (json == NULL) {
        throw_msg(env, "json string is null");
        return NULL;
    }
    const char *js = (*env)->GetStringUTFChars(env, json, NULL);
    if (js == NULL) {
        return NULL; /* OOM pending */
    }
    char err[512];
    void *handle = fn(js, err, sizeof(err));
    (*env)->ReleaseStringUTFChars(env, json, js);
    if (handle == NULL) {
        throw_on_null(env, err);
        return NULL;
    }
    return handle;
}

/* ======================================================================= */
/* Version / format helpers                                                */
/* ======================================================================= */

JNIEXPORT jstring JNICALL
Java_ai_rillml_example_RillMl_nativeVersion(JNIEnv *env, jclass clazz) {
    (void)clazz;
    char buf[64];
    char err[512];
    int rc = rill_ml_version(buf, sizeof(buf), err, sizeof(err));
    if (rc != RILL_ML_OK) {
        throw_by_rc(env, rc, err);
        return NULL;
    }
    return (*env)->NewStringUTF(env, buf);
}

JNIEXPORT jint JNICALL
Java_ai_rillml_example_RillMl_nativeSnapshotFormatVersion(JNIEnv *env,
                                                          jclass clazz) {
    (void)env;
    (void)clazz;
    return (jint)rill_ml_snapshot_format_version();
}

/* ======================================================================= */
/* Mean                                                                    */
/* ======================================================================= */

JNIEXPORT jlong JNICALL
Java_ai_rillml_example_RillMl_nativeMeanNew(JNIEnv *env, jclass clazz) {
    (void)clazz;
    char err[512];
    void *handle = rill_ml_mean_new(err, sizeof(err));
    if (handle == NULL) {
        throw_on_null(env, err);
        return 0;
    }
    return (jlong)(intptr_t)handle;
}

JNIEXPORT void JNICALL
Java_ai_rillml_example_RillMl_nativeMeanUpdate(JNIEnv *env, jclass clazz,
                                               jlong handle, jdouble value) {
    (void)clazz;
    char err[512];
    int rc = rill_ml_mean_update((void *)(intptr_t)handle, (double)value,
                                 err, sizeof(err));
    throw_by_rc(env, rc, err);
}

JNIEXPORT jdouble JNICALL
Java_ai_rillml_example_RillMl_nativeMeanValue(JNIEnv *env, jclass clazz,
                                              jlong handle) {
    (void)clazz;
    char err[512];
    double out = 0.0;
    int rc = rill_ml_mean_value((void *)(intptr_t)handle, &out, err, sizeof(err));
    if (rc != RILL_ML_OK) {
        throw_by_rc(env, rc, err);
        return 0.0;
    }
    return (jdouble)out;
}

JNIEXPORT jlong JNICALL
Java_ai_rillml_example_RillMl_nativeMeanCount(JNIEnv *env, jclass clazz,
                                              jlong handle) {
    (void)clazz;
    char err[512];
    uint64_t out = 0;
    int rc = rill_ml_mean_count((void *)(intptr_t)handle, &out, err, sizeof(err));
    if (rc != RILL_ML_OK) {
        throw_by_rc(env, rc, err);
        return 0;
    }
    return (jlong)out;
}

JNIEXPORT jstring JNICALL
Java_ai_rillml_example_RillMl_nativeMeanToJson(JNIEnv *env, jclass clazz,
                                               jlong handle) {
    (void)clazz;
    return to_json_string(env, rill_ml_mean_to_json, (void *)(intptr_t)handle);
}

JNIEXPORT jlong JNICALL
Java_ai_rillml_example_RillMl_nativeMeanFromJson(JNIEnv *env, jclass clazz,
                                                 jstring json) {
    (void)clazz;
    void *handle = from_json_handle(env, json, rill_ml_mean_from_json);
    return (jlong)(intptr_t)handle;
}

JNIEXPORT void JNICALL
Java_ai_rillml_example_RillMl_nativeMeanDestroy(JNIEnv *env, jclass clazz,
                                                jlong handle) {
    (void)clazz;
    char err[512];
    int rc = rill_ml_mean_destroy((void *)(intptr_t)handle, err, sizeof(err));
    throw_by_rc(env, rc, err);
}

/* ======================================================================= */
/* LinearRegression                                                        */
/* ======================================================================= */

JNIEXPORT jlong JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionNew(JNIEnv *env,
                                                        jclass clazz,
                                                        jint featureCount,
                                                        jdouble learningRate) {
    (void)clazz;
    char err[512];
    void *handle = rill_ml_linear_regression_new(
        (size_t)featureCount, (double)learningRate, err, sizeof(err));
    if (handle == NULL) {
        throw_on_null(env, err);
        return 0;
    }
    return (jlong)(intptr_t)handle;
}

JNIEXPORT jdouble JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionPredict(JNIEnv *env,
                                                            jclass clazz,
                                                            jlong handle,
                                                            jdoubleArray features) {
    (void)clazz;
    jsize n = 0;
    double *fs = array_to_doubles(env, features, &n);
    if (fs == NULL) {
        return 0.0;
    }
    char err[512];
    double out = 0.0;
    int rc = rill_ml_linear_regression_predict(
        (void *)(intptr_t)handle, fs, (size_t)n, &out, err, sizeof(err));
    free(fs);
    if (rc != RILL_ML_OK) {
        throw_by_rc(env, rc, err);
        return 0.0;
    }
    return (jdouble)out;
}

JNIEXPORT void JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionLearn(JNIEnv *env,
                                                          jclass clazz,
                                                          jlong handle,
                                                          jdoubleArray features,
                                                          jdouble target) {
    (void)clazz;
    jsize n = 0;
    double *fs = array_to_doubles(env, features, &n);
    if (fs == NULL) {
        return;
    }
    char err[512];
    int rc = rill_ml_linear_regression_learn(
        (void *)(intptr_t)handle, fs, (size_t)n, (double)target, err, sizeof(err));
    free(fs);
    throw_by_rc(env, rc, err);
}

JNIEXPORT jdoubleArray JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionWeights(JNIEnv *env,
                                                            jclass clazz,
                                                            jlong handle) {
    (void)clazz;
    char err[512];
    size_t n = 0;
    /* Query mode: out == NULL returns the required element count. */
    int rc = rill_ml_linear_regression_weights((void *)(intptr_t)handle,
                                               NULL, &n, err, sizeof(err));
    if (rc != RILL_ML_OK) {
        throw_by_rc(env, rc, err);
        return NULL;
    }
    jdoubleArray out = (*env)->NewDoubleArray(env, (jsize)n);
    if (out == NULL || n == 0) {
        return out; /* OOM pending, or empty model */
    }
    double *tmp = (double *)malloc(n * sizeof(double));
    if (tmp == NULL) {
        throw_msg(env, "rill-ml-ffi out of memory");
        return NULL;
    }
    rc = rill_ml_linear_regression_weights((void *)(intptr_t)handle,
                                           tmp, &n, err, sizeof(err));
    if (rc != RILL_ML_OK) {
        free(tmp);
        throw_by_rc(env, rc, err);
        return NULL;
    }
    (*env)->SetDoubleArrayRegion(env, out, 0, (jsize)n, tmp);
    free(tmp);
    return out;
}

JNIEXPORT jdouble JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionIntercept(JNIEnv *env,
                                                              jclass clazz,
                                                              jlong handle) {
    (void)clazz;
    char err[512];
    double out = 0.0;
    int rc = rill_ml_linear_regression_intercept((void *)(intptr_t)handle,
                                                 &out, err, sizeof(err));
    if (rc != RILL_ML_OK) {
        throw_by_rc(env, rc, err);
        return 0.0;
    }
    return (jdouble)out;
}

JNIEXPORT jlong JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionSamplesSeen(JNIEnv *env,
                                                                jclass clazz,
                                                                jlong handle) {
    (void)clazz;
    char err[512];
    uint64_t out = 0;
    int rc = rill_ml_linear_regression_samples_seen((void *)(intptr_t)handle,
                                                    &out, err, sizeof(err));
    if (rc != RILL_ML_OK) {
        throw_by_rc(env, rc, err);
        return 0;
    }
    return (jlong)out;
}

JNIEXPORT jstring JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionToJson(JNIEnv *env,
                                                           jclass clazz,
                                                           jlong handle) {
    (void)clazz;
    return to_json_string(env, rill_ml_linear_regression_to_json,
                          (void *)(intptr_t)handle);
}

JNIEXPORT jlong JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionFromJson(JNIEnv *env,
                                                             jclass clazz,
                                                             jstring json) {
    (void)clazz;
    void *handle = from_json_handle(env, json, rill_ml_linear_regression_from_json);
    return (jlong)(intptr_t)handle;
}

JNIEXPORT void JNICALL
Java_ai_rillml_example_RillMl_nativeLinearRegressionDestroy(JNIEnv *env,
                                                            jclass clazz,
                                                            jlong handle) {
    (void)clazz;
    char err[512];
    int rc = rill_ml_linear_regression_destroy((void *)(intptr_t)handle,
                                               err, sizeof(err));
    throw_by_rc(env, rc, err);
}

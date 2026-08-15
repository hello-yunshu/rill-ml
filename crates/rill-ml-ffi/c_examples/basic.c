/*
 * basic.c — C smoke test for the rill-ml-ffi opaque-handle ABI.
 *
 * Compiled and run by scripts/ffi-c-smoke.sh. Exercises the full lifecycle
 * (new → update/learn → value/predict/transform → to_json → from_json →
 * destroy) for every exported model type, plus the error-code contract.
 * Exits non-zero on the first failed check.
 */

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "rill_ml.h"

static int failures = 0;

static void check(int condition, const char *label) {
    if (condition) {
        printf("  ok  %s\n", label);
    } else {
        printf("  FAIL %s\n", label);
        failures++;
    }
}

static void check_rc(int rc, const char *label, const char *err) {
    if (rc == RILL_ML_OK) {
        printf("  ok  %s\n", label);
    } else {
        printf("  FAIL %s (rc=%d, %s)\n", label, rc, err);
        failures++;
    }
}

static int expect_err(int rc, int expected, const char *label, const char *err) {
    if (rc == expected && err[0] != '\0') {
        printf("  ok  %s\n", label);
        return 0;
    }
    printf("  FAIL %s (rc=%d expected %d, err=%s)\n", label, rc, expected, err);
    failures++;
    return 1;
}

int main(void) {
    char err[512];

    printf("rill-ml-ffi C smoke test\n");

    /* --- Version helpers ------------------------------------------------ */
    {
        char ver[64];
        int rc = rill_ml_version(ver, sizeof(ver), err, sizeof(err));
        check_rc(rc, "rill_ml_version", err);
        check(rc == RILL_ML_OK && strncmp(ver, "0.", 2) == 0, "version string is 0.x");
        check(rill_ml_snapshot_format_version() == 1, "snapshot format version is 1");
    }

    /* --- Mean ----------------------------------------------------------- */
    {
        void *m = rill_ml_mean_new(err, sizeof(err));
        check(m != NULL, "mean_new");
        int rc = rill_ml_mean_update(m, 1.0, err, sizeof(err));
        check_rc(rc, "mean_update(1.0)", err);
        rc = rill_ml_mean_update(m, 2.0, err, sizeof(err));
        check_rc(rc, "mean_update(2.0)", err);
        rc = rill_ml_mean_update(m, 3.0, err, sizeof(err));
        check_rc(rc, "mean_update(3.0)", err);

        double value = 0.0;
        uint64_t count = 0;
        rc = rill_ml_mean_value(m, &value, err, sizeof(err));
        check_rc(rc, "mean_value", err);
        check(fabs(value - 2.0) < 1e-9, "mean == 2.0");
        rc = rill_ml_mean_count(m, &count, err, sizeof(err));
        check_rc(rc, "mean_count", err);
        check(count == 3, "mean count == 3");

        char json[512];
        rc = rill_ml_mean_to_json(m, json, sizeof(json), err, sizeof(err));
        check_rc(rc, "mean_to_json", err);
        check(strstr(json, "format_version") != NULL, "snapshot has format envelope");

        void *m2 = rill_ml_mean_from_json(json, err, sizeof(err));
        check(m2 != NULL, "mean_from_json");
        if (m2 != NULL) {
            rc = rill_ml_mean_value(m2, &value, err, sizeof(err));
            check_rc(rc, "restored mean_value", err);
            check(fabs(value - 2.0) < 1e-9, "restored mean == 2.0");
            rill_ml_mean_destroy(m2, err, sizeof(err));
        }

        rc = rill_ml_mean_destroy(m, err, sizeof(err));
        check_rc(rc, "mean_destroy", err);
    }

    /* --- LinearRegression ---------------------------------------------- */
    {
        void *lr = rill_ml_linear_regression_new(1, 0.05, err, sizeof(err));
        check(lr != NULL, "linear_regression_new");
        double x = 2.0;
        for (int i = 0; i < 100; i++) {
            int rc = rill_ml_linear_regression_learn(lr, &x, 1, 10.0, err, sizeof(err));
            if (rc != RILL_ML_OK) {
                check_rc(rc, "linear_regression_learn", err);
                break;
            }
        }
        double pred = 0.0;
        int rc = rill_ml_linear_regression_predict(lr, &x, 1, &pred, err, sizeof(err));
        check_rc(rc, "linear_regression_predict", err);
        check(fabs(pred - 10.0) < 1.0, "prediction near 10.0");

        size_t n = 0;
        rc = rill_ml_linear_regression_weights(lr, NULL, &n, err, sizeof(err));
        check_rc(rc, "linear_regression_weights(query)", err);
        check(n == 1, "weight count == 1");
        double w = 0.0;
        rc = rill_ml_linear_regression_weights(lr, &w, &n, err, sizeof(err));
        check_rc(rc, "linear_regression_weights(copy)", err);

        char json[2048];
        rc = rill_ml_linear_regression_to_json(lr, json, sizeof(json), err, sizeof(err));
        check_rc(rc, "linear_regression_to_json", err);
        void *lr2 = rill_ml_linear_regression_from_json(json, err, sizeof(err));
        check(lr2 != NULL, "linear_regression_from_json");
        if (lr2 != NULL) {
            uint64_t seen = 0;
            rill_ml_linear_regression_samples_seen(lr2, &seen, err, sizeof(err));
            check(seen == 100, "restored samples_seen == 100");
            rill_ml_linear_regression_destroy(lr2, err, sizeof(err));
        }
        rill_ml_linear_regression_destroy(lr, err, sizeof(err));
    }

    /* --- LogisticRegression -------------------------------------------- */
    {
        void *logr = rill_ml_logistic_regression_new(2, 0.1, err, sizeof(err));
        check(logr != NULL, "logistic_regression_new");
        double pos[2] = {1.0, 2.0};
        double neg[2] = {-1.0, -2.0};
        for (int i = 0; i < 50; i++) {
            rill_ml_logistic_regression_learn(logr, pos, 2, 1, err, sizeof(err));
            rill_ml_logistic_regression_learn(logr, neg, 2, 0, err, sizeof(err));
        }
        double proba = 0.0;
        int rc = rill_ml_logistic_regression_predict_proba(logr, pos, 2, &proba, err, sizeof(err));
        check_rc(rc, "logistic_regression_predict_proba", err);
        check(proba >= 0.0 && proba <= 1.0, "proba in [0, 1]");
        int cls = -1;
        rc = rill_ml_logistic_regression_predict(logr, pos, 2, &cls, err, sizeof(err));
        check_rc(rc, "logistic_regression_predict", err);
        check(cls == 1, "positive sample classified as 1");

        /* Invalid binary target must be rejected. */
        rc = rill_ml_logistic_regression_learn(logr, pos, 2, 7, err, sizeof(err));
        expect_err(rc, RILL_ML_ERR_INVALID_ARGUMENT, "target=7 rejected", err);

        rill_ml_logistic_regression_destroy(logr, err, sizeof(err));
    }

    /* --- StandardScaler ------------------------------------------------- */
    {
        void *sc = rill_ml_standard_scaler_new(2, err, sizeof(err));
        check(sc != NULL, "standard_scaler_new");
        double s1[2] = {1.0, 2.0};
        double s2[2] = {3.0, 4.0};
        int rc = rill_ml_standard_scaler_update(sc, s1, 2, err, sizeof(err));
        check_rc(rc, "standard_scaler_update(1)", err);
        rc = rill_ml_standard_scaler_update(sc, s2, 2, err, sizeof(err));
        check_rc(rc, "standard_scaler_update(2)", err);

        uint64_t seen = 0;
        rill_ml_standard_scaler_samples_seen(sc, &seen, err, sizeof(err));
        check(seen == 2, "scaler samples_seen == 2");

        double q[2] = {2.0, 3.0};
        double out[2] = {0.0, 0.0};
        rc = rill_ml_standard_scaler_transform(sc, q, 2, out, err, sizeof(err));
        check_rc(rc, "standard_scaler_transform", err);
        check(isfinite(out[0]) && isfinite(out[1]), "transform output finite");

        rill_ml_standard_scaler_destroy(sc, err, sizeof(err));
    }

    /* --- RegressionPipeline --------------------------------------------- */
    {
        void *pipe = rill_ml_regression_pipeline_new(2, 0.05, err, sizeof(err));
        check(pipe != NULL, "regression_pipeline_new");
        double x[2] = {0.1, 0.2};
        int rc = rill_ml_regression_pipeline_learn(pipe, x, 2, 0.5, err, sizeof(err));
        check_rc(rc, "regression_pipeline_learn", err);
        double pred = 0.0;
        rc = rill_ml_regression_pipeline_predict(pipe, x, 2, &pred, err, sizeof(err));
        check_rc(rc, "regression_pipeline_predict", err);
        check(isfinite(pred), "regression pipeline prediction finite");

        uint64_t seen = 0;
        rill_ml_regression_pipeline_samples_seen(pipe, &seen, err, sizeof(err));
        check(seen == 1, "regression pipeline samples_seen == 1");

        rill_ml_regression_pipeline_destroy(pipe, err, sizeof(err));
    }

    /* --- ClassificationPipeline ------------------------------------------ */
    {
        void *pipe = rill_ml_classification_pipeline_new(2, 0.05, err, sizeof(err));
        check(pipe != NULL, "classification_pipeline_new");
        double pos[2] = {0.1, 0.2};
        double neg[2] = {-0.1, -0.2};
        int rc = rill_ml_classification_pipeline_learn(pipe, pos, 2, 1, err, sizeof(err));
        check_rc(rc, "classification_pipeline_learn(1)", err);
        rc = rill_ml_classification_pipeline_learn(pipe, neg, 2, 0, err, sizeof(err));
        check_rc(rc, "classification_pipeline_learn(0)", err);
        double proba = 0.0;
        rc = rill_ml_classification_pipeline_predict_proba(pipe, pos, 2, &proba, err, sizeof(err));
        check_rc(rc, "classification_pipeline_predict_proba", err);
        check(proba >= 0.0 && proba <= 1.0, "classification pipeline proba in [0, 1]");

        rill_ml_classification_pipeline_destroy(pipe, err, sizeof(err));
    }

    /* --- Error-code contract --------------------------------------------- */
    {
        double out = 0.0;
        int rc = rill_ml_mean_value(NULL, &out, err, sizeof(err));
        expect_err(rc, RILL_ML_ERR_INVALID_HANDLE, "NULL handle rejected", err);

        rc = rill_ml_mean_destroy(NULL, err, sizeof(err));
        expect_err(rc, RILL_ML_ERR_INVALID_HANDLE, "destroy(NULL) rejected", err);

        char tiny[1] = {0};
        rc = rill_ml_version(tiny, sizeof(tiny), err, sizeof(err));
        expect_err(rc, RILL_ML_ERR_BUFFER_TOO_SMALL, "undersized version buffer", err);

        rc = rill_ml_mean_from_json("not json", err, sizeof(err)) == NULL ? RILL_ML_OK : -999;
        check(rc == RILL_ML_OK && err[0] != '\0', "malformed JSON rejected with message");
    }

    if (failures == 0) {
        printf("ALL C SMOKE CHECKS PASSED\n");
        return 0;
    }
    printf("%d C SMOKE CHECK(S) FAILED\n", failures);
    return 1;
}

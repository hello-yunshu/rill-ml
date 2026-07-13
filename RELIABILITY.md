# Production Reliability Guide

RillML is a bounded-memory, in-process online-learning library. It can make the
model update path reliable, but it is not a distributed service: replication,
traffic failover, durable storage, and host-process supervision belong to the
embedding application.

## Recommended activation path

1. Validate and cap incoming feature dimensions, snapshot payload sizes, and
   category/feature growth before calling RillML.
2. Keep a simple baseline model active during cold start.
3. Process each sample in `predict -> metric.update -> learn` order.
4. Treat any `RillError`, non-finite upstream value, or unhealthy model report
   as a rejected update. Continue serving from the last valid state.
5. Serialize a versioned `Snapshot<T>` only after the model passes health and
   application checks. Write it atomically in the host application.
6. On restore, deserialize under a payload-size limit, call
   `into_model_with_validation()`, warm the candidate off the serving path, and
   only then swap it into service.
7. Retain at least one last-known-good snapshot for rollback.

## Failure semantics

- Public numeric inputs are checked for `NaN` and infinity.
- Core statistics, regression/classification metrics, SGD, AdaGrad,
  `StandardScaler`, sparse feature merging, and feature hashing reject
  non-finite arithmetic results instead of storing them.
- Regression and classification pipelines provide `learn_transactional()` for
  all-or-nothing updates. It clones both stages, so use it at reliability
  boundaries when the clone cost is acceptable.
- Snapshot versions must match exactly. The envelope does not silently migrate
  model state.
- `Snapshot<T>` cannot infer arbitrary model invariants. Use
  `into_model_with_validation()` at trust boundaries. Several high-risk built-in
  types also validate their own state during deserialization.

## Minimum operational signals

Track these per model and deployment version:

- accepted and rejected sample counts;
- prediction and learning error counts by `RillError` variant;
- snapshot save, load, validation, and rollback outcomes;
- warm-up state and samples seen;
- recent error versus the active baseline;
- model-health failures and non-finite output attempts;
- drift warning/drift events and the action taken;
- p95/p99 prediction and learning latency.

Alert on sustained rejection or load-failure rates, a model that underperforms
its baseline, repeated drift resets, or the absence of a recent validated
snapshot.

## Release gate

Before activation, run the same checks enforced by CI:

```sh
cargo fmt --check
cargo check --all-targets
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps
cargo package
```

Run dependency audits on every dependency change and on a schedule, because a
clean build does not prove that dependencies remain free of newly disclosed
advisories.

# Runtime diagnostics v1

The production qualification surface exposes `rill_runtime::RuntimeDiagnosticsV1`
through `StatefulRuntimeEngineV3::diagnostics_at(now_unix_ms)`. The caller owns
the clock so tests and consumers can make timestamps deterministic.

The payload contains:

- runtime/protocol/channel identity;
- model and state generations;
- health (`healthy`, `resource_pressure`, `failed_closed`);
- bounded resource usage for state, snapshots and decision ledgers;
- rollback/candidate availability and restart count;
- the last fail-closed reason.

Resource limits are enforcement boundaries, not advisory metrics. When an IPC
frame, snapshot or ledger would exceed its configured limit, the operation is
rejected and the previous state remains unchanged. Completed decision history
is never silently evicted, because eviction would turn later duplicate
feedback into an unknown decision.

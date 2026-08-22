# Runtime v3 state lifecycle

The v3 Preview runtime keeps the handler state and the delayed-decision ledger
under one checksum. A successful handler call is the only point at which the
current state generation advances:

```text
candidate → validate → snapshot → activate → observe health → promote
                                      └────── rollback previous-good
```

`StatefulRuntimeSnapshotV3` is written atomically by `preview-serve`. A
truncated file, checksum mismatch, unsupported schema, oversized state, or
ledger capacity violation rejects startup; the previous file is not replaced.
The ledger records pending decisions and completed outcomes, so feedback after
restart is either applied exactly once or rejected with a stable v3 error.

Migration is intentionally copy-on-write. A `StatefulStateMigratorV1` receives
the old schema and returns a new schema plus bytes. The runtime validates the
returned schema and checksum before activation and retains the old snapshot as
`previous-good`. No migration hook may mutate its input in place.

The v3 API remains Preview until candidate and public-release conformance both
pass. Stable v1/v2 state and IPC semantics are not silently redirected to v3.

## Operational boundary

`RuntimeDiagnosticsV1` is the consumer-facing, clock-injected diagnostic
snapshot. It reports health (`healthy`, `resource_pressure`, or
`failed_closed`), reason codes, state/snapshot/ledger usage, generation and
rollback availability. IPC frames, snapshots, state and completed decision
history are hard bounded; saturation rejects the operation and preserves the
prior state. Reset, candidate promotion and rollback clear delayed decisions
whose generation belongs to the replaced state.

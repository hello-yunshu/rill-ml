# Resource Profile v1

`ResourceProfileV1` gives a runtime host finite defaults for IPC frames,
model state, snapshots, package sizes, features, decision history and
diagnostic records. A zero limit, a profile larger than the protocol frame
bound, or a completed-history capacity smaller than pending capacity is
invalid. Hosts may tighten a profile; missing limits are never treated as
unbounded.

The runtime exposes current usage through v3 `inspect` and returns
`capacityExceeded` when a hard bound prevents a state or decision update.
The response includes the current generation and the request remains
side-effect free. WASM fuel, memory and handler I/O remain enforced by the
existing sandbox adapter; this profile is the common host/runtime policy
surface rather than a replacement for those sandbox limits.

The profile is intentionally additive to the frozen v1/v2 wire contracts.

## Enforcement matrix

The fields below are deliberately classified by the actual enforcement point;
declaring a value in a profile is not evidence that the runtime enforces it.

| Field | Enforcement point | Admission/failure | Test/evidence | Status |
| --- | --- | --- | --- | --- |
| `max_ipc_frame_bytes` | v3 JSON entrypoint | before decode / `payloadTooLarge` | malformed and oversized IPC tests | enforced |
| `max_model_state_bytes` | engine initial/restore/next-state validation | before commit / capacity mapping | atomic state-limit test | enforced |
| `max_snapshot_bytes` | prospective runtime snapshot validation | before commit / `capacityExceeded` | snapshot-limit regression | enforced |
| `max_pending_decisions` | decision admission | before handler/state mutation / capacity mapping | resource-cap probe | enforced |
| `max_completed_decisions` | decision and feedback admission | before mutation / capacity mapping | ledger saturation qualification | enforced |
| `max_handler_package_bytes` | signed handler package loader | package load | handler package tests | enforced by loader, not profile-configurable |
| `max_model_pack_bytes` | signed model package loader | package load | model package tests | enforced by loader, not profile-configurable |
| `max_features` | host/model policy | consumer admission | consumer contract | host-enforced |
| `max_diagnostic_records` | diagnostic collection policy | host/collector admission | no runtime collector in v3 | declared/not-runtime-enforced |
| `request_deadline_ms` | envelope deadline supplied by caller | request validation / `expiredRequest` | deadline regression | protocol deadline, profile not wired |
| `shutdown_deadline_ms` | process lifecycle owner | shutdown orchestration | no runtime shutdown controller | host-enforced |
| `snapshot_deadline_ms` | snapshot lifecycle owner | snapshot orchestration | no clock-driven snapshot worker | host-enforced |
| `restart_backoff_ms` | process supervisor | restart policy | no supervisor in runtime | host-enforced |

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

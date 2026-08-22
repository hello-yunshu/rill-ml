# Runtime fault scenario registry

`schemas/runtime-fault-scenarios-v1.json` is the repeatable registry used by
future bounded and extended runs. Each scenario has a stable id, a category,
an explicit expected fail-closed outcome, and a deterministic seed where
randomness is relevant.

The registry covers snapshot and migration interruption, state corruption and
truncation, bad trust/SBOM/release-index identity, missing model/handler,
handler crash/timeout/memory pressure, malformed IPC, slow consumers,
duplicate/late outcomes, disk/read-only state failures, stale locks and
concurrent startup. Registry presence is not execution evidence; the final
1.5 report must link each executed scenario's machine-readable result.

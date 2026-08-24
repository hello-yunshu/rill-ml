# RillML 1.5.1 final closure audit

Date: 2026-08-25

This document records the 1.5.1 patch line separately from the immutable
v1.5.0 release. It must not be used to rewrite the v1.5.0 release evidence.

## Current qualification state

- Stable target: `1.5.1`.
- Preview target: `0.15.0`; IPC v3 remains explicit opt-in Preview.
- Authoritative SemVer baseline: published `1.5.0`.
- Historical sanity baselines: `1.3.0`, `1.2.0`, `1.0.0`.
- Action reference policy follows the current maintainer instruction: readable
  refs are accepted and fixed commit SHAs are rejected by the local gate.

## Published v1.5.0 supplemental audit

The exact `v1.5.0` tag was checked in a detached worktree against the published
`1.3.0` baseline with `cargo-semver-checks 0.49.0`:

- `rill-ml`: PASS, 196 checks, 57 skipped.
- `rill-handler-api`: PASS, 196 checks, 57 skipped.
- `rill-runtime-protocol`: PASS, 194 checks, 59 skipped.
- `rill-runtime`: PASS, 196 checks, 57 skipped.

The audit did not modify the v1.5.0 tag or its public Release assets.

## Implemented 1.5.1 changes

- Final qualification now aggregates every consumer phase, resource-cap probe,
  and requested soak as required checks; a required FAIL exits non-zero even
  when soak is disabled.
- Qualification latency samples use one persistent child-process session per
  phase, correlate each response by request ID, and calculate tails from the
  individual request samples.
- RSS evidence is explicitly labeled as the process-tree child high-water mark
  from `RUSAGE_CHILDREN`, not as independent per-PID/per-cycle RSS.
- Preview capacity and snapshot limits are checked before committing state;
  completed-history saturation rejects new decisions before mutation.
- Preview state ownership uses an OS advisory exclusive lock. Atomic writes use
  a process-unique temporary file, file fsync, rename, and Unix parent-directory
  fsync.

## Evidence boundary

Pre-merge evidence for head `0ce2e80` is complete: CI, Cross-platform
Verification, Security audit, Docs, and Fuzz all completed successfully. The
Cross-platform workflow completed 13/13 jobs, including native FreeBSD and
RISC-V gates; the CI semver job used the published `1.5.0` baseline.

The final local qualification passed all required checks: three simulated
consumer phases, resource-cap probe, 10-cycle bounded soak, and continuous
same-state saturation. The default profile reached its effective snapshot
boundary at 2,664 completed decisions, returned `capacityExceeded` without a
generation change, and retained the ledger after restart. The deterministic
fault smoke passed 6/20 registry scenarios: corrupted/truncated state,
malformed IPC, concurrent startup, stale lock, and readonly state directory.
The other 14 registry scenarios remain explicitly `NOT_RUN`.

Action references use readable names/tags; fixed commit references are rejected
by the local Action-ref gate. Simulated consumer evidence remains `SIMULATED`,
not real PM/Xray/Agent integration; build-only or cross-build results remain
distinct from native execution. Merge, the v1.5.1 tag, public Release, and
post-release asset verification remain `NOT_RUN` until those exact states are
observed.

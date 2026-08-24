# RillML 1.5.0 final audit

Date: 2026-08-23

## Release decision

- RillML 1.3.0: **RELEASE READY**.
- RillML 1.5.0: **RELEASE READY for the published Stable 1.5.0 artifact**.
- Runtime IPC v3 remains **Preview**, not Stable. Stable v1/v2 compatibility is
  preserved and the v3 executable requires the explicit `preview-serve` opt-in.

The release decision is based on the remote release workflow and its
post-release asset verification. Simulated consumer and soak output below is
additional audit evidence and is never treated as native or external-consumer
evidence.

## 1.3.0 formal release

- Main merge SHA: `dbf1011832a50a54ab47688a626794abb6052d8f`.
- Immutable tag target: `a0baf438d85cf4c7e6cc50cc82d68803794fda0e`.
- Release: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.3.0
- Official same-tag workflow: https://github.com/hello-yunshu/rill-ml/actions/runs/32575090473
- Workflow conclusion: success.
- Verified outputs included runtime/model/handler/PM assets, checksums,
  signatures, CycloneDX and SPDX SBOMs, stable index, host smoke, FreeBSD VM,
  QEMU and native Windows/ARM verification.

## 1.4 and 1.5 development convergence

- PR #29 (1.4 Preview stateful surface):
  https://github.com/hello-yunshu/rill-ml/pull/29
  merged as `1b54115217e36d79f37b1338f5d558a7900f86ed`; CI, Cross, Docs,
  Security, Fuzz, API baseline and semver gates passed.
- PR #30 (production state qualification):
  https://github.com/hello-yunshu/rill-ml/pull/30
  merged as `aa490096d8bb6ec1c4d82f0bb75e6a8c32e79442`; final normal Actions:
  CI `32581581474`, Cross `32581581463`, Docs `32581581468`, Security
  `32581581461`, Fuzz `32581581466`.
- PR #31 (1.5 version/release convergence):
  https://github.com/hello-yunshu/rill-ml/pull/31
  merged as `4994e9b9f08a9618a34e9e9bb66b31f2be521a04`; final normal Actions:
  CI `32582993270`, Cross `32582993305`, Docs `32582993307`, Security
  `32582993291`.

The resulting main SHA contains the 1.5.0 version convergence, additive
diagnostics API, state snapshot/ledger bounds, atomic persistence, migration,
candidate/rollback paths, fail-closed resource checks, real child-process
qualification and fault harnesses.

## 1.5.0 official release

- Tag: `v1.5.0`.
- Tag target / release SHA: `4994e9b9f08a9618a34e9e9bb66b31f2be521a04`.
- Release: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.5.0
- Main push gates: CI `32583997378`, Security `32583997403`, Docs
  `32583997389`, Cross `32583997404`.
- Auto Release tag/dispatch: `32584588581`.
- Official release workflow: https://github.com/hello-yunshu/rill-ml/actions/runs/32584736523
- Final conclusion: success; release admission, package dry-run, signing,
  published Stable crates, model/handler packages, signed index, CycloneDX,
  SPDX, public release, host smoke, native Windows/FreeBSD, QEMU and all
  released-asset verification jobs passed.

## Candidate and final simulated qualification

Candidate artifact, local only:

- Binary version: `rill-runtime 1.5.0`.
- Candidate commit supplied and checked: `4994e9b9f08a9618a34e9e9bb66b31f2be521a04`.
- Binary SHA-256: `f0631b1b7463c7c9c3fa9b858073251d9badd7e65e012e6471bf09c4da40ed58`.
- `conformance/run.py --mode candidate`: PASS.

Post-push simulated consumer qualification, local child processes only:

- Three simulated modes, 1,000 observations per mode, batch size 128: PASS.
- Decision latency seconds/observation: P50 `0.0055152`, P95 `0.0065307`,
  P99 `0.0066005`.
- Feedback latency seconds/observation: P50 `0.0071813`, P95 `0.0075107`,
  P99 `0.0075963`.
- Cold start, restore, snapshot, inspect, state-file size and child RSS were
  collected; pending cap probe accepted 1,024 and rejected the next request
  without exceeding the cap.
- Deterministic fault smoke: PASS for corrupted state, truncated state and
  malformed IPC; registry contained 20 scenarios.

Bounded soak, simulated `simulated-consumer` mode only:

- 10 cycles x 1,000 observations = 10,000 observations: PASS.
- State file: 224,110 bytes in every cycle; child RSS maximum: 5,029,888
  bytes.
- Decision P50/P95/P99: `0.0060515` / `0.0069534` / `0.0070190` seconds per
  observation.
- Feedback P50/P95/P99: `0.0071877` / `0.0074133` / `0.0074161` seconds per
  observation.

These are `evidenceType: simulated` results. They do not prove a real PM,
Xray, Agent, iOS or other external consumer, and they do not prove native
hardware behavior.

## Platform and compatibility boundary

- Native GitHub runner verification: Linux, macOS ARM, Windows x86/ARM.
- Native VM verification: FreeBSD x86_64.
- QEMU execution verification: riscv64 and loongarch64.
- Cross/released-asset verification: aarch64, armv7, s390x, powerpc64le,
  musl and other declared targets.
- Build-only or cross-only results are not relabeled as native support.
- Stable API/protocol v1/v2 remains frozen; additive diagnostics and Preview
  v3 types do not replace the frozen public response types.
- Preview state v3 is opt-in and remains separate from the Stable serve path.

## Security, SBOM and trust

The official 1.5.0 release workflow generated and verified signed model and
handler packages, a signed release index, CycloneDX and SPDX SBOMs, checksums,
and release-bound identity metadata before publishing. The post-release jobs
re-downloaded the public assets and re-verified their identity and runtime
smoke behavior. The 1.3.0 release had the same verified asset/SBOM boundary.

## Remaining risks and handoff boundary

1. No external consumer repository or real downstream credentials were in
   scope; simulated consumer qualification is not an integration certification.
2. No iOS native runtime or physical-device qualification was claimed.
3. IPC v3 is Preview and must not be described as Stable v3 without a separate
   stable-admission decision.
4. The post-simulation runner/doc refinement was pushed in PR #33 at the time
   of this audit (`06b8649`). It is subsequent audit tooling and does not alter
   the immutable v1.5.0 tag or its public assets. Runtime behavior changes from
   the follow-up work belong to the separate 1.5.1 line.

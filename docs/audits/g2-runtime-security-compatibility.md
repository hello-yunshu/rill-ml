# G2 Runtime security and compatibility qualification

**G2 local security/compatibility gate: GO for G3 development.**

G3 follow-up (2026-10-16): fresh registry-index inspection reports the locked
`zip 2.4.2` as yanked. The original G2 manifest below is a historical snapshot,
not the identity of the subsequent candidate. The ZIP dependency replacement,
archive API adaptation, fresh audit, and Rust 1.96 source regressions are recorded
in `g3-archive-evidence/manifest.json`. Package qualification must bind that new
lock and the complete ordered patch set; old G2/package evidence cannot qualify
the changed bytes. OpenWrt rebuild and downstream qualification remain separate.

Local candidate audit, 2026-10-15. This document does not authorize publication,
change an installed runtime, or qualify an OpenWrt package/feed.

## Source and channel boundary

- Preview candidate base: `da8389fec7f879b826d8d17cbc6bb98c03ef8462` on
  `codex/openwrt-rill-partition`, plus the source hashes in `g2-security-evidence/manifest.json`.
- Historical Stable baseline: `v1.5.6`,
  `b990cd7043d313b0ff29c9693f091a94a5bdaf47`. Its tag and assets are untouched.
  A separate temporary archive of this commit was patched with exactly
  `g2-security-evidence/stable-dependency.patch` and independently built/tested.
  It is a compatibility proof for the backport, not a published replacement.
- Observed remote main: `9602750202f56ed860fc8815bb91e81083d0ffdb`.
  No merge of main into the Preview candidate was needed.
- Stable and Preview have distinct v3 wire/state contracts. Preview requires
  `partitionKey`; Stable consumers must not be silently moved to Preview.
  Frozen v1 component bytes and all four public API baseline files are unchanged.

## Security change and triggering conditions

Wasmtime 46.0.3 is replaced with 49.0.3 (including its resolved engine/compiler
family). Default WASM, typed Component Model handlers, signatures, fuel, epoch
and memory limits remain enabled. No advisory is ignored and no audit failure
is suppressed. Native permission to write UCI/proxy configuration or execute
host commands is not added.

The before lock contains twelve findings: RUSTSEC-2026-0316, 0327, 0353, 0354,
0355, 0356, 0357, 0358, 0359, 0361, 0362, 0363. The final audit has zero findings
and zero warnings against RustSec database commit
`edb58f5974b83eb8302332821cb6d389777aa351`, updated
`2026-10-14T22:18:32+02:00` (1329 advisories).

The async component feature is absent in resolved default target trees. Rill
uses typed handler bindgen; a dependency finding alone does not demonstrate
that every advisory is exploitable through those adapters. The upgrade removes
the findings rather than relying on that narrower reachability observation.
The CI profile rejects an unreviewed async feature union or removal of full
WASM support. Real engine tests reject valid async components and unresolved
filesystem host functions; no WASI linker is installed.

Sources: [RustSec async callback advisory](https://rustsec.org/advisories/RUSTSEC-2026-0327.html),
[dynamic value lifting advisory](https://rustsec.org/advisories/RUSTSEC-2026-0316.html).
The machine-readable before/final reports retain every advisory and patched range.

## Compatibility and local evidence

| Gate | Evidence and boundary |
|---|---|
| Default feature union | x86_64 musl, ARM64 musl, RISC-V musl, ARM64 OHOS, ARM64 macOS retain full WASM; async absent. Feature resolution is not target execution. |
| Core MSRV | Rust 1.94.0, libraries/protocol/handler API and explicit core-only runtime check passed. |
| Full runtime MSRV | Rust 1.96.0, default and explicit WASM checks passed. Downstream SDK compiler must change before adopting the candidate. |
| Preview release tests | 1183 passed, zero failed/ignored across 30 result groups, with four freshly compiled real WASM fixtures. |
| Stable patched archive | 173 passed, zero failed/ignored across 14 result groups; independent target directory and all real WASM fixtures. |
| Frozen v1/v2 and signatures | Includes immutable v1 component loading, signed model/handler/archive rejection paths, schema/state/generation validation, trap/fuel/epoch/output limits and ticker cleanup. See complete test logs. |
| Preview v3 | Includes partition/client isolation, delayed-feedback lineage, restart persistence and rejection paths. |
| Public API | Four frozen baseline files unchanged. Upgrade/original source API outputs match. Rustdoc renders two Read re-export paths differently; only that exact alias is normalized, with negative tests preserving real drift rejection. |
| Actual consumers | CF feedback and full lifecycle passed on the native release binary and Linux x86_64 musl binary. PM original fixture passes against independently built patched Stable. |
| PM Preview impact | Original PM fixture fails against both original and patched Preview because partitionKey is missing. An in-memory diagnostic probe adding only partitionKey passes; PM source remains untouched. This probe is not qualification of the unchanged PM consumer. |
| Linux target binaries | x86_64 and ARM64 musl build and actual signed handler handshake/health/invoke pass in Linux containers. RISC-V musl builds; the former missing REG_* compile failure is not reproduced. RISC-V execution is not claimed. |
| Fault smoke | 6/20 scenarios executed and passed. Its JSON explicitly lists the other 14 unexecuted registry scenarios; other unit tests are not relabeled as this harness coverage. |
| Python/workflow | 137 Python tests, actionlint, rustfmt and diff checks; final evidence is recorded with source/artifact digests. |

## Qualification harness repairs

The original real-process qualification failed before negotiation because its
request builder omitted partitionKey. After fixing that identity, continuous
learning exposed another fixture defect: feedback always named route-a even
when the actual selected action changed. The runtime correctly returned
stateMismatch. Feedback now binds the actual response decisionId and
selectedActionId. Capacity rejection, unchanged generation, restart and
ledger convergence assertions are retained. Before failures are preserved.

## Bounded load, resource delta and identity

The final bounded qualification passes all five required checks: three synthetic
consumer modes at 1000 observations each (3000 decisions and 3000 feedbacks),
pending-ledger capacity rejection, and continuous same-state saturation followed
by restart. These are simulated consumers using the real release process,
not PM production or physical soak. The continuous test now reaches its actual
capacity boundary rather than failing on incorrect fixture feedback.

Using identical signed model/handler bytes and the same three v2 requests,
macOS default-feature binaries before/after are 15,787,824 / 15,977,664 bytes
(+1.20%). `/usr/bin/time -l` reports maximum RSS 18,137,088 / 20,103,168 bytes
for one short smoke. This is a cost observation, not a long-run memory bound
or a router measurement. WASM hard limits are separately exercised by tests.

CycloneDX and SPDX inventories include the resolved dependency graph and
candidate binary hashes. Their local tag labels are deliberately unpublished;
commit fields identify the base, while the manifest binds the modified source.
They must be regenerated for the eventual immutable source and release identity.

## Delivery and G3 handoff

- No push, PR, remote workflow dispatch, tag, Release, router install, feed/pin
  change, or proxy configuration write was performed.
- Source/build identity is a local dirty candidate, identified by full file and
  binary hashes. It is not a new immutable release commit or hosted CI run.
- G3 must raise the full-runtime OpenWrt Rust toolchain to at least 1.96,
  qualify the exact new source/archive and both package channels, preserve
  sync/qualification metadata, and bind all artifacts to the eventual commit.
- PM keeps its compatible Stable contract. Adopting Preview requires an
  explicit partition/state migration and consumer qualification; this round
  does not weaken partition checks to accept old requests.
- Hosted full platform release CI, non-executed platforms, router resources,
  installation/upgrade and physical soak remain OPEN for their later gates.
  Container/native proofs do not establish physical OpenWrt acceptance.
- Rollback: retain the candidate and evidence, leave production pins/tags as-is;
  do not restore the vulnerable dependency as a supposed security-approved release.

# OpenWrt Consumer Qualification

This document defines how an OpenWrt consumer can qualify RillML without
turning product-specific host control into RillML ownership.

## Surface and libc contract

- `musl` is the expected libc for OpenWrt packages. A GNU asset must never be
  selected as a musl fallback, and the signed release index records the libc
  variant explicitly.
- Core qualification covers the RillML library: build, target-architecture
  execution, stable state fixtures, snapshot, drift, decision, malformed-state
  rejection, capacity bounds, and cross-architecture serialization.
- Full Runtime qualification additionally requires the default WASM feature,
  signed model and handler load/invoke, sandbox and capability checks, stable
  IPC, release asset identity, and post-release execution of the downloaded
  asset.
- A simulated consumer is not OpenWrt qualification. Evidence must identify the
  consumer repository, RillML version, target triple, libc, artifact URL/hash,
  and whether execution was on a real device, VM/QEMU, or a host simulation.

## Consumer-owned boundary

RillML owns generic Core, Runtime, protocol, signed model/handler, platform
assets, resource profiles, and qualification harnesses. The OpenWrt consumer
owns observation schema, actions, UCI/firewall/configuration mutations,
transactions, rollback, health validation, reward/outcome logic, and product
specific adapter code. Rill Runtime must not directly mutate OpenWrt host
state.

## Required OpenWrt evidence

Qualification is read-only or shadow-first before mutation is enabled and must
record:

- target mapping and `libc=musl`;
- process memory and startup/invoke ceilings from the resource profile;
- bounded batch and long-run smoke results;
- signed release artifact and stable-index binding;
- crash, timeout, invalid-handler, and fallback behavior;
- state-file lifecycle, flash/write budget, and sysupgrade handling;
- 32-bit width and endian fixture results where applicable;
- rollback and health-check results for every host mutation.

The current RillML repository contains no qualified external OpenWrt consumer
for the new musl targets. Until a consumer supplies the evidence above, the
Consumer-qualified column remains `Not listed` even when Core or Full Runtime
has independently passed.

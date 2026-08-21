# RillML Consumer Conformance Kit

This kit is reusable by downstream consumers and has no secret or registry
dependency.

```bash
python3 conformance/run.py --mode offline --json
```

Offline mode is deterministic and checks the signed-index shape, HTTPS/target
selection, unique artifact identity, SHA-256/size fields, and the Stable IPC
v2 positive and negative case inventory. It reports only `PASS`, `FAIL`,
`BLOCKED`, or `NOT_RUN`.

The released-artifact mode delegates to
`smoke-test/host_smoke.py`, which downloads a signed index and signed model and
handler packs, verifies hashes/signatures with `rill-pack`, launches the real
runtime process, and checks v2 Handshake/Health/Invoke, malformed JSON, and
clean shutdown. It must be run with an explicit release index URL and version;
it never silently treats a missing external artifact as a pass.

The fixtures are test data, not consumer adoption claims. Current
tested/supported/pinned status belongs in the consumer's own adoption manifest.

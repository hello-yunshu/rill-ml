# Consumer qualification harness

`scripts/run_runtime_qualification.py` is a bounded, deterministic smoke
harness for the opt-in Preview runtime. It exercises a real child process,
handshake, decision, atomic state persistence, restart, delayed feedback and
duplicate-feedback rejection. Its JSON output is an audit input, not a claim
of real PM, Xray, Agent, or other downstream integration.

The eventual 1.5 qualification matrix has three distinct evidence classes:

1. repository-owned simulated consumer;
2. PM-style and network/Xray-style simulated consumers;
3. real external consumer repositories, when those repositories and their
   credentials are available.

The harness keeps those classes separate in its result schema. Full synthetic
load, extended soak and fault-injection runs are deliberately post-push
activities and must not replace normal Actions or public-asset verification.

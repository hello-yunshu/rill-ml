# DecisionLedger v1

`DecisionLedger<C, A>` is a bounded, caller-clocked primitive for delayed
feedback. It records immutable decision facts and applies an outcome only when
the identifier, action, generation, and time window match.

## Contract

- `max_pending` and `max_completed` are explicit hard limits. The ledger never
  silently evicts live decisions or completed tombstones.
- `created_at <= observed_at <= expires_at` is required for accepted feedback;
  the clock is supplied by the caller and is not read from the host.
- `model_generation` binds feedback to the model generation that made the
  decision. A mismatch is rejected without mutation.
- `DecisionId` is idempotent only for an exact replay. Reusing an identifier
  with different immutable facts is a conflict.
- Rewards must be finite, and context/action values must satisfy their
  configured byte bounds and type-specific validation.
- Feedback validation completes before the pending entry moves to the bounded
  completed tombstone set, so rejected operations are failure-atomic.

## Replay and cleanup

An exact registration replay returns `PendingReplay` or `CompletedReplay`.
Duplicate feedback is rejected. Cleanup is explicit through
`clear_expired(now)` and `clear_completed_before(before)`; callers must choose
retention policy and persist the state with the versioned `serde` envelope.

`DecisionReplayHarness` adds feature-schema and model-generation checks around
the generic ledger. The harness is still caller-clocked and deterministic; it
does not infer product semantics or hide capacity exhaustion.

The implementation and negative tests in `src/decision/mod.rs` are the
authoritative executable contract for bounds, replay, rollback-on-error,
expiration, action mismatch, generation mismatch, and state validation.

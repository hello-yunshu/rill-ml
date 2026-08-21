# Trust key lifecycle v1

RillML 1.3 keeps the Stable release-index payload at schema v3. Existing v1.2
consumers may continue verifying `SignedReleaseIndex` with their existing
multi-key `TrustStore`. Key rotation and rollback protection are an explicit
opt-in reader, represented by `TrustMetadataV1` and
`SignedReleaseIndexWithGenerationV1`.

## Metadata contract

`TrustMetadataV1` contains:

- one or more `Current`/`Next` Ed25519 public keys;
- `notBeforeUnixMs` and optional `notAfterUnixMs` validity bounds;
- timestamp revocation and an emergency revocation bit;
- `minimumReleaseGeneration`, a consumer-owned monotonic rollback floor.

The metadata itself must be authenticated by the consumer's existing trust
root or secure update channel. RillML does not store private keys or decide
which consumer is authoritative.

## Verification rules

`verify_release_index_with_trust_metadata` first rejects an unknown metadata
schema, duplicate/damaged keys, future/expired/revoked keys, and a release
generation below the rollback floor. It then verifies the unchanged signed v3
index using every currently active `Current`/`Next` key. During an overlap
window both keys are accepted; after `notAfterUnixMs` or revocation, the old
key is rejected. Unknown publishers and bad signatures remain fail-closed.

## Consumer migration

1. Keep the existing v3 `stable-index.json` reader and trust root.
2. Publish trust metadata with a new `Next` key before its `notBeforeUnixMs`.
3. During overlap, accept both current and next keys and advance the local
   metadata generation/rollback floor atomically.
4. Sign a `SignedReleaseIndexWithGenerationV1` envelope for consumers that
   enforce the lifecycle policy.
5. After all qualified consumers migrate, revoke or expire the old key and
   keep the old key only for historical verification, never for new releases.

Consumers that do not opt into the lifecycle envelope are not silently treated
as migrated. The v1.3 release process must report that boundary explicitly.

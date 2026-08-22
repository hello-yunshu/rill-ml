# FTRL resource diagnostics

FTRL stores one parameter pair per distinct feature identifier. The default
`max_features = None` remains backward-compatible and allows growth with the
observed feature vocabulary; services consuming untrusted or open-ended input
should set a finite cap.

`FtrlRegressor::resource_diagnostics()` and
`FtrlClassifier::resource_diagnostics()` expose:

- `current_features` — the number of stored feature identifiers;
- `configured_max` — the configured cap, if any;
- `saturation` — `current_features / configured_max` for a finite cap;
- `new_features_rejected` — whether the current size would reject an unseen
  feature under `NewFeaturePolicy::Reject`.

The diagnostics are observational: reading them does not mutate the model or
reserve memory. The admission policy is still enforced by `learn`; `Reject`
keeps the call failure-atomic, while `Ignore` updates existing features and
skips unseen features beyond the cap.

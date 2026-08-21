# Fuzz seed corpus

The corpus is intentionally small and checked in so the bounded CI smoke is
offline and deterministic. `cargo fuzz` may grow per-target corpus files in a
scheduled job; those generated artifacts are retained by CI and are not used
as release evidence until a reproducible crash is minimized and reviewed.

#!/usr/bin/env python3
"""Pure-Python policy decision for the auto-release tag-and-dispatch step.

The auto-release workflow (`/.github/workflows/auto-release.yml`) needs to
decide, after a successful CI run on ``main``, whether to:

* create a new version tag at the CI head SHA and dispatch the Release
  workflow;
* safely re-release an existing tag whose SHA already matches the CI head
  (immutable-tag safe retry when a previous Release run failed);
* skip dispatch because the tag already has an active Release run (avoid
  queuing redundant work);
* fail loudly because the tag exists but its SHA could not be resolved,
  OR because the tag already exists at a *different* SHA.

Formal release tags are IMMUTABLE: a tag may be created, or safely retried
at the identical SHA, or rejected. It may never be force-moved
(no ``git tag --force`` / ``git push --force``). A tag that already exists
at a different commit than the current CI head is a HARD FAIL: the SHA that
CI ran on no longer matches the version tag, so publishing would bind the
wrong commit to that release.

The decision used to live entirely inside a bash ``run:`` block, which made
it impossible to unit-test the policy. This module exposes the same decision
as a pure function so the workflow can call it via ``python3
scripts/release_tag_policy.py`` and the test-suite can cover every branch.

Inputs are the four observable facts the workflow gathers before deciding:

* ``tag_exists`` — does ``refs/tags/$TAG`` already exist?
* ``tag_sha`` — the commit SHA the existing tag points at (only meaningful
  when ``tag_exists`` is true; pass ``None`` otherwise).
* ``target_sha`` — the SHA of the successful CI run on ``main``.
* ``has_successful_release`` — is there already a successful Release
  workflow run titled ``Release $TAG``?
* ``has_active_release`` — is there an in-flight Release workflow run
  titled ``Release $TAG``?

The output is a :class:`Decision` describing what the workflow should do.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class Decision:
    """What the auto-release workflow should do for a given tag."""

    action: str
    reason: str

    def as_github_output(self, github_output: Optional[str]) -> None:
        """Write ``dispatch=<true|false>`` to ``$GITHUB_OUTPUT`` if requested."""
        if github_output is None:
            return
        dispatch = "true" if self.action == "dispatch" else "false"
        with open(github_output, "a", encoding="utf-8") as handle:
            handle.write(f"dispatch={dispatch}\n")


def decide_release_tag(
    *,
    tag_exists: bool,
    tag_sha: Optional[str],
    target_sha: str,
    has_successful_release: bool,
    has_active_release: bool,
) -> Decision:
    """Apply the overwrite policy and return the resulting :class:`Decision`."""
    if not target_sha:
        # Without a CI head SHA we cannot create or validate any tag. This
        # should never happen in practice (the workflow only runs when CI
        # succeeded on a concrete commit) but defending in depth keeps the
        # helper safe to call from tests.
        return Decision("fail", "target SHA is empty")

    if tag_exists:
        if tag_sha is None:
            # Defensive: the workflow always resolves ``tag_sha`` before
            # calling us. Treat a missing SHA as a hard failure rather than
            # guessing it matches.
            return Decision(
                "fail",
                "tag exists but its SHA could not be resolved; refusing to retry",
            )
        # If a Release run is already in-flight for this tag, skip to avoid
        # queuing redundant work. The active run will complete and produce
        # the release (release tags are immutable, so the active run is bound
        # to the same version).
        if has_active_release:
            return Decision(
                "skip",
                "tag already has an active Release run",
            )
        # Immutable-tag policy: a tag pointing at a DIFFERENT commit than the
        # current CI head must never be force-moved. The SHA that CI passed on
        # no longer matches the version tag, so publishing would silently bind
        # the wrong commit to this release. This is a HARD FAIL, not an
        # overwrite: the inability to re-point the tag means the intended
        # release (on the current head) cannot be produced without a new
        # version, so the release must be blocked.
        if tag_sha != target_sha:
            return Decision(
                "fail",
                f"tag already exists at {tag_sha}, which differs from CI head {target_sha}; "
                f"immutable release tags cannot move — use a new version",
            )
        # Same-SHA safe retry: the tag is already at the CI head. Dispatch so
        # the Release job can (re)run against the same commit and publish.
        return Decision(
            "dispatch",
            f"re-releasing tag at {tag_sha} (same-SHA safe retry)",
        )

    # Brand-new tag: the workflow will create it at target_sha and dispatch.
    return Decision(
        "dispatch",
        f"creating new tag at {target_sha}",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag-exists", action="store_true")
    parser.add_argument("--tag-sha", default=None)
    parser.add_argument("--target-sha", required=True)
    parser.add_argument("--has-successful-release", action="store_true")
    parser.add_argument("--has-active-release", action="store_true")
    parser.add_argument("--github-output", default=None)
    parser.add_argument(
        "--json",
        action="store_true",
        help="emit the decision as JSON on stdout (for tests and CI logs)",
    )
    args = parser.parse_args()

    decision = decide_release_tag(
        tag_exists=args.tag_exists,
        tag_sha=args.tag_sha,
        target_sha=args.target_sha,
        has_successful_release=args.has_successful_release,
        has_active_release=args.has_active_release,
    )

    if decision.action == "fail":
        print(f"::error::{decision.reason}", file=sys.stderr)

    if args.json:
        print(json.dumps({"action": decision.action, "reason": decision.reason}))
    else:
        print(decision.reason)

    decision.as_github_output(args.github_output)
    return 0 if decision.action != "fail" else 1


if __name__ == "__main__":
    raise SystemExit(main())

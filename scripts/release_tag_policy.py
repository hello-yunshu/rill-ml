#!/usr/bin/env python3
"""Pure-Python policy decision for the auto-release tag-and-dispatch step.

The auto-release workflow (`/.github/workflows/auto-release.yml`) needs to
decide, after a successful CI run on ``main``, whether to:

* create a new immutable version tag at the CI head SHA and dispatch the
  Release workflow;
* safely retry an existing tag whose SHA matches the CI head SHA;
* skip dispatch because the tag already has a successful Release run;
* skip dispatch because the tag already has an active Release run;
* fail loudly because the existing tag points at a different SHA (version
  tags are immutable and must not be moved).

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
    """Apply the immutability policy and return the resulting :class:`Decision`."""
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
        if has_successful_release:
            return Decision(
                "skip",
                "tag already has a successful Release run",
            )
        if has_active_release:
            return Decision(
                "skip",
                "tag already has an active Release run",
            )
        if tag_sha != target_sha:
            return Decision(
                "fail",
                (
                    f"version tag exists at {tag_sha} but the successful CI "
                    f"head is {target_sha}; version tags are immutable and "
                    "cannot be moved — bump the version number in Cargo.toml"
                ),
            )
        return Decision(
            "dispatch",
            f"retrying immutable tag at {tag_sha} with the current Release workflow",
        )

    # Brand-new tag: the workflow will create it at target_sha and dispatch.
    return Decision(
        "dispatch",
        f"creating new immutable tag at {target_sha}",
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

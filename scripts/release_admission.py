#!/usr/bin/env python3
"""Pure-Python release admission gate for the ``CI / Release`` pipeline.

The ``release-admission`` job runs at the root of the release tree on every
``workflow_dispatch`` (manual dispatch included), so the pipeline itself is the
final authority — it does not rely on ``auto-release.yml`` (which a human can
bypass by dispatching ``workflow_dispatch`` directly).

It enforces, for the exact tag commit SHA (never "latest success"):

* a successful ``CI / Release`` push run on that SHA;
* a successful ``Security audit`` run on that SHA;
* a successful ``Cross-Platform Verification`` run on that SHA whenever the
  commit touched cross-platform-relevant paths (Cargo/source/crates/scripts/
  workflows/release config). Only a genuinely docs-only commit may admit a
  missing Cross-Platform run (see the execution prompt §26);
* no *successful* Release run already exists for the tag (successful release
  assets are immutable — a re-dispatch must never mutate them, §29/§30);
* no *active* Release run is already in flight for the tag (avoid racing a
  concurrent release).

Any gate that is missing, queued, in_progress, failed, cancelled or timed out
where success is required is a HARD FAIL of the admission (execution prompt
§25), which fails the job and therefore blocks the whole release tree.

The decision logic is a pure function so ``scripts/tests/test_release_admission.py``
can cover every branch without mocking GitHub; the ``main()`` wrapper only
gathers the observable facts through ``gh``.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass, field
from typing import Optional

# Path prefixes that make a commit "cross-platform-relevant". This is the union
# of the Cross-Platform workflow push trigger (cross-platform.yml) and the
# execution-prompt §26 list: any of these changing while the Cross-Platform run
# is missing is a HARD FAIL, never a guessed "doc-only".
CROSS_PLATFORM_RELEVANT_PREFIXES = (
    "Cargo.toml",
    "Cargo.lock",
    "src/",
    "crates/",
    "handlers/",
    "models/",
    "scripts/",
    "smoke-test/",
    "Dockerfile",
    ".github/workflows/",
    "release-plan.toml",
)


@dataclass(frozen=True)
class AdmissionDecision:
    """Whether a release for the tag is admitted, and why."""

    admitted: bool
    reasons: list[str] = field(default_factory=list)


def classify_cross_platform_relevance(changed_files: list[str]) -> bool:
    """Return True if any changed file is cross-platform relevant (§26)."""
    for path in changed_files:
        for prefix in CROSS_PLATFORM_RELEVANT_PREFIXES:
            if path == prefix or path.startswith(prefix):
                return True
    return False


# A gate status is "success" only when a completed+success run exists for the
# exact SHA; "missing" when no run exists at all; "failed" when only failed /
# cancelled / timed_out runs exist; "in_progress" when a queued/in_progress run
# exists but nothing has completed yet (success or failure).
def _aggregate_run_status(runs: list[dict]) -> str:
    if not runs:
        return "missing"
    if any(r["conclusion"] == "success" for r in runs):
        return "success"
    if any(r["status"] in ("queued", "in_progress") for r in runs):
        return "in_progress"
    return "failed"


def evaluate_admission(
    *,
    tag_sha: str,
    ci_status: str,
    cross_status: str,
    cross_relevant: bool,
    security_status: str,
    has_successful_release: bool,
    has_active_release: bool,
) -> AdmissionDecision:
    """Apply the admission policy and return the resulting decision."""
    reasons: list[str] = []

    if has_successful_release:
        reasons.append(
            "a successful Release already exists for this tag; successful release "
            "assets are immutable and must not be mutated — use a new version"
        )
    if has_active_release:
        reasons.append(
            "a Release run is already active for this tag; refusing to run a "
            "concurrent release"
        )

    # Same-SHA push CI gate.
    if ci_status != "success":
        reasons.append(
            f"CI / Release push gate on {tag_sha} is {ci_status} (must be completed+success)"
        )

    # Same-SHA Security audit gate.
    if security_status != "success":
        reasons.append(
            f"Security audit gate on {tag_sha} is {security_status} (must be completed+success)"
        )

    # Cross-Platform Verification gate. A missing run is only tolerated for a
    # genuinely docs-only commit; never guessed.
    if cross_relevant:
        if cross_status != "success":
            reasons.append(
                f"Cross-Platform Verification gate on {tag_sha} is {cross_status} "
                "(must be completed+success; the commit touched cross-platform-relevant paths)"
            )
    else:
        if cross_status == "missing":
            pass  # docs-only: a Cross-Platform run is not required
        elif cross_status != "success":
            reasons.append(
                f"Cross-Platform Verification run on {tag_sha} is {cross_status}"
            )

    return AdmissionDecision(admitted=not reasons, reasons=reasons)


def _gh_run_list(workflow: str, fields: list[str], limit: int = 100) -> list[dict]:
    """Query ``gh run list`` and parse the JSON result."""
    cmd = [
        "gh", "run", "list",
        "--workflow", workflow,
        "--json", ",".join(fields),
        "--limit", str(limit),
    ]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return json.loads(out.stdout)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True, help="release tag, e.g. v1.2.0-rc.2")
    parser.add_argument("--tag-sha", required=True, help="commit SHA the tag points at")
    parser.add_argument(
        "--current-run-id",
        default=None,
        help="the pipeline run invoking this gate (excluded from active-release checks)",
    )
    parser.add_argument("--github-output", default=None)
    parser.add_argument(
        "--json",
        action="store_true",
        help="emit the decision as JSON on stdout (for tests and CI logs)",
    )
    args = parser.parse_args()

    repo = subprocess.run(
        ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()

    # Files changed by the tag commit — used to decide cross-platform relevance
    # (§26: never guess "doc-only").
    files = subprocess.run(
        ["gh", "api", f"repos/{repo}/commits/{args.tag_sha}",
         "--jq", ".files[].filename"],
        check=True, capture_output=True, text=True,
    ).stdout.splitlines()
    cross_relevant = classify_cross_platform_relevance(files)

    # Same-SHA runs for the three mandatory gates.
    ci_runs = [
        r for r in _gh_run_list("CI / Release", ["headSha", "event", "status", "conclusion"])
        if r.get("event") == "push" and r["headSha"] == args.tag_sha
    ]
    cross_runs = [
        r for r in _gh_run_list("Cross-Platform Verification", ["headSha", "status", "conclusion"])
        if r["headSha"] == args.tag_sha
    ]
    security_runs = [
        r for r in _gh_run_list("Security audit", ["headSha", "status", "conclusion"])
        if r["headSha"] == args.tag_sha
    ]

    # Prior Release runs for this tag (immutability / concurrency checks).
    release_runs = [
        r for r in _gh_run_list(
            "CI / Release", ["displayTitle", "status", "conclusion", "databaseId"]
        )
        if r["displayTitle"] == f"Release {args.tag}"
    ]
    current_run_id = args.current_run_id
    successful_release = any(
        r["conclusion"] == "success"
        and (current_run_id is None or str(r.get("databaseId")) != current_run_id)
        for r in release_runs
    )
    active_release = any(
        r["status"] in ("queued", "in_progress")
        and (current_run_id is None or str(r.get("databaseId")) != current_run_id)
        for r in release_runs
    )

    decision = evaluate_admission(
        tag_sha=args.tag_sha,
        ci_status=_aggregate_run_status(ci_runs),
        cross_status=_aggregate_run_status(cross_runs),
        cross_relevant=cross_relevant,
        security_status=_aggregate_run_status(security_runs),
        has_successful_release=successful_release,
        has_active_release=active_release,
    )

    if args.json:
        print(json.dumps({
            "admitted": decision.admitted,
            "reasons": decision.reasons,
            "cross_relevant": cross_relevant,
        }))
    else:
        for reason in decision.reasons:
            print(f"::error::{reason}", file=sys.stderr)
        if decision.admitted:
            print(f"Release {args.tag} admitted at {args.tag_sha} (CI/cross-platform/security same-SHA gates passed)")

    if args.github_output is not None:
        admitted = "true" if decision.admitted else "false"
        with open(args.github_output, "a", encoding="utf-8") as handle:
            handle.write(f"admitted={admitted}\n")

    return 0 if decision.admitted else 1


if __name__ == "__main__":
    raise SystemExit(main())

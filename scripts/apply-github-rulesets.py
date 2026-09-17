#!/usr/bin/env python3
# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Create or update repository rulesets.

GitHub Free does not enforce rulesets on private repos. Run this once after
the repository is public (or on a plan that allows private rulesets):

    ./scripts/apply-github-rulesets.py

Organization admins bypass (git push --admin / merge with admin), same as
the family repos. Develop required checks match SymWorx job ids:
fmt, rust-checks, python-bindings.
"""

from __future__ import annotations

import json
import subprocess
import sys

REPO = "symworx/symsight"

BYPASS = [
    {
        "actor_id": 1,
        "actor_type": "OrganizationAdmin",
        "bypass_mode": "always",
    }
]

PULL_REQUEST = {
    "type": "pull_request",
    "parameters": {
        "required_approving_review_count": 1,
        "dismiss_stale_reviews_on_push": True,
        "require_code_owner_review": False,
        "require_last_push_approval": True,
        "required_review_thread_resolution": True,
        "require_extra_approval_for_unattributed_changes": True,
        "allowed_merge_methods": ["merge", "squash", "rebase"],
    },
}

RULESETS = [
    {
        "name": "develop",
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": BYPASS,
        "conditions": {
            "ref_name": {"include": ["refs/heads/develop"], "exclude": []},
        },
        "rules": [
            {"type": "deletion"},
            {"type": "non_fast_forward"},
            PULL_REQUEST,
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": True,
                    "do_not_enforce_on_create": False,
                    "required_status_checks": [
                        {"context": "fmt"},
                        {"context": "rust-checks"},
                        {"context": "python-bindings"},
                    ],
                },
            },
        ],
    },
    {
        "name": "stage-main",
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": BYPASS,
        "conditions": {
            "ref_name": {
                "include": [
                    "refs/heads/main",
                    "refs/heads/master",
                    "refs/heads/stage",
                    "refs/heads/staging",
                ],
                "exclude": [],
            },
        },
        "rules": [
            {"type": "deletion"},
            {"type": "non_fast_forward"},
            PULL_REQUEST,
        ],
    },
    {
        "name": "release-branches",
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": BYPASS,
        "conditions": {
            "ref_name": {"include": ["refs/heads/release/**"], "exclude": []},
        },
        "rules": [
            {"type": "deletion"},
            {"type": "non_fast_forward"},
            PULL_REQUEST,
        ],
    },
    {
        "name": "topic-no-force-push",
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": BYPASS,
        "conditions": {
            "ref_name": {
                "include": ["~ALL"],
                "exclude": [
                    "refs/heads/develop",
                    "refs/heads/main",
                    "refs/heads/master",
                    "refs/heads/stage",
                    "refs/heads/staging",
                    "refs/heads/release/**",
                ],
            },
        },
        "rules": [{"type": "non_fast_forward"}],
    },
    {
        "name": "version-tags",
        "target": "tag",
        "enforcement": "active",
        "bypass_actors": BYPASS,
        "conditions": {
            "ref_name": {"include": ["refs/tags/v*"], "exclude": []},
        },
        "rules": [{"type": "deletion"}, {"type": "update"}],
    },
]


def gh_json(args: list[str], input_obj: object | None = None) -> tuple[int, object, str]:
    cmd = ["gh", "api", *args]
    raw_in = None if input_obj is None else json.dumps(input_obj)
    proc = subprocess.run(cmd, input=raw_in, text=True, capture_output=True)
    payload: object
    try:
        payload = json.loads(proc.stdout) if proc.stdout.strip() else None
    except json.JSONDecodeError:
        payload = proc.stdout
    return proc.returncode, payload, proc.stderr.strip()


def existing_by_name() -> dict[str, int]:
    code, payload, err = gh_json([f"repos/{REPO}/rulesets"])
    if code != 0:
        print(err or payload, file=sys.stderr)
        sys.exit(code)
    if not isinstance(payload, list):
        print(f"unexpected ruleset list: {payload!r}", file=sys.stderr)
        sys.exit(1)
    return {str(item["name"]): int(item["id"]) for item in payload}


def upsert(spec: dict, existing: dict[str, int]) -> None:
    name = spec["name"]
    if name in existing:
        rid = existing[name]
        code, payload, err = gh_json(
            ["-X", "PUT", f"repos/{REPO}/rulesets/{rid}", "--input", "-"],
            spec,
        )
        action = "updated"
    else:
        code, payload, err = gh_json(
            ["-X", "POST", f"repos/{REPO}/rulesets", "--input", "-"],
            spec,
        )
        action = "created"
    if code != 0:
        print(f"{name}: FAILED\n{err or payload}", file=sys.stderr)
        sys.exit(code)
    rid = payload.get("id") if isinstance(payload, dict) else "?"
    print(f"{name}: {action} (id {rid})")


def main() -> int:
    print(f"Applying family rulesets on {REPO}")
    existing = existing_by_name()
    for spec in RULESETS:
        upsert(spec, existing)
    return 0


if __name__ == "__main__":
    sys.exit(main())

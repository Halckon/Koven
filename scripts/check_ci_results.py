#!/usr/bin/env python3
"""Reject failed or unexpectedly skipped required CI jobs (SPEC-0239)."""

import json
import os


def check_results(needs, event, ref):
    """Return errors for the exact jobs required by this event and change set."""
    errors = []
    changes = needs.get("changes", {})
    if changes.get("result") != "success":
        return ["changes must succeed before any skipped gate can be accepted"]
    outputs = changes.get("outputs", {})
    for name in ("docs", "rust"):
        if outputs.get(name) not in ("true", "false"):
            errors.append(f"changes.{name} is missing or invalid")
    if errors:
        return errors
    force = ref == "refs/heads/main" or event == "workflow_dispatch"
    rust = outputs["rust"] == "true" or force
    full = rust and (event in ("pull_request", "workflow_dispatch") or ref == "refs/heads/main")
    required = {"docs": outputs["docs"] == "true" or force, "fmt": rust,
                "clippy": full, "test": full}
    for name, must_run in required.items():
        expected = "success" if must_run else "skipped"
        actual = needs.get(name, {}).get("result")
        if actual != expected:
            errors.append(f"{name}: expected {expected}, got {actual}")
    return errors


if __name__ == "__main__":
    problems = check_results(json.loads(os.environ["CI_NEEDS"]),
                             os.environ["GITHUB_EVENT_NAME"], os.environ["GITHUB_REF"])
    if problems:
        raise SystemExit("\n".join(problems))
    print("All required jobs succeeded; only event/path-exempt jobs were skipped.")

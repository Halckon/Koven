#!/usr/bin/env python3
"""Reject failed or unexpectedly skipped required CI jobs (SPEC-0239)."""

import json
import os


def check_results(needs, event, ref, reuse_verified=False):
    """Return errors for the exact jobs required by this event and change set."""
    errors = []
    changes = needs.get("changes", {})
    if changes.get("result") != "success":
        return ["changes must succeed before any skipped gate can be accepted"]
    outputs = changes.get("outputs", {})
    for name in ("docs", "rust", "editors", "preview"):
        if outputs.get(name) not in ("true", "false"):
            errors.append(f"changes.{name} is missing or invalid")
    if errors:
        return errors
    reuse = outputs.get("reuse", "false")
    if reuse not in ("true", "false"):
        return ["changes.reuse is invalid"]
    if reuse == "true" and (event != "pull_request" or not reuse_verified):
        return ["code CI reuse requires independently verified same-PR evidence"]
    reused = reuse == "true"
    force = ref == "refs/heads/main" or event == "workflow_dispatch"
    rust = outputs["rust"] == "true" or outputs["preview"] == "true" or force
    full = rust and (event in ("pull_request", "workflow_dispatch") or ref == "refs/heads/main")
    preview = (outputs["preview"] == "true" or force) and full
    required = {"rust-size": True, "dependencies": True, "docs": outputs["docs"] == "true" or force or event == "pull_request", "fmt": rust,
                "clippy": full, "test": full, "editors": outputs["editors"] == "true" or force}
    required.update(dict.fromkeys(("preview-macos-produce", "preview-linux-produce",
                                   "preview-macos-consume", "preview-linux-consume"), preview))
    if reused:
        for name in required:
            if name not in ("docs", "rust-size", "dependencies"):
                required[name] = False
    for name, must_run in required.items():
        expected = "success" if must_run else "skipped"
        actual = needs.get(name, {}).get("result")
        if actual != expected:
            errors.append(f"{name}: expected {expected}, got {actual}")
    return errors


if __name__ == "__main__":
    needs = json.loads(os.environ["CI_NEEDS"])
    outputs = needs.get("changes", {}).get("outputs", {})
    verified = False
    if outputs.get("reuse") == "true":
        from pr_ci_reuse import verify_selected
        try:
            verified = verify_selected(outputs)
        except Exception as error:
            print(f"Reuse evidence unavailable at final verification: {type(error).__name__}")
    problems = check_results(needs, os.environ["GITHUB_EVENT_NAME"],
                             os.environ["GITHUB_REF"], reuse_verified=verified)
    if problems:
        raise SystemExit("\n".join(problems))
    print("All required jobs succeeded; skips follow event/path rules or independently verified prior code CI.")

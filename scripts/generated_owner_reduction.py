#!/usr/bin/env python3
"""SPEC-0269: Real end-to-end same-cause reduction acceptance (G5).

Performs bounded structural minimization on a real generated case using
actual compiler export and execution, verifying stable witness, same-cause
fingerprint retention, candidate monotonicity, and 3 independent confirmations.
"""
import argparse
import copy
import json
from pathlib import Path
import sys
import time

if __package__:
    from . import generated_owners as model, generated_owner_checks as checks, check_generated_owners as gate
else:
    import generated_owners as model
    import generated_owner_checks as checks
    import check_generated_owners as gate

Failure = checks.Failure


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def run_reduction(executor, root_dir):
    root_dir.mkdir(parents=True, exist_ok=True)
    # Select an I1 case with redundant extra locals and move chains
    original_case = copy.deepcopy(next(c for c in model.cases() if c["shape"] == "I1"))
    expected_mismatch_code = "L0133"

    initial_dir = root_dir / "original"
    gate.prepare_case(original_case, initial_dir)
    # Inject an expectation mismatch: expect L0133 while compiler reports L0131
    rendered = model.render(original_case)
    mismatched = [dict(code=expected_mismatch_code, primary=[0, 0], labels=[])]
    write_json(initial_dir / "expected-diagnostics.json", mismatched)
    checks.seal_inputs(initial_dir, ["case.ko", "case.json", "expected-diagnostics.json",
                                     "oracle.json", "expected.stdout", "expected-allocations.txt"])

    initial_failure = None
    try:
        executor.export(initial_dir)
        checks.check_diagnostics(initial_dir, mismatched)
    except Failure as caught:
        initial_failure = caught

    expected_fingerprint = ("frontend", "diagnostic_mismatch", f"{expected_mismatch_code}:code")
    if initial_failure is None or initial_failure.fingerprint != expected_fingerprint:
        raise Failure("reduction", "tool_or_harness_failure", "initial-failure-not-established", repr(initial_failure))

    write_json(initial_dir / "verdict.json", initial_failure.record())

    reductions_dir = root_dir / "reduction"
    reductions_dir.mkdir(parents=True, exist_ok=True)
    executor.deadline = time.monotonic() + 120

    def replay(candidate, attempt):
        attempt_path = reductions_dir / f"attempt-{attempt:02d}"
        gate.prepare_case(candidate, attempt_path)
        write_json(attempt_path / "expected-diagnostics.json", mismatched)
        checks.seal_inputs(attempt_path, ["case.ko", "case.json", "expected-diagnostics.json",
                                          "oracle.json", "expected.stdout", "expected-allocations.txt"])
        try:
            executor.export(attempt_path)
            checks.check_diagnostics(attempt_path, mismatched)
        except Failure as observed:
            write_json(attempt_path / "verdict.json", observed.record())
            return observed
        return None

    reduced = checks.minimize(
        original_case,
        initial_failure,
        model.shrink_candidates,
        replay,
        lambda item: len(model.render(item)["source"].encode()),
        max_candidates=32,
        seconds=120,
        deadline=executor.deadline,
    )

    if reduced["status"] != "reproduced":
        raise Failure("reduction", "tool_or_harness_failure", "reduction-status-not-reproduced", reduced["status"])
    if reduced["confirmation_count"] != 3:
        raise Failure("reduction", "tool_or_harness_failure", "reduction-confirmation-count-not-three", str(reduced["confirmation_count"]))
    if model.complexity(reduced["minimal"]) >= model.complexity(original_case):
        raise Failure("reduction", "tool_or_harness_failure", "minimal-not-strictly-smaller")

    write_json(root_dir / "reduction.json", reduced)
    return reduced


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--exporter", type=Path)
    args = parser.parse_args(argv)
    root = args.artifacts.resolve()
    executor = gate.Execution(root, args.exporter)
    executor.setup()
    try:
        reduced = run_reduction(executor, root)
        print(f"generated owner real reduction: reduced from {len(reduced['original']['operations'])} "
              f"to {len(reduced['minimal']['operations'])} operations with 3 confirmations", flush=True)
        return 0
    except Failure as failure:
        write_json(root / "reduction-failure.json", failure.record())
        print(f"real reduction failed: {failure}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

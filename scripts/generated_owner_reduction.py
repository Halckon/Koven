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
    # Select a V2 case with redundant extra locals and move operations
    # 'v2-3-83' has 9 operations: Holder, Leaf, extra0, extra1, move, replace, inspect, return_if, marker
    original_case = copy.deepcopy(next(c for c in model.cases() if c["id"] == "v2-3-83"))

    # 1. Clean baseline verification for original case (未注入对照)
    initial_clean_dir = root_dir / "original-clean"
    gate.prepare_case(original_case, initial_clean_dir)
    try:
        executor.case(initial_clean_dir)
    except Failure as clean_failure:
        raise Failure("reduction", "tool_or_harness_failure", "original-clean-baseline-failed", repr(clean_failure))

    # 2. Controlled fault injection on original case (因果对照：关闭无故障，开启必触发目标故障)
    initial_fault_dir = root_dir / "original-fault"
    gate.prepare_case(original_case, initial_fault_dir)
    (initial_fault_dir / "fault.txt").write_text("missing_deinit\n")
    checks.seal_inputs(initial_fault_dir, ["case.ko", "case.json", "expected-diagnostics.json",
                                          "oracle.json", "expected.stdout", "expected-allocations.txt",
                                          "fault.txt"])

    initial_failure = None
    try:
        executor.case(initial_fault_dir)
    except Failure as caught:
        initial_failure = caught

    expected_fingerprint = ("native", "native_output_mismatch", "drop:holder")
    if initial_failure is None or initial_failure.fingerprint != expected_fingerprint:
        raise Failure("reduction", "tool_or_harness_failure", "initial-failure-not-established", repr(initial_failure))
    if not initial_failure.stable_witness:
        raise Failure("reduction", "tool_or_harness_failure", "initial-failure-not-stable-witness")

    write_json(initial_fault_dir / "verdict.json", initial_failure.record())

    reductions_dir = root_dir / "reduction"
    reductions_dir.mkdir(parents=True, exist_ok=True)
    executor.deadline = time.monotonic() + 180

    attempt_log = []

    def replay(candidate, attempt):
        attempt_prefix = f"attempt-{attempt:02d}"

        # Step A: Clean baseline verification - candidate must compile and run cleanly when not injected
        attempt_clean_path = reductions_dir / f"{attempt_prefix}-clean"
        gate.prepare_case(candidate, attempt_clean_path)
        try:
            executor.case(attempt_clean_path)
        except Failure as clean_err:
            attempt_log.append(dict(
                attempt=attempt, candidate=candidate,
                clean_status="failed", clean_error=clean_err.record(),
                fault_status="skipped", accepted=False,
                reason="clean baseline failed"
            ))
            return None

        # Step B: Fault injection - candidate must reproduce identical semantic fault
        attempt_fault_path = reductions_dir / f"{attempt_prefix}-fault"
        gate.prepare_case(candidate, attempt_fault_path)
        (attempt_fault_path / "fault.txt").write_text("missing_deinit\n")
        checks.seal_inputs(attempt_fault_path, ["case.ko", "case.json", "expected-diagnostics.json",
                                               "oracle.json", "expected.stdout", "expected-allocations.txt",
                                               "fault.txt"])
        try:
            executor.case(attempt_fault_path)
        except Failure as observed:
            write_json(attempt_fault_path / "verdict.json", observed.record())
            same_cause = (observed.stable_witness and observed.fingerprint == expected_fingerprint)
            attempt_log.append(dict(
                attempt=attempt, candidate=candidate,
                clean_status="pass",
                fault_status="reproduced" if same_cause else "diverged",
                failure=observed.record(),
                accepted=same_cause,
                reason="same-cause preserved" if same_cause else f"fingerprint diverged: {observed.fingerprint}"
            ))
            return observed

        attempt_log.append(dict(
            attempt=attempt, candidate=candidate,
            clean_status="pass", fault_status="clean",
            accepted=False, reason="fault did not trigger expected failure"
        ))
        return None

    reduced = checks.minimize(
        original_case,
        initial_failure,
        model.shrink_candidates,
        replay,
        lambda item: len(model.render(item)["source"].encode()),
        max_candidates=32,
        seconds=180,
        deadline=executor.deadline,
    )

    if reduced["status"] != "reproduced":
        raise Failure("reduction", "tool_or_harness_failure", "reduction-status-not-reproduced", reduced["status"])
    if reduced["confirmation_count"] != 3:
        raise Failure("reduction", "tool_or_harness_failure", "reduction-confirmation-count-not-three", str(reduced["confirmation_count"]))
    if model.complexity(reduced["minimal"]) >= model.complexity(original_case):
        raise Failure("reduction", "tool_or_harness_failure", "minimal-not-strictly-smaller")
    if len(reduced["minimal"]["operations"]) >= len(original_case["operations"]):
        raise Failure("reduction", "tool_or_harness_failure", "minimal-ops-not-strictly-smaller")

    # Step C: 1-minimal verification (Delta Debugging 1-minimal property)
    # Proves all single-step reductions from minimal cannot further reduce while reproducing the fault
    one_step_exhaustion = []
    for step_cand in model.shrink_candidates(reduced["minimal"]):
        if model.complexity(step_cand) >= model.complexity(reduced["minimal"]):
            continue
        test_dir = reductions_dir / f"1minimal-test-{len(one_step_exhaustion):02d}"
        gate.prepare_case(step_cand, test_dir)
        (test_dir / "fault.txt").write_text("missing_deinit\n")
        checks.seal_inputs(test_dir, ["case.ko", "case.json", "expected-diagnostics.json",
                                     "oracle.json", "expected.stdout", "expected-allocations.txt",
                                     "fault.txt"])
        try:
            executor.case(test_dir)
            outcome = "fault_not_observed"
        except Failure as f:
            if f.fingerprint == expected_fingerprint and f.stable_witness:
                outcome = "unexpectedly_reducible"
            else:
                outcome = f"diverged_fingerprint:{f.fingerprint}"
        one_step_exhaustion.append(dict(
            candidate_ops=len(step_cand["operations"]),
            outcome=outcome
        ))
        if outcome == "unexpectedly_reducible":
            raise Failure("reduction", "tool_or_harness_failure", "minimal-was-not-1-minimal")

    # Governance disclosure & comprehensive audit trail
    reduced["nature"] = "fault_injected_runtime_reduction_verification"
    reduced["nature_disclosure"] = "受控运行期资源故障注入的单调有界同因缩减能力验证（非现存编译器缺陷）"
    reduced["fault_target"] = {
        "fault_kind": "missing_deinit",
        "semantic_resource": "Holder",
        "witness_event": "drop:holder",
        "injection_location": "drop glue call to Holder.__deinit erased in LLVM IR"
    }
    reduced["causal_contrast"] = {
        "original_clean": "pass",
        "original_fault": initial_failure.record(),
        "minimal_clean": "pass",
        "minimal_fault": reduced["original_failure"]
    }
    reduced["source_delta"] = {
        "original_ops": len(original_case["operations"]),
        "minimal_ops": len(reduced["minimal"]["operations"]),
        "original_bytes": len(model.render(original_case)["source"].encode()),
        "minimal_bytes": len(model.render(reduced["minimal"])["source"].encode()),
        "original_source": model.render(original_case)["source"],
        "minimal_source": model.render(reduced["minimal"])["source"]
    }
    reduced["is_1_minimal"] = True
    reduced["one_step_exhaustion"] = one_step_exhaustion
    reduced["detailed_attempts"] = attempt_log

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

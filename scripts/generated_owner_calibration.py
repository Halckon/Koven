#!/usr/bin/env python3
"""SPEC-0269: Fault calibration on generated resource programs."""
import argparse
import json
from pathlib import Path
import platform
import re
import sys

if __package__:
    from . import generated_owner_checks as checks, check_native_sanitizers as native
else:
    import generated_owner_checks as checks
    import check_native_sanitizers as native

Failure = checks.Failure
ROOT = Path(__file__).resolve().parents[1]
EXPORT_CALIBRATION_TEST = "native_generated_owner_tests::export_generated_owner_calibration"


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def assert_export_calibration(text):
    if (len(re.findall(rf"^test {re.escape(EXPORT_CALIBRATION_TEST)} \.\.\. ok$", text, re.M)) != 1
            or len(re.findall(r"test result: ok\. 1 passed; 0 failed; 0 ignored;", text)) != 1):
        raise Failure("calibration-export", "tool_or_harness_failure", "exact-test-hit")


def read_mutants(path):
    rows = []
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        if len(parts) != 6:
            raise Failure("calibration", "tool_or_harness_failure", "malformed-mutants-tsv", line)
        rows.append(dict(id=parts[0], category=parts[1], base_case=parts[2],
                         target_function=parts[3], detector=parts[4], witness=parts[5]))
    expected_categories = {"address", "leak", "missing_deinit", "premature_holder_free"}
    if {r["category"] for r in rows} != expected_categories:
        raise Failure("calibration", "tool_or_harness_failure", "missing-calibration-mutant-category")
    return rows


def verify(executor, directory):
    """Execute clean baseline and 4 physical/logical mutants against their detectors."""
    directory.mkdir(parents=True, exist_ok=True)
    # 1. Export calibration artifacts
    result = executor.run(
        [executor.binary, EXPORT_CALIBRATION_TEST, "--exact", "--nocapture", "--test-threads=1"],
        directory, "export-calibration", timeout=30,
        env={"KOVEN_GENERATED_OWNER_CALIBRATION": str(directory)},
    )
    if result.returncode:
        raise Failure("calibration-export", "ssa_or_codegen_failure", "export-failed",
                      result.stderr.decode(errors="replace"), stable_witness=False)
    assert_export_calibration(result.stdout.decode())

    mutants_tsv = directory / "mutants.tsv"
    if not mutants_tsv.is_file():
        raise Failure("calibration", "tool_or_harness_failure", "missing-mutants-tsv")
    mutants = read_mutants(mutants_tsv)

    v1_dir = directory / "v1"
    v2_dir = directory / "v2"
    clean_v1_stdout = (v1_dir / "clean.stdout").read_bytes()
    clean_v2_stdout = (v2_dir / "clean.stdout").read_bytes()

    calibration_records = []

    # 2. Clean baselines
    clean_v1_run = executor.build_run(v1_dir, "clean-counter", v1_dir / "clean.counter.ll", counter=True)
    executor.accept_native(clean_v1_run, clean_v1_stdout, "counter")
    calibration_records.append(dict(name="clean-v1", status="pass", exit=clean_v1_run.returncode))

    clean_v2_run = executor.build_run(v2_dir, "clean-counter", v2_dir / "clean.counter.ll", counter=True)
    executor.accept_native(clean_v2_run, clean_v2_stdout, "counter")
    calibration_records.append(dict(name="clean-v2", status="pass", exit=clean_v2_run.returncode))

    if executor.linux:
        clean_v1_asan = executor.build_run(v1_dir, "clean-asan", v1_dir / "clean.asan.ll", detector="asan")
        executor.accept_native(clean_v1_asan, clean_v1_stdout, "asan")
        clean_v2_asan = executor.build_run(v2_dir, "clean-asan", v2_dir / "clean.asan.ll", detector="asan")
        executor.accept_native(clean_v2_asan, clean_v2_stdout, "asan")

    # 3. Mutant verification
    for row in mutants:
        category = row["category"]
        mutant_id = row["id"]
        target = row["target_function"]
        case_dir = v1_dir if row["base_case"] == "v1" else v2_dir

        if category == "address":
            # Control: detector-off must NOT trigger detector
            raw_ll = case_dir / "fault-address.raw.ll"
            raw_run = executor.build_run(case_dir, "fault-address-raw", raw_ll)
            if b"AddressSanitizer" in raw_run.stderr:
                raise Failure("calibration", "tool_or_harness_failure",
                              "detector-off-reported-sanitizer", raw_run.stderr.decode(errors="replace"))
            # Detection: on Linux with ASan
            if executor.linux:
                asan_ll = case_dir / "fault-address.asan.ll"
                asan_run = executor.build_run(case_dir, "fault-address-asan", asan_ll, detector="asan")
                if asan_run.returncode == 0:
                    raise Failure("calibration", "unexpected_acceptance", "address-fault-not-caught-by-asan")
                failure = checks.sanitizer_failure(asan_run.stderr, "AddressSanitizer")
                if failure is None or target.encode() not in asan_run.stderr:
                    raise Failure("calibration", "diagnostic_mismatch", "address-sanitizer-target-mismatch",
                                  asan_run.stderr.decode(errors="replace"))
                calibration_records.append(dict(name=mutant_id, category=category, detected_by="asan",
                                                fingerprint=failure.fingerprint, status="rejected_as_expected"))
            else:
                calibration_records.append(dict(name=mutant_id, category=category, detector_off_verified=True,
                                                asan_skipped="macos-counter-only", status="skipped",
                                                reason="macos-asan-unsupported"))

        elif category == "leak":
            leak_ll = case_dir / "fault-leak.counter.ll"
            leak_run = executor.build_run(case_dir, "fault-leak-counter", leak_ll, counter=True)
            if leak_run.returncode == 0:
                raise Failure("calibration", "unexpected_acceptance", "leak-fault-not-caught-by-counter")
            if not any(mark in leak_run.stderr for mark in (b"counter.c", b"counted_free", b"verify_counts", b"releases == EXPECTED_ALLOCATIONS")):
                raise Failure("calibration", "resource_counter_failure", "leak-not-caught-by-pointer-ledger",
                              leak_run.stderr.decode(errors="replace"))
            record = dict(name=mutant_id, category=category, detected_by="counter",
                          status="rejected_as_expected", exit=leak_run.returncode)
            if executor.linux:
                leak_raw_ll = case_dir / "fault-leak.raw.ll"
                lsan_run = executor.build_run(case_dir, "fault-leak-lsan", leak_raw_ll, detector="lsan")
                if b"LeakSanitizer has encountered a fatal error" in lsan_run.stderr:
                    record["lsan_status"] = "runtime-unavailable"
                    record["lsan_detail"] = lsan_run.stderr.decode(errors="replace")
                elif lsan_run.returncode != 0 and b"LeakSanitizer" in lsan_run.stderr:
                    lsan_failure = checks.sanitizer_failure(lsan_run.stderr, "LeakSanitizer")
                    record["lsan_status"] = "rejected_as_expected"
                    if lsan_failure:
                        record["lsan_fingerprint"] = lsan_failure.fingerprint
                else:
                    raise Failure("calibration", "unexpected_acceptance", "leak-fault-not-caught-by-lsan")
            else:
                record["lsan_skipped"] = "macos-counter-only"
            calibration_records.append(record)

        elif category == "missing_deinit":
            deinit_ll = case_dir / "fault-missing_deinit.counter.ll"
            deinit_run = executor.build_run(case_dir, "fault-missing_deinit-counter", deinit_ll, counter=True)
            if deinit_run.stdout == clean_v1_stdout:
                raise Failure("calibration", "unexpected_acceptance", "missing-deinit-not-caught")
            output_diff = checks.output_failure(clean_v1_stdout, deinit_run.stdout)
            if "leaf" not in output_diff.witness:
                raise Failure("calibration", "diagnostic_mismatch", "missing-deinit-witness-mismatch",
                              output_diff.witness)
            calibration_records.append(dict(name=mutant_id, category=category, detected_by="output_diff",
                                            witness=output_diff.witness, status="rejected_as_expected"))

        elif category == "premature_holder_free":
            holder_ll = case_dir / "fault-premature_holder_free.counter.ll"
            holder_run = executor.build_run(case_dir, "fault-premature-counter", holder_ll, counter=True)
            if holder_run.returncode == 0:
                raise Failure("calibration", "unexpected_acceptance", "premature-holder-free-not-caught")
            if not any(mark in holder_run.stderr for mark in (b"counter.c", b"release_order", b"id == release_order")):
                raise Failure("calibration", "resource_counter_failure", "premature-holder-free-order-not-rejected",
                              holder_run.stderr.decode(errors="replace"))
            calibration_records.append(dict(name=mutant_id, category=category, detected_by="counter_order",
                                            status="rejected_as_expected", exit=holder_run.returncode))

    skipped_reasons = []
    for rec in calibration_records:
        if rec.get("status") == "skipped":
            skipped_reasons.append(f"{rec['name']}:{rec.get('reason') or rec.get('asan_skipped')}")
        if rec.get("lsan_skipped"):
            skipped_reasons.append(f"{rec['name']}:{rec['lsan_skipped']}")
        elif rec.get("lsan_status") == "runtime-unavailable":
            skipped_reasons.append(f"{rec['name']}:lsan-runtime-unavailable")

    status = "partial" if skipped_reasons else "pass"
    report = dict(status=status, records=calibration_records)
    if skipped_reasons:
        report["skipped_reasons"] = skipped_reasons
    write_json(directory / "calibration.json", report)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--exporter", type=Path)
    args = parser.parse_args(argv)
    root = args.artifacts.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if __package__:
        from . import check_generated_owners as gate
    else:
        import check_generated_owners as gate
    executor = gate.Execution(root, args.exporter)
    executor.setup()
    try:
        report = verify(executor, root)
        status, _ = gate.calibration_verdict(report, linux=executor.linux)
        if status == "pass":
            print("generated owner calibration: all fault categories verified", flush=True)
        else:
            print(f"generated owner calibration: partial ({', '.join(report.get('skipped_reasons', []))})", flush=True)
        return 0
    except Failure as failure:
        write_json(root / "calibration-failure.json", failure.record())
        print(f"calibration failed: {failure}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

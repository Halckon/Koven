"""A failed compiler/probe must not be mistaken for a generated ownership witness."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from scripts import generated_owner_checks as checks
from scripts import generated_owners as model


class GeneratedOwnerCheckTests(unittest.TestCase):
    def test_absolute_executor_deadline_precedes_relative_reducer_budget(self):
        from unittest import mock
        now = [100.0]
        failure = checks.Failure("native", "native_output_mismatch", "done")
        timeout = checks.Failure("batch", "timeout", "execution-budget")
        def replay(_case, _attempt):
            now[0] = 100.6
            return timeout
        with mock.patch.object(checks.time, "monotonic", side_effect=lambda: now[0]):
            result = checks.minimize({"size": 5}, failure, lambda _: [], replay,
                                     lambda c: c["size"], deadline=100.5)
        self.assertEqual("minimization_incomplete", result["status"])
        self.assertEqual(0, result["confirmation_count"])
        self.assertEqual(failure.record(), result["original_failure"])

    def test_sanitizer_addresses_do_not_change_stable_function_witness(self):
        first = b"ERROR: AddressSanitizer: heap-use-after-free on address 0x111 at pc 0x112\n#0 0x113 in f2.inspect (program+0x11)\n"
        second = first.replace(b"0x11", b"0x22").replace(b"f2.inspect", b"f8.inspect")
        a = checks.sanitizer_failure(first, "AddressSanitizer")
        b = checks.sanitizer_failure(second, "AddressSanitizer")
        self.assertEqual(a.fingerprint, b.fingerprint)
        self.assertTrue(a.stable_witness)
        other = checks.sanitizer_failure(second.replace(b"inspect", b"work"), "AddressSanitizer")
        self.assertNotEqual(a.fingerprint, other.fingerprint)
        unknown = checks.sanitizer_failure(first.split(b"\n")[0], "AddressSanitizer")
        self.assertFalse(unknown.stable_witness)
        result = checks.minimize({}, unknown, lambda _: self.fail("cannot reduce without witness"),
                                 lambda *_: None, lambda _: 1)
        self.assertEqual(result["status"], "minimization_incomplete")

    def test_sanitizer_caller_and_allocation_frames_do_not_identify_fault(self):
        header = b"ERROR: AddressSanitizer: heap-use-after-free on address 0x111\n"
        direct = checks.sanitizer_failure(header + b"#0 0x12 in f2.inspect (program+0x11)\n", "AddressSanitizer")
        for stack in (b"#0 0x12 in __interceptor_write\n#1 0x13 in f8.inspect\n",
                      b"#0 0x12 (program+0x11)\nfreed by thread T0 here:\n#0 0x13 in f8.inspect\n",
                      b"freed by thread T0 here:\n#0 0x13 in f8.inspect\n"):
            observed = checks.sanitizer_failure(header + stack, "AddressSanitizer")
            self.assertFalse(observed.stable_witness)
            self.assertNotEqual(direct.fingerprint, observed.fingerprint)

    def test_exact_diagnostic_includes_primary_and_associated_byte_spans(self):
        expected = [{"code": "L0131", "primary": [20, 24], "labels": [[10, 14]]}]
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "stages.tsv").write_text("parse\t0\nnames\t0\ntypes\t0\nownership\t1\n")
            good = "ownership\tL0131\t20\t24\t10:14\n"
            for actual in (good, "", good.replace("L0131", "L0133"), good.replace("20", "21"),
                           good.replace("10:14", "11:14")):
                (directory / "diagnostics.tsv").write_text(actual)
                if actual == good:
                    checks.check_diagnostics(directory, expected)
                else:
                    with self.assertRaises(checks.Failure) as caught:
                        checks.check_diagnostics(directory, expected)
                    self.assertIn(caught.exception.kind, {"unexpected_acceptance", "diagnostic_mismatch"})
            (directory / "stages.tsv").write_text("parse\t1\n")
            with self.assertRaises(checks.Failure) as caught:
                checks.check_diagnostics(directory, expected)
            self.assertEqual(caught.exception.kind, "unexpected_frontend_rejection")

    def test_diagnostics_do_not_accept_an_empty_or_unfinished_frontend(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "diagnostics.tsv").write_text("")
            for stages in ("", "parse\t0\n", "parse\t0\nnames\t0\ntypes\t0\nownership\t1\n"):
                (directory / "stages.tsv").write_text(stages)
                with self.assertRaises(checks.Failure):
                    checks.check_diagnostics(directory, [])

    def test_fingerprint_keeps_missing_event_but_not_machine_output(self):
        a = checks.output_failure(b"borrow\ndrop:owner\ndone\n", b"borrow\ndone\n")
        b = checks.output_failure(b"drop:owner\ndone\n", b"done\n")
        other = checks.output_failure(b"drop:other\ndone\n", b"done\n")
        self.assertEqual(a.fingerprint, b.fingerprint)
        self.assertNotEqual(a.fingerprint, other.fingerprint)
        ambiguous = checks.output_failure(b"borrow\ndrop:a\ndrop:a\n", b"borrow\ndrop:a\n")
        self.assertFalse(ambiguous.stable_witness, "equal labels cannot distinguish Borrow read from deinit")

    def test_line_ending_only_difference_remains_native_output_failure(self):
        for actual in (b"done", b"done\r\n"):
            with self.subTest(actual=actual):
                failure = checks.output_failure(b"done\n", actual)
                self.assertEqual(failure.kind, "native_output_mismatch")
                self.assertFalse(failure.stable_witness)
        with self.assertRaises(ValueError):
            checks.output_failure(b"done\n", b"done\n")

    def test_replay_verifies_saved_source_and_expected_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "case.ko").write_text("original")
            (directory / "expected.stdout").write_text("done\n")
            checks.seal_inputs(directory, ["case.ko", "expected.stdout"])
            checks.verify_inputs(directory)
            (directory / "case.ko").write_text("different")
            with self.assertRaises(checks.Failure):
                checks.verify_inputs(directory)

    def test_shrinker_rejects_changed_cause_and_keeps_original_on_flaky(self):
        original = {"size": 5}
        failure = checks.Failure("native", "native_output_mismatch", "drop:x")
        different = checks.Failure("frontend", "unexpected_frontend_rejection", "parse")
        def candidates(case):
            return ({"size": n} for n in range(1, case["size"]))
        calls = []
        def replay(case, attempt):
            calls.append((case, attempt))
            return failure if case["size"] >= 2 else different
        result = checks.minimize(original, failure, candidates, replay, lambda c: c["size"], 10, 5)
        self.assertEqual(result["minimal"], {"size": 2})
        self.assertEqual(result["status"], "reproduced")
        self.assertEqual(result["confirmation_count"], 3)
        self.assertTrue(any(row["accepted"] is False for row in result["attempts"]))
        def unstable(case, attempt):
            return failure if attempt == 0 else None
        result = checks.minimize(original, failure, candidates, unstable, lambda c: c["size"], 10, 5)
        self.assertEqual(result["status"], "flaky")
        self.assertEqual(result["minimal"], original)

    def test_shrink_budget_cannot_be_reported_as_fully_minimized(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x")
        result = checks.minimize({"size": 5}, failure, lambda _: iter([{"size": 4}]),
                                 lambda *_: failure, lambda c: c["size"], 0, 5)
        self.assertEqual(result["status"], "minimization_incomplete")

    def test_default_candidate_budget_stops_before_the_thirty_third_replay(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x", "original evidence")
        calls = []
        def replay(case, attempt):
            calls.append((case, attempt))
            return failure if case == 100 else None
        result = checks.minimize(100, failure, lambda _: iter(range(99, 0, -1)), replay, int)
        self.assertEqual(result["status"], "minimization_incomplete")
        self.assertEqual(result["minimal"], 100)
        self.assertEqual(len(result["attempts"]), 32)
        self.assertEqual(calls, [(100, n) for n in range(3)]
                         + [(99 - n, n + 3) for n in range(32)])
        self.assertEqual(result["original_failure"], failure.record())
        self.assertEqual(result["confirmation_count"], 0)

    def test_final_three_replays_are_independent_of_candidate_acceptance(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x")
        calls = []
        def replay(case, attempt):
            calls.append((case, attempt))
            return failure
        result = checks.minimize(3, failure, lambda case: iter([case - 1]) if case > 1 else iter(()),
                                 replay, int)
        self.assertEqual(result["status"], "reproduced")
        self.assertEqual(result["minimal"], 1)
        self.assertEqual(calls, [(3, 0), (3, 1), (3, 2), (2, 3), (1, 4), (1, 5), (1, 6), (1, 7)])
        self.assertEqual(result["confirmation_count"], 3)

    def test_candidate_budget_is_shared_across_accepted_reduction_rounds(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x")
        for original, expected_status, expected_minimal in ((33, "reproduced", 1),
                                                            (34, "minimization_incomplete", 2)):
            with self.subTest(original=original):
                calls = []
                def replay(case, attempt):
                    calls.append((case, attempt))
                    return failure
                result = checks.minimize(original, failure,
                                         lambda case: iter([case - 1]) if case > 1 else iter(()), replay, int)
                self.assertEqual(result["status"], expected_status)
                self.assertEqual(result["minimal"], expected_minimal)
                self.assertEqual(len(result["attempts"]), 32)
                self.assertTrue(all(row["accepted"] for row in result["attempts"]))
                self.assertEqual(len(calls), 38 if expected_status == "reproduced" else 35)
                self.assertEqual([attempt for _, attempt in calls], list(range(len(calls))))

    def test_non_smaller_or_unwitnessed_candidates_cannot_be_accepted(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x")
        unwitnessed = checks.Failure(*failure.fingerprint, stable_witness=False)
        changed = checks.Failure("native", "asan_error", "drop:x")
        calls = []
        def replay(case, attempt):
            calls.append(case)
            return {5: failure, 4: unwitnessed, 3: changed, 2: None}[case]
        result = checks.minimize(5, failure, lambda _: iter([6, 5, 4, 3, 2]), replay, int)
        self.assertEqual(result["minimal"], 5)
        self.assertEqual(result["status"], "reproduced")
        self.assertEqual(calls, [5, 5, 5, 4, 3, 2, 5, 5, 5])
        self.assertFalse(any(row["accepted"] for row in result["attempts"]))

    def test_unwitnessed_original_is_retained_without_running_callbacks(self):
        failure = checks.Failure("export", "ssa_or_codegen_failure", "unknown", "original log",
                                 stable_witness=False)
        forbidden = mock.Mock(side_effect=AssertionError("must not run without a semantic witness"))
        result = checks.minimize({"original": True}, failure, forbidden, forbidden, forbidden)
        forbidden.assert_not_called()
        self.assertEqual(result["original"], result["minimal"])
        self.assertEqual(result["original_failure"], failure.record())
        self.assertEqual(result["status"], "minimization_incomplete")
        self.assertEqual(result["confirmation_count"], 0)

    def test_final_replay_flakiness_keeps_original_and_smallest_candidate(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x", "first failure")
        for final_result in (None, checks.Failure("frontend", "unexpected_acceptance", "L0131"),
                             checks.Failure(*failure.fingerprint, stable_witness=False)):
            with self.subTest(final_result=final_result):
                calls = []
                def replay(case, attempt):
                    calls.append((case, attempt))
                    return final_result if attempt == 5 else failure
                result = checks.minimize(2, failure, lambda case: iter([1]) if case == 2 else iter(()),
                                         replay, int)
                self.assertEqual(result["status"], "flaky")
                self.assertEqual(result["original"], 2)
                self.assertEqual(result["minimal"], 1)
                self.assertEqual(result["original_failure"], failure.record())
                self.assertEqual(result["confirmation_count"], 0)
                self.assertEqual(calls[-1], (1, 5))

    def test_replay_finishing_at_deadline_is_incomplete_even_on_final_confirmation(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x")
        for finish_attempt in (2, 5):
            with self.subTest(finish_attempt=finish_attempt):
                clock = [0.0]
                calls = []
                def replay(case, attempt):
                    calls.append(attempt)
                    if attempt == finish_attempt:
                        clock[0] = 120.0
                    return failure
                with mock.patch.object(checks.time, "monotonic", side_effect=lambda: clock[0]):
                    result = checks.minimize(1, failure, lambda _: iter(()), replay, int)
                self.assertEqual(result["status"], "minimization_incomplete")
                self.assertEqual(result["confirmation_count"], 0)
                self.assertEqual(calls[-1], finish_attempt)
                self.assertEqual(result["elapsed_seconds"], 120.0)

    def test_expired_replay_budget_is_not_flakiness_or_an_accepted_candidate(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x")
        timeout = checks.Failure("batch", "timeout", "execution-budget")
        for expire_on, observed in ((0, timeout), (3, failure), (3, timeout)):
            with self.subTest(expire_on=expire_on, observed=observed):
                clock = [0.0]
                def replay(case, attempt):
                    if attempt == expire_on:
                        clock[0] = 120.01
                        return observed
                    return failure
                with mock.patch.object(checks.time, "monotonic", side_effect=lambda: clock[0]):
                    result = checks.minimize(2, failure, lambda case: iter([1]) if case == 2 else iter(()),
                                             replay, int)
                self.assertEqual(result["status"], "minimization_incomplete")
                self.assertEqual(result["minimal"], 2)
                self.assertFalse(any(row["accepted"] for row in result["attempts"]))
                self.assertEqual(result["original_failure"], failure.record())

    def test_reduction_errors_preserve_prior_evidence_and_smallest_candidate(self):
        failure = checks.Failure("native", "native_output_mismatch", "drop:x", "first failure")
        for phase in ("initial", "replay", "candidates", "measure", "final"):
            for error in (OSError("disk unavailable"), ValueError("invalid candidate"),
                          AssertionError("bad invariant"), RuntimeError("callback failed")):
                with self.subTest(phase=phase, error=type(error).__name__):
                    def replay(case, attempt):
                        if ((phase == "initial" and attempt == 0) or (phase == "replay" and attempt == 3)
                                or (phase == "final" and attempt == 4)):
                            raise error
                        return failure
                    def candidates(case):
                        if case == 2:
                            yield 1
                        elif phase == "candidates":
                            raise error
                        elif phase == "measure":
                            yield 0
                    def measure(case):
                        if phase == "measure" and case == 0:
                            raise error
                        return case
                    result = checks.minimize(2, failure, candidates, replay, measure)
                    self.assertEqual(result["status"], "minimization_incomplete")
                    self.assertEqual(result["original_failure"], failure.record())
                    self.assertEqual(result["original"], 2)
                    self.assertEqual(result["minimal"], 2 if phase in ("initial", "replay") else 1)
                    self.assertEqual(len(result["attempts"]), 0 if phase in ("initial", "replay") else 1)
                    self.assertEqual(result["reduction_error"]["witness"], type(error).__name__)
                    self.assertEqual(result["reduction_error"]["detail"], str(error))
                    self.assertEqual(result["confirmation_count"], 0)

    def test_invalid_shrink_chains_keep_the_exact_ownership_root_and_occurrences(self):
        for original in model.cases():
            if original["shape"] not in ("I1", "I2"):
                continue
            failure = checks.Failure("frontend", "unexpected_acceptance",
                                     "L0131" if original["shape"] == "I1" else "L0133")
            calls = []
            def replay(case, attempt):
                model.validate(case)
                rendered = model.render(case)
                raw = rendered["source"].encode()
                self.assertEqual(len(rendered["expected_diagnostics"]), 1)
                diagnostic = rendered["expected_diagnostics"][0]
                self.assertEqual(diagnostic["code"], failure.witness)
                self.assertEqual(len(diagnostic["labels"]), 1)
                primary, label = diagnostic["primary"], diagnostic["labels"][0]
                token = b"source" if case["shape"] == "I1" else b"item"
                self.assertEqual(raw[slice(*primary)], token)
                self.assertEqual(raw[slice(*label)], token)
                self.assertGreater(primary[0], len(raw[:primary[0]].decode()))
                self.assertGreater(label[0], len(raw[:label[0]].decode()))
                self.assertNotEqual(primary, label)
                self.assertEqual(raw[primary[0] - 8:primary[0]], b"inspect(" if case["shape"] == "I1" else b"consume(")
                if case["shape"] == "I1":
                    line = raw[raw.rfind(b"\n", 0, label[0]) + 1:label[0]]
                    self.assertRegex(line, rb"^    val moved[0-9]* = $")
                else:
                    self.assertEqual(raw[label[0] - 12:label[0]], b"fun invalid(")
                calls.append(attempt)
                return failure
            result = checks.minimize(original, failure, model.shrink_candidates, replay,
                                     lambda case: len(model.render(case)["source"].encode()))
            self.assertEqual(result["status"], "reproduced")
            self.assertEqual(result["confirmation_count"], 3)
            self.assertTrue(result["attempts"])
            self.assertEqual(calls, list(range(len(calls))))


if __name__ == "__main__":
    unittest.main()

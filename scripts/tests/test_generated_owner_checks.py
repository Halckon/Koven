"""A failed compiler/probe must not be mistaken for a generated ownership witness."""
import json
from pathlib import Path
import tempfile
import unittest

from scripts import generated_owner_checks as checks


class GeneratedOwnerCheckTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()

"""Tests for SPEC-0269 generated owner fault calibration."""
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest import mock

from scripts import generated_owner_calibration as calib
from scripts import generated_owner_checks as checks


class GeneratedOwnerCalibrationTests(unittest.TestCase):
    def test_exact_export_calibration_assertion(self):
        good = f"test {calib.EXPORT_CALIBRATION_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 800 filtered out;"
        calib.assert_export_calibration(good)
        for bad in ("", good.replace("1 passed", "0 passed"), good.replace("0 ignored", "1 ignored")):
            with self.assertRaises(checks.Failure):
                calib.assert_export_calibration(bad)

    def test_read_mutants_requires_all_four_categories(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "mutants.tsv"
            full = ("address\taddress\tv1\tf1.inspect\tasan\theap-buffer-overflow\n"
                    "leak\tleak\tv1\tkoven.drop.t12\tcounter\tpointer-ledger\n"
                    "missing_deinit\tmissing_deinit\tv1\tf0.__deinit.t19\toutput\tdrop:leaf_b7af\n"
                    "premature_holder_free\tpremature_holder_free\tv2\tkoven.drop.t14\tcounter\tpointer-ledger\n")
            path.write_text(full)
            rows = calib.read_mutants(path)
            self.assertEqual(4, len(rows))
            # Missing one category
            path.write_text("\n".join(full.splitlines()[:3]) + "\n")
            with self.assertRaises(checks.Failure):
                calib.read_mutants(path)

    def test_verify_requires_clean_pass_and_rejects_unexpected_acceptance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            mutants_tsv = root / "mutants.tsv"
            mutants_tsv.write_text(
                "address\taddress\tv1\tf1.inspect\tasan\theap-buffer-overflow\n"
                "leak\tleak\tv1\tkoven.drop.t12\tcounter\tpointer-ledger\n"
                "missing_deinit\tmissing_deinit\tv1\tf0.__deinit.t19\toutput\tdrop:leaf_b7af\n"
                "premature_holder_free\tpremature_holder_free\tv2\tkoven.drop.t14\tcounter\tpointer-ledger\n"
            )
            v1_dir = root / "v1"
            v2_dir = root / "v2"
            v1_dir.mkdir()
            v2_dir.mkdir()
            (v1_dir / "clean.stdout").write_bytes(b"borrow\ndrop:leaf_b7af\nconsume\ndone\n")
            (v2_dir / "clean.stdout").write_bytes(b"borrow\ndrop:old_b7af\ndone\n")

            executor = mock.Mock()
            executor.linux = False
            executor.binary = Path("/bin/true")
            executor.run.return_value = subprocess.CompletedProcess(
                [], 0, f"test {calib.EXPORT_CALIBRATION_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;".encode(), b""
            )

            # If leak mutant exits 0 unexpectedly
            clean_proc = subprocess.CompletedProcess([], 0, (v1_dir / "clean.stdout").read_bytes(), b"")
            executor.build_run.side_effect = [
                clean_proc,  # clean v1
                clean_proc,  # clean v2
                subprocess.CompletedProcess([], 0, b"", b""),  # address detector-off
                subprocess.CompletedProcess([], 0, b"", b""),  # leak unexpectedly passes
            ]

            with self.assertRaises(checks.Failure) as caught:
                calib.verify(executor, root)
            self.assertEqual("unexpected_acceptance", caught.exception.kind)

    def test_verify_records_partial_when_detectors_skipped_on_macos(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            mutants_tsv = root / "mutants.tsv"
            mutants_tsv.write_text(
                "address\taddress\tv1\tf1.inspect\tasan\theap-buffer-overflow\n"
                "leak\tleak\tv1\tkoven.drop.t12\tcounter\tpointer-ledger\n"
                "missing_deinit\tmissing_deinit\tv1\tf0.__deinit.t19\toutput\tdrop:leaf_b7af\n"
                "premature_holder_free\tpremature_holder_free\tv2\tkoven.drop.t14\tcounter\tpointer-ledger\n"
            )
            v1_dir = root / "v1"
            v2_dir = root / "v2"
            v1_dir.mkdir()
            v2_dir.mkdir()
            (v1_dir / "clean.stdout").write_bytes(b"borrow\ndrop:leaf_b7af\nconsume\ndone\n")
            (v2_dir / "clean.stdout").write_bytes(b"borrow\ndrop:old_b7af\ndone\n")

            executor = mock.Mock()
            executor.linux = False
            executor.binary = Path("/bin/true")
            executor.run.return_value = subprocess.CompletedProcess(
                [], 0, f"test {calib.EXPORT_CALIBRATION_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;".encode(), b""
            )

            clean_proc = subprocess.CompletedProcess([], 0, (v1_dir / "clean.stdout").read_bytes(), b"")
            executor.build_run.side_effect = [
                clean_proc,  # clean v1
                clean_proc,  # clean v2
                subprocess.CompletedProcess([], 0, b"", b""),  # address detector-off
                subprocess.CompletedProcess([], -6, b"", b"counter.c: counted_free: releases == EXPECTED_ALLOCATIONS"),  # leak caught by counter
                subprocess.CompletedProcess([], 0, b"borrow\nconsume\ndone\n", b""),  # missing deinit
                subprocess.CompletedProcess([], -6, b"", b"counter.c: release_order: id == release_order"),  # premature free caught
            ]

            report = calib.verify(executor, root)
            self.assertEqual("partial", report["status"])
            self.assertTrue(any("macos-counter-only" in s for s in report["skipped_reasons"]))


if __name__ == "__main__":
    unittest.main()


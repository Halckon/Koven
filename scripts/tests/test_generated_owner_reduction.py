"""Unit tests for SPEC-0269 generated owner reduction acceptance."""
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest import mock

from scripts import generated_owner_reduction as reducer
from scripts import generated_owner_checks as checks


class GeneratedOwnerReductionTests(unittest.TestCase):
    def test_run_reduction_requires_strictly_smaller_and_three_confirmations(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock()
            executor.deadline = 999999.0

            # Mock case execution: clean succeeds, fault raises missing_deinit
            def case_mock(directory):
                if (directory / "fault.txt").is_file():
                    raise checks.Failure("native", "native_output_mismatch", "drop:holder", stable_witness=True)
                return None

            executor.case.side_effect = case_mock
            result = reducer.run_reduction(executor, root)

            self.assertEqual("reproduced", result["status"])
            self.assertEqual(3, result["confirmation_count"])
            self.assertLess(
                reducer.model.complexity(result["minimal"]),
                reducer.model.complexity(result["original"]),
            )
            self.assertTrue(result["attempts"])
            self.assertTrue((root / "reduction.json").is_file())

    def test_incomplete_reduction_preserves_reduction_json_and_raises(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock()
            executor.deadline = 999999.0

            def case_mock(directory):
                if (directory / "fault.txt").is_file():
                    raise checks.Failure("native", "native_output_mismatch", "drop:holder", stable_witness=True)
                return None

            executor.case.side_effect = case_mock

            original_case = next(c for c in reducer.model.cases() if c["id"] == "v2-3-83")
            mock_reduced = {
                "original": original_case,
                "original_failure": {"stage": "native", "kind": "native_output_mismatch", "witness": "drop:holder"},
                "minimal": original_case,
                "status": "minimization_incomplete",
                "attempts": [],
                "confirmation_count": 0,
                "elapsed_seconds": 12.0,
            }
            with mock.patch.object(checks, "minimize", return_value=mock_reduced):
                with self.assertRaises(checks.Failure) as cm:
                    reducer.run_reduction(executor, root)
                self.assertIn("reduction-status-not-reproduced", str(cm.exception))

            reduction_file = root / "reduction.json"
            self.assertTrue(reduction_file.is_file())
            saved = reducer.json.loads(reduction_file.read_text())
            self.assertEqual("minimization_incomplete", saved["status"])
            self.assertFalse(saved["is_1_minimal"])
            self.assertIn("failure_reason", saved)

    def test_tool_failure_in_1_minimal_check_fails_and_preserves_audit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock()
            executor.deadline = 999999.0

            def case_mock(directory):
                if "1minimal-test" in str(directory):
                    raise checks.Failure("tool", "tool_or_harness_failure", "simulated-harness-crash")
                if (directory / "fault.txt").is_file():
                    raise checks.Failure("native", "native_output_mismatch", "drop:holder", stable_witness=True)
                return None

            executor.case.side_effect = case_mock

            original_case = next(c for c in reducer.model.cases() if c["id"] == "v2-3-83")
            cand = next(reducer.model.shrink_candidates(original_case))
            mock_reduced = {
                "original": original_case,
                "original_failure": {"stage": "native", "kind": "native_output_mismatch", "witness": "drop:holder"},
                "minimal": cand,
                "status": "reproduced",
                "attempts": [{"candidate": cand, "accepted": True}],
                "confirmation_count": 3,
                "elapsed_seconds": 5.0,
            }
            with mock.patch.object(checks, "minimize", return_value=mock_reduced):
                with self.assertRaises(checks.Failure) as cm:
                    reducer.run_reduction(executor, root)
                self.assertIn("1-minimal-tool-failure", str(cm.exception))

            reduction_file = root / "reduction.json"
            self.assertTrue(reduction_file.is_file())
            saved = reducer.json.loads(reduction_file.read_text())
            self.assertFalse(saved["is_1_minimal"])
            self.assertIn("1-minimal-tool-failure", saved["failure_reason"])
            self.assertTrue(any("tool_failure" in entry["outcome"] for entry in saved["one_step_exhaustion"]))

    def test_unexpectedly_reducible_candidate_fails_1_minimal_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock()
            executor.deadline = 999999.0

            def case_mock(directory):
                if (directory / "fault.txt").is_file():
                    raise checks.Failure("native", "native_output_mismatch", "drop:holder", stable_witness=True)
                return None

            executor.case.side_effect = case_mock

            original_case = next(c for c in reducer.model.cases() if c["id"] == "v2-3-83")
            cand = next(reducer.model.shrink_candidates(original_case))
            mock_reduced = {
                "original": original_case,
                "original_failure": {"stage": "native", "kind": "native_output_mismatch", "witness": "drop:holder"},
                "minimal": cand,
                "status": "reproduced",
                "attempts": [{"candidate": cand, "accepted": True}],
                "confirmation_count": 3,
                "elapsed_seconds": 5.0,
            }
            with mock.patch.object(checks, "minimize", return_value=mock_reduced):
                with self.assertRaises(checks.Failure) as cm:
                    reducer.run_reduction(executor, root)
                self.assertIn("minimal-was-not-1-minimal", str(cm.exception))

            reduction_file = root / "reduction.json"
            self.assertTrue(reduction_file.is_file())
            saved = reducer.json.loads(reduction_file.read_text())
            self.assertFalse(saved["is_1_minimal"])
            self.assertEqual("minimal-was-not-1-minimal", saved["failure_reason"])


if __name__ == "__main__":
    unittest.main()

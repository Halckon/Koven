"""Unit tests for SPEC-0269 generated owner reduction acceptance."""
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest import mock

from scripts import generated_owner_reduction as reducer
from scripts import generated_owner_checks as checks


class GeneratedOwnerReductionTests(unittest.TestCase):
    def test_final_budget_write_error_preserves_primary_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock(deadline=None)
            primary = checks.Failure("reduction", "timeout", "execution-budget")
            write_json = reducer.write_json

            def fail_final_budget(path, value):
                if path.name == "budget.json" and value["status"] != "running":
                    raise OSError("budget receipt unavailable")
                write_json(path, value)

            with mock.patch.object(reducer, "_run_reduction", side_effect=primary), \
                 mock.patch.object(reducer, "write_json", side_effect=fail_final_budget):
                with self.assertRaises(checks.Failure) as caught:
                    reducer.run_reduction(executor, root)
            self.assertIs(primary, caught.exception)
            evidence = reducer.json.loads((root / "budget-write-failure.json").read_text())
            self.assertEqual(primary.record(), evidence["primary_failure"])
            self.assertEqual("budget-write-failed", evidence["witness"])

    def test_success_does_not_swallow_budget_write_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            executor = mock.Mock(deadline=None)
            write_json = reducer.write_json

            def fail_final_budget(path, value):
                if path.name == "budget.json" and value["status"] != "running":
                    raise OSError("budget receipt unavailable")
                write_json(path, value)

            with mock.patch.object(reducer, "_run_reduction", return_value={}), \
                 mock.patch.object(reducer, "write_json", side_effect=fail_final_budget):
                with self.assertRaisesRegex(OSError, "budget receipt unavailable"):
                    reducer.run_reduction(executor, Path(temporary))

    def test_final_deadline_rejects_empty_one_step_exhaustion(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock(deadline=None)
            clock = [1000.0]
            original = next(c for c in reducer.model.cases() if c["id"] == "v2-3-83")
            candidate = next(reducer.model.shrink_candidates(original))

            def case_mock(directory):
                if (directory / "fault.txt").is_file():
                    raise checks.Failure("native", "native_output_mismatch", "drop:holder",
                                         stable_witness=True)

            def minimize_at_deadline(*args, **kwargs):
                clock[0] = executor.deadline
                return dict(original=original, minimal=candidate, status="reproduced",
                            confirmation_count=3, original_failure={}, attempts=[])

            executor.case.side_effect = case_mock
            with mock.patch.object(reducer.time, "monotonic", side_effect=lambda: clock[0]), \
                 mock.patch.object(checks, "minimize", side_effect=minimize_at_deadline), \
                 mock.patch.object(reducer.model, "shrink_candidates", return_value=[]):
                with self.assertRaises(checks.Failure) as caught:
                    reducer.run_reduction(executor, root)
            self.assertEqual(("reduction", "timeout", "execution-budget"), caught.exception.fingerprint)
            saved = reducer.json.loads((root / "reduction.json").read_text())
            self.assertEqual(candidate, saved["minimal"])
            self.assertFalse(saved["is_1_minimal"])
            self.assertEqual("failed", reducer.json.loads((root / "budget.json").read_text())["status"])

    def test_expired_previous_phase_gets_one_shared_reduction_window(self):
        # CI-only generated-owner regression: no real compiler or native process.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock()
            clock = [1000.0]
            executor.deadline = 999.0
            observed = []

            def case_with_deadline(directory):
                if clock[0] >= executor.deadline:
                    raise checks.Failure("batch", "timeout", "execution-budget")
                observed.append((directory.name, executor.deadline))
                clock[0] += 0.1
                if (directory / "fault.txt").is_file():
                    raise checks.Failure("native", "native_output_mismatch", "drop:holder",
                                         stable_witness=True)

            executor.case.side_effect = case_with_deadline
            with mock.patch.object(reducer.time, "monotonic", side_effect=lambda: clock[0]):
                result = reducer.run_reduction(executor, root)
            self.assertEqual("original-clean", observed[0][0])
            self.assertEqual("original-fault", observed[1][0])
            self.assertEqual({1120.0}, {deadline for _, deadline in observed})
            self.assertEqual("reproduced", result["status"])
            self.assertEqual(3, result["confirmation_count"])
            self.assertTrue(result["is_1_minimal"])
            budget = reducer.json.loads((root / "budget.json").read_text())
            self.assertEqual("completed", budget["status"])
            self.assertEqual(120, budget["budget_seconds"])
            self.assertEqual(32, budget["max_candidates"])
            self.assertEqual(999.0, budget["previous_deadline"])
            self.assertEqual(clock[0] - 1000.0, budget["elapsed_seconds"])

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

    def test_missing_artifact_during_1_minimal_check_preserves_reduction_json(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executor = mock.Mock()
            executor.deadline = 999999.0

            def case_mock(directory):
                # Simulate a missing artifact (e.g. FileNotFoundError when checking 1minimal-test)
                if "1minimal-test" in str(directory):
                    raise FileNotFoundError("diagnostics.tsv missing")
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
                self.assertIn("1-minimal-io-failure", str(cm.exception))

            reduction_file = root / "reduction.json"
            self.assertTrue(reduction_file.is_file())
            saved = reducer.json.loads(reduction_file.read_text())
            self.assertEqual("reproduced", saved["status"])
            self.assertEqual(cand, saved["minimal"])
            self.assertFalse(saved["is_1_minimal"])
            self.assertIn("1-minimal-io-failure", saved["failure_reason"])
            self.assertTrue(any("tool_io_failure" in entry["outcome"] for entry in saved["one_step_exhaustion"]))

    def test_preparation_io_failure_during_1_minimal_check_preserves_reduction_json(self):
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
            real_prepare = reducer.gate.prepare_case
            def mock_prepare(case, directory):
                if "1minimal-test" in str(directory):
                    raise OSError("disk read error")
                return real_prepare(case, directory)

            with mock.patch.object(checks, "minimize", return_value=mock_reduced), \
                 mock.patch.object(reducer.gate, "prepare_case", side_effect=mock_prepare):
                with self.assertRaises(checks.Failure) as cm:
                    reducer.run_reduction(executor, root)
                self.assertIn("1-minimal-io-failure", str(cm.exception))

            reduction_file = root / "reduction.json"
            self.assertTrue(reduction_file.is_file())
            saved = reducer.json.loads(reduction_file.read_text())
            self.assertEqual("reproduced", saved["status"])
            self.assertFalse(saved["is_1_minimal"])
            self.assertIn("1-minimal-io-failure", saved["failure_reason"])


if __name__ == "__main__":
    unittest.main()

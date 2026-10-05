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


if __name__ == "__main__":
    unittest.main()

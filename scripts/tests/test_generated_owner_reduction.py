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

            # Mock export to produce a stages.tsv and diagnostics.tsv reporting L0131
            def export(d):
                (d / "stages.tsv").write_text("parse\t0\nnames\t0\ntypes\t0\nownership\t1\n")
                (d / "diagnostics.tsv").write_text("ownership\tL0131\t10\t15\t5:9\n")

            executor.export.side_effect = export
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

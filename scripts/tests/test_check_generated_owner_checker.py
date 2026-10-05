"""Unit tests for SPEC-0269 frontend checker mutant calibration."""
import tempfile
from pathlib import Path
import unittest
from unittest import mock

from scripts import check_generated_owner_checker as checker_gate
from scripts import generated_owner_checks as checks


class CheckerMutantTests(unittest.TestCase):
    def test_mutate_checker_replaces_exact_push_statement(self):
        original = (
            'fn ensure_place_available(&mut self) {\n'
            '    diagnostic.add_label(self.sources, origin, "value was moved here")?;\n'
            '    self.diagnostics.push(diagnostic);\n'
            '    return Ok(());\n'
            '}'
        )
        mutated = checker_gate.mutate_checker(original)
        self.assertIn("MUTANT_CHECKER_DISABLED", mutated)
        self.assertNotIn("    self.diagnostics.push(diagnostic);", mutated)
        with self.assertRaises(checks.Failure):
            checker_gate.mutate_checker("unrelated text without target")

    def test_verify_restores_source_on_exception(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / checker_gate.CHECKER_FILE
            target.parent.mkdir(parents=True, exist_ok=True)
            content = (
                'fn ensure_place_available(&mut self) {\n'
                '    diagnostic.add_label(self.sources, origin, "value was moved here")?;\n'
                '    self.diagnostics.push(diagnostic);\n'
                '}'
            )
            target.write_text(content, encoding="utf-8")
            artifacts = root / "artifacts"

            with mock.patch("subprocess.run", side_effect=RuntimeError("controlled build abort")):
                with self.assertRaises(RuntimeError):
                    checker_gate.verify_checker_mutant(root, artifacts)

            # Target file MUST be restored to original content
            self.assertEqual(content, target.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()

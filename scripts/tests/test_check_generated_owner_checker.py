"""Unit tests for SPEC-0269 frontend checker mutant calibration."""
import subprocess
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

    def test_verify_cleans_up_worktree_on_exception(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = root / "artifacts"
            calls = []

            def fake_run(cmd, *args, **kwargs):
                calls.append(cmd)
                if cmd[:3] == ["git", "worktree", "add"]:
                    # Create the fake worktree dir and checker file
                    worktree_dir = Path(cmd[3])
                    checker = worktree_dir / checker_gate.CHECKER_FILE
                    checker.parent.mkdir(parents=True, exist_ok=True)
                    checker.write_text(
                        'fn ensure_place_available(&mut self) {\n'
                        '    diagnostic.add_label(self.sources, origin, "value was moved here")?;\n'
                        '    self.diagnostics.push(diagnostic);\n'
                        '}\n', encoding="utf-8"
                    )
                    return subprocess.CompletedProcess(cmd, 0, b"", b"")
                if "cargo" in cmd:
                    raise RuntimeError("controlled build abort")
                return subprocess.CompletedProcess(cmd, 0, b"", b"")

            with mock.patch("subprocess.run", side_effect=fake_run):
                with self.assertRaises(RuntimeError):
                    checker_gate.verify_checker_mutant(root, artifacts)

            # Assert git worktree remove was called in finally
            self.assertTrue(any(c[:3] == ["git", "worktree", "remove"] for c in calls))


if __name__ == "__main__":
    unittest.main()

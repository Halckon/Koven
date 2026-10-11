from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]


class SpecDagDeterminismTests(unittest.TestCase):
    def test_parallel_draft_versions_are_stable_across_hash_seeds(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for version, number in (("v0.43", "0292"), ("v0.44", "0291")):
                path = root / "docs/specs/drafts" / version / f"{number}-candidate.md"
                path.parent.mkdir(parents=True)
                path.write_text(f"# SPEC-{number}: Candidate\n", encoding="utf-8")
            program = (
                "import json, sys; from pathlib import Path; "
                "sys.path.insert(0, sys.argv[1]); "
                "from gen_spec_dag import render_targets; "
                "root = Path(sys.argv[2]); "
                "print(json.dumps({str(p.relative_to(root)): text "
                "for p, text in render_targets(root).items()}, sort_keys=True))"
            )
            outputs = [
                subprocess.check_output(
                    [sys.executable, "-c", program, str(SCRIPTS), str(root)],
                    env={**os.environ, "PYTHONHASHSEED": str(seed)},
                    text=True,
                )
                for seed in range(16)
            ]
            self.assertTrue(all(output == outputs[0] for output in outputs[1:]))
            self.assertLess(outputs[0].index("Gdrafts_v043"), outputs[0].index("Gdrafts_v044"))


if __name__ == "__main__":
    unittest.main()

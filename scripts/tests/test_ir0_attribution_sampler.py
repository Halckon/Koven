"""P0-2：IR0 §1 所有权与降低修复历史归因统计（C1–C4）与止损门槛单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "ir0_attribution_sampler.py"
SPEC = importlib.util.spec_from_file_location("ir0_attribution_sampler", PATH)
ATTR = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = ATTR
SPEC.loader.exec_module(ATTR)


class Ir0AttributionSamplerTests(unittest.TestCase):
    def test_parse_and_classify_c1_c2_c3_c4_and_filter_non_fix_or_unrelated(self):
        """只计入触及 ownership_checking 或 codegen/ssa 的 fix 提交，并互斥归入 C1–C4。"""
        log_text = """
COMMIT 1111111 fix(ssa): handle MissingFact on nullable enum drop
crates/lang-codegen/src/ssa/drop.rs

COMMIT 2222222 fix(codegen): lower UnsupportedNode for when subject
crates/lang-codegen/src/ssa/lower.rs

COMMIT 3333333 fix(ownership): align drop order between frontend and ssa
crates/lang-frontend/src/ownership_checking/drops.rs
crates/lang-codegen/src/ssa/drops.rs

COMMIT 4444444 fix(ownership): improve diagnostic span on move error
crates/lang-frontend/src/ownership_checking/diag.rs

COMMIT 5555555 feat(ssa): add new optimization pass
crates/lang-codegen/src/ssa/opt.rs

COMMIT 6666666 fix(docs): typo in readme
docs/README.md
"""
        records = ATTR.parse_git_log_text(log_text)
        self.assertEqual(6, len(records))
        summary = ATTR.summarize_attribution(records, stop_loss_threshold=0.5)
        self.assertEqual(4, summary.total_fix_commits)
        self.assertEqual({"C1": 1, "C2": 1, "C3": 1, "C4": 1}, summary.counts)
        self.assertAlmostEqual(0.75, summary.graph_solvable_ratio)
        self.assertEqual("PROCEED_IR1_ADR", summary.recommendation)

    def test_stop_loss_downgrades_when_c4_dominates(self):
        """当 C1+C2+C3 合计占比低于止损门槛时，建议降级为仅参考检查器使用。"""
        records = [
            ATTR.CommitRecord("aaa", "fix(ownership): tweak span offset", ("crates/lang-frontend/src/ownership_checking/a.rs",)),
            ATTR.CommitRecord("bbb", "fix(ssa): fix ABI register width", ("crates/lang-codegen/src/ssa/abi.rs",)),
            ATTR.CommitRecord("ccc", "fix(ssa): fix MissingFact on loop", ("crates/lang-codegen/src/ssa/loop.rs",)),
        ]
        summary = ATTR.summarize_attribution(records, stop_loss_threshold=0.5)
        self.assertEqual(3, summary.total_fix_commits)
        self.assertEqual({"C1": 0, "C2": 1, "C3": 0, "C4": 2}, summary.counts)
        self.assertAlmostEqual(1 / 3, summary.graph_solvable_ratio)
        self.assertEqual("DOWNGRADE_REFERENCE_ONLY", summary.recommendation)


if __name__ == "__main__":
    unittest.main()

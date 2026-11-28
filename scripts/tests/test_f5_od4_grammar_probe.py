"""OD4（L2/L3 候选语法前三项）F5 增量门禁 Bison 冲突与见证单元测试（缺 bison/cc 时自动跳过）。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import shutil
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("loan_kernel_f5_od4_grammar_probe", ROOT / "f5_od4_grammar_probe.py")
PROBE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PROBE
SPEC.loader.exec_module(PROBE)

BISON = shutil.which("bison")
CC = shutil.which("cc")


@unittest.skipUnless(BISON is not None and CC is not None, "bison and cc required in PATH")
class F5Od4GrammarProbeTests(unittest.TestCase):
    def test_expected_bison_conflicts_for_baseline_increments_and_contrasts(self):
        """复现 OD4 基准、三项增量、合并文法及三组反面对照的精确 Bison 冲突计数。"""
        all_grammars = {**PROBE.GRAMMARS, "3_combined_od4": PROBE.COMBINED_GRAMMAR}
        for name, expected in PROBE.EXPECTED_CONFLICTS.items():
            with self.subTest(grammar=name):
                res = PROBE.check_grammar_conflicts(name, all_grammars[name], bison=BISON)
                self.assertEqual(0, res.returncode, res.stderr)
                self.assertEqual(expected, (res.shift_reduce, res.reduce_reduce), res.stderr)

    def test_compiled_combined_od4_witness_matrix(self):
        """编译 3_combined_od4（%expect 0）并验证 11 组正例（含 <view > 与接口成员）与 11 组反例（精确返回语法错误码 1）。"""
        pos, neg = PROBE.run_witness_matrix(bison=BISON, cc=CC)
        self.assertEqual(11, len(pos))
        self.assertEqual(11, len(neg))
        for label, ok in pos:
            with self.subTest(positive=label):
                self.assertTrue(ok, f"expected accept: {label}")
        for label, ok in neg:
            with self.subTest(negative=label):
                self.assertTrue(ok, f"expected reject: {label}")


if __name__ == "__main__":
    unittest.main()

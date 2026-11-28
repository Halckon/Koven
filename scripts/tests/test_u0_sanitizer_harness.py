"""U0 预 U1 阶段可复跑 C/消毒器代理反例单元测试（clang 缺失时自动跳过）。"""
from __future__ import annotations

import dataclasses
import importlib.util
from pathlib import Path
import shutil
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("loan_kernel_u0_sanitizer_harness", ROOT / "u0_sanitizer_harness.py")
HARNESS = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = HARNESS
SPEC.loader.exec_module(HARNESS)

CLANG = shutil.which("clang")


@unittest.skipUnless(CLANG is not None, "clang unavailable in PATH")
class U0SanitizerHarnessTests(unittest.TestCase):
    def test_all_u0_sanitizer_surrogate_cases_capture_expected_ub(self):
        """全部 10 组 U0 C/消毒器代理反例与 FFI5 终止基线均可复跑并命中预期签名。"""
        self.assertEqual(10, len(HARNESS.CASES))
        for case in HARNESS.CASES:
            with self.subTest(case=case.name):
                res = HARNESS.run_case(case, compiler=CLANG)
                self.assertTrue(res.passed, f"{case.name} failed: rc={res.returncode}, {res.summary}")

    def test_ffi5_baseline_rejects_plain_exit_1(self):
        """从真实 FFI5 条目派生负例（仅替换 C 源码为 return 1），验证其不放行普通 exit(1)。"""
        real_ffi5 = next(c for c in HARNESS.CASES if c.name == "UB9_ffi5_abort_termination_baseline")
        fake_case = dataclasses.replace(real_ffi5, source="int main(void) { return 1; }\n")
        res = HARNESS.run_case(fake_case, compiler=CLANG)
        self.assertFalse(res.passed)
        self.assertEqual(1, res.returncode)


if __name__ == "__main__":
    unittest.main()

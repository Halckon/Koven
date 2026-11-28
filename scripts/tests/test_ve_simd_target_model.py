"""P2-3：VE1 环绕/饱和算术、VE2 固定宽度向量对齐与点积等价性、VE3 目标特性分派单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "ve_simd_target_model.py"
SPEC = importlib.util.spec_from_file_location("ve_simd_target_model", PATH)
VE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = VE
SPEC.loader.exec_module(VE)


class VeSimdTargetModelTests(unittest.TestCase):
    def test_ve1_explicit_wrapping_and_saturating_preserve_default_checked(self):
        """VE1：显式 wrapping/saturating 函数不改变默认 checked + 的溢出 abort 语义。"""
        val, err = VE.checked_add_i32(VE.I32_MAX, 1)
        self.assertIsNone(val)
        self.assertIn("VE1_CHECKED_OVERFLOW", err)
        self.assertEqual(VE.I32_MIN, VE.wrapping_add_i32(VE.I32_MAX, 1))
        self.assertEqual(VE.I32_MAX, VE.saturating_add_i32(VE.I32_MAX, 1))
        self.assertEqual(VE.I32_MIN, VE.saturating_add_i32(VE.I32_MIN, -1))

    def test_ve2_simd_i32x4_alignment_and_dot_product_matches_scalar(self):
        """VE2：向量加载校验 16 字节对齐，手写向量点积（含非 4 倍数尾部）与标量版逐位一致。"""
        xs = (1, 2, 3, 4, 5, -6, 7, 8, 9, 10)
        ys = (10, -2, 3, 4, -5, 6, 7, -8, 9, 2)
        self.assertEqual(VE.dot_product_scalar(xs, ys), VE.dot_product_simd_i32x4(xs, ys))

        _, err_align = VE.SimdI32x4.load_aligned(xs, byte_offset=4)
        self.assertIn("VE2_MISALIGNED_VECTOR_ACCESS", err_align)

    def test_ve3_target_feature_guard_and_runtime_multiversion_dispatch(self):
        """VE3：无守卫直接调用 @targetFeature 报错；运行时按 CPU 特性选最高优先级实现并回退标量。"""
        avx2 = VE.TargetImpl("dot_avx2", frozenset({"avx2", "fma"}), priority=20)
        neon = VE.TargetImpl("dot_neon", frozenset({"neon"}), priority=10)
        scalar = VE.TargetImpl("dot_scalar", frozenset(), priority=0)
        candidates = (scalar, neon, avx2)

        self.assertIn("VE3_UNGUARDED_TARGET_FEATURE", VE.check_target_feature_call(frozenset(), frozenset(), avx2))
        self.assertIsNone(VE.check_target_feature_call(frozenset(), frozenset({"avx2", "fma"}), avx2))

        self.assertEqual(avx2, VE.dispatch_target_version(frozenset({"avx2", "fma"}), candidates))
        self.assertEqual(neon, VE.dispatch_target_version(frozenset({"neon"}), candidates))
        self.assertEqual(scalar, VE.dispatch_target_version(frozenset({"avx2"}), candidates))  # 缺少 fma 回退标量
        self.assertEqual(scalar, VE.dispatch_target_version(frozenset(), candidates))


if __name__ == "__main__":
    unittest.main()

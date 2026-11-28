"""P1-2：U1 布局/填充字节（MM7）、AL6 裸指针配对释放（UB10/UB11）与 FFI1/FFI3 单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "u1_layout_ffi_model.py"
SPEC = importlib.util.spec_from_file_location("u1_layout_ffi_model", PATH)
U1 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = U1
SPEC.loader.exec_module(U1)


class U1LayoutFfiModelTests(unittest.TestCase):
    def test_c_struct_layout_and_mm7_padding_read_rejected(self):
        """MM7：计算内部与尾部填充字节，读取填充字节报 UB5_PADDING_READ，读取已初始化字段合法。"""
        # struct { u8 a (0..1); pad(1..8); i64 b (8..16); u8 c (16..17); pad(17..24) } -> size=24, align=8
        layout = U1.compute_struct_layout(
            (
                U1.FieldLayoutSpec("a", 1, 1),
                U1.FieldLayoutSpec("b", 8, 8),
                U1.FieldLayoutSpec("c", 1, 1),
            )
        )
        self.assertEqual(24, layout.size)
        self.assertEqual(8, layout.align)
        self.assertEqual((("a", 0, 1), ("b", 8, 8), ("c", 16, 1)), layout.field_offsets)
        self.assertEqual(((1, 8), (17, 24)), layout.padding_ranges)

        self.assertIsNone(U1.check_mm7_read_range(layout, 0, 1))
        self.assertIsNone(U1.check_mm7_read_range(layout, 8, 8))
        self.assertIn("UB5_PADDING_READ", U1.check_mm7_read_range(layout, 1, 1))
        self.assertIn("UB5_PADDING_READ", U1.check_mm7_read_range(layout, 0, 24))

    def test_al6_provenance_oob_misaligned_and_paired_dealloc(self):
        """AL6 / UB10 / UB11 / UB4：检验越界、未对齐、内部偏移释放、布局不匹配释放与双释放。"""
        mem = U1.U1MemoryTable()
        p = mem.alloc(16, 8)
        self.assertIsNone(mem.check_deref(p, 8, 8))
        self.assertIsNone(mem.check_deref(p.offset_by(8), 8, 8))
        self.assertIn("UB10_OOB", mem.check_deref(p.offset_by(16), 1, 1))
        self.assertIn("UB10_MISALIGNED", mem.check_deref(p.offset_by(1), 8, 8))

        self.assertIn("UB11_INTERIOR_DEALLOC", mem.dealloc(p.offset_by(1), 16, 8))
        self.assertIn("UB11_LAYOUT_MISMATCH", mem.dealloc(p, 8, 8))
        self.assertIn("UB11_LAYOUT_MISMATCH", mem.dealloc(p, 16, 4))
        self.assertIsNone(mem.dealloc(p, 16, 8))
        self.assertIn("UB4_DOUBLE_FREE", mem.dealloc(p, 16, 8))
        self.assertIn("UB2_UAF", mem.check_deref(p, 8, 8))

    def test_ffi1_signature_match_and_ffi3_callback_rules(self):
        """FFI1 / FFI3：C 签名类型不匹配报 UB9_FFI1；带捕获或非 C ABI 回调报 UB9_FFI3。"""
        c_sym = U1.CAbiSig("c_read", ("*u8", "usize"), "isize", "C")
        k_ok = U1.CAbiSig("c_read", ("*u8", "usize"), "isize", "C")
        k_bad = U1.CAbiSig("c_read", ("i32", "usize"), "isize", "C")
        self.assertIsNone(U1.check_ffi1_signature(k_ok, c_sym))
        self.assertIn("UB9_FFI1_ABI_MISMATCH", U1.check_ffi1_signature(k_bad, c_sym))

        self.assertEqual([], U1.check_ffi3_callback(U1.CallbackSpec("cmp_c", "C", ())))
        errs = U1.check_ffi3_callback(U1.CallbackSpec("bad_cb", "Koven", ("ctx",)))
        self.assertTrue(any("UB9_FFI3_CAPTURING_CLOSURE" in e for e in errs))
        self.assertTrue(any("UB9_FFI3_NON_C_ABI" in e for e in errs))


if __name__ == "__main__":
    unittest.main()

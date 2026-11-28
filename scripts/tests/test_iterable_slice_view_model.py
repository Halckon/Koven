"""P1-3：StringView（UTF-8 边界零拷贝）、Slice<T> 与 Iterable/for 协议单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "iterable_slice_view_model.py"
SPEC = importlib.util.spec_from_file_location("iterable_slice_view_model", PATH)
V = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = V
SPEC.loader.exec_module(V)


class IterableSliceViewModelTests(unittest.TestCase):
    def test_string_view_utf8_boundary_and_zero_copy_tokenizer(self):
        """StringView：合法 UTF-8 切片与零拷贝分词共享原 owner 与 backing；切在多字节中间报 UTF8_BOUNDARY_ABORT。"""
        sv = V.StringView.from_owner("src", "你好 koven\t世界")
        tokens = sv.split_ascii_whitespace()
        self.assertEqual(["你好", "koven", "世界"], [t.as_str() for t in tokens])
        self.assertTrue(all(t.origin_owner == "src" and t._backing is sv._backing for t in tokens))

        # "你" 占 3 字节 [0, 3)：切 [0, 3) 合法，切 [0, 2) 落在续字节上必须拒绝
        ok_sub, err_ok = sv.slice_bytes(0, 3)
        self.assertIsNone(err_ok)
        self.assertEqual("你", ok_sub.as_str())

        bad_sub, err_bad = sv.slice_bytes(0, 2)
        self.assertIsNone(bad_sub)
        self.assertIn("UTF8_BOUNDARY_ABORT", err_bad)

        oob_sub, err_oob = sv.slice_bytes(0, 999)
        self.assertIsNone(oob_sub)
        self.assertIn("OOB_SLICE", err_oob)

    def test_slice_view_subslice_preserves_origin_and_bounds(self):
        s = V.SliceView("arr", (10, 20, 30, 40), 0, 4)
        sub, err = s.subslice(1, 3)
        self.assertIsNone(err)
        self.assertEqual((20, 30), sub.items())
        self.assertEqual("arr", sub.origin_owner)

        bad, err2 = s.subslice(2, 5)
        self.assertIsNone(bad)
        self.assertIn("OOB_SLICE", err2)

    def test_for_borrowed_vs_consuming_iteration_loan_rules(self):
        """for 协议：借用迭代期间修改集合报 FOR_LOAN_CONFLICT，循环后恢复可写；消费迭代后集合失效。"""
        xs = V.OwnedCollection("xs", ["a", "b"])
        seen = []
        for elem in xs.iter_borrow():
            seen.append(elem)
            self.assertIn("FOR_LOAN_CONFLICT", xs.mutate_push("c"))
        self.assertEqual(["a", "b"], seen)
        self.assertIsNone(xs.mutate_push("c"))

        consumed = list(xs.iter_own())
        self.assertEqual(["a", "b", "c"], consumed)
        length, err = xs.read_len()
        self.assertIsNone(length)
        self.assertIn("USE_AFTER_CONSUME_FOR", err)
        self.assertIn("USE_AFTER_CONSUME_FOR", xs.mutate_push("d"))


if __name__ == "__main__":
    unittest.main()

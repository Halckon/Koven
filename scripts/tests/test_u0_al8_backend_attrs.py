"""U0 §3.3（AL8）参数模式到后端别名与捕获属性映射单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "u0_al8_backend_attrs.py"
SPEC = importlib.util.spec_from_file_location("u0_al8_backend_attrs", PATH)
AL8 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = AL8
SPEC.loader.exec_module(AL8)


class U0Al8BackendAttrsTests(unittest.TestCase):
    def setUp(self):
        self.types = {
            "Int": AL8.TypeDef("Int"),
            "RawCellInt": AL8.TypeDef("RawCellInt", is_raw_cell=True),
            "PlainBox": AL8.TypeDef("PlainBox", fields=("Int",)),
            "NestedCellBox": AL8.TypeDef("NestedCellBox", fields=("PlainBox", "RawCellInt")),
            "RecPlain": AL8.TypeDef("RecPlain", fields=("RecPlain", "Int")),
            "RecWithCell": AL8.TypeDef("RecWithCell", fields=("RecWithCell", "NestedCellBox")),
            "RcBox": AL8.TypeDef("RcBox", fields=("PlainBox",), is_shared_handle=True),
        }

    def test_borrow_plain_vs_recursive_rawcell(self):
        """AL8：不含 RawCell 的 borrow 可发 readonly+nocapture；递归含 RawCell（含环类型）严禁 readonly 与 noalias。"""
        safe = AL8.BodyEffects()
        p_plain = AL8.ParamSpec("x", "borrow", "RecPlain")
        self.assertEqual(
            frozenset({"readonly", "nonnull", "align", "nocapture"}),
            AL8.derive_backend_attrs(p_plain, self.types, safe),
        )

        p_cell = AL8.ParamSpec("c", "borrow", "RecWithCell")
        self.assertEqual(
            frozenset({"nonnull", "align", "nocapture"}),
            AL8.derive_backend_attrs(p_cell, self.types, safe),
        )
        errs = AL8.audit_proposed_attrs(p_cell, frozenset({"readonly", "noalias", "nonnull"}), self.types, safe)
        self.assertTrue(any("AL8_FORBIDDEN_READONLY" in e for e in errs))
        self.assertTrue(any("AL8_FORBIDDEN_NOALIAS" in e for e in errs))

    def test_inout_keeps_noalias_even_with_rawcell(self):
        """AL8：inout 即使所指递归含 RawCell 仍独占访问，允许 noalias，禁止 readonly。"""
        safe = AL8.BodyEffects()
        p_inout = AL8.ParamSpec("m", "inout", "NestedCellBox")
        self.assertEqual(
            frozenset({"noalias", "nonnull", "align", "nocapture"}),
            AL8.derive_backend_attrs(p_inout, self.types, safe),
        )
        errs = AL8.audit_proposed_attrs(p_inout, frozenset({"readonly", "noalias"}), self.types, safe)
        self.assertTrue(any("AL8_FORBIDDEN_ATTR" in e for e in errs))

    def test_own_unique_vs_shared_handle_rc(self):
        """AL8：唯一 owner 的 own 间接指针允许 noalias；Rc/Arc 共享句柄严禁对底层载荷标 noalias。"""
        safe = AL8.BodyEffects()
        p_unique = AL8.ParamSpec("b", "own", "PlainBox", indirect_pointer=True)
        self.assertEqual(
            frozenset({"noalias", "nonnull", "align"}),
            AL8.derive_backend_attrs(p_unique, self.types, safe),
        )
        p_rc = AL8.ParamSpec("r", "own", "RcBox", indirect_pointer=True)
        self.assertEqual(
            frozenset({"nonnull", "align"}),
            AL8.derive_backend_attrs(p_rc, self.types, safe),
        )
        errs = AL8.audit_proposed_attrs(p_rc, frozenset({"noalias", "nonnull"}), self.types, safe)
        self.assertTrue(any("AL8_FORBIDDEN_NOALIAS" in e for e in errs))

    def test_nocapture_requires_both_no_from_flows_and_capture_proof(self):
        """AL8：缺少 from/flows 单独不足以发 nocapture；from/flows、unsafe 地址逃逸或未证实被调方均禁止 nocapture。"""
        p_from = AL8.ParamSpec("x", "borrow", "PlainBox", in_from_or_flows=True)
        self.assertNotIn("nocapture", AL8.derive_backend_attrs(p_from, self.types, AL8.BodyEffects()))

        p_plain = AL8.ParamSpec("x", "borrow", "PlainBox", in_from_or_flows=False)
        for bad_effects in (
            AL8.BodyEffects(has_unsafe_address_escape=True),
            AL8.BodyEffects(calls_unverified_callee=True),
            AL8.BodyEffects(escaped_params=frozenset({"x"})),
        ):
            with self.subTest(effects=bad_effects):
                self.assertNotIn("nocapture", AL8.derive_backend_attrs(p_plain, self.types, bad_effects))
                errs = AL8.audit_proposed_attrs(
                    p_plain, frozenset({"readonly", "nonnull", "align", "nocapture"}), self.types, bad_effects
                )
                self.assertTrue(any("AL8_FORBIDDEN_NOCAPTURE" in e for e in errs))


if __name__ == "__main__":
    unittest.main()

"""P1-1：J38 条件实现（where 多约束）、J37 重叠联动与 L5 泛型放开单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "generic_conditional_impl_model.py"
SPEC = importlib.util.spec_from_file_location("generic_conditional_impl_model", PATH)
G = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = G
SPEC.loader.exec_module(G)

TyCon = G.TyCon
TyVar = G.TyVar


class GenericConditionalImplTests(unittest.TestCase):
    def setUp(self):
        self.t_int = TyCon("Int", "core")
        self.t_rc_int = TyCon("Rc", "alloc", (self.t_int,))
        self.t_list_int = TyCon("List", "alloc", (self.t_int,))
        self.t_list_list_int = TyCon("List", "alloc", (self.t_list_int,))
        self.t_list_rc = TyCon("List", "alloc", (self.t_rc_int,))
        self.t_pair_ok = TyCon("Pair", "core", (self.t_int, self.t_list_int))
        self.t_pair_bad = TyCon("Pair", "core", (self.t_int, self.t_rc_int))

        self.impls = (
            G.ConditionalImpl("core", "Transferable", "core", self.t_int, at_type_decl=True),
            G.ConditionalImpl("core", "Equatable", "core", self.t_int, at_type_decl=True),
            G.ConditionalImpl("core", "Equatable", "core", TyCon("Rc", "alloc", (TyVar("T"),)), (("T", "Equatable"),)),
            G.ConditionalImpl(
                "alloc",
                "Transferable",
                "core",
                TyCon("List", "alloc", (TyVar("T"),)),
                (("T", "Transferable"),),
                at_type_decl=True,
            ),
            G.ConditionalImpl(
                "alloc",
                "Equatable",
                "core",
                TyCon("List", "alloc", (TyVar("T"),)),
                (("T", "Equatable"),),
                at_type_decl=True,
            ),
            G.ConditionalImpl(
                "core",
                "Transferable",
                "core",
                TyCon("Pair", "core", (TyVar("A"), TyVar("B"))),
                (("A", "Transferable"), ("B", "Transferable")),
                at_type_decl=True,
            ),
        )

    def test_recursive_conditional_impl_resolution_and_where_multi_bounds(self):
        """J38：递归求解 List<List<Int>> 与 Pair<Int, List<Int>> 的条件能力，拒绝含 Rc 的组合。"""
        self.assertEqual([], G.check_conditional_coherence(self.impls))
        self.assertTrue(G.satisfies_bound(self.t_list_int, "Transferable", self.impls))
        self.assertTrue(G.satisfies_bound(self.t_list_list_int, "Equatable", self.impls))
        self.assertFalse(G.satisfies_bound(self.t_list_rc, "Transferable", self.impls))
        self.assertTrue(G.satisfies_bound(self.t_list_rc, "Equatable", self.impls))

        self.assertEqual(
            [],
            G.check_where_clause({"X": self.t_pair_ok}, (("X", "Transferable"),), self.impls),
        )
        errs = G.check_where_clause({"X": self.t_pair_bad}, (("X", "Transferable"),), self.impls)
        self.assertTrue(any("J38_UNSATISFIED_BOUND" in e for e in errs))

    def test_j37_rejects_overlapping_conditional_impls_even_with_different_where_clauses(self):
        """J37 + J38：头部均可合一的两个条件实现，即使 where 条件不同仍判重叠。"""
        overlapping = (
            G.ConditionalImpl("core", "Display", "core", TyCon("List", "alloc", (TyVar("T"),)), (("T", "Copyable"),)),
            G.ConditionalImpl("core", "Display", "core", TyCon("List", "alloc", (TyVar("T"),)), (("T", "Equatable"),)),
        )
        self.assertTrue(any("J37_CONDITIONAL_OVERLAP" in e for e in G.check_conditional_coherence(overlapping)))

    def test_l5_view_type_parameter_relaxes_default_escapable_bound(self):
        """L5：默认泛型参数拒绝非逃逸视图，声明 view T 后放行。"""
        self.assertIsNotNone(G.check_l5_view_generic_instantiation(param_allows_view=False, actual_is_escapable=False))
        self.assertIsNone(G.check_l5_view_generic_instantiation(param_allows_view=True, actual_is_escapable=False))
        self.assertIsNone(G.check_l5_view_generic_instantiation(param_allows_view=False, actual_is_escapable=True))


if __name__ == "__main__":
    unittest.main()

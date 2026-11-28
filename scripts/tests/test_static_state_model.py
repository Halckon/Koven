"""顶层状态、静态存储（G0–G4）与之六 J5–J7 单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "static_state_model.py"
SPEC = importlib.util.spec_from_file_location("static_state_model", PATH)
S = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = S
SPEC.loader.exec_module(S)


class StaticStateModelTests(unittest.TestCase):
    def setUp(self):
        self.int_ty = S.StaticType("Int", copyable=True, has_deinit=False, transferable=True, shareable=True)
        self.table_ty = S.StaticType("LookupTable", copyable=False, has_deinit=False, transferable=True, shareable=True)
        self.rc_ty = S.StaticType("RcInt", copyable=False, has_deinit=True, transferable=False, shareable=False)
        self.guard_ty = S.StaticType("MutexGuard", copyable=False, has_deinit=True, transferable=False, shareable=False)
        self.atomic_ty = S.StaticType(
            "AtomicInt", copyable=False, has_deinit=False, transferable=True, shareable=True, is_interior_mut_shareable=True
        )
        self.lazy_ty = S.StaticType(
            "LazyConfig", copyable=False, has_deinit=False, transferable=True, shareable=True, is_lazy_shell=True
        )

    def test_g1_narrowing_rejects_runtime_globals_and_keeps_const_val(self):
        """G1：v1 收窄顶层与 object/companion 接纳面，拒绝运行时 val/var，保留 const val。"""
        c = S.GlobalDecl("MAX", "const_val", self.int_ty, static_constructible=True)
        self.assertEqual([], S.check_global_decl(c, stage="G1"))

        rv = S.GlobalDecl("G_VAL", "runtime_val", self.int_ty, static_constructible=False)
        self.assertTrue(any("G1_UNSUPPORTED_RUNTIME_GLOBAL" in e for e in S.check_global_decl(rv, stage="G1")))

        gvar = S.GlobalDecl("G_VAR", "runtime_var", self.int_ty, static_constructible=True)
        self.assertTrue(any("C1_GLOBAL_VAR_FORBIDDEN" in e for e in S.check_global_decl(gvar, stage="G3_G4")))

    def test_g2_static_immutable_constraints_access_and_j5_origin(self):
        """G2 + J5：纯内存且满足能力约束的静态不可变表只允许 borrow（不可 move/inout/assign），且返回需声明 Static(decl)。"""
        tbl = S.GlobalDecl("TABLE", "static_val", self.table_ty, static_constructible=True)
        self.assertEqual([], S.check_global_decl(tbl, stage="G2_v1"))
        self.assertIsNone(S.check_static_access(tbl, "borrow"))
        self.assertIsNotNone(S.check_static_access(tbl, "copy_read"))
        for bad_op in ("move_out", "inout", "assign"):
            self.assertIsNotNone(S.check_static_access(tbl, bad_op))

        self.assertIsNone(S.check_static_origin_return("TABLE", frozenset({"Static(TABLE)"})))
        self.assertIsNotNone(S.check_static_origin_return("TABLE", frozenset({"a"})))

        # Rc 与 MutexGuard 含 deinit / 不可共享，禁止进入静态存储
        bad_rc = S.GlobalDecl("BAD_RC", "static_val", self.rc_ty, static_constructible=True)
        errs = S.check_global_decl(bad_rc, stage="G2_v2")
        self.assertTrue(any("G2_OBSERVABLE_DROP_FORBIDDEN" in e for e in errs))
        self.assertTrue(any("G2_CAP_REQUIRED" in e for e in errs))

        bad_guard = S.GlobalDecl("BAD_GUARD", "static_val", self.guard_ty, static_constructible=True)
        self.assertTrue(any("G2_OBSERVABLE_DROP_FORBIDDEN" in e for e in S.check_global_decl(bad_guard, stage="G3_G4")))

    def test_g2_init_cycle_detected(self):
        """G0/G2：静态声明初始化依赖成环被拒绝。"""
        decls = {
            "A": S.GlobalDecl("A", "static_val", self.int_ty, deps=("B",)),
            "B": S.GlobalDecl("B", "static_val", self.int_ty, deps=("A",)),
        }
        self.assertTrue(any("G2_INIT_CYCLE" in e for e in S.check_static_init_cycles(decls)))

    def test_g3_g4_lazy_once_reentrancy_and_atomic_allowed_only_in_v2(self):
        """G3/G4：AtomicInt 与 Lazy 仅在 v2 Shareable 后允许；Lazy 只初始化一次且重入触发 abort。"""
        hits = S.GlobalDecl("HITS", "static_val", self.atomic_ty, static_constructible=True)
        self.assertTrue(any("G2_V1_FORBIDS_INTERIOR_MUT" in e for e in S.check_global_decl(hits, stage="G2_v1")))
        self.assertEqual([], S.check_global_decl(hits, stage="G3_G4"))

        cfg = S.GlobalDecl("CFG", "static_val", self.lazy_ty, static_constructible=True)
        self.assertEqual([], S.check_global_decl(cfg, stage="G3_G4"))

        cell = S.LazyCell("CFG")
        self.assertEqual(42, cell.get(lambda: 42))
        self.assertEqual(42, cell.get(lambda: 99))
        self.assertEqual(1, cell.init_count)

        reentrant = S.LazyCell("BAD_LAZY")
        with self.assertRaisesRegex(RuntimeError, "G3_REENTRANT_ABORT"):
            reentrant.get(lambda: reentrant.get(lambda: 1))


if __name__ == "__main__":
    unittest.main()

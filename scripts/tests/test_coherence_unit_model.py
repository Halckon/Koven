"""编译单元依赖（J45）、接口产物哈希（J46）、语言版本（J47）与一致性规则（J37）单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "coherence_unit_model.py"
SPEC = importlib.util.spec_from_file_location("coherence_unit_model", PATH)
M = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = M
SPEC.loader.exec_module(M)


class CoherenceUnitModelTests(unittest.TestCase):
    def test_j45_inner_package_cycles_allowed_and_outer_unit_cycle_reports_shortest(self):
        """J45：同单元内 package 允许成环；跨单元依赖图必须无环且报告最短环路。"""
        stdlib_dag = {
            "koven.core": M.CompilationUnit("koven.core", package_edges=(("ast", "visitor"), ("visitor", "ast"))),
            "koven.alloc": M.CompilationUnit("koven.alloc", deps=("koven.core",)),
            "koven.std": M.CompilationUnit("koven.std", deps=("koven.core", "koven.alloc")),
        }
        self.assertIsNone(M.find_shortest_unit_cycle(stdlib_dag))

        cyclic = {
            "unitA": M.CompilationUnit("unitA", deps=("unitB",)),
            "unitB": M.CompilationUnit("unitB", deps=("unitC",)),
            "unitC": M.CompilationUnit("unitC", deps=("unitA",)),
        }
        self.assertEqual(["unitA", "unitB", "unitC", "unitA"], M.find_shortest_unit_cycle(cyclic))

    def test_j45_visibility_and_whitebox_vs_blackbox_tests(self):
        """J45 + §4.3：白盒单测可见 internal，黑盒集成测试仅可见 public，private 跨单元不可见。"""
        lib = M.CompilationUnit(
            "lib",
            symbols=(
                M.SymbolDecl("api", "public", "fun", "fun api(): Unit"),
                M.SymbolDecl("helper", "internal", "fun", "fun helper(): Unit"),
                M.SymbolDecl("secret", "private", "fun", "fun secret(): Unit"),
            ),
        )
        self.assertIsNone(M.check_visibility("app", lib, "api"))
        self.assertIsNotNone(M.check_visibility("app", lib, "helper"))
        self.assertIsNone(M.check_visibility("lib_whitebox_test", lib, "helper", is_whitebox_test=True))
        self.assertIsNotNone(M.check_visibility("lib_blackbox_test", lib, "helper", is_whitebox_test=False))
        self.assertIsNotNone(M.check_visibility("lib_whitebox_test", lib, "secret", is_whitebox_test=True))

    def test_j37_orphan_blanket_and_first_order_unification_overlap(self):
        """J37：类型实参区分不同实现但不绕过孤儿规则，禁止 R0-R1 一揽子实现，一阶合一拦截重叠实现。"""
        int_ty = M.TyCon("Int", "core")
        str_ty = M.TyCon("String", "core")
        local_ty = M.TyCon("MyKey", "app")
        list_int = M.TyCon("List", "alloc", (int_ty,))
        list_str = M.TyCon("List", "alloc", (str_ty,))
        list_t = M.TyCon("List", "alloc", (M.TyVar("T"),))
        list_local = M.TyCon("List", "alloc", (local_ty,))
        pair_t_int = M.TyCon("Pair", "core", (M.TyVar("T"), int_ty))
        pair_str_u = M.TyCon("Pair", "core", (str_ty, M.TyVar("U")))

        # 合法：不同类型实参 List<Int> 与 List<String> 在接口所在单元并存，不冲突。
        ok_units = {
            "core": M.CompilationUnit(
                "core",
                impls=(
                    M.ImplDecl("core", "Display", "core", list_int),
                    M.ImplDecl("core", "Display", "core", list_str),
                ),
            ),
        }
        self.assertEqual([], M.check_coherence(ok_units))

        # 冲突 1：List<T> 与 List<Int> 一阶合一重叠。
        overlap_1 = {
            "core": M.CompilationUnit(
                "core",
                impls=(
                    M.ImplDecl("core", "Display", "core", list_t),
                    M.ImplDecl("core", "Display", "core", list_int),
                ),
            ),
        }
        self.assertTrue(any("J37_OVERLAP" in e for e in M.check_coherence(overlap_1)))

        # 冲突 2：Pair<T, Int> 与 Pair<String, U> 经双向替换合一重叠。
        overlap_2 = {
            "core": M.CompilationUnit(
                "core",
                impls=(
                    M.ImplDecl("core", "Hashable", "core", pair_t_int),
                    M.ImplDecl("core", "Hashable", "core", pair_str_u),
                ),
            ),
        }
        self.assertTrue(any("J37_OVERLAP" in e for e in M.check_coherence(overlap_2)))

        # 孤儿 1：类型实参 MyKey 在 app 内，但头类型 List 在 alloc、接口 Display 在 core，不能绕过孤儿规则。
        orphan_arg = {
            "app": M.CompilationUnit(
                "app",
                impls=(M.ImplDecl("app", "Display", "core", list_local, at_type_decl=False),),
            ),
        }
        self.assertTrue(any("J37_ORPHAN" in e for e in M.check_coherence(orphan_arg)))

        # 孤儿 2：类型所属单元其他文件零散追加外部接口实现（at_type_decl=False）亦被拒绝。
        scattered = {
            "app": M.CompilationUnit(
                "app",
                impls=(M.ImplDecl("app", "Display", "core", local_ty, at_type_decl=False),),
            ),
        }
        self.assertTrue(any("J37_ORPHAN" in e for e in M.check_coherence(scattered)))

        # 一揽子实现：R0-R1 禁止 impl <T> for Display。
        blanket = {
            "core": M.CompilationUnit(
                "core",
                impls=(M.ImplDecl("core", "Display", "core", M.TyVar("T")),),
            ),
        }
        self.assertTrue(any("J37_BLANKET_FORBIDDEN" in e for e in M.check_coherence(blanket)))

    def test_j46_interface_artifact_hash_invalidation_and_schema_mismatch(self):
        """J46：修改无关私有函数体不改哈希；修改公开泛型传递调用的私有泛型体必改哈希；schema 不匹配快速失败。"""
        def make_dep(priv_body: str, priv_gen_body: str, schema: int = 1) -> M.CompilationUnit:
            return M.CompilationUnit(
                "dep",
                schema_version=schema,
                symbols=(
                    M.SymbolDecl("pub_fn", "public", "fun", "fun pub_fn(): Int", body="return priv_helper()"),
                    M.SymbolDecl("priv_helper", "private", "fun", "fun priv_helper(): Int", body=priv_body),
                    M.SymbolDecl("pub_gen", "public", "generic_fun", "fun <T> pub_gen(x: T): T",
                                 body="return priv_gen(x)", callees=("priv_gen",)),
                    M.SymbolDecl("priv_gen", "private", "generic_fun", "fun <T> priv_gen(x: T): T",
                                 body=priv_gen_body),
                ),
            )

        a1, e1 = M.build_interface_artifact(make_dep("return 1", "return x"), {})
        a2, e2 = M.build_interface_artifact(make_dep("return 2", "return x"), {})
        a3, e3 = M.build_interface_artifact(make_dep("return 1", "return identity(x)"), {})
        self.assertEqual([], e1)
        self.assertEqual([], e2)
        self.assertEqual([], e3)
        self.assertEqual(a1.content_hash, a2.content_hash, "非泛型私有函数体变化不应改变接口产物哈希")
        self.assertNotEqual(a1.content_hash, a3.content_hash, "公开泛型传递调用的私有泛型体变化必须改变哈希")

        bad, errs = M.build_interface_artifact(make_dep("return 1", "return x", schema=99), {})
        self.assertIsNone(bad)
        self.assertTrue(any("J46_SCHEMA_MISMATCH" in e for e in errs))

    def test_j47_language_version_isolation_and_min_caller_version(self):
        """J47：1.0 单元可把 2.0 新保留字作标识符且默认可依赖 2.0 单元；若声明 min_caller_language_version 则明确拦截。"""
        u1 = M.CompilationUnit("old_app", language_version="1.0", deps=("new_lib",), identifiers=("view", "flows"))
        u2_ok = M.CompilationUnit("new_lib", language_version="2.0", identifiers=("view",))
        self.assertEqual([], M.check_unit_keywords(u1))
        self.assertTrue(any("J47_RESERVED_KEYWORD" in e for e in M.check_unit_keywords(u2_ok)))

        art_ok, errs_ok = M.build_interface_artifact(u2_ok, {})
        self.assertEqual([], errs_ok)
        _, caller_errs = M.build_interface_artifact(u1, {"new_lib": art_ok})
        self.assertEqual([], caller_errs, "默认允许 1.0 单元依赖 2.0 单元")

        u2_gated = M.CompilationUnit("new_lib", language_version="2.0", min_caller_language_version="2.0")
        art_gated, _ = M.build_interface_artifact(u2_gated, {})
        _, gated_errs = M.build_interface_artifact(u1, {"new_lib": art_gated})
        self.assertTrue(any("J47_MIN_CALLER_VERSION" in e for e in gated_errs))


if __name__ == "__main__":
    unittest.main()

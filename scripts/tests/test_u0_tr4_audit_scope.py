"""U0 §7（TR2–TR4）受信任声明与人工审查范围单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "u0_tr4_audit_scope.py"
SPEC = importlib.util.spec_from_file_location("u0_tr4_audit_scope", PATH)
TR = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = TR
SPEC.loader.exec_module(TR)


class U0Tr4AuditScopeTests(unittest.TestCase):
    def test_tr2_vs_tr3_trusted_capability_and_logical_interfaces(self):
        """TR2 / TR3：Transferable/Shareable 手工声明须标 trusted；Equatable/Hashable/Comparable 不要求 unsafe。"""
        self.assertEqual([], TR.check_tr2_tr3_capability_impl(TR.CapabilityImplSpec("Arc", "Shareable", True)))
        self.assertTrue(
            any("TR2_REQUIRES_TRUSTED" in e for e in TR.check_tr2_tr3_capability_impl(TR.CapabilityImplSpec("Arc", "Transferable", False)))
        )
        self.assertEqual([], TR.check_tr2_tr3_capability_impl(TR.CapabilityImplSpec("MyKey", "Hashable", False)))
        self.assertTrue(
            any("TR3_UNNECESSARY_UNSAFE" in e for e in TR.check_tr2_tr3_capability_impl(TR.CapabilityImplSpec("MyKey", "Comparable", True)))
        )

    def test_tr4_invariant_and_borrow_fields_must_be_private(self):
        """TR4.1：含 trusted/unsafe 的类型，借用字段与不变量字段必须为 private。"""
        ok_list = TR.TypeAuditSpec(
            name="RawVec",
            file_path="vec.kv",
            fields=(
                TR.FieldSpec("ptr", "private", invariant_maintained_by_unsafe=True),
                TR.FieldSpec("len", "private", invariant_maintained_by_unsafe=True),
                TR.FieldSpec("tag", "public", invariant_maintained_by_unsafe=False),
            ),
            has_trusted_signature=True,
            uses_unsafe_invariant=True,
        )
        self.assertEqual([], TR.check_tr4_private_fields(ok_list))

        bad_list = TR.TypeAuditSpec(
            name="BadVec",
            file_path="vec.kv",
            fields=(
                TR.FieldSpec("len", "public", invariant_maintained_by_unsafe=True),
                TR.FieldSpec("view_ref", "internal", is_borrow_field=True),
            ),
            has_trusted_signature=True,
        )
        errs = TR.check_tr4_private_fields(bad_list)
        self.assertEqual(2, len(errs))
        self.assertTrue(all("TR4_NON_PRIVATE_INVARIANT_FIELD" in e for e in errs))

    def test_tr4_audit_scope_classifier_members_only_and_expands_if_file_wide(self):
        """TR4.2 / TR4.3：Guide 02 下审查范围是该类型全部成员（含非 unsafe 成员，不含同文件顶层/扩展函数）；若扩大为文件级则同步扩大。"""
        vec = TR.TypeAuditSpec(
            name="RawVec",
            file_path="vec.kv",
            fields=(TR.FieldSpec("len", "private", invariant_maintained_by_unsafe=True),),
            uses_unsafe_invariant=True,
        )
        funcs = (
            TR.FunctionSpec("RawVec.push_unsafe", "vec.kv", owner_type="RawVec", is_trusted_or_unsafe=True),
            TR.FunctionSpec("RawVec.set_len_safe_member", "vec.kv", owner_type="RawVec", is_trusted_or_unsafe=False),
            TR.FunctionSpec("vec_top_level_helper", "vec.kv", owner_type="", is_trusted_or_unsafe=False),
            TR.FunctionSpec("RawVec.ext_pretty_print", "vec.kv", owner_type="RawVec", is_extension=True),
            TR.FunctionSpec("Other.method", "other.kv", owner_type="Other", is_trusted_or_unsafe=False),
        )
        guide02_scope = TR.compute_tr4_audit_scope(vec, funcs, private_visibility_model="classifier")
        self.assertEqual(frozenset({"RawVec.push_unsafe", "RawVec.set_len_safe_member"}), guide02_scope)

        file_wide_scope = TR.compute_tr4_audit_scope(vec, funcs, private_visibility_model="file_wide")
        self.assertEqual(
            frozenset({"RawVec.push_unsafe", "RawVec.set_len_safe_member", "vec_top_level_helper", "RawVec.ext_pretty_print"}),
            file_wide_scope,
        )


if __name__ == "__main__":
    unittest.main()

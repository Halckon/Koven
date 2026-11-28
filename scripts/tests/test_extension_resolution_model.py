"""受限与完整扩展函数（步骤 1→2→3 / 之六 J9–J10）单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "extension_resolution_model.py"
SPEC = importlib.util.spec_from_file_location("extension_resolution_model", PATH)
EXT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = EXT
SPEC.loader.exec_module(EXT)


class ExtensionResolutionModelTests(unittest.TestCase):
    def test_step1_to_step3_staged_diagnostics_and_intrinsic_identity(self):
        """§3.1 / §6.5：步骤 1 按编译器身份区分 intrinsic List 与同名用户 List；步骤 2/3 依次放开 L1/L2；全阶段禁访 private。"""
        std_list = EXT.CallableDecl("filter", "intrinsic:List", "borrow", ("Pred",), "koven.collections")
        user_named_list = EXT.CallableDecl("filter", "user:app::List", "borrow", ("Pred",), "koven.collections")
        user_pkg_ext = EXT.CallableDecl("filter", "intrinsic:List", "borrow", ("Pred",), "app.utils")
        priv_ext = EXT.CallableDecl(
            "peek", "intrinsic:List", "borrow", (), "koven.collections", accesses_receiver_private=True
        )

        self.assertEqual([], EXT.check_extension_declaration(std_list, step=1))
        self.assertTrue(any("EXT_L1_NON_INTRINSIC" in e for e in EXT.check_extension_declaration(user_named_list, step=1)))
        self.assertEqual([], EXT.check_extension_declaration(user_named_list, step=2))
        self.assertTrue(any("EXT_L2_NON_STDLIB" in e for e in EXT.check_extension_declaration(user_pkg_ext, step=1)))
        self.assertTrue(any("EXT_L2_NON_STDLIB" in e for e in EXT.check_extension_declaration(user_pkg_ext, step=2)))
        self.assertEqual([], EXT.check_extension_declaration(user_pkg_ext, step=3))
        self.assertTrue(any("EXT_PRIVATE_ACCESS_DENIED" in e for e in EXT.check_extension_declaration(priv_ext, step=3)))

    def test_member_wins_over_extension_and_same_shape_extensions_conflict(self):
        """§3.4 / J9：member 永远优先于同形扩展函数；两个可见同形扩展报歧义；receiver_mode 不构成重载。"""
        recv = EXT.ReceiverType("List", "intrinsic:List")
        member_size = EXT.CallableDecl("count", "intrinsic:List", "borrow", (), "koven.core", is_member=True)
        ext_a = EXT.CallableDecl("count", "intrinsic:List", "borrow", (), "pkg.a", is_member=False)
        ext_b = EXT.CallableDecl("count", "intrinsic:List", "borrow", (), "pkg.b", is_member=False)

        chosen, err = EXT.resolve_call(recv, "count", (), (member_size,), (ext_a, ext_b))
        self.assertIsNone(err)
        self.assertEqual(member_size, chosen)

        chosen2, err2 = EXT.resolve_call(recv, "count", (), (), (ext_a, ext_b))
        self.assertIsNone(chosen2)
        self.assertIn("EXT_AMBIGUOUS", err2)

        ext_borrow = EXT.CallableDecl("consume", "intrinsic:List", "borrow", (), "koven.collections")
        ext_own = EXT.CallableDecl("consume", "intrinsic:List", "own", (), "koven.collections")
        self.assertTrue(
            any("EXT_RECEIVER_MODE_OVERLOAD" in e for e in EXT.check_duplicate_receiver_mode_overloads((ext_borrow, ext_own)))
        )

    def test_j10_extension_never_satisfies_interface(self):
        """之六 J10：即使可见作用域存在签名完全匹配的扩展函数，也绝不能使类型满足 interface。"""
        recv = EXT.ReceiverType("List", "intrinsic:List")
        ext_display = EXT.CallableDecl("format", "intrinsic:List", "borrow", (), "koven.fmt", is_member=False)
        errs = EXT.check_interface_satisfaction(recv, (("format", ()),), (), (ext_display,))
        self.assertTrue(any("J10_EXT_NOT_INTERFACE_IMPL" in e for e in errs))


if __name__ == "__main__":
    unittest.main()

"""路线 X（X1–X4 存在类型）与 E2/E3 通用错误转换单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "existential_route_x_model.py"
SPEC = importlib.util.spec_from_file_location("existential_route_x_model", PATH)
X = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = X
SPEC.loader.exec_module(X)


class ExistentialRouteXModelTests(unittest.TestCase):
    def setUp(self):
        self.visitor_iface = X.InterfaceDecl(
            "ExprVisitor",
            (
                X.MethodSig("onLit", receiver_mode="inout", param_types=("Int",)),
                X.MethodSig("onAdd", receiver_mode="inout"),
            ),
        )
        self.eq_self_iface = X.InterfaceDecl(
            "EquatableSelf",
            (X.MethodSig("eq", param_types=("Self",), return_type="Boolean"),),
        )
        self.generic_method_iface = X.InterfaceDecl(
            "Mapper",
            (X.MethodSig("map", type_params=("U",), param_types=("U",), return_type="U"),),
        )

    def test_x1_dynamic_compatibility_rejects_self_and_generic_methods(self):
        """XN4：含 Self 参数或泛型方法的 interface 不可进入值位置。"""
        self.assertEqual([], X.check_dynamic_compatible(self.visitor_iface))
        self.assertTrue(any("X1_SELF_IN_PARAM" in e for e in X.check_dynamic_compatible(self.eq_self_iface)))
        self.assertTrue(any("X1_GENERIC_METHOD" in e for e in X.check_dynamic_compatible(self.generic_method_iface)))

    def test_x2_borrow_and_inout_existential_zero_alloc_and_reject_overwrite_and_escape(self):
        """X2 + XN1 + XN3：栈上 value class 与普通 class 均零分配打包；载荷可变访问禁整体替换且禁无 from 逃逸。"""
        counter = X.ConcreteType("NodeCounter", "value_class", implements=frozenset({"ExprVisitor"}))
        summer = X.ConcreteType("LitSumVisitor", "class", implements=frozenset({"ExprVisitor"}))

        v1, e1 = X.pack_borrowed(counter, self.visitor_iface, mode="inout")
        v2, e2 = X.pack_borrowed(summer, self.visitor_iface, mode="inout")
        self.assertEqual([], e1)
        self.assertEqual([], e2)
        self.assertEqual(0, v1.extra_allocations)
        self.assertEqual(0, v2.extra_allocations)
        self.assertIn("X2_PAYLOAD_OVERWRITE_FORBIDDEN", X.check_overwrite_existential(v1, v2))
        self.assertIn("X2_BORROW_EXISTENTIAL_ESCAPE", X.check_escape_existential(v1, has_from_contract=False))
        self.assertIsNone(X.check_escape_existential(v1, has_from_contract=True))

    def test_x3_owned_existential_zero_alloc_class_box_for_value_class_and_container_invariance(self):
        """X3 + XN2：普通 class 零额外分配 pack_owned；value class 在 X3 须显式 Box（X4 可隐式装箱）；容器不协变。"""
        audit = X.ConcreteType("AuditHandler", "class", implements=frozenset({"ExprVisitor"}))
        metric = X.ConcreteType("MetricHandler", "value_class", implements=frozenset({"ExprVisitor"}))
        boxed_metric = X.ConcreteType("Box<MetricHandler>", "box", implements=frozenset({"ExprVisitor"}))

        o1, e1 = X.pack_owned(audit, self.visitor_iface, stage="X3")
        self.assertEqual([], e1)
        self.assertEqual(0, o1.extra_allocations)

        _, e_inline = X.pack_owned(metric, self.visitor_iface, stage="X3")
        self.assertTrue(any("X3_INLINE_REQUIRES_BOX" in e for e in e_inline))

        o2, e2 = X.pack_owned(boxed_metric, self.visitor_iface, stage="X3")
        self.assertEqual([], e2)
        self.assertEqual(0, o2.extra_allocations)
        self.assertIsNone(X.check_overwrite_existential(o1, o2))

        o_x4, e_x4 = X.pack_owned(metric, self.visitor_iface, stage="X4")
        self.assertEqual([], e_x4)
        self.assertEqual(1, o_x4.extra_allocations)

        self.assertIn("X3_CONTAINER_INVARIANT", X.check_container_coercion("AuditHandler", "EventHandler"))
        self.assertIsNone(X.check_container_coercion("EventHandler", "EventHandler"))

    def test_e2_e3_explicit_into_required_and_cancel_never_converts(self):
        """之十一 E2/E3：仅实现 Error 不隐式转换，必须显式声明 Into<Error>；Cancelled 永不隐式转为 Error。"""
        io_implicit = X.ConcreteType("IoError", "enum_class", implements=frozenset({"Error"}))
        io_explicit = X.ConcreteType("IoError", "enum_class", implements=frozenset({"Error"}), into_targets=frozenset({"Error"}))
        cancelled = X.ConcreteType("Cancelled", "enum_class")

        self.assertIn("E2_MISSING_EXPLICIT_INTO", X.check_e2_error_conversion(io_implicit, "Error"))
        self.assertIsNone(X.check_e2_error_conversion(io_explicit, "Error"))
        self.assertIn("E2_FORBIDDEN_CANCEL_TO_ERR", X.check_e2_error_conversion(cancelled, "Error"))


if __name__ == "__main__":
    unittest.main()

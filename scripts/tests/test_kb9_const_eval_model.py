"""P2-2：KB9 图上封闭编译期求值器（溢出、步数上限、value class 构造与副作用拒绝）单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "kb9_const_eval_model.py"
SPEC = importlib.util.spec_from_file_location("kb9_const_eval_model", PATH)
KB9 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = KB9
SPEC.loader.exec_module(KB9)


class Kb9ConstEvalModelTests(unittest.TestCase):
    def test_loop_sum_and_value_class_static_construction(self):
        """在 CFG 上循环求和 1+2+3+4=10 并构造纯内存 value class Point(10, 'ok')。"""
        g = KB9.ConstGraph(
            name="const_point",
            entry="bb0",
            blocks={
                "bb0": KB9.ConstBlock(
                    "bb0",
                    (
                        KB9.ConstInstr("ConstInt", "sum", (0,)),
                        KB9.ConstInstr("ConstInt", "i", (1,)),
                        KB9.ConstInstr("ConstInt", "limit", (5,)),
                        KB9.ConstInstr("ConstInt", "one", (1,)),
                        KB9.ConstInstr("ConstStr", "tag", ("ok",)),
                    ),
                    KB9.ConstTerm("Goto", target="bb_cond"),
                ),
                "bb_cond": KB9.ConstBlock(
                    "bb_cond",
                    (KB9.ConstInstr("CmpLt", "c", ("i", "limit")),),
                    KB9.ConstTerm("Branch", cond_reg="c", then_bb="bb_body", else_bb="bb_done"),
                ),
                "bb_body": KB9.ConstBlock(
                    "bb_body",
                    (
                        KB9.ConstInstr("AddChecked", "sum", ("sum", "i")),
                        KB9.ConstInstr("AddChecked", "i", ("i", "one")),
                    ),
                    KB9.ConstTerm("Goto", target="bb_cond"),
                ),
                "bb_done": KB9.ConstBlock(
                    "bb_done",
                    (
                        KB9.ConstInstr(
                            "ConstructValueClass",
                            "pt",
                            ("sum", "tag"),
                            type_name="StaticConfig",
                            field_names=("total", "label"),
                        ),
                    ),
                    KB9.ConstTerm("Return", ret_reg="pt"),
                ),
            },
        )
        val, err, steps = KB9.eval_const_graph(g, max_steps=100)
        self.assertIsNone(err)
        self.assertEqual(KB9.ValueRecord("StaticConfig", (("total", 10), ("label", "ok"))), val)
        self.assertLess(steps, 100)

    def test_overflow_div_zero_infinite_loop_and_runtime_effect_rejected(self):
        """验证整数溢出、除零、死循环步数上限与运行时副作用全部结构化拒绝。"""
        overflow_g = KB9.ConstGraph(
            "ovf",
            "bb0",
            {
                "bb0": KB9.ConstBlock(
                    "bb0",
                    (
                        KB9.ConstInstr("ConstInt", "a", (KB9.I32_MAX,)),
                        KB9.ConstInstr("ConstInt", "b", (1,)),
                        KB9.ConstInstr("AddChecked", "r", ("a", "b")),
                    ),
                    KB9.ConstTerm("Return", ret_reg="r"),
                )
            },
        )
        _, err_ovf, _ = KB9.eval_const_graph(overflow_g)
        self.assertIn("KB9_ARITH_OVERFLOW", err_ovf)

        div0_g = KB9.ConstGraph(
            "div0",
            "bb0",
            {
                "bb0": KB9.ConstBlock(
                    "bb0",
                    (
                        KB9.ConstInstr("ConstInt", "a", (10,)),
                        KB9.ConstInstr("ConstInt", "b", (0,)),
                        KB9.ConstInstr("DivChecked", "r", ("a", "b")),
                    ),
                    KB9.ConstTerm("Return", ret_reg="r"),
                )
            },
        )
        _, err_div, _ = KB9.eval_const_graph(div0_g)
        self.assertIn("KB9_DIV_BY_ZERO", err_div)

        inf_g = KB9.ConstGraph("inf", "bb0", {"bb0": KB9.ConstBlock("bb0", (), KB9.ConstTerm("Goto", target="bb0"))})
        _, err_inf, _ = KB9.eval_const_graph(inf_g, max_steps=20)
        self.assertIn("KB9_STEP_LIMIT_EXCEEDED", err_inf)

        eff_g = KB9.ConstGraph(
            "eff",
            "bb0",
            {
                "bb0": KB9.ConstBlock(
                    "bb0",
                    (KB9.ConstInstr("RuntimeEffect", type_name="File.read"),),
                    KB9.ConstTerm("Return", ret_reg=""),
                )
            },
        )
        _, err_eff, _ = KB9.eval_const_graph(eff_g)
        self.assertIn("KB9_FORBIDDEN_RUNTIME_EFFECT", err_eff)


if __name__ == "__main__":
    unittest.main()

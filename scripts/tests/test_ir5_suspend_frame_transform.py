"""P2-1：IR5 挂起帧状态机变换（S0.1 稳定地址、S5 parallel 可转移、销毁逆序清理）单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"


def _load(name: str):
    mod_name = f"loan_kernel_{name}"
    if mod_name in sys.modules:
        return sys.modules[mod_name]
    spec = importlib.util.spec_from_file_location(mod_name, ROOT / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = mod
    spec.loader.exec_module(mod)
    return mod


IR0 = _load("ir0_graph")
LOWER = _load("ir1_ir2_lowering")
IR5 = _load("ir5_suspend_frame_transform")
S = IR0.Span(0, 10)
P = IR0.Place


class Ir5SuspendFrameTransformTests(unittest.TestCase):
    def test_real_lk_suspend_function_lowers_to_state_machine_with_lifo_destroy_order(self):
        """对真实 cases/P4.lk 中的挂起函数执行 P0-1 构图 + P2-1 状态机变换，核对存活局部与逆序清理。"""
        text = (ROOT / "cases" / "P4.lk").read_text(encoding="utf-8")
        graphs, _ = LOWER.lower_lk_to_ownership_graphs(text, early_drop=False)
        self.assertIn("sameThreadIsFine", graphs)
        frame = IR5.transform_suspend_frame(graphs["sameThreadIsFine"], is_parallel=False)
        self.assertEqual((), frame.errors)
        self.assertGreaterEqual(len(frame.states), 1)
        # P4.lk 中 sameThreadIsFine 先声明 g 再声明 f，故销毁逆序中 f 先于 g
        self.assertTrue(
            any("f" in st.live_locals and "g" in st.live_locals and st.drop_order_on_destroy.index("f") < st.drop_order_on_destroy.index("g")
                for st in frame.states)
        )

    def test_s01_internal_borrow_requires_stable_frame_and_s5_parallel_rejects_local_rc(self):
        """S0.1：跨挂起点的帧内借用要求帧地址稳定（frames_movable=True 报 S1）；S5：parallel 拒绝跨挂起点 Rc。"""
        graph = IR0.OwnershipGraph(
            name="self_ref_parallel",
            status="Ok",
            locals={
                "buf": IR0.LocalDecl("buf", "Buffer"),
                "rc": IR0.LocalDecl("rc", "RcInt"),
            },
            entry="bb0",
            blocks={
                "bb0": IR0.BasicBlock(
                    "bb0",
                    (
                        IR0.GraphStmt("Init", S, "Direct", place=P("buf")),
                        IR0.GraphStmt("Init", S, "Direct", place=P("rc")),
                        IR0.GraphStmt("LoanBegin", S, "Direct", place=P("buf"), loan="L1", loan_kind="Shared"),
                        IR0.GraphStmt("SuspendPoint", S, "Direct"),
                        IR0.GraphStmt("LoanEnd", S, "Direct", loan="L1"),
                        IR0.GraphStmt("Drop", S, "ImplicitDrop", place=P("rc"), drop_policy="Immediate"),
                        IR0.GraphStmt("Drop", S, "ImplicitDrop", place=P("buf"), drop_policy="Immediate"),
                    ),
                    IR0.GraphTerminator("Return", S),
                )
            },
        )
        ok_frame = IR5.transform_suspend_frame(
            graph, is_parallel=False, non_transferable_types=frozenset({"RcInt"}), frames_movable=False
        )
        self.assertEqual((), ok_frame.errors)
        self.assertTrue(ok_frame.requires_stable_address)
        self.assertEqual(("rc", "buf"), ok_frame.states[0].drop_order_on_destroy)

        bad_frame = IR5.transform_suspend_frame(
            graph, is_parallel=True, non_transferable_types=frozenset({"RcInt"}), frames_movable=True
        )
        self.assertTrue(any("S1_MOVED_SELF_REF_FRAME" in e for e in bad_frame.errors))
        self.assertTrue(any("S5_NON_TRANSFERABLE_ACROSS_SUSPEND" in e for e in bad_frame.errors))


if __name__ == "__main__":
    unittest.main()

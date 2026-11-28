"""IR0 所有权图分层校验与故障注入测试（对齐 docs/proposals/ir0-ownership-graph-draft.md §2）。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "ir0_graph.py"
SPEC = importlib.util.spec_from_file_location("ir0_graph", PATH)
IR0 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = IR0
SPEC.loader.exec_module(IR0)

S = IR0.Span(0, 10)
P = IR0.Place


class Ir0GraphTests(unittest.TestCase):
    def test_layer1_rejects_poisoned_and_structural_faults(self):
        """IR0 §2.5 #1 + #3：拒绝 Poisoned 图、悬空块/局部/loan、非法 Span/Origin 与带后继的 FatalAbort。"""
        base_locals = {"x": IR0.LocalDecl("x", "File")}
        poisoned = IR0.OwnershipGraph(
            name="poisoned",
            status="Poisoned",
            locals=base_locals,
            entry="bb0",
            blocks={"bb0": IR0.BasicBlock("bb0", (), IR0.GraphTerminator("Return", S))},
        )
        self.assertTrue(any("STRUCT_POISONED" in e for e in IR0.verify_structure(poisoned)))

        faulty = IR0.OwnershipGraph(
            name="faulty",
            status="Ok",
            locals=base_locals,
            entry="bb0",
            blocks={
                "bb0": IR0.BasicBlock(
                    "bb0",
                    (
                        IR0.GraphStmt("Init", IR0.Span(5, 2), "Direct", place=P("x")),
                        IR0.GraphStmt("Move", S, "UnknownOrigin", place=P("ghost")),
                        IR0.GraphStmt("LoanEnd", S, "Direct", loan="L_missing"),
                    ),
                    IR0.GraphTerminator("FatalAbort", S, target="bb_nowhere", reason="deinit_fail"),
                )
            },
        )
        errs = IR0.verify_structure(faulty)
        self.assertTrue(any("STRUCT_SPAN" in e for e in errs))
        self.assertTrue(any("STRUCT_ORIGIN" in e for e in errs))
        self.assertTrue(any("STRUCT_LOCAL" in e for e in errs))
        self.assertTrue(any("STRUCT_LOAN" in e for e in errs))
        self.assertTrue(any("STRUCT_FATAL_ABORT" in e for e in errs))
        self.assertTrue(any("STRUCT_BLOCK" in e for e in errs))

    def test_layer1_does_not_reject_user_use_after_move_caught_by_layer2(self):
        """IR0 §2.5 #1 分层边界：用户源码二次移动图结构完好（第一层通过），交由第二层报 USE_AFTER_MOVE。"""
        graph = IR0.OwnershipGraph(
            name="double_move",
            status="Ok",
            locals={"x": IR0.LocalDecl("x", "File")},
            entry="bb0",
            blocks={
                "bb0": IR0.BasicBlock(
                    "bb0",
                    (
                        IR0.GraphStmt("Init", S, "Direct", place=P("x")),
                        IR0.GraphStmt("Move", S, "Direct", place=P("x")),
                        IR0.GraphStmt("Move", S, "Direct", place=P("x")),
                    ),
                    IR0.GraphTerminator("Return", S),
                )
            },
        )
        self.assertEqual([], IR0.verify_structure(graph))
        errs = IR0.verify_ir2_invariants(graph)
        self.assertTrue(any("USE_AFTER_MOVE" in e for e in errs))

    def test_orthogonal_lexical_required_with_conditional_drop_flag_and_fault_injection(self):
        """IR0 §2.3：LexicalRequired 与 Conditional(DropFlagId) 正交组合；去掉标志故障注入必须报 DOUBLE_DROP。"""
        def build(use_flag: bool) -> IR0.OwnershipGraph:
            return IR0.OwnershipGraph(
                name="cond_drop",
                status="Ok",
                locals={"g": IR0.LocalDecl("g", "Guard", drop_policy="LexicalRequired")},
                entry="bb0",
                blocks={
                    "bb0": IR0.BasicBlock(
                        "bb0",
                        (
                            IR0.GraphStmt("Init", S, "Direct", place=P("g")),
                            IR0.GraphStmt("SetFlag", S, "ImplicitDrop", drop_flag="f_g", flag_value=True),
                        ),
                        IR0.GraphTerminator("Branch", S, targets=("bb_then", "bb_else")),
                    ),
                    "bb_then": IR0.BasicBlock(
                        "bb_then",
                        (
                            IR0.GraphStmt("Move", S, "Direct", place=P("g")),
                            IR0.GraphStmt("SetFlag", S, "ImplicitDrop", drop_flag="f_g", flag_value=False),
                        ),
                        IR0.GraphTerminator("Goto", S, target="bb_join"),
                    ),
                    "bb_else": IR0.BasicBlock(
                        "bb_else",
                        (),
                        IR0.GraphTerminator("Goto", S, target="bb_join"),
                    ),
                    "bb_join": IR0.BasicBlock(
                        "bb_join",
                        (
                            IR0.GraphStmt(
                                "Drop",
                                S,
                                "ImplicitDrop",
                                place=P("g"),
                                drop_policy="LexicalRequired",
                                drop_flag="f_g" if use_flag else None,
                            ),
                        ),
                        IR0.GraphTerminator("Return", S),
                    ),
                },
            )

        self.assertEqual([], IR0.verify_ir2_invariants(build(use_flag=True)))
        fault_errs = IR0.verify_ir2_invariants(build(use_flag=False))
        self.assertTrue(any("DOUBLE_DROP" in e for e in fault_errs))

    def test_call_failure_edge_cleanup_vs_fatal_abort(self):
        """IR0 §2.3（FD0 预留 #3）：Call failure_bb 须清理活跃资源，漏清报 LEAK_ON_RETURN；FatalAbort 不要求析构。"""
        def build(drop_on_fail: bool, fatal: bool) -> IR0.OwnershipGraph:
            fail_stmts = (
                (IR0.GraphStmt("Drop", S, "ImplicitDrop", place=P("f"), drop_policy="LexicalRequired"),)
                if drop_on_fail
                else ()
            )
            term = (
                IR0.GraphTerminator("FatalAbort", S, reason="deinit_failed")
                if fatal
                else IR0.GraphTerminator("Call", S, normal_bb="bb_ok", failure_bb="bb_fail")
            )
            blocks = {
                "bb0": IR0.BasicBlock(
                    "bb0",
                    (IR0.GraphStmt("Init", S, "Direct", place=P("f")),),
                    term,
                ),
            }
            if not fatal:
                blocks["bb_ok"] = IR0.BasicBlock(
                    "bb_ok",
                    (IR0.GraphStmt("Drop", S, "ImplicitDrop", place=P("f"), drop_policy="LexicalRequired"),),
                    IR0.GraphTerminator("Return", S),
                )
                blocks["bb_fail"] = IR0.BasicBlock(
                    "bb_fail",
                    fail_stmts,
                    IR0.GraphTerminator("Return", S),
                )
            return IR0.OwnershipGraph(
                name="call_fail_edge",
                status="Ok",
                locals={"f": IR0.LocalDecl("f", "File", drop_policy="LexicalRequired")},
                entry="bb0",
                blocks=blocks,
            )

        self.assertEqual([], IR0.verify_ir2_invariants(build(drop_on_fail=True, fatal=False)))
        leak_errs = IR0.verify_ir2_invariants(build(drop_on_fail=False, fatal=False))
        self.assertTrue(any("LEAK_ON_RETURN" in e for e in leak_errs))
        self.assertEqual([], IR0.verify_ir2_invariants(build(drop_on_fail=False, fatal=True)))

    def test_two_phase_loan_activate_allows_ended_shared_and_rejects_live_shared(self):
        """IR0 §2.3：LoanActivate 区分实参期已结束的临时共享读与持续到激活点的共享借用冲突。"""
        def build(end_shared_before_activate: bool) -> IR0.OwnershipGraph:
            stmts = [
                IR0.GraphStmt("Init", S, "Direct", place=P("v")),
                IR0.GraphStmt("LoanBegin", S, "TwoPhaseReceiver", place=P("v"), loan="L_recv", loan_kind="Reserved"),
                IR0.GraphStmt("LoanBegin", S, "Direct", place=P("v"), loan="L_arg", loan_kind="Shared"),
            ]
            if end_shared_before_activate:
                stmts.append(IR0.GraphStmt("LoanEnd", S, "Direct", loan="L_arg"))
            stmts.extend([
                IR0.GraphStmt("LoanActivate", S, "TwoPhaseReceiver", loan="L_recv"),
                IR0.GraphStmt("LoanEnd", S, "TwoPhaseReceiver", loan="L_recv"),
            ])
            if not end_shared_before_activate:
                stmts.append(IR0.GraphStmt("LoanEnd", S, "Direct", loan="L_arg"))
            stmts.append(IR0.GraphStmt("Drop", S, "ImplicitDrop", place=P("v"), drop_policy="Immediate"))
            return IR0.OwnershipGraph(
                name="two_phase",
                status="Ok",
                locals={"v": IR0.LocalDecl("v", "Vec")},
                entry="bb0",
                blocks={"bb0": IR0.BasicBlock("bb0", tuple(stmts), IR0.GraphTerminator("Return", S))},
            )

        self.assertEqual([], IR0.verify_ir2_invariants(build(end_shared_before_activate=True)))
        conflict_errs = IR0.verify_ir2_invariants(build(end_shared_before_activate=False))
        self.assertTrue(any("TWO_PHASE_ACTIVATE_CONFLICT" in e for e in conflict_errs))

    def test_shadow_diff_blocks_ir3_switch_until_all_diffs_categorized(self):
        """IR0 §2.5 #3：生产检查器与图参考检查器存在未归因差异时禁止 IR3 切换权威，全部归入四类后放行。"""
        prod = {
            "ok_case": IR0.DiagnosticOutcome("ok_case", True),
            "span_diff": IR0.DiagnosticOutcome("span_diff", False, ("E_MOVE",), IR0.Span(10, 15)),
            "relaxed": IR0.DiagnosticOutcome("relaxed", False, ("E_BORROW",), IR0.Span(20, 25)),
        }
        graph = {
            "ok_case": IR0.DiagnosticOutcome("ok_case", True),
            "span_diff": IR0.DiagnosticOutcome("span_diff", False, ("E_MOVE",), IR0.Span(10, 18)),
            "relaxed": IR0.DiagnosticOutcome("relaxed", True),
        }
        blocked = IR0.evaluate_shadow_diff(prod, graph, {"relaxed": "ApprovedRelaxation"})
        self.assertFalse(blocked.ir3_switch_allowed)
        self.assertEqual(("span_diff",), blocked.unexplained)

        allowed = IR0.evaluate_shadow_diff(
            prod,
            graph,
            {"span_diff": "GraphBuilderBug", "relaxed": "ApprovedRelaxation"},
        )
        self.assertTrue(allowed.ir3_switch_allowed)
        self.assertEqual(1, allowed.matched)
        self.assertEqual((), allowed.unexplained)


if __name__ == "__main__":
    unittest.main()

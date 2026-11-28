"""P0-1：.lk → IR0 OwnershipGraph 构图、IR2 展开与真实全语料影子差分单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("ir1_ir2_lowering", ROOT / "ir1_ir2_lowering.py")
LOWER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = LOWER
SPEC.loader.exec_module(LOWER)


class Ir1Ir2LoweringTests(unittest.TestCase):
    def test_real_lk_corpus_shadow_diff_passes_ir3_switch_gate(self):
        """在全部真实 cases/*.lk 整值切片函数上跑 IR1 构图 + IR2 校验 + 影子差分，未归因差异必须为零。"""
        cases = sorted((ROOT / "cases").glob("*.lk"))
        for early in (False, True):
            with self.subTest(early_drop=early):
                report, total_graphs, _ = LOWER.run_corpus_shadow_diff(cases, early_drop=early)
                self.assertGreater(total_graphs, 150, "受支持整值切片应覆盖 150+ 真实 .lk 函数")
                self.assertEqual((), report.unexplained, f"存在未归因差分: {report.unexplained[:5]}")
                self.assertTrue(report.ir3_switch_allowed)
                self.assertEqual(total_graphs, report.matched)

    def test_lowered_z48_al2_drop_barrier_graph_passes_structure_and_ir2(self):
        """验证 Z48.lk（含 AL2 drop_barrier）构图为 OwnershipGraph 后通过两层校验。"""
        text = (ROOT / "cases" / "Z48.lk").read_text(encoding="utf-8")
        graphs, _ = LOWER.lower_lk_to_ownership_graphs(text, early_drop=True)
        self.assertIn("al2_drop_barrier_keeps_owner_alive", graphs)
        g = graphs["al2_drop_barrier_keeps_owner_alive"]
        self.assertEqual([], LOWER.IR0.verify_structure(g))
        self.assertEqual([], LOWER.IR0.verify_ir2_invariants(g))

    def test_fault_injection_on_lowered_failure_bb_and_drop_flag_is_caught(self):
        """对真实构图后的 OwnershipGraph 注入故障（清空 failure_bb 清理或去掉条件析构标志），第二层必须稳定拦截。"""
        src = """
class File(fd: Int) deinit
fun mayFail(): Unit trusted
fun consume(own f: File): Unit trusted
fun cond_move(): Unit {
    val f = File(1)
    if (*) {
        consume(move f)
    }
    mayFail()
}
"""
        graphs, _ = LOWER.lower_lk_to_ownership_graphs(src, early_drop=True)
        g = graphs["cond_move"]
        self.assertEqual([], LOWER.IR0.verify_ir2_invariants(g))

        # 故障注入 1：清空某个 failure_bb 的清理语句 -> 必须报 LEAK_ON_RETURN
        fail_bb_id = next(bid for bid, bb in g.blocks.items() if bid.startswith("bb_fail") and bb.stmts)
        mutated_blocks = dict(g.blocks)
        orig_fail = mutated_blocks[fail_bb_id]
        mutated_blocks[fail_bb_id] = LOWER.IR0.BasicBlock(fail_bb_id, (), orig_fail.terminator)
        g_leak = LOWER.IR0.OwnershipGraph(g.name, g.status, g.locals, g.entry, mutated_blocks)
        self.assertTrue(any("LEAK_ON_RETURN" in e for e in LOWER.IR0.verify_ir2_invariants(g_leak)))

        # 故障注入 2：把合流处条件 Drop 的 drop_flag 抹掉 -> 必须报 DOUBLE_DROP
        mutated_blocks2 = dict(g.blocks)
        found_cond_drop = False
        for bid, bb in g.blocks.items():
            new_stmts = []
            for st in bb.stmts:
                if st.kind == "Drop" and st.drop_flag is not None and not found_cond_drop:
                    found_cond_drop = True
                    new_stmts.append(
                        LOWER.IR0.GraphStmt(
                            "Drop",
                            st.span,
                            st.origin,
                            place=st.place,
                            drop_policy=st.drop_policy,
                            drop_flag=None,
                        )
                    )
                else:
                    new_stmts.append(st)
            mutated_blocks2[bid] = LOWER.IR0.BasicBlock(bid, tuple(new_stmts), bb.terminator)
        self.assertTrue(found_cond_drop)
        g_double = LOWER.IR0.OwnershipGraph(g.name, g.status, g.locals, g.entry, mutated_blocks2)
        self.assertTrue(any("DOUBLE_DROP" in e for e in LOWER.IR0.verify_ir2_invariants(g_double)))


if __name__ == "__main__":
    unittest.main()

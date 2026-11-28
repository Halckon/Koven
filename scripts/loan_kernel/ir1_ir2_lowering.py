"""P0-1：.lk / 借用检查器输出 → IR0 OwnershipGraph 构图、IR2 展开与真实影子差分原型。

非规范、不被编译器引用。对齐 docs/proposals/ir0-ownership-graph-draft.md §2 与之八 §2.5（IR1–IR3）：
- 将 checker.py 对受支持切片产出的显式析构程序（含 let/set/move/drop/if/loop/break/call/suspend 与失败清理清单）
  构图为标准 CFG 形态的 ir0_graph.OwnershipGraph；
- 用 ir0_graph.verify_structure（第一层）与 ir0_graph.verify_ir2_invariants（第二层）独立检验每张图；
- 与 checker.py / DP.Verifier 的接受判定在真实 cases/*.lk 语料上跑 ir0_graph.evaluate_shadow_diff，
  检验声明支持切片内的 IR3 权威切换门槛（未归因差异必须为零）。
"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
from typing import Dict, List, Optional, Set, Tuple

ROOT = Path(__file__).resolve().parent


def _load_sibling(name: str):
    mod_name = f"loan_kernel_{name}"
    if mod_name in sys.modules:
        return sys.modules[mod_name]
    spec = importlib.util.spec_from_file_location(mod_name, ROOT / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = module
    spec.loader.exec_module(module)
    return module


CHECKER = _load_sibling("checker")
IR0 = _load_sibling("ir0_graph")

UNSUPPORTED_PARTIAL_KINDS = frozenset({"open", "take", "replace", "this"})


def _has_unsupported_stmt(stmts) -> bool:
    for s in stmts:
        if s.kind in UNSUPPORTED_PARTIAL_KINDS:
            return True
        if _has_unsupported_stmt(s.body) or _has_unsupported_stmt(s.orelse):
            return True
    return False


def _collect_locals(types: Dict[str, object], stmts) -> Dict[str, IR0.LocalDecl]:
    locals_map: Dict[str, IR0.LocalDecl] = {}

    def walk(body) -> None:
        for s in body:
            if s.kind in ("let", "set") and s.var:
                ty_name = s.ty or "Unknown"
                tdecl = types.get(ty_name)
                policy = "LexicalRequired" if (tdecl is not None and getattr(tdecl, "deinit", False)) else "Immediate"
                locals_map.setdefault(
                    s.var,
                    IR0.LocalDecl(
                        name=s.var,
                        ty=ty_name,
                        move_only=True,
                        drop_policy=policy,
                        is_temp=s.var.startswith("#tmp"),
                    ),
                )
            walk(s.body)
            walk(s.orelse)

    walk(stmts)
    return locals_map


class _GraphBuilder:
    def __init__(self, fun_name: str, locals_map: Dict[str, IR0.LocalDecl]):
        self.fun_name = fun_name
        self.locals = locals_map
        self.blocks: Dict[str, IR0.BasicBlock] = {}
        self._counter = 0
        self.cur_id = self._new_id("bb_entry")
        self.cur_stmts: List[IR0.GraphStmt] = []

    def _new_id(self, prefix: str) -> str:
        self._counter += 1
        return f"{prefix}_{self._counter}"

    def _span(self, line: int) -> IR0.Span:
        start = max(0, int(line))
        return IR0.Span(start, start + 1)

    def _finish_block(self, term: IR0.GraphTerminator) -> None:
        self.blocks[self.cur_id] = IR0.BasicBlock(self.cur_id, tuple(self.cur_stmts), term)
        self.cur_stmts = []

    def _start_block(self, block_id: str) -> None:
        self.cur_id = block_id
        self.cur_stmts = []

    def _drop_stmt(self, var: str, line: int, if_owned: bool) -> List[IR0.GraphStmt]:
        decl = self.locals.get(var)
        policy = decl.drop_policy if decl is not None else "Immediate"
        span = self._span(line)
        flag = f"flag_{var}" if if_owned else None
        out = [
            IR0.GraphStmt(
                "Drop",
                span,
                "ImplicitDrop",
                place=IR0.Place(var),
                drop_policy=policy,
                drop_flag=flag,
            )
        ]
        if not if_owned:
            out.append(IR0.GraphStmt("SetFlag", span, "ImplicitDrop", drop_flag=f"flag_{var}", flag_value=False))
        return out

    def _emit_cleanup_block(self, cleanup: List[Tuple[str, bool]], line: int) -> str:
        fail_id = self._new_id("bb_fail")
        stmts: List[IR0.GraphStmt] = []
        for var, if_owned in cleanup:
            if var in self.locals:
                stmts.extend(self._drop_stmt(var, line, if_owned))
        self.blocks[fail_id] = IR0.BasicBlock(
            fail_id,
            tuple(stmts),
            IR0.GraphTerminator("Return", self._span(line), "ImplicitDrop"),
        )
        return fail_id

    def lower_seq(self, stmts, loop_exit: Optional[str] = None) -> bool:
        """返回序列末尾是否仍可达（False 表示已由 break 提前跳出）。"""
        for s in stmts:
            span = self._span(s.line)
            if s.kind == "let":
                if s.cleanup:
                    fail_id = self._emit_cleanup_block(s.cleanup, s.line)
                    cont_id = self._new_id("bb_let_ok")
                    self._finish_block(IR0.GraphTerminator("Call", span, "Direct", normal_bb=cont_id, failure_bb=fail_id))
                    self._start_block(cont_id)
                self.cur_stmts.append(IR0.GraphStmt("Init", span, "Direct", place=IR0.Place(s.var)))
                self.cur_stmts.append(
                    IR0.GraphStmt("SetFlag", span, "ImplicitDrop", drop_flag=f"flag_{s.var}", flag_value=True)
                )
            elif s.kind == "set":
                self.cur_stmts.append(IR0.GraphStmt("Init", span, "Direct", place=IR0.Place(s.var)))
                self.cur_stmts.append(
                    IR0.GraphStmt("SetFlag", span, "ImplicitDrop", drop_flag=f"flag_{s.var}", flag_value=True)
                )
            elif s.kind == "move":
                self.cur_stmts.append(IR0.GraphStmt("Move", span, "Direct", place=IR0.Place(s.var)))
                self.cur_stmts.append(
                    IR0.GraphStmt("SetFlag", span, "ImplicitDrop", drop_flag=f"flag_{s.var}", flag_value=False)
                )
            elif s.kind == "drop":
                self.cur_stmts.extend(self._drop_stmt(s.var, s.line, bool(s.if_owned)))
            elif s.kind == "use":
                # 借用/使用不改变所有权状态，不发射重复 Move
                continue
            elif s.kind in ("call", "suspend"):
                if s.kind == "suspend":
                    self.cur_stmts.append(IR0.GraphStmt("SuspendPoint", span, "Direct"))
                fail_id = self._emit_cleanup_block(s.cleanup, s.line)
                cont_id = self._new_id(f"bb_{s.kind}_ok")
                self._finish_block(IR0.GraphTerminator("Call", span, "Direct", normal_bb=cont_id, failure_bb=fail_id))
                self._start_block(cont_id)
            elif s.kind == "block":
                if not self.lower_seq(s.body, loop_exit=loop_exit):
                    return False
            elif s.kind == "break":
                assert loop_exit is not None
                self._finish_block(IR0.GraphTerminator("Goto", span, "Direct", target=loop_exit))
                return False
            elif s.kind == "if":
                then_id = self._new_id("bb_then")
                else_id = self._new_id("bb_else")
                join_id = self._new_id("bb_join")
                self._finish_block(IR0.GraphTerminator("Branch", span, "Direct", targets=(then_id, else_id)))

                self._start_block(then_id)
                then_live = self.lower_seq(s.body, loop_exit=loop_exit)
                if then_live:
                    self._finish_block(IR0.GraphTerminator("Goto", span, "Direct", target=join_id))

                self._start_block(else_id)
                else_live = self.lower_seq(s.orelse, loop_exit=loop_exit)
                if else_live:
                    self._finish_block(IR0.GraphTerminator("Goto", span, "Direct", target=join_id))

                if not then_live and not else_live:
                    return False
                self._start_block(join_id)
            elif s.kind == "loop":
                head_id = self._new_id("bb_loop_head")
                body_id = self._new_id("bb_loop_body")
                exit_id = self._new_id("bb_loop_exit")
                self._finish_block(IR0.GraphTerminator("Goto", span, "Direct", target=head_id))

                self._start_block(head_id)
                self._finish_block(IR0.GraphTerminator("Branch", span, "Direct", targets=(body_id, exit_id)))

                self._start_block(body_id)
                body_live = self.lower_seq(s.body, loop_exit=exit_id)
                if body_live:
                    self._finish_block(IR0.GraphTerminator("Goto", span, "Direct", target=head_id))

                self._start_block(exit_id)
        return True

    def build(self, stmts) -> IR0.OwnershipGraph:
        entry_id = self.cur_id
        if self.lower_seq(stmts):
            self._finish_block(IR0.GraphTerminator("Return", self._span(0), "Direct"))
        return IR0.OwnershipGraph(
            name=self.fun_name,
            status="Ok",
            locals=self.locals,
            entry=entry_id,
            blocks=self.blocks,
        )


def lower_lk_to_ownership_graphs(
    text: str,
    early_drop: bool = False,
) -> Tuple[Dict[str, IR0.OwnershipGraph], List[str]]:
    """将 .lk 源码中所有被接受且位于整值切片内的函数构图为 IR0 OwnershipGraph。

    返回 (graphs_by_fun, skipped_partial_field_funs)。
    """
    lowered: Dict[str, tuple] = {}
    CHECKER.check(text, early_drop=early_drop, lowered=lowered)
    graphs: Dict[str, IR0.OwnershipGraph] = {}
    skipped: List[str] = []
    for fun_name, (dp_types, program, accepted) in sorted(lowered.items()):
        if not accepted:
            continue
        if fun_name.startswith("deinit ") or _has_unsupported_stmt(program):
            skipped.append(fun_name)
            continue
        locals_map = _collect_locals(dp_types, program)
        graphs[fun_name] = _GraphBuilder(fun_name, locals_map).build(program)
    return graphs, skipped


def run_corpus_shadow_diff(
    case_paths: List[Path],
    early_drop: bool = False,
) -> Tuple[IR0.ShadowDiffReport, int, int]:
    """在真实 .lk 语料上对比 DP.Verifier（参考析构检查器）与 IR0 OwnershipGraph 第二层校验器。"""
    prod_outcomes: Dict[str, IR0.DiagnosticOutcome] = {}
    graph_outcomes: Dict[str, IR0.DiagnosticOutcome] = {}
    total_graphs = 0
    total_skipped = 0

    for path in sorted(case_paths):
        text = path.read_text(encoding="utf-8")
        lowered: Dict[str, tuple] = {}
        CHECKER.check(text, early_drop=early_drop, lowered=lowered)
        graphs, skipped = lower_lk_to_ownership_graphs(text, early_drop=early_drop)
        total_skipped += len(skipped)
        for fun_name, graph in sorted(graphs.items()):
            total_graphs += 1
            cid = f"{path.stem}::{fun_name}"
            dp_types, program, _ = lowered[fun_name]
            dp_violations = CHECKER.DP.Verifier(dp_types).run_all(program)
            ir2_violations = IR0.verify_ir2_invariants(graph)
            prod_outcomes[cid] = IR0.DiagnosticOutcome(
                cid,
                accepted=not dp_violations,
                codes=tuple(sorted(dp_violations.keys())),
            )
            graph_outcomes[cid] = IR0.DiagnosticOutcome(
                cid,
                accepted=not ir2_violations,
                codes=tuple(ir2_violations),
            )

    return IR0.evaluate_shadow_diff(prod_outcomes, graph_outcomes), total_graphs, total_skipped

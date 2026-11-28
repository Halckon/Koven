"""P2-1：IR5 挂起帧状态机变换与跨挂起点存活布局原型（KB8 / S0.1 / S3 / S5）。

非规范、不被编译器引用。对齐 docs/proposals/evolution-08-compiler-ir-escape-simd-selfhost.md §2.5（IR5）
与 docs/proposals/evolution-04-route-capability-unsafe-suspend.md（S0–S6）：
- 扫描 IR0 OwnershipGraph 中的所有 SuspendPoint，分配状态编号（State 0=未启动，State 1..k=各挂起点，Done=完成）；
- 计算每个挂起点处仍存活的局部变量集合、帧销毁时的逆序清理清单与自引用稳定地址需求（S0.1）；
- 在 parallel suspend 模式下校验所有跨挂起点存活的局部均满足 Transferable（S5）；
- 支持直接消费 P0-1（ir1_ir2_lowering）从真实 .lk 挂起函数构图产出的 OwnershipGraph。
"""
from __future__ import annotations

from dataclasses import dataclass
import importlib.util
from pathlib import Path
import sys
from typing import Dict, FrozenSet, List, Optional, Set, Tuple

ROOT = Path(__file__).resolve().parent


def _load_sibling(name: str):
    mod_name = f"loan_kernel_{name}"
    if mod_name in sys.modules:
        return sys.modules[mod_name]
    spec = importlib.util.spec_from_file_location(mod_name, ROOT / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = mod
    spec.loader.exec_module(mod)
    return mod


IR0 = _load_sibling("ir0_graph")


@dataclass(frozen=True)
class SuspendStateLayout:
    state_id: int
    block_id: str
    live_locals: Tuple[str, ...]
    drop_order_on_destroy: Tuple[str, ...]  # 帧在挂起点被销毁时的清理顺序（声明/初始化逆序）
    has_internal_borrow: bool
    non_transferable_locals: Tuple[str, ...]


@dataclass(frozen=True)
class StateMachineFrame:
    fun_name: str
    is_parallel: bool
    states: Tuple[SuspendStateLayout, ...]
    requires_stable_address: bool
    errors: Tuple[str, ...]


def transform_suspend_frame(
    graph: IR0.OwnershipGraph,
    is_parallel: bool = False,
    non_transferable_types: FrozenSet[str] = frozenset(),
    frames_movable: bool = False,
) -> StateMachineFrame:
    """将含 SuspendPoint 的 OwnershipGraph 变换为挂起帧状态机布局并检查 S0.1/S5。"""
    struct_errs = IR0.verify_structure(graph)
    if struct_errs:
        return StateMachineFrame(graph.name, is_parallel, (), False, tuple(struct_errs))

    # 沿所有可达路径记录到达每个 SuspendPoint 时处于 Live 的局部变量及初始化顺序、活跃内部借用
    suspend_hits: Dict[str, Tuple[Set[str], List[str], bool]] = {}
    start_state = (graph.entry, tuple(), tuple())  # (bb_id, live_order_tuple, active_loans_tuple)
    seen: Set[Tuple] = set()
    stack: List[Tuple] = [start_state]

    while stack:
        state = stack.pop()
        if state in seen:
            continue
        seen.add(state)
        bb_id, live_order_t, loans_t = state
        live_order = list(live_order_t)
        active_loans: Dict[str, str] = dict(loans_t)  # loan_id -> root local
        bb = graph.blocks[bb_id]

        for stmt in bb.stmts:
            if stmt.kind == "Init" and stmt.place is not None:
                root = stmt.place.root
                if root in live_order:
                    live_order.remove(root)
                live_order.append(root)
            elif stmt.kind in ("Move", "Drop") and stmt.place is not None:
                root = stmt.place.root
                if stmt.drop_flag is None and root in live_order:
                    live_order.remove(root)
            elif stmt.kind == "LoanBegin" and stmt.place is not None and stmt.loan:
                active_loans[stmt.loan] = stmt.place.root
            elif stmt.kind == "LoanEnd" and stmt.loan:
                active_loans.pop(stmt.loan, None)
            elif stmt.kind == "SuspendPoint":
                has_internal = any(root in live_order for root in active_loans.values())
                prev = suspend_hits.get(bb_id)
                if prev is None:
                    suspend_hits[bb_id] = (set(live_order), list(live_order), has_internal)
                else:
                    prev_set, prev_order, prev_internal = prev
                    merged_set = prev_set | set(live_order)
                    merged_order = list(prev_order)
                    for name in live_order:
                        if name not in merged_order:
                            merged_order.append(name)
                    suspend_hits[bb_id] = (merged_set, merged_order, prev_internal or has_internal)

        term = bb.terminator
        if term is not None and term.kind not in ("Return", "FatalAbort"):
            # 沿正常路径继续推进；不从 failure_bb（异常清理分支）进入正常后续状态
            next_succs = (term.normal_bb,) if term.normal_bb is not None else term.successors()
            next_live = tuple(live_order)
            next_loans = tuple(sorted(active_loans.items()))
            for succ in next_succs:
                stack.append((succ, next_live, next_loans))

    states: List[SuspendStateLayout] = []
    errors: List[str] = []
    requires_stable = False

    for idx, bb_id in enumerate(sorted(suspend_hits), start=1):
        live_set, init_order, has_internal = suspend_hits[bb_id]
        live_sorted = tuple(sorted(live_set))
        drop_order = tuple(reversed(init_order))
        non_trans = tuple(
            sorted(
                name
                for name in live_sorted
                if graph.locals[name].ty in non_transferable_types
            )
        )
        if has_internal:
            requires_stable = True
            if frames_movable:
                errors.append(f"S1_MOVED_SELF_REF_FRAME: {graph.name} 在状态 {idx}({bb_id}) 含帧内借用，帧不可移动")
        if is_parallel and non_trans:
            errors.append(
                f"S5_NON_TRANSFERABLE_ACROSS_SUSPEND: parallel 函数 {graph.name} 在状态 {idx}({bb_id}) "
                f"跨越挂起点存活不可转移局部 {non_trans}"
            )
        states.append(
            SuspendStateLayout(
                state_id=idx,
                block_id=bb_id,
                live_locals=live_sorted,
                drop_order_on_destroy=drop_order,
                has_internal_borrow=has_internal,
                non_transferable_locals=non_trans,
            )
        )

    return StateMachineFrame(
        fun_name=graph.name,
        is_parallel=is_parallel,
        states=tuple(states),
        requires_stable_address=requires_stable,
        errors=tuple(errors),
    )

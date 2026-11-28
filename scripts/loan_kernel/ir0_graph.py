"""IR0 所有权图最小规格原型、分层校验器与故障注入检验。

非规范、不被编译器引用。对齐 docs/proposals/ir0-ownership-graph-draft.md §2：
- 第一层（verify_structure）：仅校验图结构完好性（CFG 终结符、Span/DesugarOrigin、
  LocalId/LoanId 闭合、FatalAbort 无后继、拒绝 Poisoned 图进入 Phase 3/IR4），
  不把用户源码所有权错误误判为内部图损坏。
- 第二层（verify_ir2_invariants）：在已构图且完成 IR2 析构展开的图上校验
  MoveOnly 恰好一次消费/析构（正常返回与可路由 failure_bb 路径，FatalAbort 不要求析构）、
  Conditional(DropFlagId) 与 LexicalRequired 正交组合，以及 LoanBegin/LoanActivate/LoanEnd
  两阶段借用激活冲突。
"""
from __future__ import annotations

from collections import deque
from dataclasses import dataclass
from typing import Dict, FrozenSet, List, Optional, Set, Tuple

DESUGAR_ORIGINS = frozenset({
    "Direct",
    "PostfixQuestion",
    "ForIteration",
    "TwoPhaseReceiver",
    "ShortCircuit",
    "CompoundAssign",
    "ImplicitDrop",
    "ClosureCapture",
})

DROP_POLICIES = frozenset({"Immediate", "LexicalRequired"})
STMT_KINDS = frozenset({
    "StorageLive",
    "StorageDead",
    "Init",
    "Assign",
    "Move",
    "Copy",
    "LoanBegin",
    "LoanActivate",
    "LoanEnd",
    "Drop",
    "SetFlag",
    "SuspendPoint",
})
TERMINATOR_KINDS = frozenset({
    "Goto",
    "Branch",
    "Switch",
    "Call",
    "CheckedOp",
    "Fail",
    "Return",
    "FatalAbort",
})


@dataclass(frozen=True)
class Span:
    start: int
    end: int

    def is_valid(self) -> bool:
        return self.start >= 0 and self.end >= self.start


@dataclass(frozen=True)
class Place:
    root: str
    projections: Tuple[str, ...] = ()


@dataclass(frozen=True)
class LocalDecl:
    name: str
    ty: str
    mutability: str = "Val"  # Val | Var
    mode: str = "Own"  # Own | Borrow | Inout
    scope: int = 0
    move_only: bool = True
    drop_policy: str = "Immediate"
    is_temp: bool = False


@dataclass(frozen=True)
class GraphStmt:
    kind: str
    span: Span
    origin: str = "Direct"
    local: str = ""
    place: Optional[Place] = None
    loan: str = ""
    loan_kind: str = ""  # Shared | Exclusive | Reserved
    parent_loan: Optional[str] = None
    drop_policy: str = ""
    drop_flag: Optional[str] = None
    flag_value: Optional[bool] = None


@dataclass(frozen=True)
class GraphTerminator:
    kind: str
    span: Span
    origin: str = "Direct"
    target: Optional[str] = None
    targets: Tuple[str, ...] = ()
    normal_bb: Optional[str] = None
    failure_bb: Optional[str] = None
    reason: str = ""

    def successors(self) -> Tuple[str, ...]:
        out: List[str] = []
        if self.target is not None:
            out.append(self.target)
        out.extend(self.targets)
        if self.normal_bb is not None:
            out.append(self.normal_bb)
        if self.failure_bb is not None:
            out.append(self.failure_bb)
        return tuple(out)


@dataclass(frozen=True)
class BasicBlock:
    block_id: str
    stmts: Tuple[GraphStmt, ...]
    terminator: Optional[GraphTerminator]


@dataclass(frozen=True)
class OwnershipGraph:
    name: str
    status: str  # Ok | Poisoned
    locals: Dict[str, LocalDecl]
    entry: str
    blocks: Dict[str, BasicBlock]


def places_overlap(a: Place, b: Place) -> bool:
    if a.root != b.root:
        return False
    n = min(len(a.projections), len(b.projections))
    return a.projections[:n] == b.projections[:n]


def verify_structure(graph: OwnershipGraph) -> List[str]:
    """第一层：构图结构校验（IR0 §2.5 #1）。只检图结构完好性，不替代 Phase 3 所有权诊断。"""
    errors: List[str] = []
    if graph.status == "Poisoned":
        errors.append("STRUCT_POISONED: Poisoned 图禁止进入 Phase 3 或 IR4")
        return errors
    if graph.status != "Ok":
        errors.append(f"STRUCT_STATUS: 未知图状态 {graph.status}")
    if graph.entry not in graph.blocks:
        errors.append(f"STRUCT_ENTRY: 入口块 {graph.entry} 不存在")
        return errors

    declared_loans: Set[str] = set()
    for bb in graph.blocks.values():
        for stmt in bb.stmts:
            if stmt.kind == "LoanBegin" and stmt.loan:
                declared_loans.add(stmt.loan)

    visited: Set[str] = set()
    queue: deque = deque([graph.entry])
    while queue:
        bb_id = queue.popleft()
        if bb_id in visited:
            continue
        visited.add(bb_id)
        bb = graph.blocks.get(bb_id)
        if bb is None:
            errors.append(f"STRUCT_BLOCK: 悬空基本块引用 {bb_id}")
            continue
        for idx, stmt in enumerate(bb.stmts):
            if stmt.kind not in STMT_KINDS:
                errors.append(f"STRUCT_STMT: {bb_id}[{idx}] 未知语句 {stmt.kind}")
            if not stmt.span.is_valid():
                errors.append(f"STRUCT_SPAN: {bb_id}[{idx}] 非法 Span {stmt.span}")
            if stmt.origin not in DESUGAR_ORIGINS:
                errors.append(f"STRUCT_ORIGIN: {bb_id}[{idx}] 非法 DesugarOrigin {stmt.origin}")
            if stmt.local and stmt.local not in graph.locals:
                errors.append(f"STRUCT_LOCAL: {bb_id}[{idx}] 悬空局部引用 {stmt.local}")
            if stmt.place is not None and stmt.place.root not in graph.locals:
                errors.append(f"STRUCT_LOCAL: {bb_id}[{idx}] 悬空 place 根 {stmt.place.root}")
            if stmt.kind == "Drop" and stmt.drop_policy not in DROP_POLICIES:
                errors.append(f"STRUCT_DROP_POLICY: {bb_id}[{idx}] 非法析构策略 {stmt.drop_policy}")
            if stmt.kind == "LoanBegin":
                if not stmt.loan:
                    errors.append(f"STRUCT_LOAN: {bb_id}[{idx}] LoanBegin 缺少 loan ID")
                if stmt.parent_loan is not None:
                    if stmt.parent_loan == stmt.loan or stmt.parent_loan not in declared_loans:
                        errors.append(f"STRUCT_LOAN: {bb_id}[{idx}] 非法 parent_loan {stmt.parent_loan}")
            elif stmt.kind in ("LoanActivate", "LoanEnd"):
                if not stmt.loan or stmt.loan not in declared_loans:
                    errors.append(f"STRUCT_LOAN: {bb_id}[{idx}] {stmt.kind} 引用未声明的 loan {stmt.loan}")

        term = bb.terminator
        if term is None:
            errors.append(f"STRUCT_TERM: 可达块 {bb_id} 缺少终结符")
            continue
        if term.kind not in TERMINATOR_KINDS:
            errors.append(f"STRUCT_TERM: {bb_id} 未知终结符 {term.kind}")
        if not term.span.is_valid():
            errors.append(f"STRUCT_SPAN: {bb_id} 终结符含非法 Span {term.span}")
        if term.origin not in DESUGAR_ORIGINS:
            errors.append(f"STRUCT_ORIGIN: {bb_id} 终结符含非法 DesugarOrigin {term.origin}")
        succs = term.successors()
        if term.kind == "FatalAbort" and succs:
            errors.append(f"STRUCT_FATAL_ABORT: {bb_id} FatalAbort 不得有后继块 {succs}")
        for succ in succs:
            if succ not in graph.blocks:
                errors.append(f"STRUCT_BLOCK: {bb_id} 终结符指向不存在的块 {succ}")
            elif succ not in visited:
                queue.append(succ)

    return errors


def verify_ir2_invariants(graph: OwnershipGraph) -> List[str]:
    """第二层：Phase 3 / IR2 析构展开与两阶段借用不变量校验（IR0 §2.5 #2）。"""
    struct_errs = verify_structure(graph)
    if struct_errs:
        return struct_errs

    violations: List[str] = []
    init_own = tuple(
        (name, "Live" if decl.mode == "Own" and decl.scope == 0 and decl.is_temp is False and False else "Uninit")
        for name, decl in sorted(graph.locals.items())
    )
    # 所有局部默认从 Uninit 开始，由 Init / 参数入口初始化
    start_state = (
        graph.entry,
        tuple((name, "Uninit") for name in sorted(graph.locals)),
        tuple(),  # active_loans: ((loan_id, kind, place, parent_loan),)
        tuple(),  # drop_flags: ((flag_name, bool),)
    )

    seen: Set[Tuple] = set()
    stack: List[Tuple] = [start_state]

    def active_conflicts(
        loans_map: Dict[str, Tuple[str, Place, Optional[str]]],
        target: Place,
        need_excl: bool,
        skip_loan: Optional[str] = None,
    ) -> Optional[str]:
        for lid, (lkind, lplace, _) in sorted(loans_map.items()):
            if lid == skip_loan:
                continue
            if lkind == "Reserved":
                continue
            if not places_overlap(target, lplace):
                continue
            if need_excl or lkind == "Exclusive":
                return lid
        return None

    while stack:
        state = stack.pop()
        if state in seen:
            continue
        seen.add(state)
        bb_id, own_tuple, loans_tuple, flags_tuple = state
        own = dict(own_tuple)
        loans: Dict[str, Tuple[str, Place, Optional[str]]] = {
            lid: (lkind, lplace, parent) for lid, lkind, lplace, parent in loans_tuple
        }
        flags = dict(flags_tuple)
        bb = graph.blocks[bb_id]

        aborted_path = False
        for stmt in bb.stmts:
            if stmt.kind == "Init" and stmt.place is not None:
                root = stmt.place.root
                if own.get(root) == "Live" and graph.locals[root].move_only:
                    violations.append(f"OVERWRITE_WITHOUT_DROP: {bb_id} Init 覆盖仍存活的 {root}")
                own[root] = "Live"
            elif stmt.kind == "Assign" and stmt.place is not None:
                root = stmt.place.root
                conflict = active_conflicts(loans, stmt.place, need_excl=True)
                if conflict is not None:
                    violations.append(f"ASSIGN_WHILE_BORROWED: {bb_id} 写 {root} 与活跃 loan {conflict} 冲突")
                if not stmt.place.projections and own.get(root) == "Live" and graph.locals[root].move_only:
                    violations.append(f"OVERWRITE_WITHOUT_DROP: {bb_id} Assign 覆盖未先析构的 {root}")
                own[root] = "Live"
            elif stmt.kind == "Move" and stmt.place is not None:
                root = stmt.place.root
                if own.get(root) != "Live":
                    violations.append(f"USE_AFTER_MOVE: {bb_id} Move 非存活局部 {root}（状态={own.get(root)}）")
                conflict = active_conflicts(loans, stmt.place, need_excl=True)
                if conflict is not None:
                    violations.append(f"MOVE_WHILE_BORROWED: {bb_id} Move {root} 与活跃 loan {conflict} 冲突")
                own[root] = "Moved"
            elif stmt.kind == "Copy" and stmt.place is not None:
                root = stmt.place.root
                if own.get(root) != "Live":
                    violations.append(f"USE_AFTER_MOVE: {bb_id} Copy 非存活局部 {root}（状态={own.get(root)}）")
                conflict = active_conflicts(loans, stmt.place, need_excl=False)
                if conflict is not None:
                    violations.append(f"COPY_WHILE_EXCL_BORROWED: {bb_id} Copy {root} 与独占 loan {conflict} 冲突")
            elif stmt.kind == "LoanBegin" and stmt.place is not None:
                need_excl = stmt.loan_kind == "Exclusive"
                conflict = active_conflicts(loans, stmt.place, need_excl=need_excl, skip_loan=stmt.parent_loan)
                if conflict is not None:
                    violations.append(f"LOAN_CONFLICT: {bb_id} {stmt.loan}({stmt.loan_kind}) 与活跃 loan {conflict} 冲突")
                loans[stmt.loan] = (stmt.loan_kind, stmt.place, stmt.parent_loan)
            elif stmt.kind == "LoanActivate":
                if stmt.loan not in loans:
                    violations.append(f"LOAN_ACTIVATE_MISSING: {bb_id} 激活不存在的 loan {stmt.loan}")
                else:
                    lkind, lplace, parent = loans[stmt.loan]
                    conflict = active_conflicts(loans, lplace, need_excl=True, skip_loan=parent)
                    if conflict is not None and conflict != stmt.loan:
                        violations.append(
                            f"TWO_PHASE_ACTIVATE_CONFLICT: {bb_id} 激活 {stmt.loan} 与存活 loan {conflict} 冲突"
                        )
                    loans[stmt.loan] = ("Exclusive", lplace, parent)
            elif stmt.kind == "LoanEnd":
                loans.pop(stmt.loan, None)
            elif stmt.kind == "SetFlag" and stmt.drop_flag is not None and stmt.flag_value is not None:
                flags[stmt.drop_flag] = stmt.flag_value
            elif stmt.kind == "Drop" and stmt.place is not None:
                if stmt.drop_flag is not None and not flags.get(stmt.drop_flag, False):
                    continue
                root = stmt.place.root
                cur = own.get(root)
                if cur in ("Dropped", "Moved"):
                    violations.append(f"DOUBLE_DROP: {bb_id} 对状态为 {cur} 的 {root} 执行重复析构")
                elif cur == "Uninit":
                    violations.append(f"DROP_UNINIT: {bb_id} 析构未初始化的 {root}")
                conflict = active_conflicts(loans, stmt.place, need_excl=True)
                if conflict is not None:
                    violations.append(f"DROP_WHILE_BORROWED: {bb_id} 析构 {root} 时仍有活跃 loan {conflict}")
                own[root] = "Dropped"

        if aborted_path:
            continue
        term = bb.terminator
        assert term is not None
        if term.kind == "Return":
            for name, decl in sorted(graph.locals.items()):
                if decl.move_only and own.get(name) == "Live":
                    violations.append(f"LEAK_ON_RETURN: {bb_id} 返回时 MoveOnly 局部 {name} 未消费也未析构")
        elif term.kind == "FatalAbort":
            # U0 FS6 / FS7 / FFI5：致命终止直接结束进程，不要求执行待运行析构
            continue
        else:
            next_own = tuple(sorted(own.items()))
            next_loans = tuple(sorted((lid, info[0], info[1], info[2]) for lid, info in loans.items()))
            next_flags = tuple(sorted(flags.items()))
            for succ in term.successors():
                stack.append((succ, next_own, next_loans, next_flags))

    return sorted(set(violations))


# ---- IR0 §2.5 #3：影子差分门槛（生产检查器 vs 图参考检查器）

DIFF_CATEGORIES = frozenset({
    "ProdBug",
    "GraphBuilderBug",
    "SpecAmbiguity",
    "ApprovedRelaxation",
})


@dataclass(frozen=True)
class DiagnosticOutcome:
    case_id: str
    accepted: bool
    codes: Tuple[str, ...] = ()
    primary_span: Optional[Span] = None


@dataclass(frozen=True)
class ShadowDiffReport:
    total: int
    matched: int
    unexplained: Tuple[str, ...]
    categorized: Tuple[Tuple[str, str], ...]

    @property
    def ir3_switch_allowed(self) -> bool:
        return len(self.unexplained) == 0


def evaluate_shadow_diff(
    prod: Dict[str, DiagnosticOutcome],
    graph: Dict[str, DiagnosticOutcome],
    classifications: Optional[Dict[str, str]] = None,
) -> ShadowDiffReport:
    """对比生产检查器与图参考检查器的接受/拒绝、诊断码与主 Span；存在未归因差异时禁止 IR3 切换权威。"""
    classifications = classifications or {}
    all_ids = sorted(set(prod) | set(graph))
    matched = 0
    unexplained: List[str] = []
    categorized: List[Tuple[str, str]] = []
    for cid in all_ids:
        p = prod.get(cid)
        g = graph.get(cid)
        if p == g and p is not None:
            matched += 1
            continue
        cat = classifications.get(cid)
        if cat in DIFF_CATEGORIES:
            categorized.append((cid, cat))
        else:
            unexplained.append(cid)
    return ShadowDiffReport(
        total=len(all_ids),
        matched=matched,
        unexplained=tuple(unexplained),
        categorized=tuple(categorized),
    )

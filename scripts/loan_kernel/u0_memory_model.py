"""U0 §5（MM1–MM6）有界执行图内存模型原型（数据竞争、happens-before 与易失访问边界）。

非规范、不被编译器引用。对齐 docs/proposals/unsafe-aliasing-contract-outline.md §5：
- MM1 / MM4：同一普通内存位置存在两个访问，至少一个为写、至少一个非原子，且无 happens-before 偏序 -> UB6 数据竞争（不设“良性竞争”例外）；
- MM2：线程创建（spawn）、join、channel send->recv、mutex unlock->lock 建立跨线程 synchronizes-with / happens-before 边；
- MM3：Release 写与读到该值的 Acquire 读（或 SeqCst）建立 synchronizes-with 边；Relaxed 不建立跨线程 happens-before；
- MM6：易失（volatile）访问仅禁止易失访问之间重排，不建立跨线程 happens-before，也不约束与普通非原子访问的相对重排。

边界声明：本脚本是小规模执行图教学/回归原型，用于检验 U0 §5 条款本身的一致性，
不替代 OD5 中单列为 U1 验收欠项的 Loom / GenMC 级弱内存并发模型检查器。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, FrozenSet, List, Optional, Set, Tuple


@dataclass(frozen=True)
class MemEvent:
    eid: str
    thread: str
    seq: int  # 同线程程序序（po）编号，递增
    kind: str  # read | write | atomic_read | atomic_write | volatile_read | volatile_write | sync_rel | sync_acq
    loc: str
    order: str = "NonAtomic"  # NonAtomic | Relaxed | Acquire | Release | SeqCst | Volatile
    value: Optional[int] = None
    reads_from: Optional[str] = None  # 若为读事件，指向其读到的写事件 eid
    sync_tag: str = ""  # spawn / join / channel / mutex 等同步配对标签


@dataclass(frozen=True)
class ExecutionGraph:
    name: str
    events: Tuple[MemEvent, ...]


def compute_happens_before(graph: ExecutionGraph) -> Dict[str, Set[str]]:
    """计算有界执行图的 happens-before 传递闭包：hb = (po U sw)+。"""
    by_id = {e.eid: e for e in graph.events}
    adj: Dict[str, Set[str]] = {e.eid: set() for e in graph.events}

    # 1. 程序序（po）：同线程按 seq 递增
    by_thread: Dict[str, List[MemEvent]] = {}
    for e in graph.events:
        by_thread.setdefault(e.thread, []).append(e)
    for evs in by_thread.values():
        evs.sort(key=lambda x: x.seq)
        for i in range(len(evs) - 1):
            adj[evs[i].eid].add(evs[i + 1].eid)

    # 2. MM2 / MM3 synchronizes-with（sw）边：
    #    - atomic_write(Release|SeqCst) -> reads_from -> atomic_read(Acquire|SeqCst)
    #    - sync_rel(tag) -> sync_acq(tag)（对应 spawn/join/channel/mutex）
    #    注意：Relaxed 与 Volatile 绝不产生跨线程 sw 边！
    rel_by_tag: Dict[str, List[str]] = {}
    for e in graph.events:
        if e.kind == "sync_rel" and e.sync_tag:
            rel_by_tag.setdefault(e.sync_tag, []).append(e.eid)

    for e in graph.events:
        if e.kind == "atomic_read" and e.order in ("Acquire", "SeqCst") and e.reads_from in by_id:
            src = by_id[e.reads_from]
            if src.kind == "atomic_write" and src.order in ("Release", "SeqCst"):
                adj[src.eid].add(e.eid)
        elif e.kind == "sync_acq" and e.sync_tag:
            for src_id in rel_by_tag.get(e.sync_tag, ()):
                adj[src_id].add(e.eid)

    # 传递闭包（Floyd-Warshall / BFS，图规模极小）
    hb: Dict[str, Set[str]] = {}
    for start in adj:
        reached: Set[str] = set()
        stack = list(adj[start])
        while stack:
            cur = stack.pop()
            if cur not in reached:
                reached.add(cur)
                stack.extend(adj[cur])
        hb[start] = reached
    return hb


def _is_access(e: MemEvent) -> bool:
    return e.kind in ("read", "write", "atomic_read", "atomic_write", "volatile_read", "volatile_write")


def _is_write(e: MemEvent) -> bool:
    return e.kind in ("write", "atomic_write", "volatile_write")


def _is_atomic(e: MemEvent) -> bool:
    return e.kind in ("atomic_read", "atomic_write")


def detect_data_races(graph: ExecutionGraph) -> List[Tuple[str, str]]:
    """MM1 / MM4：返回全部构成 UB6 数据竞争的无序事件对 (eid1, eid2)。"""
    hb = compute_happens_before(graph)
    accesses = [e for e in graph.events if _is_access(e)]
    races: List[Tuple[str, str]] = []
    for i in range(len(accesses)):
        for j in range(i + 1, len(accesses)):
            a, b = accesses[i], accesses[j]
            if a.loc != b.loc or a.thread == b.thread:
                continue
            if not (_is_write(a) or _is_write(b)):
                continue
            if _is_atomic(a) and _is_atomic(b):
                continue
            if b.eid in hb[a.eid] or a.eid in hb[b.eid]:
                continue
            races.append(tuple(sorted((a.eid, b.eid))))
    return sorted(races)


def check_volatile_reorder_allowed(first: MemEvent, second: MemEvent) -> bool:
    """MM6：两个易失访问之间严禁重排；易失访问与普通非原子访问之间允许重排（不提供内存栅栏）。"""
    v1 = first.kind in ("volatile_read", "volatile_write")
    v2 = second.kind in ("volatile_read", "volatile_write")
    if v1 and v2:
        return False
    return True

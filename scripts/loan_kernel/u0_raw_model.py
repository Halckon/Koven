"""U0 别名、来历与未初始化槽位的最小可执行参考解释器原型（AL1 / AL2 / AL7 / FS8）。

本模块用于为 docs/proposals/unsafe-aliasing-contract-outline.md §9 提供
AL1 / AL2 附加前提与 UB1 / UB2 / UB4 / UB5 / FS8 的可执行反例捕获验证：
- AL1 + AL7：裸指针读写对存活 Shared/Exclusive loan 派生树与 RawCell 内部可变例外的检查（UB1）；
- AL2：addressOf(place) 的 place 级析构屏障（禁止提前析构）、owner 析构/移出后解引用（UB2），
  以及 ptr_read / ptr_write 的逐槽位（含祖先与后代）初始化状态机（UB5）；
- AL3 / FS3 / FS8：析构义务恰好一次校验（UB4）与在每个 FS1 操作处注入可展开失败的逆序清理检查。

仅使用 Python 3.9+ 标准库，不修改生产编译器或现行 Guide。
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Sequence, Set, Tuple


def split_place(place: str) -> Tuple[str, ...]:
    return tuple(place.split("."))


def root_of(place: str) -> str:
    return split_place(place)[0]


def overlaps(a: str, b: str) -> bool:
    pa, pb = split_place(a), split_place(b)
    n = min(len(pa), len(pb))
    return pa[:n] == pb[:n]


def is_prefix_or_equal(prefix: str, target: str) -> bool:
    pp, pt = split_place(prefix), split_place(target)
    return len(pp) <= len(pt) and pt[: len(pp)] == pp


@dataclass(frozen=True)
class Loan:
    loan_id: int
    place: str
    kind: str  # "Shared" | "Exclusive"
    parent_id: Optional[int] = None


@dataclass(frozen=True)
class RawPtr:
    ptr_id: int
    place: str
    root: str
    alloc_id: int
    derived_mode: str  # "Shared" | "Exclusive" | "Cell"
    provenance_kind: str  # 实际派生权限："Shared" | "Exclusive" | "Cell"
    derived_loan_id: Optional[int] = None
    out_of_scope_loan: Optional[str] = None


@dataclass
class OwnerState:
    root: str
    alloc_id: int
    slots: Dict[str, str] = field(default_factory=dict)  # place -> "Init" | "Uninit"
    is_cell: bool = False
    is_resource: bool = False
    alive: bool = True
    moved: bool = False
    dropped: bool = False
    drop_count: int = 0
    drop_barrier: bool = False
    handle_id: Optional[int] = None


@dataclass(frozen=True)
class Violation:
    code: str  # "UB1" | "UB2" | "UB4" | "UB5" | "AL2_BARRIER"
    step: int
    detail: str


class U0RawInterpreter:
    """最小借用栈 + 槽位初始化位图 + 析构计数参考解释器。"""

    def __init__(self) -> None:
        self.owners: Dict[str, OwnerState] = {}
        self.loans: Dict[int, Loan] = {}
        self.loan_history: Dict[int, Loan] = {}
        self.ptrs: Dict[int, RawPtr] = {}
        self.handles_dropped: Dict[int, int] = {}
        self.next_alloc_id = 1
        self.next_loan_id = 1
        self.next_ptr_id = 1
        self.next_handle_id = 1
        self.step = 0
        self.violations: List[Violation] = []
        self.scope_order: List[str] = []

    def _record(self, code: str, detail: str) -> None:
        self.violations.append(Violation(code=code, step=self.step, detail=detail))

    def _new_alloc_id(self) -> int:
        alloc_id = self.next_alloc_id
        self.next_alloc_id += 1
        return alloc_id

    def _new_handle(self) -> int:
        handle_id = self.next_handle_id
        self.next_handle_id += 1
        self.handles_dropped[handle_id] = 0
        return handle_id

    def alloc_owner(
        self,
        root: str,
        *,
        fields: Sequence[str] = (),
        initialized: bool = True,
        is_cell: bool = False,
        resource_handle: bool = False,
    ) -> None:
        self.step += 1
        state = "Init" if initialized else "Uninit"
        slots = {root: state}
        for f in fields:
            slots[f"{root}.{f}"] = state
        handle_id = self._new_handle() if (resource_handle and initialized) else None
        self.owners[root] = OwnerState(
            root=root,
            alloc_id=self._new_alloc_id(),
            slots=slots,
            is_cell=is_cell,
            is_resource=resource_handle,
            handle_id=handle_id,
        )
        self.scope_order.append(root)

    def borrow(self, place: str, kind: str, *, parent_id: Optional[int] = None) -> int:
        self.step += 1
        assert kind in ("Shared", "Exclusive")
        loan_id = self.next_loan_id
        self.next_loan_id += 1
        loan = Loan(loan_id=loan_id, place=place, kind=kind, parent_id=parent_id)
        self.loans[loan_id] = loan
        self.loan_history[loan_id] = loan
        return loan_id

    def end_loan(self, loan_id: int) -> None:
        self.step += 1
        self.loans.pop(loan_id, None)

    def _effective_provenance(self, derived_mode: str, derived_loan_id: Optional[int]) -> str:
        """沿完整借用派生历史核对真实权限：即便父借用已结束，任一祖先为 Shared 均不得提权为 Exclusive。"""
        if derived_loan_id is None:
            return derived_mode
        cur: Optional[int] = derived_loan_id
        saw_shared = False
        visited: Set[int] = set()
        while cur is not None and cur in self.loan_history and cur not in visited:
            visited.add(cur)
            loan = self.loan_history[cur]
            if loan.kind == "Shared":
                saw_shared = True
            cur = loan.parent_id
        if saw_shared:
            return "Cell" if derived_mode == "Cell" else "Shared"
        return derived_mode

    def _check_derived_loan_scope(self, place: str, derived_loan_id: Optional[int]) -> Optional[str]:
        """校验目标 place 是否落在派生借用链覆盖范围内；越界时返回首个不覆盖它的借用位置。"""
        if derived_loan_id is None:
            return None
        cur: Optional[int] = derived_loan_id
        visited: Set[int] = set()
        while cur is not None and cur in self.loan_history and cur not in visited:
            visited.add(cur)
            loan = self.loan_history[cur]
            if not is_prefix_or_equal(loan.place, place):
                return loan.place
            cur = loan.parent_id
        return None

    def address_of(
        self,
        place: str,
        *,
        derived_mode: str,
        derived_loan_id: Optional[int] = None,
    ) -> int:
        """U1 R1.2 + AL2：建立带分配实例 ID、范围核对与真实派生权限的裸指针，并在 root owner 上设置 place 级析构屏障。"""
        self.step += 1
        assert derived_mode in ("Shared", "Exclusive", "Cell")
        root = root_of(place)
        owner = self.owners[root]
        owner.drop_barrier = True
        ptr_id = self.next_ptr_id
        self.next_ptr_id += 1
        self.ptrs[ptr_id] = RawPtr(
            ptr_id=ptr_id,
            place=place,
            root=root,
            alloc_id=owner.alloc_id,
            derived_mode=derived_mode,
            provenance_kind=self._effective_provenance(derived_mode, derived_loan_id),
            derived_loan_id=derived_loan_id,
            out_of_scope_loan=self._check_derived_loan_scope(place, derived_loan_id),
        )
        return ptr_id

    def _ancestors(self, loan_id: Optional[int]) -> Set[int]:
        anc: Set[int] = set()
        cur = loan_id
        while cur is not None and cur in self.loans:
            anc.add(cur)
            cur = self.loans[cur].parent_id
        return anc

    def _check_live(self, ptr: RawPtr, op: str) -> Optional[OwnerState]:
        owner = self.owners.get(ptr.root)
        if owner is None or owner.alloc_id != ptr.alloc_id or not owner.alive or owner.dropped or owner.moved:
            self._record(
                "UB2",
                f"{op} on dangling RawPtr({ptr.place}, alloc#{ptr.alloc_id}): owner {ptr.root} is dead/moved/reallocated",
            )
            return None
        return owner

    def _check_al1_read(self, ptr: RawPtr) -> bool:
        if ptr.out_of_scope_loan is not None:
            self._record(
                "UB1",
                f"raw_read({ptr.place}) is outside derived loan #{ptr.derived_loan_id} scope ({ptr.out_of_scope_loan})",
            )
            return False
        allowed = self._ancestors(ptr.derived_loan_id)
        for loan in self.loans.values():
            if loan.loan_id in allowed:
                continue
            if loan.kind == "Exclusive" and overlaps(loan.place, ptr.place):
                self._record(
                    "UB1",
                    f"raw_read({ptr.place}) conflicts with active Exclusive loan #{loan.loan_id} on {loan.place}",
                )
                return False
        return True

    def _check_rawcell_write_no_exclusive(self, ptr: RawPtr, op: str) -> bool:
        """AL7：RawCell 允许经 Shared/Cell 路径写入，但仍不得与存活 Exclusive loan 重叠冲突。"""
        allowed = self._ancestors(ptr.derived_loan_id)
        for loan in self.loans.values():
            if loan.loan_id in allowed:
                continue
            if loan.kind == "Exclusive" and overlaps(loan.place, ptr.place):
                self._record(
                    "UB1",
                    f"{op}({ptr.place}) on RawCell conflicts with active Exclusive loan #{loan.loan_id}",
                )
                return False
        return True

    def _check_al1_write(self, ptr: RawPtr, owner: OwnerState, op: str) -> bool:
        if ptr.out_of_scope_loan is not None:
            self._record(
                "UB1",
                f"{op}({ptr.place}) is outside derived loan #{ptr.derived_loan_id} scope ({ptr.out_of_scope_loan})",
            )
            return False
        if ptr.provenance_kind in ("Shared", "Cell"):
            if not owner.is_cell:
                self._record(
                    "UB1",
                    f"{op}({ptr.place}) writes through {ptr.provenance_kind}-provenance RawPtr "
                    f"(claimed {ptr.derived_mode}) on non-RawCell owner {owner.root}",
                )
                return False
            return self._check_rawcell_write_no_exclusive(ptr, op)

        # Exclusive 派生写：若绑定了派生 loan，则该 loan 必须仍存活，且不得与其他重叠存活 loan 冲突
        if ptr.derived_loan_id is not None and ptr.derived_loan_id not in self.loans:
            self._record(
                "UB1",
                f"{op}({ptr.place}) uses Exclusive RawPtr after its derived loan #{ptr.derived_loan_id} ended",
            )
            return False
        allowed = self._ancestors(ptr.derived_loan_id)
        for loan in self.loans.values():
            if loan.loan_id in allowed:
                continue
            if overlaps(loan.place, ptr.place):
                self._record(
                    "UB1",
                    f"{op}({ptr.place}) conflicts with active {loan.kind} loan #{loan.loan_id} on {loan.place}",
                )
                return False
        return True

    def _is_initialized(self, owner: OwnerState, place: str) -> bool:
        """按目标粒度检查初始化状态：
        - 若 place 自身已显式建槽，要求其自身及全部后代槽位均为 Init（不因兄弟字段未初始化而误拒）；
        - 若 place 未单独建槽，回退检查最近的祖先槽位以及全部后代槽位。
        """
        if place in owner.slots:
            if owner.slots[place] != "Init":
                return False
        else:
            ancestors = [
                (len(split_place(slot)), st)
                for slot, st in owner.slots.items()
                if is_prefix_or_equal(slot, place)
            ]
            if not ancestors or max(ancestors)[1] != "Init":
                return False
        for slot, st in owner.slots.items():
            if is_prefix_or_equal(place, slot) and st != "Init":
                return False
        return True

    def _set_slot_state(self, owner: OwnerState, place: str, state: str) -> None:
        """更新 place 及其全部后代槽位的初始化状态，并维护根槽位的聚合完整性。"""
        owner.slots[place] = state
        for slot in list(owner.slots.keys()):
            if is_prefix_or_equal(place, slot):
                owner.slots[slot] = state
        if place != owner.root:
            children = [s for s in owner.slots if s != owner.root]
            if state == "Uninit":
                owner.slots[owner.root] = "Uninit"
            elif children and all(owner.slots[s] == "Init" for s in children):
                owner.slots[owner.root] = "Init"

    def raw_read(self, ptr_id: int) -> None:
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "raw_read")
        if owner is None:
            return
        self._check_al1_read(ptr)
        if not self._is_initialized(owner, ptr.place):
            self._record("UB5", f"raw_read({ptr.place}) reads uninitialized storage")

    def raw_write(self, ptr_id: int) -> None:
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "raw_write")
        if owner is None:
            return
        if self._check_al1_write(ptr, owner, "raw_write"):
            self._set_slot_state(owner, ptr.place, "Init")
            if owner.is_resource and owner.handle_id is None and ptr.place == owner.root:
                owner.handle_id = self._new_handle()

    def ptr_read(self, ptr_id: int, *, into_owner: Optional[str] = None) -> None:
        """AL2：从存活槽位移出值；槽位（含后代）变为 Uninit，析构义务一并移交新 owner。"""
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "ptr_read")
        if owner is None:
            return
        if not self._check_al1_write(ptr, owner, "ptr_read"):
            return
        if not self._is_initialized(owner, ptr.place):
            self._record("UB5", f"ptr_read({ptr.place}) reads uninitialized slot")
            return
        self._set_slot_state(owner, ptr.place, "Uninit")
        moved_handle = owner.handle_id if ptr.place == owner.root else None
        if ptr.place == owner.root:
            owner.handle_id = None
        if into_owner is not None:
            self.owners[into_owner] = OwnerState(
                root=into_owner,
                alloc_id=self._new_alloc_id(),
                slots={into_owner: "Init"},
                is_resource=moved_handle is not None,
                handle_id=moved_handle,
            )
            self.scope_order.append(into_owner)

    def ptr_read_duplicate_handle(self, ptr_id: int, into_owner: str) -> None:
        """模拟错误地把同一份析构义务复制给第二个存活 owner（不清除原槽位归属），析构时触发 UB4。"""
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "ptr_read_duplicate_handle")
        if owner is None:
            return
        if not self._is_initialized(owner, ptr.place):
            self._record("UB5", f"ptr_read_duplicate_handle({ptr.place}) reads uninitialized slot")
            return
        self.owners[into_owner] = OwnerState(
            root=into_owner,
            alloc_id=self._new_alloc_id(),
            slots={into_owner: "Init"},
            is_resource=owner.handle_id is not None,
            handle_id=owner.handle_id,
        )
        self.scope_order.append(into_owner)

    def ptr_write(self, ptr_id: int, *, resource_handle: Optional[bool] = None) -> None:
        """AL2：向存活槽位写入新值，将槽位（含后代）恢复为 Init，并为资源类型分配新析构义务。"""
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "ptr_write")
        if owner is None:
            return
        if self._check_al1_write(ptr, owner, "ptr_write"):
            self._set_slot_state(owner, ptr.place, "Init")
            needs_handle = owner.is_resource if resource_handle is None else resource_handle
            if needs_handle and ptr.place == owner.root:
                owner.is_resource = True
                owner.handle_id = self._new_handle()

    def move_owner(self, root: str) -> None:
        self.step += 1
        owner = self.owners[root]
        owner.moved = True
        owner.alive = False

    def drop_owner(self, root: str, *, early: bool = False) -> None:
        self.step += 1
        owner = self.owners[root]
        if early and owner.drop_barrier:
            self._record(
                "AL2_BARRIER",
                f"early drop of {root} blocked by addressOf place-level drop barrier (U1 R1.2)",
            )
            return
        if owner.dropped or owner.drop_count >= 1:
            self._record("UB4", f"owner {root} dropped more than once")
            return
        for place, st in owner.slots.items():
            if st != "Init":
                self._record("UB5", f"drop({root}) observes uninitialized slot {place}")
        owner.drop_count += 1
        owner.dropped = True
        owner.alive = False
        if owner.handle_id is not None:
            self.handles_dropped[owner.handle_id] += 1
            if self.handles_dropped[owner.handle_id] > 1:
                self._record(
                    "UB4",
                    f"resource handle #{owner.handle_id} dropped {self.handles_dropped[owner.handle_id]} times",
                )

    def exit_scope(self) -> None:
        """按词法逆序析构仍存活的 owner。"""
        for root in reversed(self.scope_order):
            owner = self.owners[root]
            if owner.alive and not owner.dropped and not owner.moved:
                self.drop_owner(root, early=False)


def simulate_fs8_insert_order(*, shift_before_clone: bool, clone_fails: bool) -> List[Violation]:
    """复现 U0 §4 FS2/FS3/FS8 列表中部插入示例：
    - 错误次序（shift_before_clone=True）：先搬移制造重复句柄，再调用可能失败的 clone() -> 失败展开触发 UB4；
    - 正确次序（shift_before_clone=False）：先调用可能失败的 clone()，再进入只含 FS4 操作的搬移区间 -> 零违规。
    """
    interp = U0RawInterpreter()
    interp.alloc_owner("elem_i", initialized=True, resource_handle=True)
    p_i = interp.address_of("elem_i", derived_mode="Exclusive")

    if shift_before_clone:
        # 1. 把下标 i 的元素搬移到 i+1（形成重复句柄），尚未写入新元素
        interp.ptr_read_duplicate_handle(p_i, into_owner="elem_i_plus_1")
        # 2. FS1 操作 clone() 注入失败 -> 展开并按逆序析构容器内全部存活元素
        if clone_fails:
            interp.exit_scope()
            return interp.violations
        # 若未失败：写入新克隆元素（由 ptr_write 自动分配新句柄）后退出
        interp.ptr_write(p_i)
        interp.exit_scope()
        return interp.violations

    # 正确次序：1. 先执行可能失败的 clone()，此时容器状态完好
    if clone_fails:
        interp.exit_scope()
        return interp.violations
    # 2. 仅含 FS4 不失败操作的搬移与写入（ptr_read 移交旧句柄，ptr_write 分配新句柄）
    interp.ptr_read(p_i, into_owner="elem_i_plus_1")
    interp.ptr_write(p_i)
    interp.exit_scope()
    return interp.violations

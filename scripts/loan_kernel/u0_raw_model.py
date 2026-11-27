"""U0 别名、来历与未初始化槽位的最小可执行参考解释器原型（AL1 / AL2 / AL7 / FS8）。

本模块用于为 docs/proposals/unsafe-aliasing-contract-outline.md §9 提供
AL1 / AL2 附加前提与 UB1 / UB2 / UB4 / UB5 / FS8 的可执行反例捕获验证：
- AL1 + AL7：裸指针读写对存活 Shared/Exclusive loan 派生树与 RawCell 内部可变例外的检查（UB1）；
- AL2：addressOf(place) 的 place 级析构屏障（禁止提前析构）、owner 析构/移出后解引用（UB2），
  以及 ptr_read / ptr_write 的逐槽位初始化状态机（UB5）；
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
    derived_mode: str  # "Shared" | "Exclusive" | "Cell"
    derived_loan_id: Optional[int] = None


@dataclass
class OwnerState:
    root: str
    slots: Dict[str, str] = field(default_factory=dict)  # place -> "Init" | "Uninit"
    is_cell: bool = False
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
        self.ptrs: Dict[int, RawPtr] = {}
        self.handles_dropped: Dict[int, int] = {}
        self.next_loan_id = 1
        self.next_ptr_id = 1
        self.next_handle_id = 1
        self.step = 0
        self.violations: List[Violation] = []
        self.scope_order: List[str] = []

    def _record(self, code: str, detail: str) -> None:
        self.violations.append(Violation(code=code, step=self.step, detail=detail))

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
        handle_id = None
        if resource_handle:
            handle_id = self.next_handle_id
            self.next_handle_id += 1
            self.handles_dropped[handle_id] = 0
        self.owners[root] = OwnerState(
            root=root,
            slots=slots,
            is_cell=is_cell,
            handle_id=handle_id,
        )
        self.scope_order.append(root)

    def borrow(self, place: str, kind: str, *, parent_id: Optional[int] = None) -> int:
        self.step += 1
        assert kind in ("Shared", "Exclusive")
        loan_id = self.next_loan_id
        self.next_loan_id += 1
        self.loans[loan_id] = Loan(loan_id=loan_id, place=place, kind=kind, parent_id=parent_id)
        return loan_id

    def end_loan(self, loan_id: int) -> None:
        self.step += 1
        self.loans.pop(loan_id, None)

    def address_of(
        self,
        place: str,
        *,
        derived_mode: str,
        derived_loan_id: Optional[int] = None,
    ) -> int:
        """U1 R1.2 + AL2：建立裸指针并在 root owner 上设置 place 级析构屏障。"""
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
            derived_mode=derived_mode,
            derived_loan_id=derived_loan_id,
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
        if owner is None or not owner.alive or owner.dropped or owner.moved:
            self._record("UB2", f"{op} on dangling RawPtr({ptr.place}): owner {ptr.root} is dead/moved")
            return None
        return owner

    def _check_al1_read(self, ptr: RawPtr) -> bool:
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

    def _check_al1_write(self, ptr: RawPtr, owner: OwnerState, op: str) -> bool:
        if ptr.derived_mode == "Shared":
            if not owner.is_cell:
                self._record(
                    "UB1",
                    f"{op}({ptr.place}) writes through Shared-derived RawPtr on non-RawCell owner {owner.root}",
                )
                return False
            # RawCell 例外（AL7）：允许经共享借用写入，但仍不得与存活 Exclusive loan 冲突
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

        if ptr.derived_mode == "Cell":
            if not owner.is_cell:
                self._record("UB1", f"{op}({ptr.place}) claims Cell mode on non-RawCell owner {owner.root}")
                return False
            return True

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

    def _slot_state(self, owner: OwnerState, place: str) -> str:
        if place in owner.slots:
            return owner.slots[place]
        return owner.slots[owner.root]

    def raw_read(self, ptr_id: int) -> None:
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "raw_read")
        if owner is None:
            return
        self._check_al1_read(ptr)
        if self._slot_state(owner, ptr.place) != "Init":
            self._record("UB5", f"raw_read({ptr.place}) reads uninitialized storage")

    def raw_write(self, ptr_id: int) -> None:
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "raw_write")
        if owner is None:
            return
        if self._check_al1_write(ptr, owner, "raw_write"):
            owner.slots[ptr.place] = "Init"

    def ptr_read(self, ptr_id: int, *, into_owner: Optional[str] = None) -> None:
        """AL2：从存活槽位移出值；槽位变为 Uninit（非释放），若读取 Uninit 槽位报 UB5。"""
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "ptr_read")
        if owner is None:
            return
        if not self._check_al1_write(ptr, owner, "ptr_read"):
            return
        if self._slot_state(owner, ptr.place) != "Init":
            self._record("UB5", f"ptr_read({ptr.place}) reads uninitialized slot")
            return
        owner.slots[ptr.place] = "Uninit"
        if into_owner is not None:
            self.owners[into_owner] = OwnerState(
                root=into_owner,
                slots={into_owner: "Init"},
                handle_id=owner.handle_id,
            )
            self.scope_order.append(into_owner)

    def ptr_read_duplicate_handle(self, ptr_id: int, into_owner: str) -> None:
        """模拟错误地把同一份析构义务复制给第二个存活 owner（不将原槽位置 Uninit），析构时触发 UB4。"""
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "ptr_read_duplicate_handle")
        if owner is None:
            return
        if self._slot_state(owner, ptr.place) != "Init":
            self._record("UB5", f"ptr_read_duplicate_handle({ptr.place}) reads uninitialized slot")
            return
        self.owners[into_owner] = OwnerState(
            root=into_owner,
            slots={into_owner: "Init"},
            handle_id=owner.handle_id,
        )
        self.scope_order.append(into_owner)

    def ptr_write(self, ptr_id: int) -> None:
        """AL2：向存活槽位写入新值，将 Uninit 槽位恢复为 Init。"""
        self.step += 1
        ptr = self.ptrs[ptr_id]
        owner = self._check_live(ptr, "ptr_write")
        if owner is None:
            return
        if self._check_al1_write(ptr, owner, "ptr_write"):
            owner.slots[ptr.place] = "Init"

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
        # 若未失败：覆盖 elem_i 为新句柄后退出
        interp.owners["elem_i"].handle_id = 999
        interp.handles_dropped[999] = 0
        interp.exit_scope()
        return interp.violations

    # 正确次序：1. 先执行可能失败的 clone()，此时容器状态完好
    if clone_fails:
        interp.exit_scope()
        return interp.violations
    # 2. 仅含 FS4 不失败操作的搬移与写入
    interp.ptr_read(p_i, into_owner="elem_i_plus_1")
    interp.ptr_write(p_i)
    interp.owners["elem_i"].handle_id = 999
    interp.handles_dropped[999] = 0
    interp.exit_scope()
    return interp.violations

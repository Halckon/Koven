"""P1-3：StringView（UTF-8 边界零拷贝切片）、Slice<T> 与 Iterable/for 协议（借用迭代 vs 消费迭代）原型。

非规范、不被编译器引用。对齐 docs/proposals/l3-litmus-programs.md 与
docs/proposals/evolution-13-implementation-roadmap.md §4.3（阶段二出口试金石）：
- StringView：UTF-8 码点边界切片校验（落在 UTF-8 续字节 0x80..0xBF 或越界时报错）、零拷贝分词器（保持同一 origin_owner）；
- Slice<T>：连续子区间视图，支持零拷贝子切片与来源追踪；
- for 协议双轨：
  1. 借用迭代（iter_borrow）：在循环期间对集合 owner 持有 Shared loan，循环体内修改集合报 FOR_LOAN_CONFLICT，循环结束后恢复可写；
  2. 消费迭代（iter_own）：移走集合 owner 产出拥有元素，循环结束后再使用原集合报 USE_AFTER_CONSUME_FOR。
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Generic, List, Optional, Tuple, TypeVar

T = TypeVar("T")


def _is_utf8_char_boundary(data: bytes, idx: int) -> bool:
    if idx == 0 or idx == len(data):
        return True
    if idx < 0 or idx > len(data):
        return False
    # UTF-8 续字节高两位为 10 (0x80..0xBF)，不是合法码点边界
    return (data[idx] & 0xC0) != 0x80


@dataclass(frozen=True)
class StringView:
    origin_owner: str
    _backing: bytes
    start: int
    end: int

    @classmethod
    def from_owner(cls, owner_name: str, text: str) -> "StringView":
        raw = text.encode("utf-8")
        return cls(owner_name, raw, 0, len(raw))

    @property
    def byte_len(self) -> int:
        return self.end - self.start

    def as_str(self) -> str:
        return self._backing[self.start : self.end].decode("utf-8")

    def slice_bytes(self, rel_start: int, rel_end: int) -> Tuple[Optional["StringView"], Optional[str]]:
        if rel_start < 0 or rel_end < rel_start or rel_end > self.byte_len:
            return None, f"OOB_SLICE: 切片 [{rel_start}, {rel_end}) 超出范围 [0, {self.byte_len})"
        abs_start = self.start + rel_start
        abs_end = self.start + rel_end
        if not _is_utf8_char_boundary(self._backing, abs_start) or not _is_utf8_char_boundary(self._backing, abs_end):
            return None, f"UTF8_BOUNDARY_ABORT: 切片 [{rel_start}, {rel_end}) 落在 UTF-8 多字节字符中间"
        return StringView(self.origin_owner, self._backing, abs_start, abs_end), None

    def split_ascii_whitespace(self) -> List["StringView"]:
        """零拷贝分词器试金石：所有返回的 StringView 共享原 backing 与 origin_owner，零新分配。"""
        tokens: List[StringView] = []
        i = 0
        n = self.byte_len
        while i < n:
            while i < n and self._backing[self.start + i] in (0x20, 0x09, 0x0A, 0x0D):
                i += 1
            tok_start = i
            while i < n and self._backing[self.start + i] not in (0x20, 0x09, 0x0A, 0x0D):
                i += 1
            if tok_start < i:
                sub, err = self.slice_bytes(tok_start, i)
                assert err is None and sub is not None
                tokens.append(sub)
        return tokens


@dataclass(frozen=True)
class SliceView(Generic[T]):
    origin_owner: str
    _backing: Tuple[T, ...]
    start: int
    end: int

    def subslice(self, rel_start: int, rel_end: int) -> Tuple[Optional["SliceView[T]"], Optional[str]]:
        length = self.end - self.start
        if rel_start < 0 or rel_end < rel_start or rel_end > length:
            return None, f"OOB_SLICE: Slice 区间 [{rel_start}, {rel_end}) 越界"
        return SliceView(self.origin_owner, self._backing, self.start + rel_start, self.start + rel_end), None

    def items(self) -> Tuple[T, ...]:
        return self._backing[self.start : self.end]


@dataclass
class OwnedCollection(Generic[T]):
    """模拟支持借用迭代（Shared loan）与消费迭代（move owner）的集合。"""
    name: str
    elements: List[T] = field(default_factory=list)
    moved: bool = False
    active_shared_loans: int = 0

    def mutate_push(self, item: T) -> Optional[str]:
        if self.moved:
            return f"USE_AFTER_CONSUME_FOR: 集合 {self.name} 已被消费迭代移走"
        if self.active_shared_loans > 0:
            return f"FOR_LOAN_CONFLICT: 集合 {self.name} 正处于借用迭代（Shared loan），禁止修改"
        self.elements.append(item)
        return None

    def read_len(self) -> Tuple[Optional[int], Optional[str]]:
        if self.moved:
            return None, f"USE_AFTER_CONSUME_FOR: 集合 {self.name} 已被消费迭代移走"
        return len(self.elements), None

    def iter_borrow(self):
        if self.moved:
            raise RuntimeError(f"USE_AFTER_CONSUME_FOR: 集合 {self.name} 已被消费")
        self.active_shared_loans += 1
        try:
            for item in self.elements:
                yield item
        finally:
            self.active_shared_loans -= 1

    def iter_own(self):
        if self.moved:
            raise RuntimeError(f"USE_AFTER_CONSUME_FOR: 集合 {self.name} 已被消费")
        if self.active_shared_loans > 0:
            raise RuntimeError(f"FOR_LOAN_CONFLICT: 集合 {self.name} 正被借用，不能消费迭代")
        self.moved = True
        taken = list(self.elements)
        self.elements = []
        for item in taken:
            yield item

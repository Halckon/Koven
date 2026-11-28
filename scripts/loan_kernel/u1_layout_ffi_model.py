"""P1-2：U1 布局/填充字节（MM7）、AL6 裸指针来历与配对释放（UB10/UB11）及 FFI1/FFI3 ABI 边界原型。

非规范、不被编译器引用。对齐 docs/proposals/unsafe-aliasing-contract-outline.md §3.2、§5 MM7、§6、§8：
- 确定性 C 兼容结构体布局计算（sizeOf / alignOf / 字段偏移 / 内部与尾部填充字节区间）；
- MM7：读取与结构体填充字节重叠的区间构成 UB5_PADDING_READ；
- AL6 / UB10 / UB11：分配表追踪 alloc_id 与 (size, align)，ptr.offset 继承来历，越界或未对齐解引用报 UB10，
  内部偏移指针释放、二次释放或 (size, align) 布局不匹配释放报 UB11 / UB4；
- FFI1 / FFI3：检验 extern "C" 声明与 C 符号的 ABI 签名一致（UB9_FFI1），以及回调必须无捕获且为 C ABI（UB9_FFI3）。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple


@dataclass(frozen=True)
class FieldLayoutSpec:
    name: str
    size: int
    align: int


@dataclass(frozen=True)
class StructLayout:
    size: int
    align: int
    field_offsets: Tuple[Tuple[str, int, int], ...]  # (name, offset, size)
    padding_ranges: Tuple[Tuple[int, int], ...]  # [start, end)


def _align_up(offset: int, align: int) -> int:
    if align <= 0 or (align & (align - 1)) != 0:
        raise ValueError(f"对齐必须是 2 的正整数次幂，得到 {align}")
    return (offset + align - 1) & ~(align - 1)


def compute_struct_layout(fields: Tuple[FieldLayoutSpec, ...]) -> StructLayout:
    """按 C ABI 规则计算结构体布局与所有内部/尾部填充字节区间。"""
    if not fields:
        return StructLayout(size=0, align=1, field_offsets=(), padding_ranges=())
    offset = 0
    max_align = 1
    offsets: List[Tuple[str, int, int]] = []
    padding: List[Tuple[int, int]] = []
    for f in fields:
        aligned = _align_up(offset, f.align)
        if aligned > offset:
            padding.append((offset, aligned))
        offsets.append((f.name, aligned, f.size))
        offset = aligned + f.size
        if f.align > max_align:
            max_align = f.align
    total_size = _align_up(offset, max_align)
    if total_size > offset:
        padding.append((offset, total_size))
    return StructLayout(
        size=total_size,
        align=max_align,
        field_offsets=tuple(offsets),
        padding_ranges=tuple(padding),
    )


def check_mm7_read_range(layout: StructLayout, read_start: int, read_size: int) -> Optional[str]:
    """MM7：强类型读取或字节读取不得触碰未初始化的结构体填充字节区间。"""
    read_end = read_start + read_size
    if read_start < 0 or read_end > layout.size:
        return f"UB10_OOB: 读取区间 [{read_start}, {read_end}) 超出结构体大小 {layout.size}"
    for p_start, p_end in layout.padding_ranges:
        if read_start < p_end and p_start < read_end:
            return f"UB5_PADDING_READ: 读取区间 [{read_start}, {read_end}) 触碰填充字节 [{p_start}, {p_end})"
    return None


@dataclass(frozen=True)
class RawAllocPtr:
    alloc_id: int
    offset: int
    alloc_size: int
    alloc_align: int

    def offset_by(self, delta_bytes: int) -> "RawAllocPtr":
        return RawAllocPtr(self.alloc_id, self.offset + delta_bytes, self.alloc_size, self.alloc_align)


class U1MemoryTable:
    """AL6 分配块与来历校验表。"""

    def __init__(self) -> None:
        self._next_id = 1
        self._live: Dict[int, Tuple[int, int]] = {}  # alloc_id -> (size, align)

    def alloc(self, size: int, align: int) -> RawAllocPtr:
        if size <= 0:
            raise ValueError("分配大小须大于 0")
        _align_up(0, align)
        aid = self._next_id
        self._next_id += 1
        self._live[aid] = (size, align)
        return RawAllocPtr(aid, 0, size, align)

    def check_deref(self, ptr: RawAllocPtr, access_size: int, access_align: int) -> Optional[str]:
        if ptr.alloc_id not in self._live:
            return f"UB2_UAF: 分配块 {ptr.alloc_id} 已释放"
        size, _ = self._live[ptr.alloc_id]
        if ptr.offset < 0 or ptr.offset + access_size > size:
            return f"UB10_OOB: 偏移 [{ptr.offset}, {ptr.offset + access_size}) 超出分配范围 [0, {size})"
        if ptr.offset % access_align != 0:
            return f"UB10_MISALIGNED: 偏移 {ptr.offset} 不满足 {access_align} 字节对齐"
        return None

    def dealloc(self, ptr: RawAllocPtr, size: int, align: int) -> Optional[str]:
        if ptr.alloc_id not in self._live:
            return f"UB4_DOUBLE_FREE: 分配块 {ptr.alloc_id} 已被释放"
        if ptr.offset != 0:
            return f"UB11_INTERIOR_DEALLOC: 不能对内部偏移指针 offset={ptr.offset} 调用 dealloc"
        live_size, live_align = self._live[ptr.alloc_id]
        if (size, align) != (live_size, live_align):
            return f"UB11_LAYOUT_MISMATCH: dealloc 布局 {(size, align)} 与 alloc {(live_size, live_align)} 不匹配"
        del self._live[ptr.alloc_id]
        return None


# ---- FFI1 & FFI3 C ABI 边界校验

@dataclass(frozen=True)
class CAbiSig:
    name: str
    param_types: Tuple[str, ...]
    return_type: str
    calling_conv: str = "C"


@dataclass(frozen=True)
class CallbackSpec:
    name: str
    calling_conv: str
    captures: Tuple[str, ...] = ()


def check_ffi1_signature(koven_extern: CAbiSig, actual_c_symbol: CAbiSig) -> Optional[str]:
    """FFI1：Koven extern "C" 声明须与实际链接 C 符号的参数个数、ABI 类型与调用约定完全匹配。"""
    if (
        koven_extern.calling_conv != actual_c_symbol.calling_conv
        or koven_extern.param_types != actual_c_symbol.param_types
        or koven_extern.return_type != actual_c_symbol.return_type
    ):
        return (
            f"UB9_FFI1_ABI_MISMATCH: extern 声明 {koven_extern} 与 C 符号 {actual_c_symbol} 不匹配"
        )
    return None


def check_ffi3_callback(cb: CallbackSpec) -> List[str]:
    """FFI3：传给 C 的回调必须无捕获且以 C ABI 导出。"""
    errors: List[str] = []
    if cb.captures:
        errors.append(f"UB9_FFI3_CAPTURING_CLOSURE: 回调 {cb.name} 捕获了 {cb.captures}，不得转为 C 函数指针")
    if cb.calling_conv != "C":
        errors.append(f"UB9_FFI3_NON_C_ABI: 回调 {cb.name} 调用约定为 {cb.calling_conv}，须为 C ABI")
    return errors

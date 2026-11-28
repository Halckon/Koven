"""P2-3：VE1–VE3 显式环绕/饱和算术、固定宽度 SIMD 向量表示与运行时目标特性分派原型。

非规范、不被编译器引用。对齐 docs/proposals/evolution-08-compiler-ir-escape-simd-selfhost.md §4（J32）：
- VE1：显式环绕（wrapping）与饱和（saturating）算术函数，不改变默认 checked 运算符语义；
- VE2：固定宽度向量类型（如 I32x4 / F32x4，不引入 const 泛型），校验自然向量对齐，逐分量运算与点积
  和标量参考实现一致；在无 SIMD 目标上退化为标量实现；
- VE3：运行时目标特性检测与多版本分派（按优先级选择已支持的最强目标实现并回退到标量），以及
  静态拒绝未经特性守卫或同特性声明即调用 @targetFeature 函数（VE3_UNGUARDED_TARGET_FEATURE）。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, FrozenSet, List, Optional, Tuple

I32_MIN = -(1 << 31)
I32_MAX = (1 << 31) - 1
I32_MASK = (1 << 32) - 1


# ---- VE1：显式环绕 / 饱和 / 带检查算术

def checked_add_i32(a: int, b: int) -> Tuple[Optional[int], Optional[str]]:
    res = a + b
    if res < I32_MIN or res > I32_MAX:
        return None, "VE1_CHECKED_OVERFLOW: 默认 + 溢出触发 abort"
    return res, None


def wrapping_add_i32(a: int, b: int) -> int:
    raw = (a + b) & I32_MASK
    return raw - (1 << 32) if raw >= (1 << 31) else raw


def saturating_add_i32(a: int, b: int) -> int:
    res = a + b
    if res > I32_MAX:
        return I32_MAX
    if res < I32_MIN:
        return I32_MIN
    return res


# ---- VE2：固定宽度向量表示、自然对齐与标量等价性

@dataclass(frozen=True)
class SimdI32x4:
    lanes: Tuple[int, int, int, int]

    @staticmethod
    def byte_size() -> int:
        return 16

    @staticmethod
    def align() -> int:
        return 16

    @classmethod
    def load_aligned(cls, buf: Tuple[int, ...], byte_offset: int) -> Tuple[Optional["SimdI32x4"], Optional[str]]:
        if byte_offset % cls.align() != 0:
            return None, f"VE2_MISALIGNED_VECTOR_ACCESS: 偏移 {byte_offset} 不满足 {cls.align} 字节向量对齐"
        idx = byte_offset // 4
        if idx < 0 or idx + 4 > len(buf):
            return None, f"VE2_OOB_VECTOR_LOAD: 索引 [{idx}, {idx + 4}) 越界"
        return cls((buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3])), None

    def wrapping_add(self, other: "SimdI32x4") -> "SimdI32x4":
        return SimdI32x4(tuple(wrapping_add_i32(a, b) for a, b in zip(self.lanes, other.lanes)))

    def mul_lanes(self, other: "SimdI32x4") -> "SimdI32x4":
        return SimdI32x4(tuple(wrapping_add_i32(a * b, 0) for a, b in zip(self.lanes, other.lanes)))

    def horizontal_sum(self) -> int:
        acc = 0
        for x in self.lanes:
            acc = wrapping_add_i32(acc, x)
        return acc


def dot_product_scalar(xs: Tuple[int, ...], ys: Tuple[int, ...]) -> int:
    acc = 0
    for a, b in zip(xs, ys):
        acc = wrapping_add_i32(acc, wrapping_add_i32(a * b, 0))
    return acc


def dot_product_simd_i32x4(xs: Tuple[int, ...], ys: Tuple[int, ...]) -> int:
    """VE2 向量点积（4 通道主循环 + 标量尾处理），结果须与标量版完全一致。"""
    n = min(len(xs), len(ys))
    acc_vec = SimdI32x4((0, 0, 0, 0))
    i = 0
    while i + 4 <= n:
        vx, e1 = SimdI32x4.load_aligned(xs, i * 4)
        vy, e2 = SimdI32x4.load_aligned(ys, i * 4)
        assert e1 is None and e2 is None and vx is not None and vy is not None
        acc_vec = acc_vec.wrapping_add(vx.mul_lanes(vy))
        i += 4
    total = acc_vec.horizontal_sum()
    while i < n:
        total = wrapping_add_i32(total, wrapping_add_i32(xs[i] * ys[i], 0))
        i += 1
    return total


# ---- VE3：目标特性注解、守卫校验与运行时多版本分派

@dataclass(frozen=True)
class TargetImpl:
    name: str
    required_features: FrozenSet[str]
    priority: int  # 越大越优先；便携标量版 priority=0, required_features=frozenset()


def check_target_feature_call(
    caller_features: FrozenSet[str],
    runtime_guarded_features: FrozenSet[str],
    callee: TargetImpl,
) -> Optional[str]:
    """VE3：调用带 @targetFeature 的函数必须由调用方同特性注解或运行时特性检测守卫覆盖。"""
    covered = caller_features | runtime_guarded_features
    missing = sorted(callee.required_features - covered)
    if missing:
        return f"VE3_UNGUARDED_TARGET_FEATURE: 调用 {callee.name} 缺少目标特性守卫 {missing}"
    return None


def dispatch_target_version(
    runtime_cpu_features: FrozenSet[str],
    candidates: Tuple[TargetImpl, ...],
) -> Optional[TargetImpl]:
    """VE3：按运行时 CPU 特性选择优先级最高且完全受支持的版本（自动回退到标量）。"""
    eligible = [c for c in candidates if c.required_features <= runtime_cpu_features]
    if not eligible:
        return None
    return max(eligible, key=lambda c: (c.priority, len(c.required_features), c.name))

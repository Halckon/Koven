"""U0 §7（TR1–TR4）：受信任声明、private 字段绑定与人工审查范围（Audit Scope）原型。

非规范、不被编译器引用。对齐 docs/proposals/unsafe-aliasing-contract-outline.md §7（2026-11-15 裁决）：
- TR2：手工断言 Transferable / Shareable 属于受信任声明（须标记 trusted/unsafe）；
- TR3：逻辑接口实现（Equatable / Hashable / Comparable）写错不破坏内存安全，不要求 unsafe；
- TR4.1：含 trusted 签名或受 unsafe 维护不变量的类型，其借用字段与不变量字段必须为 private；
- TR4.2：按 Guide 02 现行规则，成员的 private 仅在声明它的 classifier 内可见，因此人工审查范围是
  该类型的全部成员函数，同文件的顶层函数与扩展函数不在其内；
- TR4.3：若未来规则把 private 扩大为文件级可见（file_private），审查范围须自动同步扩大到同文件顶层/扩展函数。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, FrozenSet, List, Tuple

TRUSTED_CAPABILITIES = frozenset({"Transferable", "Shareable"})
LOGICAL_INTERFACES = frozenset({"Equatable", "Hashable", "Comparable"})


@dataclass(frozen=True)
class FieldSpec:
    name: str
    visibility: str  # public | internal | private
    is_borrow_field: bool = False
    invariant_maintained_by_unsafe: bool = False


@dataclass(frozen=True)
class FunctionSpec:
    name: str
    file_path: str
    owner_type: str = ""  # 空字符串表示顶层函数；非空表示该类型的成员函数或扩展函数
    is_extension: bool = False  # 扩展函数不享有 Guide 02 classifier 内 private 成员访问权
    is_trusted_or_unsafe: bool = False


@dataclass(frozen=True)
class CapabilityImplSpec:
    type_name: str
    capability_or_iface: str
    is_manual_trusted: bool


@dataclass(frozen=True)
class TypeAuditSpec:
    name: str
    file_path: str
    fields: Tuple[FieldSpec, ...]
    has_trusted_signature: bool = False
    uses_unsafe_invariant: bool = False


def check_tr2_tr3_capability_impl(impl: CapabilityImplSpec) -> List[str]:
    """TR2 / TR3：Transferable/Shareable 手工断言必须标 trusted；Equatable/Hashable/Comparable 不要求 unsafe。"""
    errors: List[str] = []
    if impl.capability_or_iface in TRUSTED_CAPABILITIES and not impl.is_manual_trusted:
        errors.append(
            f"TR2_REQUIRES_TRUSTED: {impl.type_name} 手工声明 {impl.capability_or_iface} 必须使用受信任声明"
        )
    if impl.capability_or_iface in LOGICAL_INTERFACES and impl.is_manual_trusted:
        errors.append(
            f"TR3_UNNECESSARY_UNSAFE: {impl.type_name} 实现逻辑接口 {impl.capability_or_iface} 不要求 unsafe/trusted"
        )
    return errors


def check_tr4_private_fields(tspec: TypeAuditSpec) -> List[str]:
    """TR4.1：含 trusted 签名或 unsafe 维护不变量的类型，其借用字段与不变量字段必须为 private。"""
    if not (tspec.has_trusted_signature or tspec.uses_unsafe_invariant):
        return []
    errors: List[str] = []
    for fld in tspec.fields:
        if (fld.is_borrow_field or fld.invariant_maintained_by_unsafe) and fld.visibility != "private":
            errors.append(
                f"TR4_NON_PRIVATE_INVARIANT_FIELD: {tspec.name}.{fld.name}({fld.visibility}) "
                f"在含 trusted/unsafe 的类型中必须声明为 private"
            )
    return errors


def compute_tr4_audit_scope(
    tspec: TypeAuditSpec,
    functions: Tuple[FunctionSpec, ...],
    private_visibility_model: str = "classifier",  # classifier (Guide 02) | file_wide (TR4.3 未来扩大对照)
) -> FrozenSet[str]:
    """TR4.2 / TR4.3：计算能访问 tspec 私有不变量字段的全部函数（即完整人工审查范围）。"""
    if not (tspec.has_trusted_signature or tspec.uses_unsafe_invariant):
        return frozenset()
    if private_visibility_model not in ("classifier", "file_wide"):
        raise ValueError(f"未知 private 可见性模型: {private_visibility_model}")

    audited = set()
    for fn in functions:
        # 1. 类型自身的所有成员函数（无论是否标 unsafe/trusted）都能访问该类型 private 字段
        if fn.owner_type == tspec.name and not fn.is_extension:
            audited.add(fn.name)
        # 2. TR4.3 对照：若未来规则把 private 扩大为文件级可见，同文件顶层函数与扩展函数一并纳入审查范围
        elif private_visibility_model == "file_wide" and fn.file_path == tspec.file_path:
            audited.add(fn.name)
    return frozenset(audited)

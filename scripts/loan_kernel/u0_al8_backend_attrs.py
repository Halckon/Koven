"""U0 §3.3（AL8 / 之八 §3.2 / VE1 前置）：参数模式到后端 LLVM 别名与捕获属性映射原型。

非规范、不被编译器引用。对齐 docs/proposals/unsafe-aliasing-contract-outline.md §3.3：
- 递归检测类型图是否含 RawCell（支持自递归/互递归类型，不死循环）；
- borrow x: T：递归不含 RawCell 可发射 readonly/nonnull/align；只要递归含 RawCell，严禁 readonly 与 noalias；
- inout x: T：无论是否含 RawCell 均可发射 noalias/nonnull/align（严禁 readonly）；
- own x: T（间接 owner 指针）：仅唯一独占 owner 可发射 noalias/nonnull/align，Rc/Arc 等共享句柄严禁 noalias；
- nocapture 双重前置：仅“未出现在 from/flows 中”不足以发射 nocapture，必须同时经独立捕获分析证实地址未经
  addressOf / ptr_to_int / FFI / 未证实 nocapture 的被调方逃逸。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, FrozenSet, List, Optional, Set, Tuple


@dataclass(frozen=True)
class TypeDef:
    name: str
    fields: Tuple[str, ...] = ()
    is_raw_cell: bool = False
    is_shared_handle: bool = False  # 如 Rc<T> / Arc<T>：底层载荷共享，不得对载荷标 noalias


@dataclass(frozen=True)
class ParamSpec:
    name: str
    mode: str  # borrow | inout | own
    ty_name: str
    in_from_or_flows: bool = False
    indirect_pointer: bool = True  # own 参数是否以间接 owner 指针表示


@dataclass(frozen=True)
class BodyEffects:
    """函数体与被调方的捕获分析摘要。"""
    has_unsafe_address_escape: bool = False  # addressOf / ptr_to_int / ffi_call 可能导致地址逃逸
    calls_unverified_callee: bool = False  # 调用了未证实 nocapture 的外部/不透明函数
    escaped_params: FrozenSet[str] = frozenset()  # 捕获分析发现地址可能逃逸的具名参数


def contains_raw_cell(ty_name: str, types: Dict[str, TypeDef], _visiting: Optional[FrozenSet[str]] = None) -> bool:
    """递归判定类型 ty_name 是否直接或间接包含 RawCell（处理递归类型环）。"""
    visiting = _visiting or frozenset()
    if ty_name in visiting:
        return False
    tdef = types.get(ty_name)
    if tdef is None:
        return False
    if tdef.is_raw_cell:
        return True
    next_visiting = visiting | {ty_name}
    return any(contains_raw_cell(field_ty, types, next_visiting) for field_ty in tdef.fields)


def is_nocapture_proven(param: ParamSpec, effects: BodyEffects) -> bool:
    """AL8 nocapture 双重前置条件：（1）不在 from/flows 中，且（2）捕获分析证实地址未逃逸。"""
    if param.in_from_or_flows:
        return False
    if param.name in effects.escaped_params:
        return False
    if effects.has_unsafe_address_escape or effects.calls_unverified_callee:
        return False
    return True


def derive_backend_attrs(
    param: ParamSpec,
    types: Dict[str, TypeDef],
    effects: BodyEffects,
) -> FrozenSet[str]:
    """按 U0 §3.3 AL8 推导允许发射的后端 LLVM 参数属性集合。"""
    tdef = types.get(param.ty_name)
    has_cell = contains_raw_cell(param.ty_name, types)
    nocapture = is_nocapture_proven(param, effects)

    attrs: Set[str] = set()
    if param.mode == "borrow":
        attrs.update({"nonnull", "align"})
        if not has_cell:
            attrs.add("readonly")
        if nocapture:
            attrs.add("nocapture")
    elif param.mode == "inout":
        attrs.update({"noalias", "nonnull", "align"})
        if nocapture:
            attrs.add("nocapture")
    elif param.mode == "own":
        if param.indirect_pointer:
            attrs.update({"nonnull", "align"})
            if not (tdef is not None and tdef.is_shared_handle):
                attrs.add("noalias")
    else:
        raise ValueError(f"未知参数模式: {param.mode}")
    return frozenset(attrs)


def audit_proposed_attrs(
    param: ParamSpec,
    proposed: FrozenSet[str],
    types: Dict[str, TypeDef],
    effects: BodyEffects,
) -> List[str]:
    """审计候选后端属性集合是否违反 AL8 禁止条件，返回所有违规项。"""
    allowed = derive_backend_attrs(param, types, effects)
    forbidden = sorted(proposed - allowed)
    violations: List[str] = []
    for attr in forbidden:
        if attr == "readonly" and param.mode == "borrow" and contains_raw_cell(param.ty_name, types):
            violations.append(f"AL8_FORBIDDEN_READONLY: {param.name}({param.ty_name}) 递归含 RawCell，严禁 readonly")
        elif attr == "noalias" and param.mode == "borrow":
            violations.append(f"AL8_FORBIDDEN_NOALIAS: borrow 参数 {param.name} 不得标 noalias")
        elif attr == "noalias" and param.mode == "own":
            violations.append(f"AL8_FORBIDDEN_NOALIAS: 共享句柄 own 参数 {param.name}({param.ty_name}) 严禁对载荷标 noalias")
        elif attr == "nocapture":
            violations.append(
                f"AL8_FORBIDDEN_NOCAPTURE: {param.name} 未同时满足无 from/flows 与独立捕获证明，严禁 nocapture"
            )
        else:
            violations.append(f"AL8_FORBIDDEN_ATTR: {param.name} 不允许发射属性 {attr}")
    return violations

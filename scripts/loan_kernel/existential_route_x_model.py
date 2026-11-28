"""路线 X（X1–X4 存在类型）与 E2/E3 通用错误转换可执行原型。

非规范、不被编译器引用。对齐 docs/proposals/existential-visitor-litmus.md 与演进总纲之三：
- X1 动态兼容检查：拒绝含非接收者位置 Self、按值返回 Self、泛型方法或无接收者方法的 interface 进入值位置（XN4）；
- X2 借用 existential（pack_borrow / pack_inout）：普通 class 与栈上 value class 均零堆分配；不可逃逸（XN3）；
  载荷可变访问允许调方法改状态但禁止整体替换槽位（XN1）；
- X3 owned existential（pack_owned）：普通 class 零额外分配移交且原绑定失效（XN5）；内联 value/enum class
  在 X3 内核层须显式 Box（X4 才引入隐式装箱糖）；owned 槽位允许整体替换并经旧见证表析构旧值；容器不协变（XN2）；
- E2/E3 显式错误转换：? 仅选中显式声明的 Into<Target> 实现，不凭“实现接口”隐式装箱，且绝不把 Cancelled 转为 Error。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, FrozenSet, List, Optional, Tuple


@dataclass(frozen=True)
class MethodSig:
    name: str
    has_receiver: bool = True
    receiver_mode: str = "borrow"  # borrow | inout | own
    type_params: Tuple[str, ...] = ()
    param_types: Tuple[str, ...] = ()
    return_type: str = "Unit"


@dataclass(frozen=True)
class InterfaceDecl:
    name: str
    methods: Tuple[MethodSig, ...]
    super_interfaces: Tuple[str, ...] = ()


@dataclass(frozen=True)
class ConcreteType:
    name: str
    kind: str  # class | value_class | enum_class | box
    implements: FrozenSet[str] = frozenset()
    into_targets: FrozenSet[str] = frozenset()


@dataclass(frozen=True)
class ExistentialValue:
    interface_name: str
    concrete_name: str
    ownership: str  # borrow | inout | own
    payload_only: bool  # True = 载荷访问（不可整体替换）；False = 槽位访问（可整体替换）
    escapable: bool
    extra_allocations: int


def check_dynamic_compatible(iface: InterfaceDecl) -> List[str]:
    """X1（R1.2 / J38 / XN4）：检验 interface 是否动态兼容（可作为存在类型出现在值位置）。"""
    errors: List[str] = []
    for m in iface.methods:
        if not m.has_receiver:
            errors.append(f"X1_NO_RECEIVER: {iface.name}.{m.name} 无接收者，不可动态分发")
        if m.type_params:
            errors.append(f"X1_GENERIC_METHOD: {iface.name}.{m.name} 含泛型参数 {m.type_params}，不可动态分发")
        if "Self" in m.param_types:
            errors.append(f"X1_SELF_IN_PARAM: {iface.name}.{m.name} 参数含 Self，不可动态分发")
        if m.return_type == "Self":
            errors.append(f"X1_SELF_IN_RETURN: {iface.name}.{m.name} 按值返回 Self，不可动态分发")
    return errors


def pack_borrowed(
    concrete: ConcreteType,
    iface: InterfaceDecl,
    mode: str = "inout",  # borrow | inout
) -> Tuple[Optional[ExistentialValue], List[str]]:
    """X2（R2.1–R2.4）：pack_borrow / pack_inout，普通 class 与栈上 value class 均零堆分配。"""
    errs = check_dynamic_compatible(iface)
    if iface.name not in concrete.implements:
        errs.append(f"X2_NOT_IMPLEMENTED: {concrete.name} 未实现 {iface.name}")
    if errs:
        return None, errs
    return (
        ExistentialValue(
            interface_name=iface.name,
            concrete_name=concrete.name,
            ownership=mode,
            payload_only=True,
            escapable=False,
            extra_allocations=0,
        ),
        [],
    )


def pack_owned(
    concrete: ConcreteType,
    iface: InterfaceDecl,
    stage: str = "X3",  # X3（要求显式 Box）| X4（允许内联类型隐式装箱糖）
) -> Tuple[Optional[ExistentialValue], List[str]]:
    """X3/X4（R3.1–R3.6）：pack_owned；普通 class 与 Box 零额外分配移交，内联类型在 X3 须显式 Box。"""
    errs = check_dynamic_compatible(iface)
    if iface.name not in concrete.implements:
        errs.append(f"X3_NOT_IMPLEMENTED: {concrete.name} 未实现 {iface.name}")
    extra_allocs = 0
    if concrete.kind in ("value_class", "enum_class"):
        if stage == "X3":
            errs.append(f"X3_INLINE_REQUIRES_BOX: X3 内核层 {concrete.name}({concrete.kind}) 须显式 Box 后再 pack_owned")
        else:
            extra_allocs = 1
    if errs:
        return None, errs
    return (
        ExistentialValue(
            interface_name=iface.name,
            concrete_name=concrete.name,
            ownership="own",
            payload_only=False,
            escapable=True,
            extra_allocations=extra_allocs,
        ),
        [],
    )


def check_overwrite_existential(slot: ExistentialValue, new_val: ExistentialValue) -> Optional[str]:
    """XN1 / R2.4 / R3.5：载荷可变访问严禁整体替换；owned 存在类型槽位允许整体替换同一 interface。"""
    if slot.payload_only:
        return f"X2_PAYLOAD_OVERWRITE_FORBIDDEN: {slot.interface_name} 载荷访问禁止整体替换具体类型"
    if slot.interface_name != new_val.interface_name:
        return f"X3_SLOT_IFACE_MISMATCH: 不能把 {new_val.interface_name} 写入 {slot.interface_name} 槽位"
    return None


def check_escape_existential(val: ExistentialValue, has_from_contract: bool = False) -> Optional[str]:
    """XN3 / R2.3：借用 existential 不可逃逸（不得存入拥有字段或无 from 返回）。"""
    if not val.escapable and not has_from_contract:
        return f"X2_BORROW_EXISTENTIAL_ESCAPE: 借用 existential {val.interface_name} 不得逃逸出当前来源"
    return None


def check_container_coercion(src_elem: str, dst_elem: str) -> Optional[str]:
    """XN2 / R3.6：容器对元素类型不变，List<Concrete> 不得隐式协变为 List<Interface>。"""
    if src_elem != dst_elem:
        return f"X3_CONTAINER_INVARIANT: 容器不协变，禁止将 List<{src_elem}> 视作 List<{dst_elem}>"
    return None


def check_e2_error_conversion(src: ConcreteType, dst_name: str) -> Optional[str]:
    """之十一 E2/E3：? 处错误转换仅选中显式声明的 Into<Target>，不凭“实现接口”隐式转换，且禁转 Cancelled。"""
    if src.name == "Cancelled" and dst_name != "Cancelled":
        return f"E2_FORBIDDEN_CANCEL_TO_ERR: 禁止将 Cancelled 隐式转为 {dst_name}"
    if src.name == dst_name:
        return None
    if dst_name in src.into_targets:
        return None
    return f"E2_MISSING_EXPLICIT_INTO: {src.name} 未显式声明 Into<{dst_name}>（仅实现接口不触发隐式转换）"

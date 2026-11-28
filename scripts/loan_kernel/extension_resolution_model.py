"""受限与完整扩展函数（步骤 1→2→3 / 之六 J9–J10）解析优先级与边界原型。

非规范、不被编译器引用。对齐 docs/proposals/restricted-extension-functions.md 与演进总纲之六 §4（J9–J10）：
- 步骤 1（受限扩展）：L1 仅允许编译器绑定身份的 intrinsic receiver（同名用户类型不获得能力）+ L2 仅允许 koven 标准库声明；
- 步骤 2（放开 receiver 类型）：移除 L1 诊断，保留 L2；
- 步骤 3（完整用户扩展）：移除 L1 与 L2，启用作用域/import 候选查找与同名同形歧义报错（J9）；
- 全阶段共同不变量：
  1. member 永远优先于同 shape 的扩展函数；
  2. receiver mode（borrow / inout / own）不参与 overload shape，仅靠 receiver mode 区分视为重复/歧义；
  3. 扩展函数绝不能访问 receiver 的 private 成员；
  4. 扩展函数永不使类型满足任何 interface（J10）。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, FrozenSet, List, Optional, Tuple

INTRINSIC_RECEIVER_IDS = frozenset({
    "intrinsic:Array",
    "intrinsic:List",
    "intrinsic:MutableList",
    "intrinsic:View",
    "intrinsic:String",
    "intrinsic:Box",
    "intrinsic:Rc",
})


@dataclass(frozen=True)
class ReceiverType:
    name: str
    compiler_id: str  # 例如 "intrinsic:List" 或 "user:app::List"


@dataclass(frozen=True)
class CallableDecl:
    name: str
    receiver_id: str  # 目标类型 compiler_id
    receiver_mode: str  # borrow | inout | own
    param_types: Tuple[str, ...]
    package: str
    is_member: bool = False
    accesses_receiver_private: bool = False

    def overload_shape(self) -> Tuple[str, str, Tuple[str, ...]]:
        """Guide 08 + §3.4：receiver_mode 不参与 overload shape。"""
        return (self.receiver_id, self.name, self.param_types)


def check_extension_declaration(decl: CallableDecl, step: int = 1) -> List[str]:
    """按步骤 1/2/3 以诊断（非语法错误）检查扩展函数声明边界（§6.2 / §6.5）。"""
    if decl.is_member:
        return []
    errors: List[str] = []
    if step == 1 and decl.receiver_id not in INTRINSIC_RECEIVER_IDS:
        errors.append(
            f"EXT_L1_NON_INTRINSIC: 步骤 1 仅允许编译器绑定 intrinsic receiver，拒绝 {decl.receiver_id}"
        )
    if step in (1, 2) and not (decl.package == "koven" or decl.package.startswith("koven.")):
        errors.append(
            f"EXT_L2_NON_STDLIB: 步骤 {step} 仅允许在 koven 标准库声明扩展函数，拒绝包 {decl.package}"
        )
    if decl.accesses_receiver_private:
        errors.append(
            f"EXT_PRIVATE_ACCESS_DENIED: 扩展函数 {decl.package}.{decl.name} 不得访问 receiver 的 private 成员"
        )
    return errors


def check_duplicate_receiver_mode_overloads(decls: Tuple[CallableDecl, ...]) -> List[str]:
    """检验同包内是否仅靠 receiver_mode 区分同名同参的扩展函数（§3.4 禁止）。"""
    seen: Dict[Tuple[str, str, str, Tuple[str, ...]], CallableDecl] = {}
    errors: List[str] = []
    for d in decls:
        key = (d.package, d.receiver_id, d.name, d.param_types)
        prev = seen.get(key)
        if prev is not None:
            errors.append(
                f"EXT_RECEIVER_MODE_OVERLOAD: {d.package}.{d.name} 不能仅靠 receiver_mode "
                f"({prev.receiver_mode} vs {d.receiver_mode}) 形成重载"
            )
        else:
            seen[key] = d
    return errors


def resolve_call(
    receiver: ReceiverType,
    method_name: str,
    arg_types: Tuple[str, ...],
    members: Tuple[CallableDecl, ...],
    visible_extensions: Tuple[CallableDecl, ...],
) -> Tuple[Optional[CallableDecl], Optional[str]]:
    """统一名称解析模型（§6.3）：member 优先于扩展函数；同形多扩展冲突报歧义（J9）。"""
    wanted_shape = (receiver.compiler_id, method_name, arg_types)

    member_hits = [m for m in members if m.is_member and m.overload_shape() == wanted_shape]
    if len(member_hits) == 1:
        return member_hits[0], None
    if len(member_hits) > 1:
        return None, f"MEMBER_AMBIGUOUS: {receiver.name}.{method_name} 存在多个同形 member"

    ext_hits = [e for e in visible_extensions if not e.is_member and e.overload_shape() == wanted_shape]
    if len(ext_hits) == 1:
        return ext_hits[0], None
    if len(ext_hits) > 1:
        pkgs = ", ".join(sorted(e.package for e in ext_hits))
        return None, f"EXT_AMBIGUOUS: {receiver.name}.{method_name} 在可见扩展中存在歧义（{pkgs}）"
    return None, f"RESOLVE_NOT_FOUND: 未找到 {receiver.name}.{method_name}{arg_types}"


def check_interface_satisfaction(
    receiver: ReceiverType,
    required_methods: Tuple[Tuple[str, Tuple[str, ...]], ...],
    members: Tuple[CallableDecl, ...],
    visible_extensions: Tuple[CallableDecl, ...],
) -> List[str]:
    """之六 J10：扩展函数永不使类型满足任何 interface；仅真实 member / 显式实现可满足。"""
    member_shapes = {m.overload_shape() for m in members if m.is_member}
    ext_shapes = {e.overload_shape() for e in visible_extensions if not e.is_member}
    errors: List[str] = []
    for name, params in required_methods:
        shape = (receiver.compiler_id, name, params)
        if shape in member_shapes:
            continue
        if shape in ext_shapes:
            errors.append(
                f"J10_EXT_NOT_INTERFACE_IMPL: 扩展函数 {receiver.name}.{name} 不能用于满足 interface 要求"
            )
        else:
            errors.append(f"IFACE_MISSING_METHOD: {receiver.name} 缺少接口方法 {name}{params}")
    return errors

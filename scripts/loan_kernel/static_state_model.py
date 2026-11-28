"""顶层状态、静态不可变存储（G0–G4）与之六 J5–J7 生命周期规则原型。

非规范、不被编译器引用。对齐 docs/proposals/global-and-static-state.md 与演进总纲之六 §3（J5–J7）：
- G1（v1 接纳面收窄）：顶层/object/companion 拒绝运行时 val/var，保留 const val；
- G2（静态不可变存储）：结构谓词校验静态可构造（无副作用/分配/循环依赖）、纯内存（无 deinit）、
  能力要求（v1 Transferable / v2 Shareable，拒绝 Rc）、只允许 Borrow 或 Copyable 读取（禁 move/inout/assign），
  且静态借用产出 Static(decl) origin（J5）；
- G3（Lazy<T>）：首次访问初始化一次、同线程重入触发 abort、不析构（要求纯内存 + Shareable）；
- G4（共享可变）：严禁全局 var，修改经 Shareable 内部可变类型暴露，MutexGuard 等资源类型不得入静态存储。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, FrozenSet, List, Optional, Set, Tuple


@dataclass(frozen=True)
class StaticType:
    name: str
    copyable: bool = False
    has_deinit: bool = False  # 含自身或递归资源字段
    transferable: bool = True
    shareable: bool = True
    is_interior_mut_shareable: bool = False  # 如 AtomicInt / Mutex<T>
    is_lazy_shell: bool = False


@dataclass(frozen=True)
class GlobalDecl:
    name: str
    decl_kind: str  # const_val | static_val | runtime_val | runtime_var
    ty: StaticType
    static_constructible: bool = True
    deps: Tuple[str, ...] = ()
    in_object_or_companion: bool = False


def check_global_decl(
    decl: GlobalDecl,
    stage: str = "G1",  # G1 | G2_v1 | G2_v2 | G3_G4
) -> List[str]:
    """按 G1–G4 分期校验顶层或 object/companion 声明的接纳与能力/析构约束。"""
    errors: List[str] = []
    if decl.decl_kind == "runtime_var":
        errors.append(f"C1_GLOBAL_VAR_FORBIDDEN: 禁止全局可变 var {decl.name}")
        return errors
    if decl.decl_kind == "const_val":
        if not decl.static_constructible:
            errors.append(f"G0_CONST_NOT_EVALUABLE: const val {decl.name} 不可编译期求值")
        return errors

    if stage == "G1":
        errors.append(f"G1_UNSUPPORTED_RUNTIME_GLOBAL: v1 G1 阶段拒绝运行时顶层声明 {decl.name}({decl.decl_kind})")
        return errors

    # G2 / G3 / G4
    if decl.decl_kind == "runtime_val":
        errors.append(f"G2_REQUIRES_STATIC_CONSTRUCTIBLE: 运行时 val {decl.name} 不可隐式急切或惰性初始化")
        return errors

    if not decl.static_constructible:
        errors.append(f"G2_NOT_STATIC_CONSTRUCTIBLE: {decl.name} 不满足 J6 静态可构造结构谓词")
    if decl.ty.has_deinit:
        errors.append(f"G2_OBSERVABLE_DROP_FORBIDDEN: {decl.name}({decl.ty.name}) 含可观察析构，不得进入静态存储")

    if stage == "G2_v1":
        if not decl.ty.transferable:
            errors.append(f"G2_CAP_REQUIRED: {decl.name}({decl.ty.name}) 在 v1 不满足 Transferable")
        if decl.ty.is_interior_mut_shareable or decl.ty.is_lazy_shell:
            errors.append(f"G2_V1_FORBIDS_INTERIOR_MUT: {decl.name}({decl.ty.name}) 需等 v2 Shareable（G3/G4）")
    else:
        if not decl.ty.shareable:
            errors.append(f"G2_CAP_REQUIRED: {decl.name}({decl.ty.name}) 不满足 v2 Shareable")

    return errors


def check_static_init_cycles(decls: Dict[str, GlobalDecl]) -> List[str]:
    """检测 G0/G2 静态初始化依赖图中的循环依赖。"""
    visiting: Set[str] = set()
    visited: Set[str] = set()
    errors: List[str] = []

    def dfs(name: str) -> None:
        visiting.add(name)
        decl = decls.get(name)
        if decl is not None:
            for dep in decl.deps:
                if dep in visiting:
                    errors.append(f"G2_INIT_CYCLE: 静态声明初始化存在循环 {name} -> {dep}")
                elif dep not in visited and dep in decls:
                    dfs(dep)
        visiting.remove(name)
        visited.add(name)

    for name in sorted(decls):
        if name not in visited:
            dfs(name)
    return errors


def check_static_access(decl: GlobalDecl, access_kind: str) -> Optional[str]:
    """G2 访问规则：仅允许 borrow 与 Copyable read；禁止 move_out / inout / assign。"""
    if access_kind == "borrow":
        return None
    if access_kind == "copy_read":
        if decl.ty.copyable:
            return None
        return f"G2_ILLEGAL_ACCESS: {decl.name}({decl.ty.name}) 非 Copyable，不能按值复制读取"
    if access_kind in ("move_out", "inout", "assign"):
        return f"G2_ILLEGAL_ACCESS: 静态存储 {decl.name} 禁止 {access_kind}"
    raise ValueError(f"未知访问类型: {access_kind}")


def check_static_origin_return(returned_static_decl: str, declared_from: FrozenSet[str]) -> Optional[str]:
    """之六 J5：静态存储是 Origin 的一个取值 Static(decl)，返回静态借用须由 from 声明覆盖。"""
    origin = f"Static({returned_static_decl})"
    if origin not in declared_from:
        return f"J5_UNDECLARED_STATIC_ORIGIN: 返回静态借用 {origin} 未在 from 中声明"
    return None


@dataclass
class LazyCell:
    """G3 Lazy<T> 状态机原型：首次访问执行一次；同线程重入触发 abort；永不 drop。"""
    name: str
    state: str = "uninit"  # uninit | initializing | initialized
    value: Optional[object] = None
    init_count: int = 0

    def get(self, init_fn) -> object:
        if self.state == "initialized":
            return self.value
        if self.state == "initializing":
            raise RuntimeError(f"G3_REENTRANT_ABORT: Lazy {self.name} 初始化重入触发进程终止")
        self.state = "initializing"
        self.init_count += 1
        self.value = init_fn()
        self.state = "initialized"
        return self.value
